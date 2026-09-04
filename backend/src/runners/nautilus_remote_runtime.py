"""Reference HTTP service for an independently deployed NautilusTrader runtime.

This process owns its NautilusTrader installation and ParquetDataCatalog. It has no
QuaZonai database, Codex credentials, operator credentials, broker credentials, or
Paper/Live control surface. Production may deploy an equivalent compatible service
on another host; QuaZonai Core only relies on the versioned HTTP contract.
"""

from __future__ import annotations

import importlib
import importlib.metadata
import json
import math
import os
import secrets
import selectors
import shutil
import stat
import subprocess
import tempfile
import threading
import time
import zipfile
import weakref
from datetime import UTC, datetime
from io import BytesIO
from pathlib import Path
from typing import Any, BinaryIO, cast

from fastapi import FastAPI, File, Header, HTTPException, UploadFile
from pydantic import BaseModel, ConfigDict

from quant_runtime.config import CONTRACT_VERSION, PINNED_NAUTILUS_VERSION
from quant_runtime.contracts import (
    ArchiveManifestDescriptor,
    ArchiveManifestSpec,
    ArchiveShardDescriptor,
    CatalogDescriptor,
    CatalogIngestSpec,
    RuntimeCapabilities,
)

_CATALOG_LOCKS: weakref.WeakValueDictionary[str, threading.Lock] = weakref.WeakValueDictionary()
_CATALOG_LOCKS_GUARD = threading.Lock()
_PLUGIN_CHILD_LOCK = threading.Lock()
_CATALOG_PREFIX = "catalog://"
_PLUGIN_ROOT = Path(
    os.environ.get("QUAZONAI_NAUTILUS_PLUGIN_ROOT", "/var/lib/quazonai/plugins")
).resolve()
_PLUGIN_STAGING_ROOT = Path(
    os.environ.get(
        "QUAZONAI_NAUTILUS_PLUGIN_STAGING_ROOT",
        "/var/lib/nautilus/plugin-staging",
    )
).resolve()
_PLUGIN_PRLIMIT_PATH = "/usr/bin/prlimit"
_PLUGIN_CHILD_MEMORY_LIMIT_BYTES = 6 * 1024 * 1024 * 1024
_PLUGIN_STAGED_OUTPUT_LIMIT_BYTES = 2 * 1024 * 1024 * 1024
_PLUGIN_STAGED_FILE_LIMIT = 10_000
_PLUGIN_CHILD_OUTPUT_LIMIT_BYTES = 8 * 1024 * 1024
_PLUGIN_CHILD_TOTAL_OUTPUT_LIMIT_BYTES = 32 * 1024 * 1024
_PLUGIN_VALIDATION_BATCH_ROWS = 16_384
_PLUGIN_VALIDATION_BATCH_BYTES = 64 * 1024 * 1024
_FORBIDDEN_BUNDLE_KEYS = {
    "api_key",
    "apikey",
    "private_key",
    "secret_key",
    "service_token",
    "access_token",
    "refresh_token",
    "account_password",
    "wallet_seed",
    "broker_url",
    "account_id",
}
_CANDIDATE_BUNDLE_CONTRACT_VERSION = "1"
_TARGET_CANDIDATE_FILES = frozenset({"manifest.json", "validation/target-portfolio-frame.json"})
_TARGET_MANIFEST_FIELDS = frozenset(
    {
        "candidate_bundle_contract_version",
        "package_kind",
        "candidate_id",
        "candidate_package_id",
        "candidate_package_revision",
        "target_portfolio_frame",
    }
)
_TARGET_FRAME_FIELDS = frozenset(
    {
        "schema_version",
        "portfolio_candidate_id",
        "portfolio_state",
        "universe_version_id",
        "as_of_time",
        "effective_from",
        "effective_until",
        "rows",
    }
)
_TARGET_FRAME_ROW_FIELDS = frozenset({"instrument_id", "target_weight", "confidence"})
_FORBIDDEN_TARGET_FRAME_KEYS = {
    "account",
    "account_id",
    "broker",
    "execution",
    "execution_retry",
    "fill",
    "fills",
    "heartbeat",
    "limit",
    "limit_price",
    "order",
    "order_id",
    "order_type",
    "orders",
    "position",
    "positions",
    "recovery",
    "side",
    "stop",
    "stop_price",
    "tif",
    "time_in_force",
    "venue",
}
_ISOLATED_ENVIRONMENT_KEYS = {
    "LANG",
    "LC_ALL",
    "PATH",
    "PYTHONHOME",
    "PYTHONPATH",
    "PYTHONUNBUFFERED",
    "PYTHONIOENCODING",
    "TEMP",
    "TMP",
    "TMPDIR",
    "TZ",
}
_FORBIDDEN_BUNDLE_PATH_PARTS = {
    "orders",
    "fills",
    "positions",
    "account",
    "accounts",
}
_PLUGIN_SANDBOX_CATALOG_PATH = "/workspace/catalog"
_PLUGIN_DOWNLOAD_TIMEOUT_SECONDS = 180
_PLUGIN_IMPORT_MIN_TIMEOUT_SECONDS = 600
_MANIFEST_SCAN_TIMEOUT_SECONDS = 900


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class CatalogValidationInput(StrictModel):
    catalog_uri: str


def _now() -> datetime:
    return datetime.now(UTC)


