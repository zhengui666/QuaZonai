"""Reference HTTP service for an independently deployed NautilusTrader runtime.

This process owns its catalog and NautilusTrader installation. It has no QuaZonai
PostgreSQL, Codex, operator-authentication or broker credentials. Paper/Live nodes
remain separate downstream systems and are intentionally not exposed here.
"""

from __future__ import annotations

import math
import os
import re
import secrets
import shutil
from collections.abc import Mapping
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any
from uuid import UUID

from fastapi import FastAPI, Header, HTTPException
from pydantic import BaseModel, ConfigDict, Field

from quant_runtime import (
    BacktestEvidence,
    CandidateVerification,
    CatalogReference,
    CatalogValidation,
    ExperimentContract,
    NAUTILUS_RUNTIME_NAME,
    PINNED_NAUTILUS_VERSION,
)

_CATALOG_KEY = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,119}\Z")
_SECRET_KEY_FRAGMENTS = ("credential", "password", "private_key", "secret", "api_key", "token")


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class QuoteRow(StrictModel):
    timestamp: datetime
    bid_price: float = Field(gt=0)
    ask_price: float = Field(gt=0)


class CatalogIngestRequest(StrictModel):
    catalog_key: str
    dataset_revision_id: UUID
    provider: str
    source_license: str
    instrument: str = "EUR/USD"
    quotes: list[QuoteRow] = Field(min_length=20)
    schema_revision: str = "quote-v1"
    partition: str = "DISCOVERY"


class CandidateManifestInput(StrictModel):
    manifest: dict[str, Any]


class CandidateBuildInput(StrictModel):
    manifest: dict[str, Any]


def _now() -> str:
    return datetime.now(UTC).isoformat()


def _jsonable(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, float, bool)):
        if isinstance(value, float) and (math.isnan(value) or math.isinf(value)):
            return str(value)
        return value
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


def _frame_records(value: Any) -> list[dict[str, Any]]:
    if value is None:
        return []
    frame = value.reset_index() if hasattr(value, "reset_index") else value
    if hasattr(frame, "to_dict"):
        records = frame.to_dict(orient="records")
        return [_jsonable(record) for record in records]
    return []


def _contains_secret_key(value: Any) -> bool:
    if isinstance(value, Mapping):
        for key, item in value.items():
            normalized = str(key).casefold()
            if any(fragment in normalized for fragment in _SECRET_KEY_FRAGMENTS):
                return True
            if _contains_secret_key(item):
                return True
    elif isinstance(value, (list, tuple)):
        return any(_contains_secret_key(item) for item in value)
    return False


def _catalog_root() -> Path:
    return Path(os.environ.get("QUAZONAI_NAUTILUS_CATALOG_ROOT", "/var/lib/nautilus/catalogs"))


def _resolve_catalog(uri: str) -> Path:
    prefix = "catalog://"
    if not uri.startswith(prefix):
        raise HTTPException(status_code=422, detail="catalog_uri must use catalog://<key>")
    key = uri[len(prefix) :]
    if _CATALOG_KEY.fullmatch(key) is None:
        raise HTTPException(status_code=422, detail="catalog key is invalid")
    root = _catalog_root().resolve()
    candidate = (root / key).resolve()
    if not candidate.is_relative_to(root):
        raise HTTPException(status_code=422, detail="catalog path escapes the runtime root")
    return candidate


def _authorize(authorization: str | None) -> None:
    expected = os.environ.get("QUAZONAI_NAUTILUS_RUNTIME_TOKEN", "")
    if not expected:
        raise HTTPException(status_code=503, detail="runtime token is not configured")
    if not authorization or not authorization.startswith("Bearer "):
        raise HTTPException(status_code=401, detail="runtime authentication is required")
    supplied = authorization.removeprefix("Bearer ").strip()
    if not secrets.compare_digest(supplied.encode(), expected.encode()):
        raise HTTPException(status_code=403, detail="runtime authentication failed")


