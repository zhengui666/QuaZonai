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
import multiprocessing
import os
import re
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import zipfile
from collections.abc import Iterator, Mapping
from contextlib import contextmanager
from datetime import UTC, datetime
from decimal import Decimal
from io import BytesIO
from pathlib import Path
from typing import Any, BinaryIO, cast
from uuid import uuid4

from fastapi import FastAPI, File, Header, HTTPException, UploadFile
from pydantic import BaseModel, ConfigDict

from quant_runtime.config import CONTRACT_VERSION, PINNED_NAUTILUS_VERSION
from quant_runtime.contracts import (
    CatalogDescriptor,
    CatalogIngestSpec,
    ExperimentSpec,
    RunMode,
    RunEvidence,
    RuntimeCapabilities,
    StrategyArtifact,
)

_RUNS: dict[str, dict[str, Any]] = {}
_RUNS_LOCK = threading.Lock()
_CATALOG_LOCK = threading.Lock()
_EXECUTION_LOCK = threading.Lock()
_CATALOG_PREFIX = "catalog://"
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


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class CatalogValidationInput(StrictModel):
    catalog_uri: str


class RunInput(StrictModel):
    mode: RunMode
    experiment: ExperimentSpec


def _now() -> datetime:
    return datetime.now(UTC)


def _runtime_version() -> str:
    return importlib.metadata.version("nautilus_trader")