def _runtime_version() -> str:
    return importlib.metadata.version("nautilus_trader")


def _catalog_root() -> Path:
    root = Path(
        os.environ.get(
            "QUAZONAI_NAUTILUS_CATALOG_ROOT",
            "/var/lib/quazonai-nautilus/catalogs",
        )
    )
    root.mkdir(parents=True, exist_ok=True)
    return root


def _plugin_staging_root() -> Path:
    _PLUGIN_STAGING_ROOT.mkdir(parents=True, exist_ok=True)
    return _PLUGIN_STAGING_ROOT


def _catalog_path(catalog_uri: str) -> Path:
    if not catalog_uri.startswith(_CATALOG_PREFIX):
        raise HTTPException(status_code=422, detail="catalog_uri must use catalog://")
    key = catalog_uri.removeprefix(_CATALOG_PREFIX)
    if not key or len(key) > 120 or not all(
        character.isalnum() or character in "._-" for character in key
    ):
        raise HTTPException(status_code=422, detail="catalog key is invalid")
    safe_key = os.path.basename(key)
    if safe_key != key:
        raise HTTPException(status_code=422, detail="catalog key is invalid")
    root = _catalog_root().resolve()
    path = (root / safe_key).resolve()
    if not path.is_relative_to(root):
        raise HTTPException(status_code=422, detail="catalog path escapes the runtime root")
    return path


def _metadata_path(catalog_path: Path) -> Path:
    return catalog_path / "quazonai-catalog.json"


def _catalog_lock(catalog_uri: str) -> threading.Lock:
    """Return an identity-scoped lock without serializing unrelated catalogs."""

    with _CATALOG_LOCKS_GUARD:
        return _CATALOG_LOCKS.setdefault(catalog_uri, threading.Lock())


def _authorize(
    authorization: str | None,
    contract_header: str | None,
) -> None:
    expected = os.environ.get("QUAZONAI_NAUTILUS_RUNTIME_TOKEN", "").strip()
    if expected:
        if not authorization or not authorization.startswith("Bearer "):
            raise HTTPException(status_code=401, detail="runtime authentication is required")
        supplied = authorization.removeprefix("Bearer ").strip()
        if not secrets.compare_digest(supplied.encode(), expected.encode()):
            raise HTTPException(status_code=403, detail="runtime authentication failed")
    if contract_header is not None and contract_header != CONTRACT_VERSION:
        raise HTTPException(status_code=409, detail="quant runtime contract mismatch")


def _read_catalog_descriptor(catalog_uri: str) -> CatalogDescriptor:
    path = _catalog_path(catalog_uri)
    metadata_path = _metadata_path(path)
    if not metadata_path.is_file():
        raise HTTPException(status_code=404, detail="catalog does not exist")
    try:
        payload = json.loads(metadata_path.read_text(encoding="utf-8"))
        return CatalogDescriptor.model_validate(payload)
    except (OSError, ValueError) as exc:
        raise HTTPException(status_code=500, detail="catalog metadata is invalid") from exc


def _approved_plugin_pythons() -> dict[str, Path]:
    """Index immutable bundle interpreters discovered from the server-owned root."""

    bundle_root = _PLUGIN_ROOT / "bundles"
    if not bundle_root.is_dir():
        return {}
    python_relative_path = "Scripts/python.exe" if os.name == "nt" else "bin/python"
    approved: dict[str, Path] = {}
    for bundle_dir in bundle_root.iterdir():
        if bundle_dir.is_symlink() or not bundle_dir.is_dir():
            continue
        python_path = bundle_dir / python_relative_path
        if python_path.is_file():
            approved[f"bundles/{bundle_dir.name}"] = python_path
    return approved


def _plugin_python(bundle_path: str) -> Path:
    """Resolve only a prewarmed, immutable plugin bundle under the plugin root."""

    python_path = _approved_plugin_pythons().get(bundle_path)
    if python_path is None:
        raise HTTPException(status_code=422, detail="plugin bundle path is invalid")
    return python_path


def _plugin_child_command(bundle_path: str, workspace: Path) -> list[str]:
    """Run a plugin in a mount namespace containing only its bundle and staging area."""

    bubblewrap = shutil.which("bwrap")
    if bubblewrap is None:
        raise HTTPException(
            status_code=503,
            detail="plugin imports require the bubblewrap sandbox in the runtime image",
        )
    if not Path(_PLUGIN_PRLIMIT_PATH).is_file():
        raise HTTPException(
            status_code=503,
            detail="plugin imports require the prlimit sandbox helper in the runtime image",
        )
    python_path = _plugin_python(bundle_path)
    bundle_root = python_path.parent.parent
    return [
        _PLUGIN_PRLIMIT_PATH,
        f"--as={_PLUGIN_CHILD_MEMORY_LIMIT_BYTES}:{_PLUGIN_CHILD_MEMORY_LIMIT_BYTES}",
        f"--fsize={_PLUGIN_STAGED_OUTPUT_LIMIT_BYTES}:{_PLUGIN_STAGED_OUTPUT_LIMIT_BYTES}",
        "--",
        bubblewrap,
        "--die-with-parent",
        "--new-session",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-ipc",
        "--unshare-cgroup-try",
        "--share-net",
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/bin",
        "/bin",
        "--ro-bind",
        "/lib",
        "/lib",
        "--ro-bind",
        "/lib64",
        "/lib64",
        "--ro-bind",
        "/etc",
        "/etc",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/workspace",
        "--bind",
        str(workspace),
        _PLUGIN_SANDBOX_CATALOG_PATH,
        "--dir",
        "/opt",
        "--dir",
        "/opt/quazonai",
        "--ro-bind",
        str(bundle_root),
        "/opt/quazonai/plugin-bundle",
        "--chdir",
        _PLUGIN_SANDBOX_CATALOG_PATH,
        "/opt/quazonai/plugin-bundle/bin/python",
        "-m",
        "plugins.runtime_call",
    ]