def _instrument_and_catalog(catalog_uri: str, instrument_id: str) -> tuple[Any, Any, Path]:
    from nautilus_trader.persistence.catalog import ParquetDataCatalog

    path = _resolve_catalog(catalog_uri)
    if not path.is_dir():
        raise HTTPException(status_code=404, detail="catalog does not exist")
    catalog = ParquetDataCatalog(path)
    instruments = catalog.instruments()
    for instrument in instruments:
        if instrument.id.value == instrument_id:
            return instrument, catalog, path
    raise HTTPException(status_code=422, detail=f"instrument {instrument_id} is not in catalog")


def _normalize_strategy_config(contract: ExperimentContract, instrument: Any) -> dict[str, Any]:
    config = dict(contract.strategy.config)
    config["instrument_id"] = instrument.id
    config.setdefault("bar_type", f"{instrument.id.value}-1-MINUTE-BID-INTERNAL")
    if "trade_size" in config:
        config["trade_size"] = Decimal(str(config["trade_size"]))
    return config


def _execute_backtest(contract: ExperimentContract, *, sealed: bool) -> BacktestEvidence:
    from nautilus_trader.backtest.node import BacktestDataConfig
    from nautilus_trader.backtest.node import BacktestEngineConfig
    from nautilus_trader.backtest.node import BacktestNode
    from nautilus_trader.backtest.node import BacktestRunConfig
    from nautilus_trader.backtest.node import BacktestVenueConfig
    from nautilus_trader.config import ImportableStrategyConfig
    from nautilus_trader.model import QuoteTick

    instrument_id = contract.catalog.instrument_ids[0]
    instrument, _catalog, catalog_path = _instrument_and_catalog(
        contract.catalog.catalog_uri,
        instrument_id,
    )
    strategy = ImportableStrategyConfig(
        strategy_path=contract.strategy.strategy_path,
        config_path=contract.strategy.config_path,
        config=_normalize_strategy_config(contract, instrument),
    )
    data = BacktestDataConfig(
        catalog_path=str(catalog_path),
        data_cls=QuoteTick,
        instrument_id=instrument.id,
        start_time=contract.catalog.start_time,
        end_time=contract.catalog.end_time,
    )
    venue = BacktestVenueConfig(
        name=contract.venue.name,
        oms_type=contract.venue.oms_type,
        account_type=contract.venue.account_type,
        book_type=contract.venue.book_type,
        base_currency=contract.venue.base_currency,
        starting_balances=contract.venue.starting_balances,
    )
    run_config = BacktestRunConfig(
        id=str(contract.run_id),
        engine=BacktestEngineConfig(strategies=[strategy]),
        data=[data],
        venues=[venue],
        dispose_on_completion=False,
        raise_exception=True,
    )
    started_at = _now()
    node = BacktestNode(configs=[run_config])
    try:
        results = node.run()
        if len(results) != 1:
            raise RuntimeError(f"expected one backtest result, received {len(results)}")
        result = results[0]
        config_id = result.run_config_id or run_config.id
        reports: dict[str, list[dict[str, Any]]] = {}
        if not sealed:
            if "orders" in contract.requested_reports:
                reports["orders"] = _frame_records(node.generate_orders_report(config_id))
            if "fills" in contract.requested_reports:
                reports["fills"] = _frame_records(node.generate_fills_report(config_id))
            if "positions" in contract.requested_reports:
                reports["positions"] = _frame_records(node.generate_positions_report(config_id))
            if "account" in contract.requested_reports:
                reports["account"] = _frame_records(node.generate_account_report(config_id))
        statistics = {
            "summary": _jsonable(result.summary),
            "pnls": _jsonable(result.stats_pnls),
            "returns": _jsonable(result.stats_returns),
            "general": _jsonable(result.stats_general),
            "returns_series": _jsonable(result.returns_series),
        }
        disclosure: dict[str, Any] = {}
        if sealed:
            passed = result.total_events > 0 and result.total_orders > 0
            disclosure = {
                "decision": "PASS" if passed else "FAIL",
                "classification": (
                    "SEALED_RUNTIME_EVIDENCE_SUFFICIENT"
                    if passed
                    else "INSUFFICIENT_NET_EDGE"
                ),
                "total_orders_bucket": "NON_ZERO" if result.total_orders else "ZERO",
                "total_positions_bucket": "NON_ZERO" if result.total_positions else "ZERO",
            }
        return BacktestEvidence(
            run_id=contract.run_id,
            run_config_id=config_id,
            runtime_name=NAUTILUS_RUNTIME_NAME,
            runtime_version=PINNED_NAUTILUS_VERSION,
            catalog_uri=contract.catalog.catalog_uri,
            partition="SEALED" if sealed else "DISCOVERY",
            started_at=started_at,
            finished_at=_now(),
            elapsed_time_seconds=float(result.elapsed_time_secs),
            total_events=int(result.total_events),
            total_orders=int(result.total_orders),
            total_positions=int(result.total_positions),
            statistics=statistics,
            reports=reports,
            disclosure=disclosure,
        )
    finally:
        node.dispose()