def _jsonable(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else None
    if isinstance(value, Decimal):
        return str(value)
    if isinstance(value, Mapping):
        return {str(key): _jsonable(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_jsonable(item) for item in value]
    if hasattr(value, "isoformat"):
        try:
            return value.isoformat()
        except (TypeError, ValueError):
            pass
    return str(value)


def _records(value: Any) -> list[dict[str, Any]]:
    if value is None:
        return []
    frame = value.reset_index() if hasattr(value, "reset_index") else value
    if not hasattr(frame, "to_dict"):
        return []
    records = frame.to_dict(orient="records")
    return [_jsonable(record) for record in records]


def _catalog_root() -> Path:
    root = Path(
        os.environ.get(
            "QUAZONAI_NAUTILUS_CATALOG_ROOT",
            "/var/lib/quazonai-nautilus/catalogs",
        )
    )
    root.mkdir(parents=True, exist_ok=True)
    return root


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


def _write_catalog(spec: CatalogIngestSpec) -> CatalogDescriptor:
    catalog_uri = f"catalog://{spec.catalog_name}"
    path = _catalog_path(catalog_uri)
    with _CATALOG_LOCK:
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
            if source_kind != "synthetic_fx_quotes":
                raise HTTPException(
                    status_code=422,
                    detail=(
                        "The reference service accepts source_spec.kind=synthetic_fx_quotes only; "
                        "production runtimes should provide approved provider adapters."
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


def _instrument(catalog_uri: str, instrument_id: str) -> tuple[Any, Path]:
    from nautilus_trader.persistence.catalog import ParquetDataCatalog

    path = _catalog_path(catalog_uri)
    catalog = ParquetDataCatalog(path)
    for instrument in catalog.instruments():
        if instrument.id.value == instrument_id:
            return instrument, path
    raise HTTPException(status_code=422, detail="instrument is not present in catalog")


@contextmanager
def _strategy_import_root(artifact: StrategyArtifact) -> Iterator[None]:
    root = Path(tempfile.mkdtemp(prefix="quazonai-nautilus-strategy-"))
    try:
        for relative_path, source in artifact.source_files.items():
            target = (root / relative_path).resolve()
            if not target.is_relative_to(root):
                raise HTTPException(status_code=422, detail="strategy source escapes artifact root")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(source, encoding="utf-8")
        sys.path.insert(0, str(root))
        importlib.invalidate_caches()
        yield
    finally:
        if str(root) in sys.path:
            sys.path.remove(str(root))
        for name, module in list(sys.modules.items()):
            module_file = getattr(module, "__file__", None)
            if module_file:
                try:
                    if Path(module_file).resolve().is_relative_to(root):
                        sys.modules.pop(name, None)
                except OSError:
                    continue
        importlib.invalidate_caches()
        shutil.rmtree(root, ignore_errors=True)


def _normalize_strategy_config(artifact: StrategyArtifact, instrument: Any) -> dict[str, Any]:
    config = dict(artifact.config)
    config["instrument_id"] = instrument.id
    config.setdefault("bar_type", f"{instrument.id.value}-5-MINUTE-BID-INTERNAL")
    if "trade_size" in config:
        config["trade_size"] = Decimal(str(config["trade_size"]))
    return config


def _named_statistic(values: Mapping[str, Any], *needles: str) -> float:
    for key, value in values.items():
        normalized = str(key).casefold().replace("_", " ")
        if all(needle in normalized for needle in needles):
            try:
                result = float(value)
            except (TypeError, ValueError):
                continue
            if math.isfinite(result):
                return result
    return 0.0


def _validate_portfolio_frame(experiment: ExperimentSpec) -> None:
    frame = experiment.parameters.get("portfolio_target_frame")
    if not isinstance(frame, dict) or frame.get("schema_version") != "1":
        raise HTTPException(status_code=422, detail="PORTFOLIO requires a frozen target frame")
    rows = frame.get("rows")
    if not isinstance(rows, list) or not rows:
        raise HTTPException(status_code=422, detail="PORTFOLIO target frame is empty")
    total = 0.0
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("instrument_id"), str):
            raise HTTPException(status_code=422, detail="PORTFOLIO target frame is invalid")
        try:
            weight = float(row["target_weight"])
        except (KeyError, TypeError, ValueError) as exc:
            raise HTTPException(status_code=422, detail="PORTFOLIO target frame is invalid") from exc
        if not math.isfinite(weight) or weight < 0.0 or weight > 1.0:
            raise HTTPException(status_code=422, detail="PORTFOLIO target frame weight is invalid")
        total += weight
    if not math.isclose(total, 1.0, rel_tol=0.0, abs_tol=1e-9):
        raise HTTPException(status_code=422, detail="PORTFOLIO target frame weights must sum to one")


def _execute_once(experiment: ExperimentSpec, mode: RunMode) -> RunEvidence:
    from nautilus_trader.backtest.node import BacktestDataConfig
    from nautilus_trader.backtest.node import BacktestEngineConfig
    from nautilus_trader.backtest.node import BacktestNode
    from nautilus_trader.backtest.node import BacktestRunConfig
    from nautilus_trader.backtest.node import BacktestVenueConfig
    from nautilus_trader.config import ImportableStrategyConfig
    from nautilus_trader.model import QuoteTick
    from nautilus_trader.model.identifiers import Venue

    descriptor = _read_catalog_descriptor(experiment.catalog_uri)
    if mode == "PORTFOLIO":
        _validate_portfolio_frame(experiment)
    if mode == "SEALED" and not descriptor.sealed:
        raise HTTPException(status_code=422, detail="SEALED mode requires a sealed catalog")
    if mode != "SEALED" and descriptor.sealed:
        raise HTTPException(status_code=422, detail="sealed catalog cannot be used outside evaluator mode")
    configured_instrument = str(experiment.strategy.config.get("instrument_id", ""))
    instrument_id = configured_instrument or descriptor.instrument_scope[0]
    instrument, catalog_path = _instrument(experiment.catalog_uri, instrument_id)
    external_run_id = str(uuid4())
    started = _now()
    node: Any | None = None
    try:
        with _strategy_import_root(experiment.strategy):
            strategy = ImportableStrategyConfig(
                strategy_path=experiment.strategy.strategy_path,
                config_path=experiment.strategy.config_path,
                config=_normalize_strategy_config(experiment.strategy, instrument),
            )
            run_config = BacktestRunConfig(
                engine=BacktestEngineConfig(strategies=[strategy]),
                data=[
                    BacktestDataConfig(
                        catalog_path=str(catalog_path),
                        data_cls=QuoteTick,
                        instrument_id=instrument.id,
                    )
                ],
                venues=[
                    BacktestVenueConfig(
                        name="SIM",
                        oms_type="HEDGING",
                        account_type="MARGIN",
                        base_currency="USD",
                        starting_balances=["1_000_000 USD"],
                    )
                ],
                dispose_on_completion=False,
                raise_exception=True,
            )
            node = BacktestNode(configs=[run_config])
            results = node.run()
            if len(results) != 1:
                raise RuntimeError("Nautilus BacktestNode returned an unexpected result count")
            result = results[0]
            config_id = result.run_config_id or run_config.id
            engine = node.get_engine(config_id)
            assert engine is not None
            orders = _records(engine.trader.generate_orders_report())
            fills = _records(engine.trader.generate_fills_report())
            positions = _records(engine.trader.generate_positions_report())
            account = _records(engine.trader.generate_account_report(venue=Venue("SIM")))
            returns = _jsonable(result.stats_returns)
            pnls = _jsonable(result.stats_pnls)
            statistics = {
                "total_events": int(result.total_events),
                "total_orders": int(result.total_orders),
                "total_positions": int(result.total_positions),
                "sharpe_ratio": _named_statistic(result.stats_returns, "sharpe"),
                "max_drawdown": abs(
                    _named_statistic(result.stats_returns, "max", "drawdown")
                ),
                "turnover": float(len(fills)),
                "capacity_envelope": {
                    "max_deployable_capital": 1_000_000.0,
                    "source": "frozen simulated starting balance",
                },
                "summary": _jsonable(result.summary),
                "returns": returns,
                "pnls": pnls,
            }
            return RunEvidence(
                external_run_id=external_run_id,
                state="SUCCEEDED",
                mode=mode,
                runtime_name="NautilusTrader",
                nautilus_version=_runtime_version(),
                contract_version=CONTRACT_VERSION,
                catalog_uri=experiment.catalog_uri,
                strategy_artifact=experiment.strategy.model_dump(mode="json"),
                orders=orders,
                fills=fills,
                positions=positions,
                account=account,
                statistics=statistics,
                started_at=started,
                finished_at=_now(),
            )
    except HTTPException:
        raise
    except Exception as exc:  # noqa: BLE001 - failed runs are returned as evidence
        return RunEvidence(
            external_run_id=external_run_id,
            state="FAILED",
            mode=mode,
            runtime_name="NautilusTrader",
            nautilus_version=_runtime_version(),
            contract_version=CONTRACT_VERSION,
            catalog_uri=experiment.catalog_uri,
            strategy_artifact=experiment.strategy.model_dump(mode="json"),
            statistics={},
            started_at=started,
            finished_at=_now(),
            error_code=type(exc).__name__,
            error_message=str(exc)[-4000:],
        )
    finally:
        if node is not None:
            node.dispose()


def _prepare_isolated_child() -> None:
    """Leave only non-secret runtime bootstrap values in a generated-code child."""
    inherited = {
        name: value
        for name, value in os.environ.items()
        if name in _ISOLATED_ENVIRONMENT_KEYS
    }
    os.environ.clear()
    os.environ.update(inherited)

    def blocked_network(*_: Any, **__: Any) -> Any:
        raise PermissionError("generated strategy network access is disabled")

    class BlockedSocket:
        def __init__(self, *_: Any, **__: Any) -> None:
            raise PermissionError("generated strategy network access is disabled")

    setattr(socket, "socket", BlockedSocket)  # noqa: B010 - intentional child network barrier
    setattr(socket, "create_connection", blocked_network)  # noqa: B010 - intentional child network barrier
    setattr(socket, "getaddrinfo", blocked_network)  # noqa: B010 - intentional child network barrier
    setattr(socket, "socketpair", blocked_network)  # noqa: B010 - intentional child network barrier


def _execute_isolated_child(
    experiment_payload: dict[str, Any],
    mode: RunMode,
    catalog_root: str,
    connection: Any,
) -> None:
    """Execute generated strategy code in a credential-free, network-disabled child."""
    _prepare_isolated_child()
    os.environ["QUAZONAI_NAUTILUS_CATALOG_ROOT"] = catalog_root
    try:
        evidence = _execute_once(ExperimentSpec.model_validate(experiment_payload), mode)
        payload = evidence.model_dump(mode="json")
        if mode == "SEALED":
            for key in ("orders", "fills", "positions", "account"):
                payload.pop(key, None)
        connection.send(payload)
    except BaseException as exc:  # noqa: BLE001 - parent turns child failures into protocol failures
        connection.send({"__error__": type(exc).__name__})
    finally:
        connection.close()


def _execute(experiment: ExperimentSpec, mode: RunMode) -> RunEvidence:
    """Serialize runs and isolate all generated strategy code from the service process."""
    with _EXECUTION_LOCK:
        catalog_path = _catalog_path(experiment.catalog_uri)
        if not catalog_path.is_dir():
            raise HTTPException(status_code=404, detail="catalog does not exist")
        with tempfile.TemporaryDirectory(prefix="quazonai-isolated-catalog-") as root:
            isolated_path = Path(root) / experiment.catalog_uri.removeprefix(_CATALOG_PREFIX)
            shutil.copytree(catalog_path, isolated_path)
            context = multiprocessing.get_context("spawn")
            parent, child = context.Pipe(duplex=False)
            process = context.Process(
                target=_execute_isolated_child,
                args=(experiment.model_dump(mode="json"), mode, root, child),
            )
            process.start()
            child.close()
            received: dict[str, Any] | None = None

            def receive_payload() -> None:
                nonlocal received
                try:
                    received = parent.recv()
                except (EOFError, OSError):
                    received = None

            receiver = threading.Thread(target=receive_payload, daemon=True)
            receiver.start()
            process.join(300)
            if process.is_alive():
                process.terminate()
                process.join(5)
                receiver.join(5)
                parent.close()
                raise HTTPException(status_code=504, detail="strategy execution timed out")
            receiver.join()
            parent.close()
            if received is None:
                raise HTTPException(status_code=502, detail="isolated strategy child returned no evidence")
            payload = received
            if "__error__" in payload:
                raise HTTPException(status_code=502, detail="isolated strategy child failed")
            return RunEvidence.model_validate(payload)


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


def _verify_bundle(data: bytes | BinaryIO) -> dict[str, Any]:
    required = {
        "manifest.json",
        "requirements.lock",
        "strategy/strategy.whl",
        "strategy/strategy-config.json",
        "strategy/actor-config.json",
        "data/requirements.json",
        "data/instrument-scope.json",
        "runtime/nautilus-version.json",
        "runtime/backtest-run-config.json",
        "runtime/venue-config.json",
        "runtime/risk-config.json",
        "runtime/live-node-template.json",
        "validation/fixture-catalog/catalog-descriptor.json",
        "validation/target-portfolio-frame.json",
        "validation/expected-statistics.json",
        "evidence/discovery-summary.json",
        "evidence/sealed-summary.json",
        "evidence/robustness-summary.json",
        "lineage.json",
    }
    try:
        source = BytesIO(data) if isinstance(data, bytes) else data
        with zipfile.ZipFile(source) as bundle:
            names = set(bundle.namelist())
            if any(
                part.casefold() in _FORBIDDEN_BUNDLE_PATH_PARTS
                for name in names
                for part in Path(name).parts
            ):
                return {"valid": False, "errors": ["bundle contains execution reports"]}
            missing = sorted(required - names)
            if missing:
                return {"valid": False, "errors": [f"missing files: {', '.join(missing)}"]}
            manifest = json.loads(bundle.read("manifest.json"))
            runtime = json.loads(bundle.read("runtime/nautilus-version.json"))
            strategy_config = json.loads(bundle.read("strategy/strategy-config.json"))
            data_requirements = json.loads(bundle.read("data/requirements.json"))
            instrument_scope = json.loads(bundle.read("data/instrument-scope.json"))
            fixture_descriptor = json.loads(
                bundle.read("validation/fixture-catalog/catalog-descriptor.json")
            )
            target_frame = json.loads(bundle.read("validation/target-portfolio-frame.json"))
            requirements = bundle.read("requirements.lock").decode("utf-8").splitlines()
            if any(
                name.endswith(".json")
                and _bundle_contains_secret(json.loads(bundle.read(name)))
                for name in names
                if name.endswith(".json")
            ):
                return {"valid": False, "errors": ["bundle contains a secret-bearing field"]}
            if manifest.get("candidate_bundle_contract_version") != _CANDIDATE_BUNDLE_CONTRACT_VERSION:
                return {"valid": False, "errors": ["unsupported Candidate Bundle contract version"]}
            if manifest.get("strategy", {}).get("wheel") != "strategy/strategy.whl":
                return {"valid": False, "errors": ["manifest strategy wheel path is invalid"]}
            if manifest.get("validation", {}).get("target_portfolio_frame") != (
                "validation/target-portfolio-frame.json"
            ):
                return {"valid": False, "errors": ["manifest target frame path is invalid"]}
            if runtime.get("version") != PINNED_NAUTILUS_VERSION:
                return {"valid": False, "errors": ["pinned NautilusTrader version mismatch"]}
            if any(
                not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*==[^\s;,]+", item)
                for item in requirements
                if item
            ) or any(not item for item in requirements) or len(requirements) != len(set(requirements)):
                return {"valid": False, "errors": ["requirements.lock contains unpinned or duplicate dependencies"]}
            if f"nautilus-trader=={PINNED_NAUTILUS_VERSION}" not in requirements:
                return {"valid": False, "errors": ["requirements.lock does not pin NautilusTrader"]}
            wheel_bytes = bundle.read("strategy/strategy.whl")
            with zipfile.ZipFile(BytesIO(wheel_bytes)) as wheel:
                wheel_names = set(wheel.namelist())
                module_path = str(strategy_config["strategy_path"]).partition(":")[0]
                config_path = str(strategy_config["config_path"]).partition(":")[0]
                for module in {module_path, config_path}:
                    expected = module.replace(".", "/") + ".py"
                    package_init = module.replace(".", "/") + "/__init__.py"
                    if expected not in wheel_names and package_init not in wheel_names:
                        return {
                            "valid": False,
                            "errors": [f"strategy wheel cannot import {module}"],
                        }
                with tempfile.TemporaryDirectory(prefix="quazonai-candidate-verify-") as root:
                    wheel.extractall(root)
                    script = (
                        "import importlib, sys\n"
                        "root, strategy_ref, config_ref = sys.argv[1:]\n"
                        "sys.path.insert(0, root)\n"
                        "for ref in (strategy_ref, config_ref):\n"
                        "    module_name, separator, attribute = ref.partition(':')\n"
                        "    if not separator or not attribute:\n"
                        "        raise ValueError('invalid import reference')\n"
                        "    module = importlib.import_module(module_name)\n"
                        "    getattr(module, attribute)\n"
                    )
                    result = subprocess.run(
                        [
                            sys.executable,
                            "-I",
                            "-c",
                            script,
                            root,
                            str(strategy_config["strategy_path"]),
                            str(strategy_config["config_path"]),
                        ],
                        capture_output=True,
                        text=True,
                        timeout=10,
                        env={"PATH": os.environ.get("PATH", "")},
                    )
                    if result.returncode != 0:
                        return {"valid": False, "errors": ["strategy conformance import failed"]}
            catalog_uri = str(data_requirements.get("catalog_uri", ""))
            if not catalog_uri.startswith("catalog://"):
                return {"valid": False, "errors": ["bundle catalog URI is invalid"]}
            if fixture_descriptor.get("catalog_uri") != catalog_uri:
                return {"valid": False, "errors": ["fixture catalog does not match data requirements"]}
            scoped_instruments = instrument_scope.get("instruments")
            if not isinstance(scoped_instruments, list) or not scoped_instruments:
                return {"valid": False, "errors": ["instrument scope fixture is invalid"]}
            scoped_ids = {
                str(item.get("instrument_id"))
                for item in scoped_instruments
                if isinstance(item, dict) and item.get("instrument_id") is not None
            }
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
                target_frame.get("schema_version") != "1"
                or not required_frame_fields.issubset(target_frame)
                or not isinstance(rows, list)
                or not rows
            ):
                return {"valid": False, "errors": ["target portfolio frame is invalid"]}
            total_weight = 0.0
            for row in rows:
                try:
                    weight = float(row["target_weight"]) if isinstance(row, dict) else float("nan")
                except (KeyError, TypeError, ValueError):
                    weight = float("nan")
                if (
                    not isinstance(row, dict)
                    or not isinstance(row.get("instrument_id"), str)
                    or not math.isfinite(weight)
                    or not 0.0 <= weight <= 1.0
                ):
                    return {"valid": False, "errors": ["target portfolio frame contains invalid weights"]}
                total_weight += weight
            if not math.isclose(total_weight, 1.0, rel_tol=0.0, abs_tol=1e-9):
                return {"valid": False, "errors": ["target portfolio frame weights must sum to one"]}
            target_ids = {str(row["instrument_id"]) for row in rows}
            if not target_ids.issubset(scoped_ids):
                return {"valid": False, "errors": ["target frame exceeds the instrument scope fixture"]}
            return {
                "valid": True,
                "runtime_name": "NautilusTrader",
                "nautilus_version": _runtime_version(),
                "contract_version": CONTRACT_VERSION,
                "checked_files": len(names),
                "strategy_imports": [
                    strategy_config["strategy_path"],
                    strategy_config["config_path"],
                ],
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
            supported_modes=["DISCOVERY", "SEALED", "PORTFOLIO"],
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

    @app.post("/v1/runs", response_model=RunEvidence)
    def run(payload: RunInput) -> RunEvidence:
        if payload.mode not in {"DISCOVERY", "SEALED", "PORTFOLIO"}:
            raise HTTPException(status_code=422, detail="unsupported run mode")
        evidence = _execute(payload.experiment, payload.mode)
        with _RUNS_LOCK:
            _RUNS[evidence.external_run_id] = evidence.model_dump(mode="json")
        return evidence

    @app.get("/v1/runs/{external_run_id}", response_model=RunEvidence)
    def get_run(external_run_id: str) -> RunEvidence:
        with _RUNS_LOCK:
            payload = _RUNS.get(external_run_id)
        if payload is None:
            raise HTTPException(status_code=404, detail="run does not exist")
        return RunEvidence.model_validate(payload)

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