def _plugin_import_timeout(source_shards: list[dict[str, Any]] | None) -> int:
    shard_count = len(source_shards) if source_shards else 1
    return max(
        _PLUGIN_IMPORT_MIN_TIMEOUT_SECONDS,
        _PLUGIN_DOWNLOAD_TIMEOUT_SECONDS * shard_count + 60,
    )


class _PluginChildLimit(RuntimeError):
    pass


def _staged_file_usage(root: Path) -> tuple[int, list[Path]]:
    total_bytes = 0
    files: list[Path] = []
    try:
        for directory, directories, filenames in os.walk(root, followlinks=False):
            for name in [*directories, *filenames]:
                path = Path(directory) / name
                if path.is_symlink():
                    raise _PluginChildLimit(
                        "plugin catalog output must not contain symbolic links"
                    )
            for name in filenames:
                path = Path(directory) / name
                file_stat = path.stat(follow_symlinks=False)
                if not stat.S_ISREG(file_stat.st_mode):
                    raise _PluginChildLimit(
                        "plugin catalog output must contain regular files only"
                    )
                files.append(path)
                total_bytes += file_stat.st_size
    except OSError as exc:
        raise _PluginChildLimit("plugin catalog output could not be inspected") from exc
    return total_bytes, files


def _validate_staged_output(root: Path, descriptor: CatalogDescriptor) -> None:
    try:
        total_bytes, files = _staged_file_usage(root)
    except _PluginChildLimit as exc:
        raise HTTPException(status_code=502, detail=str(exc)) from exc
    if total_bytes > _PLUGIN_STAGED_OUTPUT_LIMIT_BYTES:
        raise HTTPException(status_code=502, detail="plugin catalog output exceeds its size limit")
    if len(files) > _PLUGIN_STAGED_FILE_LIMIT:
        raise HTTPException(status_code=502, detail="plugin catalog output contains too many files")
    data_directories = {"Bar": "bar", "QuoteTick": "quote_tick", "TradeTick": "trade_tick"}
    expected_data_directory = data_directories.get(descriptor.nautilus_data_type)
    if expected_data_directory is None:
        raise HTTPException(status_code=502, detail="plugin returned an unsupported catalog data type")
    for path in files:
        relative = path.relative_to(root)
        if (
            len(relative.parts) < 3
            or relative.parts[0] != "data"
            or relative.parts[1] != expected_data_directory
            or path.suffix != ".parquet"
        ):
            raise HTTPException(
                status_code=502,
                detail="plugin catalog output contains an unexpected file",
            )


def _run_plugin_child(
    command: list[str],
    request: dict[str, Any],
    *,
    cwd: Path,
    env: dict[str, str],
    timeout_seconds: int,
    staged_root: Path | None = None,
) -> tuple[int, str, str]:
    """Run a plugin child with bounded protocol output and optional staging quota."""

    process = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=cwd,
        env=env,
    )
    selector = selectors.DefaultSelector()
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    total_output = 0
    deadline = time.monotonic() + timeout_seconds
    request_bytes = json.dumps(request).encode("utf-8")
    input_offset = 0
    try:
        if process.stdout is None or process.stderr is None:
            raise _PluginChildLimit("plugin child protocol pipes are unavailable")
        if process.stdin is not None:
            os.set_blocking(process.stdin.fileno(), False)
            if request_bytes:
                selector.register(process.stdin, selectors.EVENT_WRITE, "stdin")
            else:
                process.stdin.close()
        selector.register(process.stdout, selectors.EVENT_READ, "stdout")
        selector.register(process.stderr, selectors.EVENT_READ, "stderr")
        while selector.get_map():
            if staged_root is not None:
                staged_bytes, staged_files = _staged_file_usage(staged_root)
                if staged_bytes > _PLUGIN_STAGED_OUTPUT_LIMIT_BYTES:
                    raise _PluginChildLimit("plugin catalog output exceeds its size limit")
                if len(staged_files) > _PLUGIN_STAGED_FILE_LIMIT:
                    raise _PluginChildLimit("plugin catalog output contains too many files")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise subprocess.TimeoutExpired(command, timeout_seconds)
            for key, _ in selector.select(min(0.25, remaining)):
                if key.data == "stdin":
                    try:
                        written = os.write(key.fd, request_bytes[input_offset:])
                    except BlockingIOError:
                        continue
                    except BrokenPipeError:
                        file_object = key.fileobj
                        selector.unregister(file_object)
                        close = getattr(file_object, "close", None)
                        if callable(close):
                            close()
                        continue
                    input_offset += written
                    if input_offset == len(request_bytes):
                        file_object = key.fileobj
                        selector.unregister(file_object)
                        close = getattr(file_object, "close", None)
                        if callable(close):
                            close()
                    continue
                chunk = os.read(key.fd, 64 * 1024)
                if not chunk:
                    file_object = key.fileobj
                    selector.unregister(file_object)
                    close = getattr(file_object, "close", None)
                    if callable(close):
                        close()
                    continue
                buffer = buffers[key.data]
                if len(buffer) + len(chunk) > _PLUGIN_CHILD_OUTPUT_LIMIT_BYTES:
                    raise _PluginChildLimit("plugin child output exceeds its size limit")
                total_output += len(chunk)
                if total_output > _PLUGIN_CHILD_TOTAL_OUTPUT_LIMIT_BYTES:
                    raise _PluginChildLimit("plugin child output exceeds its total size limit")
                buffer.extend(chunk)
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise subprocess.TimeoutExpired(command, timeout_seconds)
        returncode = process.wait(timeout=remaining)
        return (
            returncode,
            bytes(buffers["stdout"]).decode("utf-8", errors="replace"),
            bytes(buffers["stderr"]).decode("utf-8", errors="replace"),
        )
    finally:
        selector.close()
        if process.poll() is None:
            process.kill()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        for stream in (process.stdin, process.stdout, process.stderr):
            if stream is not None:
                stream.close()