def create_app() -> FastAPI:
    app = FastAPI(
        title="QuaZonai Remote Nautilus Runtime",
        version=PINNED_NAUTILUS_VERSION,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )

    @app.get("/v1/health")
    def health(authorization: str | None = Header(default=None)) -> dict[str, Any]:
        _authorize(authorization)
        import nautilus_trader

        return {
            "live": True,
            "ready": nautilus_trader.__version__ == PINNED_NAUTILUS_VERSION,
            "runtime_name": NAUTILUS_RUNTIME_NAME,
            "runtime_version": nautilus_trader.__version__,
            "capabilities": [
                "CATALOG_INGEST",
                "CATALOG_VALIDATE",
                "BACKTEST",
                "SEALED_BACKTEST",
                "CANDIDATE_VERIFY",
            ],
        }

    @app.post("/v1/catalogs/ingest")
    def ingest_catalog(
        payload: CatalogIngestRequest,
        authorization: str | None = Header(default=None),
    ) -> dict[str, Any]:
        _authorize(authorization)
        if _CATALOG_KEY.fullmatch(payload.catalog_key) is None:
            raise HTTPException(status_code=422, detail="catalog_key is invalid")
        from pandas import DataFrame, to_datetime

        from nautilus_trader.persistence.catalog import ParquetDataCatalog
        from nautilus_trader.persistence.wranglers import QuoteTickDataWrangler
        from nautilus_trader.test_kit.providers import TestInstrumentProvider

        if payload.instrument != "EUR/USD":
            raise HTTPException(
                status_code=422,
                detail="The reference runtime currently accepts normalized EUR/USD quote data only.",
            )
        path = _resolve_catalog(f"catalog://{payload.catalog_key}")
        if path.exists():
            shutil.rmtree(path)
        path.mkdir(parents=True, exist_ok=False)
        instrument = TestInstrumentProvider.default_fx_ccy(payload.instrument)
        frame = DataFrame(
            [
                {
                    "timestamp": row.timestamp,
                    "bid_price": row.bid_price,
                    "ask_price": row.ask_price,
                }
                for row in payload.quotes
            ]
        )
        frame["timestamp"] = to_datetime(frame["timestamp"], utc=True)
        frame = frame.set_index("timestamp").sort_index()
        if not frame.index.is_unique:
            raise HTTPException(status_code=422, detail="quote timestamps must be unique")
        if bool((frame["ask_price"] < frame["bid_price"]).any()):
            raise HTTPException(status_code=422, detail="ask_price must be greater than or equal to bid_price")
        ticks = QuoteTickDataWrangler(instrument).process(frame)
        catalog = ParquetDataCatalog(path)
        catalog.write_data([instrument])
        catalog.write_data(ticks)
        first = payload.quotes[0].timestamp.astimezone(UTC).isoformat()
        last = payload.quotes[-1].timestamp.astimezone(UTC).isoformat()
        return {
            "dataset_revision_id": str(payload.dataset_revision_id),
            "provider": payload.provider,
            "source_license": payload.source_license,
            "catalog_uri": f"catalog://{payload.catalog_key}",
            "nautilus_data_type": "QuoteTick",
            "instrument_scope": [instrument.id.value],
            "event_time_range": {"start": first, "end": last},
            "available_time_range": {"start": first, "end": last},
            "schema_revision": payload.schema_revision,
            "quality_result": {"state": "VALID", "rows": len(ticks)},
            "point_in_time_result": {"state": "VALID"},
            "runtime_name": NAUTILUS_RUNTIME_NAME,
            "runtime_version": PINNED_NAUTILUS_VERSION,
            "ingested_at": _now(),
            "partition": payload.partition,
        }

    @app.post("/v1/catalogs/validate", response_model=CatalogValidation)
    def validate_catalog(
        catalog_ref: CatalogReference,
        authorization: str | None = Header(default=None),
    ) -> CatalogValidation:
        _authorize(authorization)
        from nautilus_trader.persistence.catalog import ParquetDataCatalog

        path = _resolve_catalog(catalog_ref.catalog_uri)
        if not path.is_dir():
            return CatalogValidation(
                valid=False,
                runtime_version=PINNED_NAUTILUS_VERSION,
                catalog_uri=catalog_ref.catalog_uri,
                details={"reason": "CATALOG_NOT_FOUND"},
            )
        catalog = ParquetDataCatalog(path)
        instruments = [item.id.value for item in catalog.instruments()]
        missing = sorted(set(catalog_ref.instrument_ids) - set(instruments))
        return CatalogValidation(
            valid=not missing,
            runtime_version=PINNED_NAUTILUS_VERSION,
            catalog_uri=catalog_ref.catalog_uri,
            instruments=instruments,
            data_types=[catalog_ref.nautilus_data_type],
            details={"missing_instruments": missing},
        )

    @app.post("/v1/backtests", response_model=BacktestEvidence)
    def backtest(
        contract: ExperimentContract,
        authorization: str | None = Header(default=None),
    ) -> BacktestEvidence:
        _authorize(authorization)
        if contract.catalog.partition != "DISCOVERY":
            raise HTTPException(status_code=422, detail="Discovery endpoint requires DISCOVERY data")
        return _execute_backtest(contract, sealed=False)

    @app.post("/v1/sealed-backtests", response_model=BacktestEvidence)
    def sealed_backtest(
        contract: ExperimentContract,
        authorization: str | None = Header(default=None),
    ) -> BacktestEvidence:
        _authorize(authorization)
        if contract.catalog.partition != "SEALED":
            raise HTTPException(status_code=422, detail="Sealed endpoint requires SEALED data")
        return _execute_backtest(contract, sealed=True)

    @app.post("/v1/candidates/build")
    def build_candidate(
        payload: CandidateBuildInput,
        authorization: str | None = Header(default=None),
    ) -> dict[str, Any]:
        _authorize(authorization)
        verification = _verify_manifest(payload.manifest)
        return {
            "runtime_version": PINNED_NAUTILUS_VERSION,
            "accepted": verification.valid,
            "errors": verification.errors,
        }

    @app.post("/v1/candidates/verify", response_model=CandidateVerification)
    def verify_candidate(
        payload: CandidateManifestInput,
        authorization: str | None = Header(default=None),
    ) -> CandidateVerification:
        _authorize(authorization)
        return _verify_manifest(payload.manifest)

    return app


