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
import shutil
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
from typing import Any
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
    root = _catalog_root().resolve()
    path = (root / key).resolve()
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
        path.mkdir(parents=True, exist_ok=False)
        catalog = ParquetDataCatalog(path)
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
        _metadata_path(path).write_text(
            descriptor.model_dump_json(indent=2),
            encoding="utf-8",
        )
        return descriptor


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


def _execute(experiment: ExperimentSpec, mode: RunMode) -> RunEvidence:
    """Serialize runs because strategy imports use a temporary module namespace."""
    with _EXECUTION_LOCK:
        return _execute_once(experiment, mode)


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


def _verify_bundle(data: bytes) -> dict[str, Any]:
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
        "validation/expected-orders.json",
        "validation/expected-positions.json",
        "validation/expected-statistics.json",
        "evidence/discovery-summary.json",
        "evidence/sealed-summary.json",
        "evidence/robustness-summary.json",
        "lineage.json",
    }
    try:
        with zipfile.ZipFile(BytesIO(data)) as bundle:
            names = set(bundle.namelist())
            missing = sorted(required - names)
            if missing:
                return {"valid": False, "errors": [f"missing files: {', '.join(missing)}"]}
            manifest = json.loads(bundle.read("manifest.json"))
            runtime = json.loads(bundle.read("runtime/nautilus-version.json"))
            strategy_config = json.loads(bundle.read("strategy/strategy-config.json"))
            data_requirements = json.loads(bundle.read("data/requirements.json"))
            requirements = bundle.read("requirements.lock").decode("utf-8").splitlines()
            if _bundle_contains_secret(manifest) or _bundle_contains_secret(strategy_config):
                return {"valid": False, "errors": ["bundle contains a secret-bearing field"]}
            if runtime.get("version") != PINNED_NAUTILUS_VERSION:
                return {"valid": False, "errors": ["pinned NautilusTrader version mismatch"]}
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
            if not str(data_requirements.get("catalog_uri", "")).startswith("catalog://"):
                return {"valid": False, "errors": ["bundle catalog URI is invalid"]}
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
        data = await bundle.read()
        if len(data) > 256 * 1024 * 1024:
            raise HTTPException(status_code=413, detail="Candidate Bundle exceeds 256 MiB")
        result = _verify_bundle(data)
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