def _validate_staged_catalog(
    staging_path: Path,
    descriptor: CatalogDescriptor,
    *,
    availability_coverage: list[tuple[datetime, datetime]] | None = None,
) -> None:
    """Check the plugin's actual Parquet output before it reaches an immutable catalog."""

    try:
        from nautilus_trader.model.data import Bar, QuoteTick, TradeTick
        from nautilus_trader.persistence.catalog import ParquetDataCatalog

        data_classes = {"Bar": Bar, "QuoteTick": QuoteTick, "TradeTick": TradeTick}
        data_class = data_classes.get(descriptor.nautilus_data_type)
        if data_class is None:
            raise ValueError("plugin returned an unsupported catalog data type")
        catalog = ParquetDataCatalog(staging_path)
        instruments = {instrument.id.value for instrument in catalog.instruments()}
        expected_instruments = set(descriptor.instrument_scope)
        if not expected_instruments or instruments != expected_instruments:
            raise ValueError("plugin catalog instruments do not match its descriptor")
        data_files = catalog._query_files(
            data_class,
            sorted(expected_instruments),
            None,
            None,
        )
        if not data_files:
            raise ValueError("plugin catalog contains no data files")
        if descriptor.event_start is None or descriptor.event_end is None:
            raise ValueError("plugin catalog descriptor is missing event bounds")
        if descriptor.available_start is None or descriptor.available_end is None:
            raise ValueError("plugin catalog descriptor is missing availability bounds")
        expected_event = (
            int(descriptor.event_start.timestamp() * 1_000_000_000),
            int(descriptor.event_end.timestamp() * 1_000_000_000),
        )
        expected_available = (
            int(descriptor.available_start.timestamp() * 1_000_000_000),
            int(descriptor.available_end.timestamp() * 1_000_000_000),
        )
        coverage_ns = [
            (
                int(start.timestamp() * 1_000_000_000),
                int(end.timestamp() * 1_000_000_000),
            )
            for start, end in availability_coverage or []
        ]
        import pyarrow.dataset as pa_dataset

        dataset = pa_dataset.dataset(data_files, filesystem=catalog.fs)
        observed_rows = 0
        event_min: int | None = None
        event_max: int | None = None
        available_min: int | None = None
        available_max: int | None = None
        scanner = dataset.scanner(
            columns=["ts_event", "ts_init"],
            batch_size=_PLUGIN_VALIDATION_BATCH_ROWS,
            batch_readahead=1,
            fragment_readahead=1,
            use_threads=False,
            cache_metadata=False,
        )
        for batch in scanner.to_batches():
            if (
                batch.num_rows > _PLUGIN_VALIDATION_BATCH_ROWS
                or batch.nbytes > _PLUGIN_VALIDATION_BATCH_BYTES
            ):
                raise ValueError("plugin catalog validation batch exceeds its size limit")
            for row in batch.to_pylist():
                event_timestamp = int(row["ts_event"])
                available_timestamp = int(row["ts_init"])
                observed_rows += 1
                event_min = event_timestamp if event_min is None else min(event_min, event_timestamp)
                event_max = event_timestamp if event_max is None else max(event_max, event_timestamp)
                available_min = (
                    available_timestamp
                    if available_min is None
                    else min(available_min, available_timestamp)
                )
                available_max = (
                    available_timestamp
                    if available_max is None
                    else max(available_max, available_timestamp)
                )
                if coverage_ns and not any(
                    start <= available_timestamp < end for start, end in coverage_ns
                ):
                    raise ValueError("plugin catalog availability is outside selected archive shards")
        if observed_rows != descriptor.row_count or event_min is None or available_min is None:
            raise ValueError("plugin catalog row count does not match its descriptor")
        if (event_min, event_max) != expected_event:
            raise ValueError("plugin catalog event bounds do not match its descriptor")
        if (available_min, available_max) != expected_available:
            raise ValueError("plugin catalog availability bounds do not match its descriptor")
    except Exception as exc:
        raise HTTPException(
            status_code=502,
            detail="plugin catalog contents do not match its descriptor",
        ) from exc


