"""Canonical NautilusTrader catalog, backtest and bundle-conformance engine."""

from __future__ import annotations

import base64
import fcntl
import importlib
import importlib.abc
import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any
from uuid import UUID, uuid4

import pandas as pd
from nautilus_trader import __version__ as nautilus_version
from nautilus_trader.backtest.config import (
    BacktestDataConfig,
    BacktestEngineConfig,
    BacktestRunConfig,
    BacktestVenueConfig,
)
from nautilus_trader.backtest.node import BacktestNode
from nautilus_trader.config import ImportableStrategyConfig, StrategyConfig
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.persistence.catalog.parquet import ParquetDataCatalog
from nautilus_trader.persistence.wranglers import QuoteTickDataWrangler
from nautilus_trader.trading.strategy import Strategy

from quazonai_nautilus_gateway import PROTOCOL_VERSION, VALIDATED_NAUTILUS_VERSION
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    CandidateVerificationRequest,
    CatalogIngestRequest,
    CatalogValidationRequest,
    ExperimentMode,
    _validate_restricted_strategy_source,
)

CANDIDATE_BUNDLE_CONTRACT = "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE"
CANDIDATE_BUNDLE_CONTRACT_VERSION = "2"
QUOTE_TICK_SCHEMA_REVISION = "nautilus.quote_tick.v2"


class GatewayContractError(ValueError):
    pass


def _finite_metric(value: Any) -> float | None:
    try:
        result = float(value)
    except (TypeError, ValueError):
        return None
    if result != result or result in {float("inf"), float("-inf")}:
        return None
    return result


def _metric_by_fragment(value: Any, fragment: str) -> float | None:
    normalized_fragment = fragment.casefold()
    if isinstance(value, dict):
        for key, item in value.items():
            if normalized_fragment in str(key).casefold():
                metric = _finite_metric(item)
                if metric is not None:
                    return metric
            nested = _metric_by_fragment(item, fragment)
            if nested is not None:
                return nested
    elif isinstance(value, list):
        for item in value:
            nested = _metric_by_fragment(item, fragment)
            if nested is not None:
                return nested
    return None


def _pnl_totals(value: Any) -> dict[str, float]:
    if not isinstance(value, dict):
        return {}
    totals: dict[str, float] = {}
    for currency, stats in value.items():
        metric = _metric_by_fragment(stats, "pnl (total)")
        if metric is None and not isinstance(stats, dict):
            metric = _finite_metric(stats)
        if metric is not None:
            totals[str(currency)] = metric
    if not totals:
        metric = _metric_by_fragment(value, "pnl (total)")
        if metric is not None:
            totals["BASE"] = metric
    return totals


def _sealed_performance_disclosure(raw: dict[str, Any]) -> dict[str, Any]:
    statistics = raw.get("statistics") if isinstance(raw.get("statistics"), dict) else {}
    returns = statistics.get("returns") if isinstance(statistics, dict) else {}
    general = statistics.get("general") if isinstance(statistics, dict) else {}
    pnl_totals = _pnl_totals(raw.get("pnl"))
    sharpe = _metric_by_fragment(returns, "sharpe ratio")
    max_drawdown = _metric_by_fragment(returns, "max drawdown")
    profit_factor = _metric_by_fragment(raw.get("pnl"), "profit factor")
    if profit_factor is None:
        profit_factor = _metric_by_fragment(general, "profit factor")

    trade_evidence = bool(raw.get("orders") and raw.get("fills") and raw.get("positions"))
    pnl_pass = bool(pnl_totals) and all(value > 0.0 for value in pnl_totals.values())
    sharpe_pass = sharpe is None or sharpe >= 0.0
    drawdown_pass = max_drawdown is None or max_drawdown >= -0.50
    profit_factor_pass = profit_factor is None or profit_factor >= 1.0
    passed = trade_evidence and pnl_pass and sharpe_pass and drawdown_pass and profit_factor_pass

    reason_codes: list[str] = []
    if not trade_evidence:
        reason_codes.append("TRANSACTION_EVIDENCE_MISSING")
    if not pnl_pass:
        reason_codes.append("TOTAL_PNL_POLICY_FAILED")
    if not sharpe_pass:
        reason_codes.append("SHARPE_POLICY_FAILED")
    if not drawdown_pass:
        reason_codes.append("DRAWDOWN_POLICY_FAILED")
    if not profit_factor_pass:
        reason_codes.append("PROFIT_FACTOR_POLICY_FAILED")
    if passed:
        reason_codes = ["SEALED_POLICY_PASSED"]

    return {
        "passed": passed,
        "quality_tier": "QUALIFIED" if passed else "REJECTED",
        "reason_codes": reason_codes,
        "policy_checks": {
            "transaction_evidence": trade_evidence,
            "positive_total_pnl": pnl_pass,
            "non_negative_sharpe_when_available": sharpe_pass,
            "max_drawdown_floor": drawdown_pass,
            "profit_factor_floor_when_available": profit_factor_pass,
        },
        "policy": "SEALED_LEVEL1_POLICY_V1",
    }


def _utc_now() -> datetime:
    return datetime.now(UTC)