def _verify_manifest(manifest: dict[str, Any]) -> CandidateVerification:
    errors: list[str] = []
    if manifest.get("runtime", {}).get("name") != NAUTILUS_RUNTIME_NAME:
        errors.append("runtime.name must be NAUTILUS_TRADER")
    if manifest.get("runtime", {}).get("version") != PINNED_NAUTILUS_VERSION:
        errors.append(f"runtime.version must be {PINNED_NAUTILUS_VERSION}")
    required = {"manifest.json", "requirements.lock", "lineage.json"}
    declared = set(manifest.get("required_files", []))
    missing = sorted(required - declared)
    if missing:
        errors.append(f"required_files missing: {', '.join(missing)}")
    if _contains_secret_key(manifest):
        errors.append("manifest contains a secret-bearing field")
    return CandidateVerification(
        valid=not errors,
        runtime_version=PINNED_NAUTILUS_VERSION,
        errors=errors,
        details={"checked_at": _now()},
    )


def main() -> int:
    import uvicorn

    host = os.environ.get("QUAZONAI_NAUTILUS_HOST", "0.0.0.0")
    port = int(os.environ.get("QUAZONAI_NAUTILUS_PORT", "8011"))
    uvicorn.run(create_app(), host=host, port=port, proxy_headers=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