def _write_plugin_catalog(spec: CatalogIngestSpec, staging_path: Path) -> CatalogDescriptor:
    """Invoke one validated historical-import plugin in a short-lived child."""

    if not spec.plugin_id or not spec.plugin_version or not spec.plugin_bundle_path:
        raise HTTPException(
            status_code=422,
            detail="plugin Catalog ingest requires an id, version and runtime bundle",
        )
    if spec.sealed:
        raise HTTPException(
            status_code=422,
            detail=(
                "sealed Catalog ingest cannot execute a plugin; provision the sealed catalog "
                "through a trusted importer before evaluation"
            ),
        )
    source_config = spec.source_spec.get("config")
    if not isinstance(source_config, dict):
        raise HTTPException(status_code=422, detail="plugin source_spec.config must be an object")
    availability_coverage = [
        (shard.coverage_start, shard.coverage_end)
        for shard in (
            [ArchiveShardDescriptor.model_validate(item) for item in spec.source_shards]
            if spec.source_shards is not None
            else []
        )
    ]
    # Keep potentially multi-gigabyte archive staging in a per-import quota-backed
    # volume. The sandbox still sees only this directory and the bundle.
    with _PLUGIN_CHILD_LOCK, tempfile.TemporaryDirectory(
        prefix="quazonai-plugin-catalog-",
        dir=_plugin_staging_root(),
    ) as workspace:
        request = {
            "plugin_id": spec.plugin_id,
            "plugin_version": spec.plugin_version,
            "action": "import_catalog",
            "public_config": source_config,
            "secret_config": {},
            "source_url": source_config.get("archive_url"),
            "source_shards": (
                [
                    ArchiveShardDescriptor.model_validate(shard).model_dump(mode="json")
                    for shard in spec.source_shards
                ]
                if spec.source_shards is not None
                else None
            ),
            "catalog_path": _PLUGIN_SANDBOX_CATALOG_PATH,
            "instrument_id": source_config.get("instrument"),
            "metadata": {
                "catalog_uri": f"{_CATALOG_PREFIX}{spec.catalog_name}",
                "provider": spec.provider,
                "source_license": spec.source_license,
                "source_spec": spec.source_spec,
                "sealed": spec.sealed,
            },
        }
        child_environment = {
            name: value for name, value in os.environ.items() if name in _ISOLATED_ENVIRONMENT_KEYS
        }
        child_environment.update(
            {
                "PYTHONNOUSERSITE": "1",
                "PYTHONDONTWRITEBYTECODE": "1",
            }
        )
        try:
            returncode, stdout, stderr = _run_plugin_child(
                _plugin_child_command(spec.plugin_bundle_path, Path(workspace)),
                request,
                cwd=Path(workspace),
                env=child_environment,
                timeout_seconds=_plugin_import_timeout(spec.source_shards),
                staged_root=Path(workspace),
            )
        except subprocess.TimeoutExpired as exc:
            raise HTTPException(status_code=504, detail="plugin catalog import timed out") from exc
        except _PluginChildLimit as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        if returncode != 0:
            detail = (stderr or stdout or "plugin catalog import failed")[-2000:]
            raise HTTPException(status_code=422, detail=detail)
        try:
            response = json.loads(stdout)
            summary = response["summary"]
            descriptor = CatalogDescriptor.model_validate(summary["descriptor"])
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
            raise HTTPException(status_code=502, detail="plugin returned an invalid catalog descriptor") from exc
        expected_uri = f"{_CATALOG_PREFIX}{spec.catalog_name}"
        if (
            descriptor.catalog_uri != expected_uri
            or descriptor.provider != spec.provider
            or descriptor.source_license != spec.source_license
            or descriptor.source_spec != spec.source_spec
            or descriptor.sealed != spec.sealed
        ):
            raise HTTPException(status_code=502, detail="plugin catalog descriptor does not match request")
        _validate_staged_output(Path(workspace), descriptor)
        _validate_staged_catalog(
            Path(workspace),
            descriptor,
            availability_coverage=availability_coverage,
        )
        shutil.copytree(workspace, staging_path, dirs_exist_ok=True)
    _validate_staged_catalog(
        staging_path,
        descriptor,
        availability_coverage=availability_coverage,
    )
    return descriptor