def _jsonable(value: Any) -> Any:
    if value is None or isinstance(value, (bool, int, float, str)):
        return value
    if isinstance(value, Decimal):
        return str(value)
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, dict):
        return {str(key): _jsonable(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_jsonable(item) for item in value]
    if hasattr(value, "to_dict"):
        try:
            return _jsonable(value.to_dict())
        except TypeError:
            try:
                return _jsonable(type(value).to_dict(value))
            except (AttributeError, TypeError, ValueError):
                return str(value)
    if hasattr(value, "as_dict"):
        try:
            return _jsonable(value.as_dict())
        except (AttributeError, TypeError, ValueError):
            return str(value)
    return str(value)


def _attr(value: Any, *names: str) -> Any:
    for name in names:
        if hasattr(value, name):
            candidate = getattr(value, name)
            if callable(candidate):
                try:
                    candidate = candidate()
                except TypeError:
                    continue
            return candidate
    return None


def _source_module_name(path: str) -> tuple[str, bool]:
    normalized = path.replace("\\", "/")
    parts = normalized.split("/")
    if not parts or any(not part for part in parts) or ".." in parts:
        raise GatewayContractError("strategy source module path is invalid")
    leaf = parts[-1]
    is_package = leaf == "__init__.py"
    if is_package:
        module_parts = parts[:-1]
    elif leaf.endswith(".py"):
        module_parts = [*parts[:-1], leaf[:-3]]
    else:
        raise GatewayContractError("strategy source modules must be Python files")
    if not module_parts or any(not part.isidentifier() for part in module_parts):
        raise GatewayContractError("strategy source module path is invalid")
    return ".".join(module_parts), is_package


class _SourceBundleFinder(importlib.abc.MetaPathFinder, importlib.abc.SourceLoader):
    """Load a validated SOURCE_BUNDLE from memory without filesystem paths."""

    def __init__(self, source_files: dict[str, str]) -> None:
        parsed: list[tuple[str, bool, str]] = []
        source_modules: set[str] = set()
        for relative, source in source_files.items():
            module_name, is_package = _source_module_name(relative)
            if module_name in source_modules:
                raise GatewayContractError("strategy source module names must be unique")
            source_modules.add(module_name)
            parsed.append((module_name, is_package, source))

        self._sources = {
            module_name: source for module_name, _, source in parsed
        }
        self._packages = {
            module_name for module_name, is_package, _ in parsed if is_package
        }
        for module_name in source_modules:
            segments = module_name.split(".")
            for index in range(1, len(segments)):
                package_name = ".".join(segments[:index])
                self._packages.add(package_name)
                self._sources.setdefault(package_name, "")

        self._filenames = {
            module_name: f"<quazonai-source-bundle:{index:04d}>"
            for index, module_name in enumerate(sorted(self._sources))
        }
        self._modules_by_filename = {
            filename: module_name for module_name, filename in self._filenames.items()
        }
        self.loaded_modules: set[str] = set()

    @property
    def module_names(self) -> set[str]:
        return set(self._sources)

    def find_spec(
        self,
        fullname: str,
        path: Any = None,
        target: Any = None,
    ) -> Any:
        del path, target
        if fullname not in self._sources:
            return None
        return importlib.util.spec_from_loader(
            fullname,
            self,
            is_package=fullname in self._packages,
        )

    def get_filename(self, fullname: str) -> str:
        try:
            return self._filenames[fullname]
        except KeyError as exc:
            raise ImportError(fullname) from exc

    def get_data(self, path: str) -> bytes:
        try:
            module_name = self._modules_by_filename[path]
        except KeyError as exc:
            raise OSError(path) from exc
        return self._sources[module_name].encode("utf-8")

    def is_package(self, fullname: str) -> bool:
        return fullname in self._packages

    def get_code(self, fullname: str) -> Any:
        self.loaded_modules.add(fullname)
        return super().get_code(fullname)


@dataclass(frozen=True, slots=True)
class _CatalogRecord:
    catalog_key: str
    storage_id: UUID
    state: str


def _parse_time(value: str | datetime) -> datetime:
    parsed = value if isinstance(value, datetime) else datetime.fromisoformat(value)
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise GatewayContractError("catalog timestamps must be timezone-aware")
    return parsed.astimezone(UTC)


def _sanitized_child_environment() -> dict[str, str]:
    allowed = {
        "PATH",
        "HOME",
        "LANG",
        "LC_ALL",
        "TZ",
        "TMPDIR",
        "PYTHONUTF8",
        "PYTHONHOME",
        "VIRTUAL_ENV",
        "CONDA_PREFIX",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "SSL_CERT_FILE",
    }
    result = {key: value for key, value in os.environ.items() if key in allowed}
    result["QUAZONAI_NAUTILUS_ISOLATED_CHILD"] = "1"
    return result


def _candidate_strategy_wheel_path(candidate_id: UUID) -> str:
    return (
        "strategy/quazonai_candidate_strategy-"
        f"0.0.{candidate_id.int}-py3-none-any.whl"
    )


def _source_bundle_sandbox_command(
    *, operation: str, workspace: Path, data_root: Path
) -> list[str]:
    """Build a fail-closed Bubblewrap command for authored Python.

    Start from a read-only host root so the interpreter and its native Nautilus
    dependencies remain executable on normal Linux distributions and hosted CI.
    Then mask the Gateway data root and common host-secret locations, bind only
    the disposable operation workspace back at /sandbox, and unshare networking.
    """
    if sys.platform != "linux":
        raise GatewayContractError("SOURCE_BUNDLE execution requires Linux OS isolation")
    configured = os.getenv("NAUTILUS_GATEWAY_OS_SANDBOX", "bwrap").strip() or "bwrap"
    sandbox = shutil.which(configured)
    if sandbox is None:
        raise GatewayContractError("SOURCE_BUNDLE OS sandbox is unavailable")
    gateway_source = Path(__file__).resolve().parents[1]
    command = [
        sandbox,
        "--die-with-parent",
        "--new-session",
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-net",
        "--unshare-uts",
        "--ro-bind", "/", "/",
        "--bind", str(workspace), "/sandbox",
        "--ro-bind", str(gateway_source), "/gateway-src",
        "--proc", "/proc",
        "--dev", "/dev",
    ]
    masked: set[str] = set()
    for candidate in (Path("/tmp"), Path("/run"), Path("/root"), data_root.resolve()):
        resolved = str(candidate.resolve())
        if candidate.exists() and resolved not in {"/", str(workspace.resolve())} and resolved not in masked:
            command.extend(["--tmpfs", resolved])
            masked.add(resolved)
    home = Path.home().resolve()
    executable = Path(sys.executable).resolve()
    if home.exists() and home != Path("/") and not executable.is_relative_to(home):
        resolved_home = str(home)
        if resolved_home not in masked:
            command.extend(["--tmpfs", resolved_home])
    command.extend([
        "--chdir", "/sandbox",
        "--setenv", "QUAZONAI_NAUTILUS_ISOLATED_CHILD", "1",
        "--setenv", "PYTHONDONTWRITEBYTECODE", "1",
        sys.executable, "-I", "-c",
        (
            "import sys; sys.path.insert(0, '/gateway-src'); "
            "from quazonai_nautilus_gateway.isolated_runner import main; main()"
        ),
        operation, "/sandbox/runtime", "/sandbox/input.json",
    ])
    return command


class NautilusGatewayEngine:
    def __init__(self, data_root: Path) -> None:
        if nautilus_version != VALIDATED_NAUTILUS_VERSION:
            raise RuntimeError(
                f"validated Nautilus version is {VALIDATED_NAUTILUS_VERSION}, got {nautilus_version}"
            )
        self._data_root = data_root.resolve()
        self._gateway_instance_path = self._data_root / ".gateway-instance-id"
        self._catalog_root = self._data_root / "catalogs"
        self._catalog_storage_root = self._catalog_root / "data"
        self._catalog_registry_path = self._catalog_root / "registry.json"
        self._catalog_registry_lock_path = self._catalog_root / ".registry.lock"
        self._artifact_root = self._data_root / "artifacts"
        self._run_root = self._data_root / "run-receipts"
        self._run_receipts_path = self._run_root / "receipts.json"
        self._run_lock_path = self._run_root / ".receipts.lock"
        self._catalog_storage_root.mkdir(parents=True, exist_ok=True)
        self._artifact_root.mkdir(parents=True, exist_ok=True)
        self._run_root.mkdir(parents=True, exist_ok=True)
        self._gateway_instance_id = self._load_or_create_gateway_instance_id()

    def _load_or_create_gateway_instance_id(self) -> UUID:
        path = self._gateway_instance_path
        if path.exists() or path.is_symlink():
            if path.is_symlink() or not path.is_file():
                raise GatewayContractError("gateway instance identity path is invalid")
            try:
                return UUID(path.read_text(encoding="ascii").strip())
            except (OSError, UnicodeError, ValueError) as exc:
                raise GatewayContractError("gateway instance identity is invalid") from exc
        candidate = uuid4()
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | getattr(os, "O_NOFOLLOW", 0)
        try:
            fd = os.open(path, flags, 0o600)
        except FileExistsError:
            try:
                return UUID(path.read_text(encoding="ascii").strip())
            except (OSError, UnicodeError, ValueError) as exc:
                raise GatewayContractError("gateway instance identity is invalid") from exc
        try:
            os.write(fd, f"{candidate}\n".encode("ascii"))
            os.fsync(fd)
        finally:
            os.close(fd)
        return candidate

    def capabilities(self) -> dict[str, Any]:
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_name": "NAUTILUS_TRADER",
            "runtime_version": nautilus_version,
            "gateway_instance_id": str(self._gateway_instance_id),
            "catalog_kind": "PARQUET_DATA_CATALOG",
            "supported_operations": [
                "CATALOG_INGEST",
                "CATALOG_VALIDATE",
                "BACKTEST",
                "SEALED_BACKTEST",
                "CANDIDATE_VERIFY",
            ],
            "live_execution_exposed": False,
        }

    def _load_catalog_registry(self) -> list[_CatalogRecord]:
        if not self._catalog_registry_path.exists():
            return []
        try:
            raw = json.loads(self._catalog_registry_path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError) as exc:
            raise GatewayContractError("catalog registry is invalid") from exc
        if not isinstance(raw, dict) or raw.get("version") != 1:
            raise GatewayContractError("catalog registry is invalid")
        entries = raw.get("catalogs")
        if not isinstance(entries, list):
            raise GatewayContractError("catalog registry is invalid")
        records: list[_CatalogRecord] = []
        seen_keys: set[str] = set()
        seen_storage_ids: set[UUID] = set()
        for entry in entries:
            if not isinstance(entry, dict):
                raise GatewayContractError("catalog registry is invalid")
            key = entry.get("catalog_key")
            state = entry.get("state")
            try:
                storage_id = UUID(str(entry.get("storage_id")))
            except (TypeError, ValueError) as exc:
                raise GatewayContractError("catalog registry is invalid") from exc
            if (
                not isinstance(key, str)
                or not key
                or state not in {"STAGING", "READY"}
                or key in seen_keys
                or storage_id in seen_storage_ids
            ):
                raise GatewayContractError("catalog registry is invalid")
            seen_keys.add(key)
            seen_storage_ids.add(storage_id)
            records.append(
                _CatalogRecord(catalog_key=key, storage_id=storage_id, state=state)
            )
        return records

    def _write_catalog_registry(self, records: list[_CatalogRecord]) -> None:
        payload = {
            "version": 1,
            "catalogs": [
                {
                    "catalog_key": record.catalog_key,
                    "storage_id": str(record.storage_id),
                    "state": record.state,
                }
                for record in sorted(records, key=lambda item: item.catalog_key)
            ],
        }
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            prefix=".registry-",
            dir=self._catalog_root,
            delete=False,
        ) as stream:
            json.dump(payload, stream, ensure_ascii=False, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
            temporary = Path(stream.name)
        os.replace(temporary, self._catalog_registry_path)

    @staticmethod
    def _find_catalog_record(
        records: list[_CatalogRecord], catalog_key: str
    ) -> _CatalogRecord | None:
        for record in records:
            if record.catalog_key == catalog_key:
                return record
        return None

    def _catalog_storage_path(self, storage_id: UUID) -> Path:
        return self._catalog_storage_root / storage_id.hex

    def _catalog_staging_path(self, storage_id: UUID) -> Path:
        return self._catalog_storage_root / f".{storage_id.hex}.staging"

    def _catalog_path(self, key: str) -> Path:
        record = self._find_catalog_record(self._load_catalog_registry(), key)
        if record is None or record.state != "READY":
            raise GatewayContractError("selected catalog is unavailable")
        path = self._catalog_storage_path(record.storage_id)
        if not path.is_dir():
            raise GatewayContractError("selected catalog is unavailable")
        return path

    @staticmethod
    def _manifest_result(manifest: dict[str, Any]) -> dict[str, Any]:
        return {
            "protocol_version": manifest["protocol_version"],
            "runtime_version": manifest["runtime_version"],
            "catalog_key": manifest["catalog_key"],
            "catalog_uri": f"nautilus-catalog://{manifest['catalog_key']}",
            "gateway_instance_id": UUID(str(manifest["gateway_instance_id"])),
            "catalog_release_id": UUID(str(manifest["catalog_release_id"])),
            "nautilus_data_type": manifest["nautilus_data_type"],
            "instrument_scope": manifest["instrument_scope"],
            "event_time_start": _parse_time(manifest["event_time_start"]),
            "event_time_end": _parse_time(manifest["event_time_end"]),
            "available_time_start": _parse_time(manifest["available_time_start"]),
            "available_time_end": _parse_time(manifest["available_time_end"]),
            "row_count": int(manifest["row_count"]),
            "schema_revision": manifest["schema_revision"],
            "quality_result": manifest["quality_result"],
            "point_in_time_result": manifest["point_in_time_result"],
            "ingested_at": _parse_time(manifest["ingested_at"]),
        }

    def _load_run_receipts(self) -> dict[str, dict[str, Any]]:
        if not self._run_receipts_path.exists():
            return {}
        try:
            raw = json.loads(self._run_receipts_path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError) as exc:
            raise GatewayContractError("stored run receipt registry is invalid") from exc
        if not isinstance(raw, dict) or not all(
            isinstance(key, str) and isinstance(value, dict)
            for key, value in raw.items()
        ):
            raise GatewayContractError("stored run receipt registry is invalid")
        return raw

    def _write_run_receipts(self, receipts: dict[str, dict[str, Any]]) -> None:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=self._run_root, delete=False
        ) as stream:
            json.dump(receipts, stream, ensure_ascii=False, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
            temporary = Path(stream.name)
        os.replace(temporary, self._run_receipts_path)

    def _idempotent_run(
        self,
        operation: str,
        request: BacktestExperimentRequest,
        runner: Callable[[BacktestExperimentRequest], dict[str, Any]],
    ) -> dict[str, Any]:
        canonical_request = request.model_dump(mode="json")
        receipt_key = str(request.experiment_id)
        experiment_lock_path = self._run_root / f"{request.experiment_id.hex}.lock"
        experiment_lock_fd = os.open(experiment_lock_path, os.O_CREAT | os.O_RDWR, 0o600)

        def registry_lock() -> int:
            fd = os.open(self._run_lock_path, os.O_CREAT | os.O_RDWR, 0o600)
            fcntl.flock(fd, fcntl.LOCK_EX)
            return fd

        def registry_unlock(fd: int) -> None:
            try:
                fcntl.flock(fd, fcntl.LOCK_UN)
            finally:
                os.close(fd)

        try:
            # Only identical experiment ids serialize for the duration of BacktestNode.
            # The global receipt registry lock is held only while reading/writing JSON.
            fcntl.flock(experiment_lock_fd, fcntl.LOCK_EX)
            global_fd = registry_lock()
            try:
                receipts = self._load_run_receipts()
                receipt = receipts.get(receipt_key)
                if receipt is not None:
                    if (
                        receipt.get("operation") != operation
                        or receipt.get("request") != canonical_request
                    ):
                        raise GatewayContractError(
                            "experiment id is already bound to another immutable backtest contract"
                        )
                    if receipt.get("state") == "FAILED":
                        raise GatewayContractError(
                            "the immutable backtest contract previously reached a terminal failure"
                        )
                    result = receipt.get("result")
                    if receipt.get("state") == "SUCCEEDED" and isinstance(result, dict):
                        return result
                    if receipt.get("state") not in {"RUNNING", "SUCCEEDED"}:
                        raise GatewayContractError("stored run receipt has an invalid state")
                receipts[receipt_key] = {
                    "operation": operation,
                    "request": canonical_request,
                    "state": "RUNNING",
                    "started_at": _utc_now().isoformat(),
                }
                self._write_run_receipts(receipts)
            finally:
                registry_unlock(global_fd)

            try:
                result = _jsonable(runner(request))
                if not isinstance(result, dict):
                    raise GatewayContractError("backtest terminal result is invalid")
            except GatewayContractError:
                global_fd = registry_lock()
                try:
                    receipts = self._load_run_receipts()
                    receipts[receipt_key] = {
                        "operation": operation,
                        "request": canonical_request,
                        "state": "FAILED",
                        "failure_code": "CONTRACT_INVALID",
                        "completed_at": _utc_now().isoformat(),
                    }
                    self._write_run_receipts(receipts)
                finally:
                    registry_unlock(global_fd)
                raise

            global_fd = registry_lock()
            try:
                receipts = self._load_run_receipts()
                receipt = receipts.get(receipt_key)
                if receipt is not None and (
                    receipt.get("operation") != operation
                    or receipt.get("request") != canonical_request
                ):
                    raise GatewayContractError("stored run receipt identity changed during execution")
                receipts[receipt_key] = {
                    "operation": operation,
                    "request": canonical_request,
                    "state": "SUCCEEDED",
                    "result": result,
                    "completed_at": _utc_now().isoformat(),
                }
                self._write_run_receipts(receipts)
            finally:
                registry_unlock(global_fd)
            return result
        finally:
            try:
                fcntl.flock(experiment_lock_fd, fcntl.LOCK_UN)
            finally:
                os.close(experiment_lock_fd)

    def run_backtest_idempotent(
        self, request: BacktestExperimentRequest
    ) -> dict[str, Any]:
        return self._idempotent_run("BACKTEST", request, self.run_backtest)

    def run_sealed_backtest_idempotent(
        self, request: BacktestExperimentRequest
    ) -> dict[str, Any]:
        return self._idempotent_run(
            "SEALED_BACKTEST", request, self.run_sealed_backtest
        )

    def _isolated_operation(
        self,
        operation: str,
        payload: dict[str, Any],
        *,
        catalog_key: str | None = None,
    ) -> dict[str, Any]:
        with tempfile.TemporaryDirectory(
            prefix="qz-isolated-", dir=self._artifact_root
        ) as directory:
            workspace = Path(directory)
            child_root = workspace / "runtime"
            child_catalog_root = child_root / "catalogs"
            child_storage_root = child_catalog_root / "data"
            child_storage_root.mkdir(parents=True)
            (child_root / ".gateway-instance-id").write_text(
                f"{self._gateway_instance_id}\n", encoding="ascii"
            )
            if catalog_key is not None:
                source_catalog = self._catalog_path(catalog_key)
                record = self._find_catalog_record(self._load_catalog_registry(), catalog_key)
                if record is None or record.state != "READY":
                    raise GatewayContractError("selected catalog is unavailable")
                child_storage_id = record.storage_id
                child_catalog_path = child_storage_root / child_storage_id.hex
                shutil.copytree(source_catalog, child_catalog_path)
                (child_catalog_root / "registry.json").write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "catalogs": [
                                {
                                    "catalog_key": catalog_key,
                                    "storage_id": str(child_storage_id),
                                    "state": "READY",
                                }
                            ],
                        },
                        sort_keys=True,
                    ),
                    encoding="utf-8",
                )
            input_path = workspace / "input.json"
            output_path = workspace / ".trusted-result.json"
            input_path.write_text(json.dumps(_jsonable(payload)), encoding="utf-8")
            completed = subprocess.run(
                _source_bundle_sandbox_command(
                    operation=operation, workspace=workspace, data_root=self._data_root
                ),
                cwd=workspace,
                env=_sanitized_child_environment(),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=900,
                check=False,
            )
            if completed.returncode != 0 or not output_path.is_file():
                raise GatewayContractError("isolated Nautilus operation failed")
            try:
                result = json.loads(output_path.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, UnicodeDecodeError) as exc:
                raise GatewayContractError("isolated Nautilus result is invalid") from exc
            if not isinstance(result, dict):
                raise GatewayContractError("isolated Nautilus result is invalid")
            return result

    @staticmethod
    def _instrument_from_definition(batch: Any) -> Any:
        try:
            instrument_module = importlib.import_module("nautilus_trader.model.instruments")
            instrument_class = getattr(instrument_module, batch.instrument_type)
            from_dict = getattr(instrument_class, "from_dict")
            instrument = from_dict(dict(batch.instrument_definition))
        except (AttributeError, ImportError, KeyError, TypeError, ValueError) as exc:
            raise GatewayContractError(
                f"invalid governed Nautilus instrument definition for {batch.instrument_id!r}"
            ) from exc
        if str(instrument.id) != batch.instrument_id:
            raise GatewayContractError(
                "governed instrument definition id does not match the requested instrument_id"
            )
        return instrument

    @staticmethod
    def _catalog_instruments(catalog: ParquetDataCatalog, requested_ids: list[str]) -> list[Any]:
        available = {str(item.id): item for item in catalog.instruments()}
        missing = sorted(set(requested_ids).difference(available))
        if missing:
            raise GatewayContractError(
                f"governed catalog is missing instrument definitions: {missing!r}"
            )
        return [available[item] for item in requested_ids]

    @staticmethod
    def _canonical_ingest_request(request: CatalogIngestRequest) -> dict[str, Any]:
        canonical = request.model_dump(mode="json", exclude={"request_id"})
        batches = list(canonical["instruments"])
        for batch in batches:
            batch["rows"] = sorted(
                batch["rows"],
                key=lambda row: (str(row["timestamp"]), str(row["available_at"])),
            )
        canonical["instruments"] = sorted(batches, key=lambda item: item["instrument_id"])
        return canonical

    def _existing_ingest_result(
        self,
        *,
        catalog_path: Path,
        canonical_request: dict[str, Any],
    ) -> dict[str, Any] | None:
        manifest_path = catalog_path / "quazonai-catalog-manifest.json"
        request_path = catalog_path / "quazonai-ingest-request.json"
        if not catalog_path.exists():
            return None
        if not manifest_path.exists() or not request_path.exists():
            raise GatewayContractError(
                "catalog key is immutable and contains an incomplete prior ingest"
            )
        existing_request = json.loads(request_path.read_text(encoding="utf-8"))
        if existing_request != canonical_request:
            raise GatewayContractError(
                "catalog key is immutable and already bound to another ingest contract"
            )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        try:
            storage_id = UUID(hex=catalog_path.name)
        except ValueError as exc:
            raise GatewayContractError("catalog storage identity is invalid") from exc
        if (
            str(manifest.get("gateway_instance_id")) != str(self._gateway_instance_id)
            or str(manifest.get("catalog_release_id")) != str(storage_id)
        ):
            raise GatewayContractError("catalog release identity does not match this Gateway")
        return self._manifest_result(manifest)

    @staticmethod
    def _batch_ticks(batch: Any) -> tuple[Any, list[QuoteTick], dict[str, Any]]:
        instrument = NautilusGatewayEngine._instrument_from_definition(batch)
        rows = sorted(batch.rows, key=lambda row: row.timestamp)
        event_times = [row.timestamp.astimezone(UTC) for row in rows]
        if len(set(event_times)) != len(event_times):
            raise GatewayContractError(
                f"event timestamps must be unique for {batch.instrument_id}"
            )
        try:
            frame = pd.DataFrame(
                {
                    "bid_price": [float(row.bid_price) for row in rows],
                    "ask_price": [float(row.ask_price) for row in rows],
                    "bid_size": [float(row.volume or "1000000") for row in rows],
                    "ask_size": [float(row.volume or "1000000") for row in rows],
                },
                index=pd.DatetimeIndex(event_times, name="timestamp"),
            )
        except ValueError as exc:
            raise GatewayContractError("quote prices and sizes must be numeric") from exc
        if not frame.index.is_monotonic_increasing or frame.index.has_duplicates:
            raise GatewayContractError(
                "event timestamps must be unique and monotonically increasing"
            )
        if (frame["bid_price"] > frame["ask_price"]).any():
            raise GatewayContractError("bid price cannot exceed ask price")

        wrangled = QuoteTickDataWrangler(instrument).process(frame)
        ticks = [
            QuoteTick(
                tick.instrument_id,
                tick.bid_price,
                tick.ask_price,
                tick.bid_size,
                tick.ask_size,
                tick.ts_event,
                int(pd.Timestamp(row.available_at).value),
            )
            for tick, row in zip(wrangled, rows, strict=True)
        ]
        if any(tick.ts_init < tick.ts_event for tick in ticks):
            raise GatewayContractError("availability timestamp cannot precede event timestamp")
        availability_times = [row.available_at.astimezone(UTC) for row in rows]
        record = {
            "instrument_type": batch.instrument_type,
            "row_count": len(ticks),
            "event_time_start": min(event_times).isoformat(),
            "event_time_end": max(event_times).isoformat(),
            "available_time_start": min(availability_times).isoformat(),
            "available_time_end": max(availability_times).isoformat(),
        }
        return instrument, ticks, record

    def ingest(self, request: CatalogIngestRequest) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
        canonical_request = self._canonical_ingest_request(request)
        lock_fd = os.open(
            self._catalog_registry_lock_path, os.O_CREAT | os.O_RDWR, 0o600
        )
        try:
            fcntl.flock(lock_fd, fcntl.LOCK_EX)
            records = self._load_catalog_registry()
            existing_record = self._find_catalog_record(records, request.catalog_key)
            if existing_record is not None and existing_record.state == "READY":
                existing = self._existing_ingest_result(
                    catalog_path=self._catalog_storage_path(existing_record.storage_id),
                    canonical_request=canonical_request,
                )
                if existing is None:
                    raise GatewayContractError(
                        "catalog key is immutable and contains an incomplete prior ingest"
                    )
                return existing
            if existing_record is not None:
                for candidate in (
                    self._catalog_staging_path(existing_record.storage_id),
                    self._catalog_storage_path(existing_record.storage_id),
                ):
                    if candidate.exists():
                        shutil.rmtree(candidate, ignore_errors=True)
                records = [
                    record
                    for record in records
                    if record.storage_id != existing_record.storage_id
                ]
                self._write_catalog_registry(records)

            storage_id = uuid4()
            staging_record = _CatalogRecord(
                catalog_key=request.catalog_key,
                storage_id=storage_id,
                state="STAGING",
            )
            records.append(staging_record)
            self._write_catalog_registry(records)
            staging_path = self._catalog_staging_path(storage_id)
            catalog_path = self._catalog_storage_path(storage_id)
            committed = False
            try:
                staging_path.mkdir(mode=0o700)
                catalog = ParquetDataCatalog(path=str(staging_path))
                instrument_records: dict[str, dict[str, Any]] = {}
                for batch in sorted(
                    request.instruments, key=lambda item: item.instrument_id
                ):
                    instrument, ticks, record = self._batch_ticks(batch)
                    catalog.write_data([instrument])
                    catalog.write_data(ticks)
                    instrument_records[str(instrument.id)] = {
                        "provider": request.provider,
                        "source": request.source,
                        "source_license": request.source_license,
                        **record,
                    }

                instrument_scope = sorted(str(item.id) for item in catalog.instruments())
                expected_scope = sorted(item.instrument_id for item in request.instruments)
                if instrument_scope != expected_scope:
                    raise GatewayContractError(
                        "persisted Nautilus instrument scope differs from the governed ingest contract"
                    )
                all_records = list(instrument_records.values())
                event_start = min(
                    _parse_time(item["event_time_start"]) for item in all_records
                )
                event_end = max(
                    _parse_time(item["event_time_end"]) for item in all_records
                )
                available_start = min(
                    _parse_time(item["available_time_start"]) for item in all_records
                )
                available_end = max(
                    _parse_time(item["available_time_end"]) for item in all_records
                )
                manifest = {
                    "protocol_version": PROTOCOL_VERSION,
                    "runtime_version": nautilus_version,
                    "gateway_instance_id": str(self._gateway_instance_id),
                    "catalog_release_id": str(storage_id),
                    "catalog_key": request.catalog_key,
                    "nautilus_data_type": request.nautilus_data_type,
                    "instrument_scope": instrument_scope,
                    "instruments": instrument_records,
                    "event_time_start": event_start.isoformat(),
                    "event_time_end": event_end.isoformat(),
                    "available_time_start": available_start.isoformat(),
                    "available_time_end": available_end.isoformat(),
                    "row_count": sum(
                        int(item["row_count"]) for item in all_records
                    ),
                    "schema_revision": QUOTE_TICK_SCHEMA_REVISION,
                    "quality_result": {
                        "state": "VALID",
                        "duplicate_timestamps": 0,
                        "crossed_quotes": 0,
                        "sorted": True,
                    },
                    "point_in_time_result": {
                        "state": "VALID",
                        "replay_order": "TS_INIT",
                        "event_time_preserved": True,
                        "availability_time_preserved": True,
                    },
                    "ingested_at": _utc_now().isoformat(),
                }
                (staging_path / "quazonai-catalog-manifest.json").write_text(
                    json.dumps(manifest, indent=2), encoding="utf-8"
                )
                (staging_path / "quazonai-ingest-request.json").write_text(
                    json.dumps(canonical_request, indent=2, sort_keys=True),
                    encoding="utf-8",
                )
                os.rename(staging_path, catalog_path)
                records = [
                    _CatalogRecord(
                        catalog_key=record.catalog_key,
                        storage_id=record.storage_id,
                        state=(
                            "READY"
                            if record.storage_id == storage_id
                            else record.state
                        ),
                    )
                    for record in records
                ]
                self._write_catalog_registry(records)
                committed = True
                return self._manifest_result(manifest)
            finally:
                if not committed:
                    for candidate in (staging_path, catalog_path):
                        if candidate.exists():
                            shutil.rmtree(candidate, ignore_errors=True)
                    current = [
                        record
                        for record in self._load_catalog_registry()
                        if record.storage_id != storage_id
                    ]
                    self._write_catalog_registry(current)
        finally:
            try:
                fcntl.flock(lock_fd, fcntl.LOCK_UN)
            finally:
                os.close(lock_fd)

    def validate_catalog(self, request: CatalogValidationRequest) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
        catalog_path = self._catalog_path(request.catalog_key)
        record = self._find_catalog_record(self._load_catalog_registry(), request.catalog_key)
        if record is None or record.state != "READY":
            raise GatewayContractError("selected catalog is unavailable")
        manifest_path = catalog_path / "quazonai-catalog-manifest.json"
        findings: list[dict[str, Any]] = []
        if not manifest_path.exists():
            return {
                "protocol_version": PROTOCOL_VERSION,
                "runtime_version": nautilus_version,
                "catalog_key": request.catalog_key,
                "valid": False,
                "instrument_scope": [],
                "row_count": 0,
                "findings": [{"code": "CATALOG_MANIFEST_MISSING"}],
            }
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if str(manifest.get("gateway_instance_id")) != str(self._gateway_instance_id):
            findings.append({"code": "GATEWAY_INSTANCE_ID_MISMATCH"})
        if str(manifest.get("catalog_release_id")) != str(record.storage_id):
            findings.append({"code": "CATALOG_RELEASE_ID_MISMATCH"})
        catalog = ParquetDataCatalog(path=str(catalog_path))
        instruments = catalog.instruments()
        scope = sorted(str(instrument.id) for instrument in instruments)
        requested = set(request.instrument_ids)
        if requested and not requested.issubset(scope):
            findings.append(
                {
                    "code": "INSTRUMENT_SCOPE_MISMATCH",
                    "missing": sorted(requested.difference(scope)),
                }
            )
        if set(manifest.get("instrument_scope", [])) != set(scope):
            findings.append({"code": "CATALOG_MANIFEST_SCOPE_MISMATCH"})
        if (
            request.nautilus_data_type
            and request.nautilus_data_type != manifest.get("nautilus_data_type")
        ):
            findings.append(
                {
                    "code": "DATA_TYPE_MISMATCH",
                    "expected": request.nautilus_data_type,
                    "actual": manifest.get("nautilus_data_type"),
                }
            )

        ticks: list[Any] = []
        if scope:
            try:
                ticks = list(catalog.quote_ticks(instrument_ids=scope))
            except (OSError, RuntimeError, TypeError, ValueError):
                findings.append({"code": "CATALOG_DATA_QUERY_FAILED"})
        if not ticks:
            findings.append({"code": "CATALOG_QUOTE_DATA_MISSING"})

        actual_row_count = len(ticks)
        actual_event_start: datetime | None = None
        actual_event_end: datetime | None = None
        actual_available_start: datetime | None = None
        actual_available_end: datetime | None = None
        if ticks:
            tick_scope = sorted({str(tick.instrument_id) for tick in ticks})
            if tick_scope != scope:
                findings.append(
                    {
                        "code": "CATALOG_DATA_SCOPE_MISMATCH",
                        "instrument_scope": scope,
                        "quote_scope": tick_scope,
                    }
                )
            event_values = [int(tick.ts_event) for tick in ticks]
            init_values = [int(tick.ts_init) for tick in ticks]
            actual_event_start = datetime.fromtimestamp(min(event_values) / 1_000_000_000, UTC)
            actual_event_end = datetime.fromtimestamp(max(event_values) / 1_000_000_000, UTC)
            actual_available_start = datetime.fromtimestamp(min(init_values) / 1_000_000_000, UTC)
            actual_available_end = datetime.fromtimestamp(max(init_values) / 1_000_000_000, UTC)
            if any(init < event for init, event in zip(init_values, event_values, strict=True)):
                findings.append({"code": "CATALOG_POINT_IN_TIME_ORDER_INVALID"})

        if actual_row_count != int(manifest.get("row_count", -1)):
            findings.append(
                {
                    "code": "CATALOG_MANIFEST_ROW_COUNT_MISMATCH",
                    "manifest": manifest.get("row_count"),
                    "actual": actual_row_count,
                }
            )
        bounds = {
            "event_time_start": actual_event_start,
            "event_time_end": actual_event_end,
            "available_time_start": actual_available_start,
            "available_time_end": actual_available_end,
        }
        for key, actual in bounds.items():
            if actual is None:
                continue
            try:
                expected = _parse_time(manifest[key])
            except (GatewayContractError, KeyError, TypeError, ValueError):
                findings.append({"code": "CATALOG_MANIFEST_TIME_INVALID", "field": key})
                continue
            if actual != expected:
                findings.append(
                    {
                        "code": "CATALOG_MANIFEST_TIME_MISMATCH",
                        "field": key,
                        "manifest": expected.isoformat(),
                        "actual": actual.isoformat(),
                    }
                )

        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "catalog_key": request.catalog_key,
            "catalog_uri": f"nautilus-catalog://{request.catalog_key}",
            "gateway_instance_id": self._gateway_instance_id,
            "catalog_release_id": record.storage_id,
            "valid": not findings and bool(instruments) and bool(ticks),
            "nautilus_data_type": manifest.get("nautilus_data_type"),
            "instrument_scope": scope,
            "row_count": actual_row_count,
            "event_time_start": actual_event_start,
            "event_time_end": actual_event_end,
            "available_time_start": actual_available_start,
            "available_time_end": actual_available_end,
            "schema_revision": manifest.get("schema_revision"),
            "quality_result": manifest.get("quality_result", {}),
            "point_in_time_result": manifest.get("point_in_time_result", {}),
            "ingested_at": _parse_time(manifest["ingested_at"]),
            "findings": findings,
        }

    @contextmanager
    def _strategy(
        self, request: BacktestExperimentRequest
    ) -> Iterator[ImportableStrategyConfig]:
        strategy = request.strategy
        if strategy.requirements:
            normalized = {
                item.replace(" ", "").lower() for item in strategy.requirements
            }
            allowed = {f"nautilus_trader=={VALIDATED_NAUTILUS_VERSION}"}
            if not normalized.issubset(allowed):
                raise GatewayContractError(
                    "remote artifact dependencies must be pre-approved in the runtime image"
                )
        if strategy.kind == "IMPORTABLE":
            yield ImportableStrategyConfig(
                strategy_path=strategy.strategy_path,
                config_path=strategy.config_path,
                config=self._materialize_config(strategy.config, request),
            )
            return

        finder = _SourceBundleFinder(strategy.source_files)
        required_modules = {
            strategy.strategy_path.split(":", 1)[0],
            strategy.config_path.split(":", 1)[0],
        }
        if not required_modules.issubset(finder.module_names):
            raise GatewayContractError(
                "strategy and config import paths must resolve inside the source bundle"
            )
        conflicts = sorted(finder.module_names.intersection(sys.modules))
        if conflicts:
            raise GatewayContractError(
                "strategy source module conflicts with an already loaded runtime module"
            )
        sys.meta_path.insert(0, finder)
        importlib.invalidate_caches()
        try:
            yield ImportableStrategyConfig(
                strategy_path=strategy.strategy_path,
                config_path=strategy.config_path,
                config=self._materialize_config(strategy.config, request),
            )
        finally:
            if finder in sys.meta_path:
                sys.meta_path.remove(finder)
            for module_name in sorted(finder.loaded_modules, reverse=True):
                sys.modules.pop(module_name, None)
            importlib.invalidate_caches()

    @staticmethod
    def _materialize_config(
        config: dict[str, Any], request: BacktestExperimentRequest
    ) -> dict[str, Any]:
        result = dict(config)
        instruments = [InstrumentId.from_str(item) for item in request.instrument_ids]
        if "instrument_id" in result:
            result["instrument_id"] = instruments[0]
        if "instrument_ids" in result:
            result["instrument_ids"] = instruments
        if "bar_type" in result:
            from nautilus_trader.model.data import BarType

            result["bar_type"] = BarType.from_str(str(result["bar_type"]))
        for key in ("trade_size", "order_qty", "quantity"):
            if key in result:
                result[key] = Decimal(str(result[key]))
        return result

    def run_backtest(
        self,
        request: BacktestExperimentRequest,
        *,
        _source_isolated: bool = False,
    ) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
        if request.strategy.kind == "SOURCE_BUNDLE" and not _source_isolated:
            return self._isolated_operation(
                "backtest",
                {"request": request.model_dump(mode="json")},
                catalog_key=request.catalog_key,
            )
        validation = self.validate_catalog(
            CatalogValidationRequest(
                catalog_key=request.catalog_key,
                instrument_ids=request.instrument_ids,
                nautilus_data_type="QuoteTick",
            )
        )
        if not validation["valid"]:
            raise GatewayContractError(
                f"catalog validation failed: {validation['findings']!r}"
            )
        catalog_path = self._catalog_path(request.catalog_key)
        catalog = ParquetDataCatalog(path=str(catalog_path))
        instruments = self._catalog_instruments(catalog, request.instrument_ids)
        venue_names = sorted({str(instrument.id).rsplit(".", 1)[-1] for instrument in instruments})
        started_at = _utc_now()

        with self._strategy(request) as strategy_config:
            engine_config = BacktestEngineConfig(strategies=[strategy_config])
            venue_overrides = dict(request.venue_config)
            venue_overrides.pop("name", None)
            venues: list[BacktestVenueConfig] = []
            for venue_name in venue_names:
                venue_defaults: dict[str, Any] = {
                    "name": venue_name,
                    "oms_type": "HEDGING",
                    "account_type": "MARGIN",
                    "base_currency": "USD",
                    "starting_balances": ["1_000_000 USD"],
                    "book_type": "L1_MBP",
                }
                venue_defaults.update(venue_overrides)
                venues.append(BacktestVenueConfig(**venue_defaults))
            data = [
                BacktestDataConfig(
                    catalog_path=str(catalog_path),
                    data_cls=QuoteTick,
                    instrument_id=instrument.id,
                    start_time=request.start_time,
                    end_time=request.end_time,
                )
                for instrument in instruments
            ]
            run_config = BacktestRunConfig(
                engine=engine_config,
                venues=venues,
                data=data,
                dispose_on_completion=False,
            )
            node = BacktestNode(configs=[run_config])
            try:
                results = node.run()
                if not results:
                    raise RuntimeError("Nautilus BacktestNode returned no result")
                result = results[0]
                try:
                    engine = node.get_engine(run_config.id)
                except (AttributeError, KeyError):
                    engine = getattr(node, "_engines", {}).get(run_config.id)
                if engine is None:
                    raise RuntimeError(
                        "Nautilus BacktestNode did not retain its engine"
                    )
                evidence = self._evidence(request, result, engine, started_at)
            finally:
                node.dispose()
        return evidence

    def _evidence(
        self,
        request: BacktestExperimentRequest,
        result: Any,
        engine: Any,
        started_at: datetime,
    ) -> dict[str, Any]:
        orders = list(engine.cache.orders())
        positions = list(engine.cache.positions())
        try:
            accounts = list(engine.cache.accounts())
        except (AttributeError, TypeError):
            accounts = []
        fills: list[Any] = []
        for order in orders:
            for event in list(_attr(order, "events") or []):
                if type(event).__name__ == "OrderFilled":
                    fills.append(event)

        order_payload = [
            {
                "order_id": str(_attr(order, "client_order_id", "id") or "UNKNOWN"),
                "instrument_id": _jsonable(_attr(order, "instrument_id")),
                "side": _jsonable(_attr(order, "side")),
                "order_type": _jsonable(_attr(order, "order_type")),
                "status": _jsonable(_attr(order, "status")),
                "quantity": _jsonable(_attr(order, "quantity")),
                "filled_quantity": _jsonable(
                    _attr(order, "filled_qty", "filled_quantity")
                ),
                "ts_init": _jsonable(_attr(order, "ts_init")),
            }
            for order in orders
        ]
        fill_payload = [
            {
                "trade_id": _jsonable(_attr(fill, "trade_id")),
                "order_id": _jsonable(_attr(fill, "client_order_id", "order_id")),
                "instrument_id": _jsonable(_attr(fill, "instrument_id")),
                "side": _jsonable(_attr(fill, "order_side", "side")),
                "quantity": _jsonable(_attr(fill, "last_qty", "quantity")),
                "price": _jsonable(_attr(fill, "last_px", "price")),
                "commission": _jsonable(_attr(fill, "commission")),
                "ts_event": _jsonable(_attr(fill, "ts_event")),
            }
            for fill in fills
        ]
        position_payload = [
            {
                "position_id": str(_attr(position, "id", "position_id") or "UNKNOWN"),
                "instrument_id": _jsonable(_attr(position, "instrument_id")),
                "side": _jsonable(_attr(position, "side")),
                "quantity": _jsonable(_attr(position, "quantity")),
                "realized_pnl": _jsonable(_attr(position, "realized_pnl")),
                "unrealized_pnl": _jsonable(_attr(position, "unrealized_pnl")),
                "opened_at": _jsonable(_attr(position, "ts_opened")),
                "closed_at": _jsonable(_attr(position, "ts_closed")),
            }
            for position in positions
        ]
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "experiment_id": request.experiment_id,
            "remote_run_id": str(_attr(result, "run_id") or uuid4()),
            "mode": request.mode,
            "started_at": started_at,
            "finished_at": _utc_now(),
            "orders": order_payload,
            "fills": fill_payload,
            "positions": position_payload,
            "balances": [_jsonable(account) for account in accounts],
            "pnl": _jsonable(_attr(result, "stats_pnls") or {}),
            "statistics": {
                "returns": _jsonable(_attr(result, "stats_returns") or {}),
                "general": _jsonable(_attr(result, "stats_general") or {}),
                "total_events": _jsonable(_attr(result, "total_events") or 0),
                "total_orders": _jsonable(_attr(result, "total_orders") or len(orders)),
                "total_positions": _jsonable(
                    _attr(result, "total_positions") or len(positions)
                ),
                "iterations": _jsonable(_attr(result, "iterations") or 0),
                "elapsed_time": _jsonable(_attr(result, "elapsed_time") or 0),
            },
            "diagnostics": {
                "catalog_key": request.catalog_key,
                "instrument_ids": request.instrument_ids,
                "loaded_instrument_count": len(request.instrument_ids),
                "strategy_artifact_id": request.strategy.artifact_id,
            },
        }

    def run_sealed_backtest(self, request: BacktestExperimentRequest) -> dict[str, Any]:
        sealed = request.model_copy(update={"mode": ExperimentMode.SEALED})
        raw = self.run_backtest(sealed)
        disclosure = _sealed_performance_disclosure(raw)
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "experiment_id": request.experiment_id,
            "remote_run_id": raw["remote_run_id"],
            "mode": ExperimentMode.SEALED,
            "disclosure": disclosure,
            "raw_evidence_withheld": True,
        }

    def verify_candidate(self, request: CandidateVerificationRequest) -> dict[str, Any]:
        findings: list[dict[str, Any]] = []
        replay: dict[str, Any] | None = None
        try:
            wheel = base64.b64decode(request.strategy_wheel_b64, validate=True)
        except ValueError:
            wheel = b""
            findings.append({"code": "STRATEGY_WHEEL_BASE64_INVALID"})

        forbidden_markers = (
            "password",
            "secret",
            "api_key",
            "private_key",
            "credential",
            "access_token",
            "auth_token",
            "token",
            "broker_token",
            "broker_secret",
        )

        def walk(value: Any, path: str = "manifest") -> None:
            if isinstance(value, dict):
                for key, item in value.items():
                    normalized = str(key).casefold().replace("-", "_")
                    if any(marker in normalized for marker in forbidden_markers) and item not in (
                        None,
                        "",
                        "INJECT_AT_REMOTE_RUNTIME_ONLY",
                    ):
                        findings.append(
                            {"code": "LIVE_SECRET_IN_BUNDLE", "path": f"{path}.{key}"}
                        )
                    walk(item, f"{path}.{key}")
            elif isinstance(value, list):
                for index, item in enumerate(value):
                    walk(item, f"{path}[{index}]")

        walk(request.manifest)
        required = {
            "contract",
            "contract_version",
            "bundle_id",
            "candidate_id",
            "runtime",
            "strategy",
            "data",
            "runtime_config",
            "validation",
            "evidence",
            "lineage",
            "target_weights",
        }
        missing = sorted(required.difference(request.manifest))
        if missing:
            findings.append({"code": "MANIFEST_FIELDS_MISSING", "fields": missing})
        if request.manifest.get("contract") != CANDIDATE_BUNDLE_CONTRACT:
            findings.append({"code": "BUNDLE_CONTRACT_INVALID"})
        if request.manifest.get("contract_version") != CANDIDATE_BUNDLE_CONTRACT_VERSION:
            findings.append({"code": "BUNDLE_CONTRACT_VERSION_INVALID"})
        if str(request.manifest.get("candidate_id")) != str(request.candidate_id):
            findings.append({"code": "CANDIDATE_ID_MISMATCH"})
        runtime = request.manifest.get("runtime", {})
        if runtime.get("name") != "NAUTILUS_TRADER":
            findings.append({"code": "RUNTIME_NAME_INVALID"})
        if runtime.get("version") != VALIDATED_NAUTILUS_VERSION:
            findings.append({"code": "RUNTIME_VERSION_INVALID"})
        if runtime.get("deployment") != "REMOTE_INDEPENDENT_RUNTIME":
            findings.append({"code": "RUNTIME_DEPLOYMENT_INVALID"})
        if runtime.get("paper_live_reuse") != "SAME_STRATEGY_WHEEL_AND_CONFIG":
            findings.append({"code": "STRATEGY_REUSE_CONTRACT_INVALID"})
        strategy = request.manifest.get("strategy", {})
        expected_wheel = _candidate_strategy_wheel_path(request.candidate_id)
        if strategy.get("wheel") != expected_wheel:
            findings.append({"code": "STRATEGY_WHEEL_PATH_INVALID"})

        fixture_required = {
            "dataset_revision_id",
            "strategy_config",
            "instrument_scope",
            "backtest_run_config",
            "venue_config",
            "risk_config",
            "orders",
            "fills",
            "positions",
            "statistics",
            "pnl",
        }
        fixture_missing = sorted(fixture_required.difference(request.fixture))
        if fixture_missing:
            findings.append(
                {"code": "CONFORMANCE_FIXTURE_INCOMPLETE", "fields": fixture_missing}
            )

        if wheel and not findings:
            run_config = request.fixture.get("backtest_run_config")
            catalog_key = run_config.get("catalog_key") if isinstance(run_config, dict) else None
            if not isinstance(catalog_key, str) or not catalog_key:
                findings.append({"code": "CONFORMANCE_CATALOG_MISSING"})
            else:
                try:
                    replay = self._isolated_operation(
                        "verify-candidate",
                        {
                            "wheel_b64": base64.b64encode(wheel).decode("ascii"),
                            "manifest": request.manifest,
                            "fixture": request.fixture,
                        },
                        catalog_key=catalog_key,
                    )
                except (
                    GatewayContractError,
                    ImportError,
                    KeyError,
                    TypeError,
                    ValueError,
                    zipfile.BadZipFile,
                ):
                    findings.append(
                        {"code": "CONFORMANCE_REPLAY_FAILED"}
                    )

        def canonical_rows(rows: Any, fields: tuple[str, ...]) -> list[dict[str, Any]]:
            if not isinstance(rows, list):
                return []
            normalized = [
                {field: _jsonable(row.get(field)) for field in fields}
                for row in rows
                if isinstance(row, dict)
            ]
            return sorted(normalized, key=lambda row: json.dumps(row, sort_keys=True))

        if replay is not None:
            comparisons = {
                "orders": (
                    "instrument_id",
                    "side",
                    "order_type",
                    "status",
                    "quantity",
                    "filled_quantity",
                ),
                "fills": (
                    "instrument_id",
                    "side",
                    "quantity",
                    "price",
                    "commission",
                ),
                "positions": (
                    "instrument_id",
                    "side",
                    "quantity",
                    "realized_pnl",
                    "unrealized_pnl",
                ),
            }
            for name, fields in comparisons.items():
                expected_rows = canonical_rows(request.fixture.get(name), fields)
                actual_rows = canonical_rows(replay.get(name), fields)
                if actual_rows != expected_rows:
                    findings.append(
                        {
                            "code": "CONFORMANCE_REFERENCE_MISMATCH",
                            "section": name,
                            "expected": expected_rows,
                            "actual": actual_rows,
                        }
                    )
            expected_pnl = _jsonable(request.fixture.get("pnl", {}))
            actual_pnl = _jsonable(replay.get("pnl", {}))
            if actual_pnl != expected_pnl:
                findings.append(
                    {
                        "code": "CONFORMANCE_REFERENCE_MISMATCH",
                        "section": "pnl",
                        "expected": expected_pnl,
                        "actual": actual_pnl,
                    }
                )
            expected_stats = request.fixture.get("statistics", {})
            actual_stats = replay.get("statistics", {})
            for key in (
                "returns",
                "general",
                "total_orders",
                "total_positions",
                "iterations",
            ):
                if isinstance(expected_stats, dict) and key in expected_stats:
                    if not isinstance(actual_stats, dict) or _jsonable(
                        actual_stats.get(key)
                    ) != _jsonable(expected_stats.get(key)):
                        findings.append(
                            {
                                "code": "CONFORMANCE_REFERENCE_MISMATCH",
                                "section": f"statistics.{key}",
                                "expected": expected_stats.get(key),
                                "actual": (
                                    actual_stats.get(key)
                                    if isinstance(actual_stats, dict)
                                    else None
                                ),
                            }
                        )

        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "candidate_id": request.candidate_id,
            "compatible": not findings,
            "findings": findings,
        }

    @staticmethod
    def _verify_wheel_inline(wheel: bytes, manifest: dict[str, Any]) -> None:
        with zipfile.ZipFile(io.BytesIO(wheel)) as archive:
            names = archive.namelist()
            for name in names:
                if not name.endswith(".py"):
                    continue
                try:
                    source = archive.read(name).decode("utf-8")
                except UnicodeDecodeError as exc:
                    raise GatewayContractError("wheel Python source must be UTF-8") from exc
                _validate_restricted_strategy_source(name, source)
            if not names or any(
                Path(name).is_absolute() or ".." in Path(name).parts for name in names
            ):
                raise GatewayContractError("wheel contains unsafe paths")
            metadata_names = [
                name for name in names if name.endswith(".dist-info/METADATA")
            ]
            if len(metadata_names) != 1:
                raise GatewayContractError(
                    "wheel must contain exactly one METADATA file"
                )
            metadata = archive.read(metadata_names[0]).decode("utf-8")
            pin = f"Requires-Dist: nautilus_trader (=={VALIDATED_NAUTILUS_VERSION})"
            alternate_pin = (
                f"Requires-Dist: nautilus-trader (=={VALIDATED_NAUTILUS_VERSION})"
            )
            if pin not in metadata and alternate_pin not in metadata:
                raise GatewayContractError(
                    "wheel does not pin the validated Nautilus version"
                )
            strategy_path = str(manifest.get("strategy", {}).get("strategy_path", ""))
            config_path = str(manifest.get("strategy", {}).get("config_path", ""))
            if ":" not in strategy_path or ":" not in config_path:
                raise GatewayContractError("manifest strategy import paths are invalid")
            strategy_module, strategy_name = strategy_path.split(":", 1)
            config_module, config_name = config_path.split(":", 1)
            with tempfile.TemporaryDirectory(prefix="qz-wheel-verify-") as directory:
                wheel_path = Path(directory) / "candidate.whl"
                wheel_path.write_bytes(wheel)
                sys.path.insert(0, str(wheel_path))
                importlib.invalidate_caches()
                try:
                    strategy_class = getattr(
                        importlib.import_module(strategy_module), strategy_name
                    )
                    config_class = getattr(
                        importlib.import_module(config_module), config_name
                    )
                    if not issubclass(strategy_class, Strategy):
                        raise GatewayContractError(
                            "strategy class is not a Nautilus Strategy"
                        )
                    if not issubclass(config_class, StrategyConfig):
                        raise GatewayContractError(
                            "config class is not a Nautilus StrategyConfig"
                        )
                finally:
                    sys.path.remove(str(wheel_path))
                    sys.modules.pop(strategy_module, None)
                    sys.modules.pop(config_module, None)