def _inspect_plugin_manifest(spec: ArchiveManifestSpec) -> ArchiveManifestDescriptor:
    """Ask a short-lived plugin child to enumerate remote archive shards."""

    if not spec.plugin_id or not spec.plugin_version or not spec.plugin_bundle_path:
        raise HTTPException(
            status_code=422,
            detail="plugin manifest inspection requires an id, version and runtime bundle",
        )
    source_config = spec.source_spec.get("config")
    if not isinstance(source_config, dict):
        raise HTTPException(status_code=422, detail="plugin source_spec.config must be an object")
    with _PLUGIN_CHILD_LOCK, tempfile.TemporaryDirectory(
        prefix="quazonai-plugin-manifest-"
    ) as workspace:
        request = {
            "plugin_id": spec.plugin_id,
            "plugin_version": spec.plugin_version,
            "action": "scan_manifest",
            "public_config": source_config,
            "secret_config": {},
            "metadata": {
                "manifest_uri": f"manifest://{spec.manifest_name}",
                "provider": spec.provider,
                "source_license": spec.source_license,
                "source_spec": spec.source_spec,
            },
        }
        child_environment = {
            name: value for name, value in os.environ.items() if name in _ISOLATED_ENVIRONMENT_KEYS
        }
        child_environment.update(
            {
                "PYTHONNOUSERSITE": "1",
                "PYTHONDONTWRITEBYTECODE": "1",
            }
        )
        try:
            returncode, stdout, stderr = _run_plugin_child(
                _plugin_child_command(spec.plugin_bundle_path, Path(workspace)),
                request,
                cwd=Path(workspace),
                env=child_environment,
                timeout_seconds=_MANIFEST_SCAN_TIMEOUT_SECONDS,
            )
        except subprocess.TimeoutExpired as exc:
            raise HTTPException(status_code=504, detail="plugin archive manifest scan timed out") from exc
        except _PluginChildLimit as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        if returncode != 0:
            detail = (stderr or stdout or "plugin archive manifest scan failed")[-2000:]
            raise HTTPException(status_code=422, detail=detail)
        try:
            response = json.loads(stdout)
            descriptor = ArchiveManifestDescriptor.model_validate(response["summary"])
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
            raise HTTPException(status_code=502, detail="plugin returned an invalid archive manifest") from exc
    expected_uri = f"manifest://{spec.manifest_name}"
    if (
        descriptor.manifest_uri != expected_uri
        or descriptor.provider != spec.provider
        or descriptor.source_license != spec.source_license
        or descriptor.source_spec != spec.source_spec
    ):
        raise HTTPException(status_code=502, detail="plugin archive manifest does not match request")
    return descriptor


def _write_catalog(spec: CatalogIngestSpec) -> CatalogDescriptor:
    catalog_uri = f"catalog://{spec.catalog_name}"
    path = _catalog_path(catalog_uri)
    with _catalog_lock(catalog_uri):
        if path.exists():
            if not path.is_dir():
                raise HTTPException(status_code=409, detail="catalog identity is already occupied")
            try:
                existing = _read_catalog_descriptor(catalog_uri)
            except HTTPException as exc:
                if exc.status_code == 404:
                    raise HTTPException(
                        status_code=409,
                        detail="catalog exists without an immutable ingestion descriptor",
                    ) from exc
                raise
            if (
                existing.provider != spec.provider
                or existing.source_license != spec.source_license
                or existing.source_spec != spec.source_spec
                or existing.sealed != spec.sealed
            ):
                raise HTTPException(
                    status_code=409,
                    detail="catalog identity is already bound to a different dataset revision",
                )
            return existing

        staging_path = Path(tempfile.mkdtemp(prefix=".catalog-staging-", dir=_catalog_root()))
        try:
            import numpy as np
            import pandas as pd

            from nautilus_trader.persistence.catalog import ParquetDataCatalog
            from nautilus_trader.persistence.wranglers import QuoteTickDataWrangler
            from nautilus_trader.test_kit.providers import TestInstrumentProvider

            source_kind = str(spec.source_spec.get("kind", ""))
            if source_kind == "plugin":
                descriptor = _write_plugin_catalog(spec, staging_path)
                _metadata_path(staging_path).write_text(
                    descriptor.model_dump_json(indent=2),
                    encoding="utf-8",
                )
                if path.exists():
                    raise HTTPException(
                        status_code=409,
                        detail="catalog identity is already occupied",
                    )
                os.replace(staging_path, path)
                return descriptor
            if source_kind != "synthetic_fx_quotes":
                raise HTTPException(
                    status_code=422,
                    detail=(
                        "The reference service accepts source_spec.kind=plugin or "
                        "synthetic_fx_quotes."
                    ),
                )
            instrument_name = str(spec.source_spec.get("instrument", "EUR/USD"))
            if instrument_name != "EUR/USD":
                raise HTTPException(status_code=422, detail="reference service supports EUR/USD only")
            rows = int(spec.source_spec.get("rows", 3000))
            seed = int(spec.source_spec.get("seed", 42))
            if rows < 500 or rows > 100_000:
                raise HTTPException(status_code=422, detail="rows must be between 500 and 100000")

            instrument = TestInstrumentProvider.default_fx_ccy(instrument_name)
            rng = np.random.default_rng(seed)
            mid = 1.10 + np.cumsum(rng.normal(0, 0.00015, rows))
            spread = np.maximum(0.00002, np.abs(rng.normal(0.00008, 0.00002, rows)))
            timestamps = pd.date_range("2024-01-01", periods=rows, freq="1min", tz="UTC")
            frame = pd.DataFrame(
                {
                    "bid_price": mid - spread / 2,
                    "ask_price": mid + spread / 2,
                },
                index=timestamps,
            )
            ticks = QuoteTickDataWrangler(instrument).process(frame)
            catalog = ParquetDataCatalog(staging_path)
            catalog.write_data([instrument])
            catalog.write_data(ticks)

            first = timestamps[0].to_pydatetime()
            last = timestamps[-1].to_pydatetime()
            descriptor = CatalogDescriptor(
                catalog_uri=catalog_uri,
                provider=spec.provider,
                source_license=spec.source_license,
                source_spec=spec.source_spec,
                nautilus_data_type="QuoteTick",
                instrument_scope=[instrument.id.value],
                event_start=first,
                event_end=last,
                available_start=first,
                available_end=last,
                row_count=len(ticks),
                schema_revision="nautilus-quote-tick-v1",
                quality_result={
                    "valid": True,
                    "sorted": True,
                    "unique_timestamps": True,
                    "non_crossed_quotes": True,
                },
                point_in_time_result={
                    "valid": True,
                    "available_time_preserved": True,
                },
                sealed=spec.sealed,
            )
            _metadata_path(staging_path).write_text(
                descriptor.model_dump_json(indent=2),
                encoding="utf-8",
            )
            if path.exists():
                raise HTTPException(
                    status_code=409,
                    detail="catalog identity is already occupied",
                )
            os.replace(staging_path, path)
            return descriptor
        except BaseException:
            shutil.rmtree(staging_path, ignore_errors=True)
            raise


def _bundle_contains_secret(value: object) -> bool:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold()
            if normalized in _FORBIDDEN_BUNDLE_KEYS or normalized.endswith("_secret"):
                return True
            if _bundle_contains_secret(item):
                return True
    elif isinstance(value, list):
        return any(_bundle_contains_secret(item) for item in value)
    return False


def _bundle_contains_execution(value: object) -> bool:
    if isinstance(value, dict):
        return any(
            str(key).casefold() in _FORBIDDEN_TARGET_FRAME_KEYS or _bundle_contains_execution(item)
            for key, item in value.items()
        )
    if isinstance(value, list):
        return any(_bundle_contains_execution(item) for item in value)
    return False


def _target_frame_timestamp(value: object) -> datetime | None:
    if not isinstance(value, str):
        return None
    try:
        result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    if result.tzinfo is None or result.utcoffset() is None:
        return None
    return result.astimezone(UTC)


def _verify_bundle(data: bytes | BinaryIO) -> dict[str, Any]:
    try:
        source = BytesIO(data) if isinstance(data, bytes) else data
        with zipfile.ZipFile(source) as bundle:
            names = set(bundle.namelist())
            missing = sorted(_TARGET_CANDIDATE_FILES - names)
            if missing:
                return {"valid": False, "errors": [f"missing files: {', '.join(missing)}"]}
            unexpected = sorted(names - _TARGET_CANDIDATE_FILES)
            if unexpected:
                return {
                    "valid": False,
                    "errors": [f"unexpected files: {', '.join(unexpected)}"],
                }
            manifest = json.loads(bundle.read("manifest.json"))
            target_frame = json.loads(bundle.read("validation/target-portfolio-frame.json"))
            if not isinstance(manifest, dict) or not isinstance(target_frame, dict):
                return {"valid": False, "errors": ["target Candidate Package must contain objects"]}
            if _bundle_contains_secret(manifest) or _bundle_contains_secret(target_frame):
                return {"valid": False, "errors": ["bundle contains a secret-bearing field"]}
            if _bundle_contains_execution(manifest) or _bundle_contains_execution(target_frame):
                return {"valid": False, "errors": ["bundle contains an execution field"]}
            if set(manifest) != _TARGET_MANIFEST_FIELDS:
                return {"valid": False, "errors": ["manifest contains unsupported fields"]}
            if (
                manifest.get("candidate_bundle_contract_version")
                != _CANDIDATE_BUNDLE_CONTRACT_VERSION
            ):
                return {
                    "valid": False,
                    "errors": ["unsupported Candidate Package contract version"],
                }
            if manifest.get("package_kind") != "TARGET_PORTFOLIO_FRAME":
                return {"valid": False, "errors": ["bundle package kind is invalid"]}
            try:
                package_id = manifest["candidate_package_id"]
                if not isinstance(package_id, str) or not package_id.strip():
                    raise ValueError
                package_revision = manifest["candidate_package_revision"]
                if (
                    isinstance(package_revision, bool)
                    or not isinstance(package_revision, int)
                    or package_revision < 1
                ):
                    raise ValueError
            except (KeyError, ValueError):
                return {"valid": False, "errors": ["manifest package identity is invalid"]}
            if manifest.get("target_portfolio_frame") != "validation/target-portfolio-frame.json":
                return {"valid": False, "errors": ["manifest target frame path is invalid"]}
            rows = target_frame.get("rows")
            required_frame_fields = {
                "as_of_time",
                "effective_from",
                "effective_until",
                "portfolio_candidate_id",
                "portfolio_state",
                "universe_version_id",
            }
            if (
                set(target_frame) != _TARGET_FRAME_FIELDS
                or target_frame.get("schema_version") != "1"
                or not required_frame_fields.issubset(target_frame)
                or not isinstance(rows, list)
                or not rows
            ):
                return {"valid": False, "errors": ["target portfolio frame is invalid"]}
            if manifest.get("candidate_id") != target_frame.get("portfolio_candidate_id"):
                return {
                    "valid": False,
                    "errors": ["target frame candidate does not match manifest"],
                }
            as_of_time = _target_frame_timestamp(target_frame["as_of_time"])
            effective_from = _target_frame_timestamp(target_frame["effective_from"])
            effective_until = target_frame["effective_until"]
            effective_until_time = (
                None if effective_until is None else _target_frame_timestamp(effective_until)
            )
            if (
                as_of_time is None
                or effective_from is None
                or effective_from < as_of_time
                or (effective_until_time is not None and effective_until_time < effective_from)
            ):
                return {"valid": False, "errors": ["target portfolio frame timestamps are invalid"]}
            if not all(
                isinstance(target_frame.get(field), str) and target_frame[field].strip()
                for field in ("portfolio_candidate_id", "portfolio_state", "universe_version_id")
            ):
                return {
                    "valid": False,
                    "errors": ["target portfolio frame identifiers are invalid"],
                }
            total_weight = 0.0
            target_ids: set[str] = set()
            for row in rows:
                try:
                    weight = (
                        float(row["target_weight"])
                        if isinstance(row, dict) and set(row) == _TARGET_FRAME_ROW_FIELDS
                        else float("nan")
                    )
                    confidence = (
                        float(row["confidence"])
                        if isinstance(row, dict) and set(row) == _TARGET_FRAME_ROW_FIELDS
                        else float("nan")
                    )
                except KeyError, TypeError, ValueError:
                    weight = float("nan")
                    confidence = float("nan")
                if (
                    not isinstance(row, dict)
                    or not isinstance(row.get("instrument_id"), str)
                    or not row["instrument_id"].strip()
                    or not math.isfinite(weight)
                    or not math.isfinite(confidence)
                    or not 0.0 <= weight <= 1.0
                    or not 0.0 <= confidence <= 1.0
                ):
                    return {
                        "valid": False,
                        "errors": ["target portfolio frame contains invalid rows"],
                    }
                if row["instrument_id"] in target_ids:
                    return {
                        "valid": False,
                        "errors": ["target portfolio frame has duplicate instruments"],
                    }
                target_ids.add(row["instrument_id"])
                total_weight += weight
            if not math.isclose(total_weight, 1.0, rel_tol=0.0, abs_tol=1e-9):
                return {
                    "valid": False,
                    "errors": ["target portfolio frame weights must sum to one"],
                }
            return {
                "valid": True,
                "checked_files": len(names),
                "target_frame_rows": len(rows),
            }
    except (KeyError, OSError, ValueError, zipfile.BadZipFile) as exc:
        return {"valid": False, "errors": [f"invalid bundle: {type(exc).__name__}"]}


def create_app() -> FastAPI:
    app = FastAPI(
        title="QuaZonai Reference Remote Nautilus Runtime",
        version=PINNED_NAUTILUS_VERSION,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )

    def authorize(
        authorization: str | None = Header(default=None),
        contract: str | None = Header(default=None, alias="X-QuaZonai-Quant-Contract"),
    ) -> None:
        _authorize(authorization, contract)

    @app.get("/v1/capabilities", response_model=RuntimeCapabilities)
    def capabilities(_: None = Header(default=None, include_in_schema=False)) -> RuntimeCapabilities:
        # FastAPI dependency injection is deliberately avoided to keep this reference
        # runtime independent from QuaZonai's API/auth dependency graph.
        return RuntimeCapabilities(
            runtime_name="NautilusTrader",
            nautilus_version=_runtime_version(),
            contract_version=CONTRACT_VERSION,
            catalog_type="ParquetDataCatalog",
            supported_modes=[],
            candidate_contract_version="1",
        )

    @app.middleware("http")
    async def authentication(request: Any, call_next: Any) -> Any:
        if request.url.path.startswith("/v1/"):
            _authorize(
                request.headers.get("authorization"),
                request.headers.get("x-quazonai-quant-contract"),
            )
        return await call_next(request)

    @app.post("/v1/catalogs/ingest", response_model=CatalogDescriptor)
    def ingest(spec: CatalogIngestSpec) -> CatalogDescriptor:
        return _write_catalog(spec)

    @app.post("/v1/catalogs/validate", response_model=CatalogDescriptor)
    def validate(payload: CatalogValidationInput) -> CatalogDescriptor:
        descriptor = _read_catalog_descriptor(payload.catalog_uri)
        from nautilus_trader.persistence.catalog import ParquetDataCatalog

        catalog = ParquetDataCatalog(_catalog_path(payload.catalog_uri))
        instruments = [instrument.id.value for instrument in catalog.instruments()]
        if not instruments:
            raise HTTPException(status_code=422, detail="catalog contains no instruments")
        return descriptor.model_copy(
            update={
                "instrument_scope": instruments,
                "quality_result": {**descriptor.quality_result, "valid": True},
            }
        )

    @app.post("/v1/archive-manifests/inspect", response_model=ArchiveManifestDescriptor)
    def inspect_archive_manifest(spec: ArchiveManifestSpec) -> ArchiveManifestDescriptor:
        return _inspect_plugin_manifest(spec)

    @app.post("/v1/candidates/verify")
    async def verify_candidate(bundle: UploadFile = File(...)) -> dict[str, Any]:
        maximum_size = 256 * 1024 * 1024
        total_size = 0
        with tempfile.SpooledTemporaryFile(max_size=8 * 1024 * 1024, mode="w+b") as staged:
            while chunk := await bundle.read(1024 * 1024):
                total_size += len(chunk)
                if total_size > maximum_size:
                    raise HTTPException(status_code=413, detail="Candidate Bundle exceeds 256 MiB")
                staged.write(chunk)
            staged.seek(0)
            result = _verify_bundle(cast(BinaryIO, staged))
        if result.get("valid") is not True:
            raise HTTPException(status_code=422, detail=result)
        return result

    return app


def main() -> int:
    import uvicorn

    host = os.environ.get("QUAZONAI_NAUTILUS_HOST", "0.0.0.0")
    port = int(os.environ.get("QUAZONAI_NAUTILUS_PORT", "9010"))
    uvicorn.run(create_app(), host=host, port=port, proxy_headers=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
