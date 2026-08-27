from __future__ import annotations

from pathlib import Path

REMOTE_RUNTIME = r'''"""Reference service for an independently deployed NautilusTrader runtime.

The process owns its NautilusTrader installation and catalog storage. It has no
QuaZonai PostgreSQL, Codex, operator-authentication, broker or exchange credentials.
Paper and Live nodes remain separate downstream systems.
"""

from __future__ import annotations

import base64
import json
import math
import os
import re
import secrets
import shutil
import sys
import tempfile
import zipfile
from collections.abc import Iterator, Mapping
from contextlib import contextmanager
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any, Literal
from uuid import UUID, uuid4

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
    StrategyArtifact,
)

_CATALOG_KEY = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,119}\Z")
_SECRET_KEY_FRAGMENTS = (
    "credential",
    "password",
    "private_key",
    "secret",
    "api_key",
    "token",
)
_RUNTIME_METADATA_FILE = "quazonai-runtime-binding.json"


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
    partition: Literal["DISCOVERY", "SEALED"] = "DISCOVERY"


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
                if not (normalized.startswith("contains_") and item is False):
                    return True
            if _contains_secret_key(item):
                return True
    elif isinstance(value, (list, tuple)):
        return any(_contains_secret_key(item) for item in value)
    return False


def _catalog_root() -> Path:
    return Path(
        os.environ.get(
            "QUAZONAI_NAUTILUS_CATALOG_ROOT",
            "/var/lib/nautilus/catalogs",
        )
    )


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


def _read_ingest_receipt(path: Path) -> dict[str, Any] | None:
    metadata_path = path / _RUNTIME_METADATA_FILE
    if not metadata_path.is_file():
        return None
    try:
        value = json.loads(metadata_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise HTTPException(
            status_code=409,
            detail="catalog exists without a valid runtime ingestion receipt",
        ) from exc
    return value if isinstance(value, dict) else None


def _instrument_and_catalog(catalog_uri: str, instrument_id: str) -> tuple[Any, Any, Path]:
    from nautilus_trader.persistence.catalog import ParquetDataCatalog

    path = _resolve_catalog(catalog_uri)
    if not path.is_dir():
        raise HTTPException(status_code=404, detail="catalog does not exist")
    catalog = ParquetDataCatalog(path)
    for instrument in catalog.instruments():
        if instrument.id.value == instrument_id:
            return instrument, catalog, path
    raise HTTPException(
        status_code=422,
        detail=f"instrument {instrument_id} is not in catalog",
    )


def _normalize_strategy_config(
    contract: ExperimentContract,
    instrument: Any,
) -> dict[str, Any]:
    config = dict(contract.strategy.config)
    config["instrument_id"] = instrument.id
    config.setdefault(
        "bar_type",
        f"{instrument.id.value}-1-MINUTE-BID-INTERNAL",
    )
    if "trade_size" in config:
        config["trade_size"] = Decimal(str(config["trade_size"]))
    return config


@contextmanager
def _strategy_import_environment(strategy: StrategyArtifact) -> Iterator[None]:
    unsupported = [
        item
        for item in strategy.requirements
        if item != f"nautilus_trader=={PINNED_NAUTILUS_VERSION}"
    ]
    if unsupported:
        raise HTTPException(
            status_code=422,
            detail="runtime dependency installation is disabled; bundle dependencies in the wheel",
        )
    encoded = strategy.wheel_base64
    if encoded is None:
        if not strategy.strategy_path.startswith("nautilus_trader."):
            raise HTTPException(
                status_code=422,
                detail="custom strategy_path requires a frozen strategy wheel",
            )
        yield
        return

    try:
        payload = base64.b64decode(encoded, validate=True)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail="strategy wheel is not valid base64") from exc
    root = Path(tempfile.mkdtemp(prefix="quazonai-nautilus-strategy-"))
    wheel = root / "strategy.whl"
    wheel.write_bytes(payload)
    try:
        with zipfile.ZipFile(wheel) as archive:
            if not any(name.endswith(".dist-info/WHEEL") for name in archive.namelist()):
                raise HTTPException(status_code=422, detail="strategy artifact is not a wheel")
    except zipfile.BadZipFile as exc:
        raise HTTPException(status_code=422, detail="strategy artifact is not a wheel") from exc
    sys.path.insert(0, str(wheel))
    try:
        yield
    finally:
        if sys.path and sys.path[0] == str(wheel):
            sys.path.pop(0)
        shutil.rmtree(root, ignore_errors=True)


def _execute_backtest(
    contract: ExperimentContract,
    *,
    sealed: bool,
) -> BacktestEvidence:
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
    with _strategy_import_environment(contract.strategy):
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
                raise RuntimeError(
                    f"expected one backtest result, received {len(results)}"
                )
            result = results[0]
            config_id = result.run_config_id or run_config.id
            if sealed:
                passed = result.total_events > 0 and result.total_orders > 0
                return BacktestEvidence(
                    run_id=contract.run_id,
                    run_config_id=config_id,
                    runtime_name=NAUTILUS_RUNTIME_NAME,
                    runtime_version=PINNED_NAUTILUS_VERSION,
                    catalog_uri=contract.catalog.catalog_uri,
                    partition="SEALED",
                    started_at=started_at,
                    finished_at=_now(),
                    elapsed_time_seconds=0.0,
                    total_events=0,
                    total_orders=0,
                    total_positions=0,
                    statistics={},
                    reports={},
                    disclosure={
                        "decision": "PASS" if passed else "FAIL",
                        "classification": (
                            "SEALED_RUNTIME_EVIDENCE_SUFFICIENT"
                            if passed
                            else "INSUFFICIENT_NET_EDGE"
                        ),
                        "total_orders_bucket": (
                            "NON_ZERO" if result.total_orders else "ZERO"
                        ),
                        "total_positions_bucket": (
                            "NON_ZERO" if result.total_positions else "ZERO"
                        ),
                    },
                )

            reports: dict[str, list[dict[str, Any]]] = {}
            if "orders" in contract.requested_reports:
                reports["orders"] = _frame_records(
                    node.generate_orders_report(config_id)
                )
            if "fills" in contract.requested_reports:
                reports["fills"] = _frame_records(
                    node.generate_fills_report(config_id)
                )
            if "positions" in contract.requested_reports:
                reports["positions"] = _frame_records(
                    node.generate_positions_report(config_id)
                )
            if "account" in contract.requested_reports:
                reports["account"] = _frame_records(
                    node.generate_account_report(config_id)
                )
            statistics = {
                "summary": _jsonable(result.summary),
                "pnls": _jsonable(result.stats_pnls),
                "returns": _jsonable(result.stats_returns),
                "general": _jsonable(result.stats_general),
                "returns_series": _jsonable(result.returns_series),
            }
            return BacktestEvidence(
                run_id=contract.run_id,
                run_config_id=config_id,
                runtime_name=NAUTILUS_RUNTIME_NAME,
                runtime_version=PINNED_NAUTILUS_VERSION,
                catalog_uri=contract.catalog.catalog_uri,
                partition="DISCOVERY",
                started_at=started_at,
                finished_at=_now(),
                elapsed_time_seconds=float(result.elapsed_time_secs),
                total_events=int(result.total_events),
                total_orders=int(result.total_orders),
                total_positions=int(result.total_positions),
                statistics=statistics,
                reports=reports,
                disclosure={},
            )
        finally:
            node.dispose()


def _ingest_catalog(
    payload: CatalogIngestRequest,
    *,
    idempotency_key: str,
) -> dict[str, Any]:
    if _CATALOG_KEY.fullmatch(payload.catalog_key) is None:
        raise HTTPException(status_code=422, detail="catalog_key is invalid")
    if payload.instrument != "EUR/USD":
        raise HTTPException(
            status_code=422,
            detail="The reference runtime accepts normalized EUR/USD quotes only.",
        )
    path = _resolve_catalog(f"catalog://{payload.catalog_key}")
    if path.exists():
        receipt = _read_ingest_receipt(path)
        if receipt is not None and receipt.get("idempotency_key") == idempotency_key:
            result = receipt.get("result")
            if isinstance(result, dict):
                return result
        raise HTTPException(
            status_code=409,
            detail="catalog key already belongs to another immutable ingestion",
        )

    from pandas import DataFrame, to_datetime

    from nautilus_trader.persistence.catalog import ParquetDataCatalog
    from nautilus_trader.persistence.wranglers import QuoteTickDataWrangler
    from nautilus_trader.test_kit.providers import TestInstrumentProvider

    root = _catalog_root().resolve()
    root.mkdir(parents=True, exist_ok=True)
    staging = root / f".{payload.catalog_key}.{uuid4().hex}.staging"
    staging.mkdir(parents=False, exist_ok=False)
    try:
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
            raise HTTPException(
                status_code=422,
                detail="ask_price must be greater than or equal to bid_price",
            )
        ticks = QuoteTickDataWrangler(instrument).process(frame)
        catalog = ParquetDataCatalog(staging)
        catalog.write_data([instrument])
        catalog.write_data(ticks)
        first = frame.index[0].isoformat()
        last = frame.index[-1].isoformat()
        result = {
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
        receipt = {
            "idempotency_key": idempotency_key,
            "dataset_revision_id": str(payload.dataset_revision_id),
            "partition": payload.partition,
            "result": result,
        }
        (staging / _RUNTIME_METADATA_FILE).write_text(
            json.dumps(receipt, ensure_ascii=False, indent=2, sort_keys=True),
            encoding="utf-8",
        )
        os.replace(staging, path)
        return result
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise


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
            "process_boundary": "REMOTE_SERVICE",
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
        idempotency_key: str = Header(
            alias="Idempotency-Key",
            min_length=1,
            max_length=200,
        ),
    ) -> dict[str, Any]:
        _authorize(authorization)
        return _ingest_catalog(payload, idempotency_key=idempotency_key)

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
            raise HTTPException(
                status_code=422,
                detail="Discovery endpoint requires DISCOVERY data",
            )
        return _execute_backtest(contract, sealed=False)

    @app.post("/v1/sealed-backtests", response_model=BacktestEvidence)
    def sealed_backtest(
        contract: ExperimentContract,
        authorization: str | None = Header(default=None),
    ) -> BacktestEvidence:
        _authorize(authorization)
        if contract.catalog.partition != "SEALED":
            raise HTTPException(
                status_code=422,
                detail="Sealed endpoint requires SEALED data",
            )
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


def main() -> int:
    import uvicorn

    host = os.environ.get("QUAZONAI_NAUTILUS_HOST", "0.0.0.0")
    port = int(os.environ.get("QUAZONAI_NAUTILUS_PORT", "8011"))
    uvicorn.run(create_app(), host=host, port=port, proxy_headers=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
'''

QUANT_EXPERIMENTS = r'''"""Execute Discovery and Sealed experiments through remote Nautilus runtimes."""

from __future__ import annotations

import argparse
import inspect
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session, sessionmaker

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    DatasetRevision,
    DownstreamSystem,
    Job,
    NautilusCatalogBinding,
    PortfolioCandidate,
    PortfolioMandate,
    PortfolioProgram,
    QuantExperiment,
    ResearchMission,
    ResearchProgram,
    SearchLedgerEntry,
    SealedEvaluation,
)
from db.session import create_database_engine, create_session_factory
from errors import QfError
from events import append_event
from jobs import enqueue_job
from quant_runtime import (
    BacktestEvidence,
    ExperimentContract,
    PINNED_NAUTILUS_VERSION,
    RemoteNautilusQuantRuntime,
)
from settings import Settings

SessionFactory = sessionmaker[Session]


@dataclass(frozen=True, slots=True)
class ExperimentLease:
    experiment_id: UUID
    zone: Literal["DISCOVERY", "SEALED"]
    contract: ExperimentContract


def _now() -> datetime:
    return datetime.now(UTC)


def _ledger(
    session: Session,
    *,
    experiment: QuantExperiment,
    outcome: str,
    evidence_summary: dict[str, Any] | None = None,
) -> SearchLedgerEntry:
    entry = SearchLedgerEntry(
        program_id=experiment.program_id,
        mission_id=experiment.mission_id,
        experiment_id=experiment.id,
        attempt_kind=f"NAUTILUS_{experiment.zone}_BACKTEST",
        outcome=outcome,
        hypothesis_key=str(
            experiment.strategy_artifact.get("strategy_path") or "unknown"
        ),
        parameters=dict(experiment.contract_json.get("parameters") or {}),
        evidence_summary=evidence_summary or {},
        created_at=_now(),
    )
    session.add(entry)
    return entry


def _enqueue(session: Session, *, kind: str, resource_id: UUID) -> Job:
    parameters = inspect.signature(enqueue_job).parameters
    kwargs: dict[str, Any] = {"kind": kind, "resource_id": resource_id}
    if "resource_type" in parameters:
        kwargs["resource_type"] = "quant_experiment"
    if "payload" in parameters:
        kwargs["payload"] = {}
    return enqueue_job(session, **kwargs)


def submit_experiment(
    session: Session,
    *,
    mission_id: UUID,
    contract: ExperimentContract,
) -> QuantExperiment:
    """Validate a Mission contract and append a durable Discovery experiment."""
    mission = session.get(ResearchMission, mission_id)
    if mission is None:
        raise QfError("MISSION_NOT_FOUND", "Research Mission does not exist.", 404)
    if mission.state not in {"READY", "RUNNING", "SUCCEEDED"}:
        raise QfError(
            "MISSION_STATE_CONFLICT",
            "The Mission cannot submit an experiment in its current state.",
            409,
            {"state": mission.state},
        )
    if mission.type not in {
        "ALPHA_DISCOVERY",
        "ROBUSTNESS",
        "PROMOTION_REVIEW",
        "PORTFOLIO_ASSEMBLY",
    }:
        raise QfError(
            "MISSION_CAPABILITY_DENIED",
            "This Mission type cannot submit a Nautilus experiment.",
            403,
            {"mission_type": mission.type},
        )
    dataset = session.get(DatasetRevision, contract.catalog.dataset_revision_id)
    binding = session.scalar(
        select(NautilusCatalogBinding).where(
            NautilusCatalogBinding.dataset_revision_id
            == contract.catalog.dataset_revision_id
        )
    )
    if dataset is None or binding is None:
        raise QfError(
            "NAUTILUS_CATALOG_NOT_GOVERNED",
            "A governed Dataset Revision and Nautilus catalog binding are required.",
            422,
        )
    if dataset.quality_state != "VALID" or dataset.point_in_time_state != "VALID":
        raise QfError(
            "DATASET_NOT_RESEARCH_READY",
            "The Dataset Revision has not passed quality and point-in-time validation.",
            422,
        )
    if dataset.partition != "DISCOVERY":
        raise QfError(
            "DISCOVERY_DATASET_REQUIRED",
            "Mission experiments must begin with a Discovery Dataset Revision.",
            422,
            {"partition": dataset.partition},
        )
    if binding.runtime_version != PINNED_NAUTILUS_VERSION:
        raise QfError(
            "NAUTILUS_RUNTIME_VERSION_MISMATCH",
            "Catalog binding does not match the pinned runtime.",
            409,
            {
                "expected": PINNED_NAUTILUS_VERSION,
                "actual": binding.runtime_version,
            },
        )
    allowed = set(binding.instrument_scope)
    requested = set(contract.catalog.instrument_ids)
    if not requested or not requested.issubset(allowed):
        raise QfError(
            "NAUTILUS_CATALOG_SCOPE_VIOLATION",
            "Experiment scope exceeds the governed catalog binding.",
            403,
            {"allowed": sorted(allowed), "requested": sorted(requested)},
        )
    normalized = contract.model_copy(
        update={
            "catalog": contract.catalog.model_copy(
                update={
                    "catalog_uri": binding.catalog_uri,
                    "nautilus_data_type": binding.nautilus_data_type,
                    "partition": "DISCOVERY",
                }
            )
        }
    )
    contract_json = normalized.model_dump(mode="json")
    existing = session.scalar(
        select(QuantExperiment).where(
            QuantExperiment.mission_id == mission.id,
            QuantExperiment.zone == "DISCOVERY",
            QuantExperiment.contract_json == contract_json,
        )
    )
    if existing is not None:
        return existing
    experiment = QuantExperiment(
        mission_id=mission.id,
        program_id=mission.program_id,
        dataset_revision_id=dataset.id,
        zone="DISCOVERY",
        state="READY",
        runtime_name="NAUTILUS_TRADER",
        runtime_version=PINNED_NAUTILUS_VERSION,
        strategy_artifact=normalized.strategy.model_dump(mode="json"),
        contract_json=contract_json,
    )
    session.add(experiment)
    session.flush()
    _ledger(session, experiment=experiment, outcome="QUEUED")
    _enqueue(
        session,
        kind="NAUTILUS_DISCOVERY_BACKTEST",
        resource_id=experiment.id,
    )
    append_event(
        session,
        kind="QUANT_EXPERIMENT_QUEUED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=mission.program_id,
        payload={
            "mission_id": str(mission.id),
            "experiment_id": str(experiment.id),
            "zone": experiment.zone,
        },
    )
    return experiment


def submit_contract_from_workspace(
    settings: Settings,
    *,
    mission_id: UUID,
    workspace: Path,
) -> UUID | None:
    """Admit a Codex-authored contract without exposing DB or runtime credentials."""
    path = workspace / "experiment-contract.json"
    if not path.is_file():
        return None
    try:
        contract = ExperimentContract.model_validate_json(
            path.read_text(encoding="utf-8")
        )
    except (OSError, ValueError) as exc:
        raise QfError(
            "EXPERIMENT_CONTRACT_INVALID",
            "experiment-contract.json violates the governed Nautilus contract.",
            422,
        ) from exc
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            return submit_experiment(
                session,
                mission_id=mission_id,
                contract=contract,
            ).id
    finally:
        engine.dispose()


def _runtime(
    zone: Literal["DISCOVERY", "SEALED"],
) -> RemoteNautilusQuantRuntime:
    return RemoteNautilusQuantRuntime.from_env(zone)


def _evidence_summary(evidence: BacktestEvidence) -> dict[str, Any]:
    return {
        "runtime_name": evidence.runtime_name,
        "runtime_version": evidence.runtime_version,
        "run_id": str(evidence.run_id),
        "total_events": evidence.total_events,
        "total_orders": evidence.total_orders,
        "total_positions": evidence.total_positions,
        "statistics": evidence.statistics,
        "disclosure": evidence.disclosure,
    }


def _same_universe(first: DatasetRevision, second: DatasetRevision) -> bool:
    if first.universe_version_id is not None or second.universe_version_id is not None:
        return first.universe_version_id == second.universe_version_id
    return first.universe_name == second.universe_name


def _create_sealed_experiment(
    session: Session,
    discovery: QuantExperiment,
) -> QuantExperiment | None:
    existing = session.scalar(
        select(QuantExperiment).where(
            QuantExperiment.parent_experiment_id == discovery.id,
            QuantExperiment.zone == "SEALED",
        )
    )
    if existing is not None:
        return existing
    discovery_dataset = session.get(DatasetRevision, discovery.dataset_revision_id)
    if discovery_dataset is None:
        raise QfError(
            "DATASET_REVISION_NOT_FOUND",
            "Discovery Dataset Revision no longer exists.",
            500,
        )
    contract = ExperimentContract.model_validate(discovery.contract_json)
    candidates = session.execute(
        select(NautilusCatalogBinding, DatasetRevision)
        .join(
            DatasetRevision,
            DatasetRevision.id == NautilusCatalogBinding.dataset_revision_id,
        )
        .where(
            DatasetRevision.partition == "SEALED",
            DatasetRevision.quality_state == "VALID",
            DatasetRevision.point_in_time_state == "VALID",
            NautilusCatalogBinding.runtime_version == PINNED_NAUTILUS_VERSION,
        )
        .order_by(DatasetRevision.created_at.desc())
    ).all()
    requested = set(contract.catalog.instrument_ids)
    sealed_binding: NautilusCatalogBinding | None = None
    sealed_dataset: DatasetRevision | None = None
    for binding, dataset in candidates:
        if not _same_universe(discovery_dataset, dataset):
            continue
        if requested.issubset(set(binding.instrument_scope)):
            sealed_binding = binding
            sealed_dataset = dataset
            break
    if sealed_binding is None or sealed_dataset is None:
        _ledger(
            session,
            experiment=discovery,
            outcome="SEALED_DATA_BLOCKED",
            evidence_summary={"reason": "NO_INDEPENDENT_SEALED_CATALOG"},
        )
        append_event(
            session,
            kind="SEALED_QUANT_EXPERIMENT_BLOCKED",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=discovery.program_id,
            payload={
                "discovery_experiment_id": str(discovery.id),
                "reason": "NO_INDEPENDENT_SEALED_CATALOG",
            },
        )
        return None
    if sealed_binding.catalog_uri == contract.catalog.catalog_uri:
        raise QfError(
            "SEALED_CATALOG_NOT_INDEPENDENT",
            "Sealed Evaluation cannot reuse the Discovery catalog.",
            409,
        )
    sealed_contract = contract.model_copy(
        update={
            "run_id": uuid4(),
            "catalog": contract.catalog.model_copy(
                update={
                    "dataset_revision_id": sealed_dataset.id,
                    "catalog_uri": sealed_binding.catalog_uri,
                    "nautilus_data_type": sealed_binding.nautilus_data_type,
                    "partition": "SEALED",
                }
            ),
        }
    )
    sealed = QuantExperiment(
        parent_experiment_id=discovery.id,
        mission_id=discovery.mission_id,
        program_id=discovery.program_id,
        dataset_revision_id=sealed_dataset.id,
        zone="SEALED",
        state="READY",
        runtime_name="NAUTILUS_TRADER",
        runtime_version=PINNED_NAUTILUS_VERSION,
        strategy_artifact=dict(discovery.strategy_artifact),
        contract_json=sealed_contract.model_dump(mode="json"),
    )
    session.add(sealed)
    session.flush()
    _ledger(session, experiment=sealed, outcome="QUEUED")
    _enqueue(session, kind="NAUTILUS_SEALED_BACKTEST", resource_id=sealed.id)
    append_event(
        session,
        kind="SEALED_QUANT_EXPERIMENT_QUEUED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=sealed.program_id,
        payload={
            "discovery_experiment_id": str(discovery.id),
            "sealed_experiment_id": str(sealed.id),
            "sealed_dataset_revision_id": str(sealed_dataset.id),
        },
    )
    return sealed


def _promote_from_sealed(
    session: Session,
    experiment: QuantExperiment,
    evidence: BacktestEvidence,
) -> None:
    decision = str(evidence.disclosure.get("decision") or "FAIL")
    evaluation = session.scalar(
        select(SealedEvaluation).where(
            SealedEvaluation.experiment_id == experiment.id
        )
    )
    if evaluation is None:
        evaluation = SealedEvaluation(
            experiment_id=experiment.id,
            dataset_revision_id=experiment.dataset_revision_id,
            state="EVALUATED",
            decision=decision,
            runtime_version=evidence.runtime_version,
            disclosure=dict(evidence.disclosure),
            created_at=_now(),
        )
        session.add(evaluation)
        session.flush()
    if decision != "PASS":
        append_event(
            session,
            kind="SEALED_QUANT_EXPERIMENT_REJECTED",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=experiment.program_id,
            payload={
                "experiment_id": str(experiment.id),
                "classification": evidence.disclosure.get("classification"),
            },
        )
        return

    contract = ExperimentContract.model_validate(experiment.contract_json)
    discovery = session.get(QuantExperiment, experiment.parent_experiment_id)
    discovery_evidence = dict(discovery.evidence_json) if discovery else {}
    alpha = session.scalar(
        select(AlphaQualification).where(
            AlphaQualification.evaluation_episode_id == evaluation.id
        )
    )
    if alpha is None:
        alpha = AlphaQualification(
            program_id=experiment.program_id,
            alpha_model_version_id=contract.strategy.artifact_id,
            universe=contract.catalog.instrument_ids[0],
            horizon="NAUTILUS_BACKTEST",
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            name=f"Nautilus strategy {contract.strategy.strategy_path}",
            scope_json={
                "discovery_dataset_revision_id": (
                    str(discovery.dataset_revision_id) if discovery else None
                ),
                "sealed_dataset_revision_id": str(experiment.dataset_revision_id),
                "runtime_version": evidence.runtime_version,
            },
            evaluation_episode_id=evaluation.id,
            metrics={
                "discovery": discovery_evidence,
                "sealed_disclosure": evidence.disclosure,
            },
            lineage=[
                {
                    "experiment_id": str(experiment.id),
                    "parent_experiment_id": (
                        str(experiment.parent_experiment_id)
                        if experiment.parent_experiment_id
                        else None
                    ),
                }
            ],
            created_at=_now(),
        )
        session.add(alpha)
        session.flush()

    existing_candidate = session.scalar(
        select(PortfolioCandidate).where(
            PortfolioCandidate.evaluation_episode_id == evaluation.id
        )
    )
    if existing_candidate is not None:
        return
    mandate = session.scalar(
        select(PortfolioMandate).where(
            PortfolioMandate.enabled.is_(True),
            PortfolioMandate.state == "ACTIVE",
        )
    )
    mandate_version_id = mandate.latest_version_id if mandate else uuid4()
    mandate_name = mandate.name if mandate else "Nautilus Research Default"
    portfolio_program = PortfolioProgram(
        mandate_version_id=mandate_version_id,
        mandate_name=mandate_name,
        state="CANDIDATE_READY",
    )
    session.add(portfolio_program)
    session.flush()
    members = contract.portfolio_targets or [
        {
            "instrument_id": contract.catalog.instrument_ids[0],
            "target_weight": 1.0,
        }
    ]
    candidate = PortfolioCandidate(
        candidate_family_id=uuid4(),
        portfolio_program_id=portfolio_program.id,
        mandate_version_id=mandate_version_id,
        mandate_name=mandate_name,
        universe_set_json=contract.catalog.instrument_ids,
        policy_version="NAUTILUS_TRANSACTION_SIMULATED_V1",
        risk_model_version=f"NAUTILUS_RISK_ENGINE_{evidence.runtime_version}",
        cost_model_version=f"NAUTILUS_VENUE_{contract.venue.name}",
        capacity_model_version="NAUTILUS_SIMULATION_EVIDENCE_V1",
        constraint_set_version="GOVERNED_MANDATE_V1",
        rebalance_policy_version="STRATEGY_NATIVE_V1",
        evaluation_episode_id=evaluation.id,
        state="READY",
        members=members,
        metrics={
            "strategy_artifact": contract.strategy.model_dump(mode="json"),
            "dataset_revision_id": (
                str(discovery.dataset_revision_id) if discovery else None
            ),
            "sealed_dataset_revision_id": str(experiment.dataset_revision_id),
            "quant_evidence": {
                "discovery": discovery_evidence,
                "sealed": evidence.model_dump(mode="json"),
            },
            "nautilus_runtime": {
                "name": evidence.runtime_name,
                "version": evidence.runtime_version,
            },
        },
        created_at=_now(),
    )
    session.add(candidate)
    session.flush()
    portfolio_program.current_candidate_id = candidate.id

    downstreams = session.scalars(
        select(DownstreamSystem).where(
            DownstreamSystem.enabled.is_(True),
            DownstreamSystem.environment_type == "PAPER",
            DownstreamSystem.preflight_state == "READY",
        )
    ).all()
    downstream = next(
        (
            item
            for item in downstreams
            if "NAUTILUS_NATIVE_CANDIDATE" in item.compatibility
        ),
        None,
    )
    if downstream is not None:
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="PENDING",
            downstream_system_id=downstream.id,
            recommendation_rationale=(
                "Independent Discovery and Sealed NautilusTrader runs produced "
                "executable transaction-level evidence."
            ),
            human_report={
                "runtime": evidence.runtime_name,
                "runtime_version": evidence.runtime_version,
                "sealed_disclosure": evidence.disclosure,
            },
            evidence_summary={
                "discovery": _evidence_summary(
                    BacktestEvidence.model_validate(discovery_evidence)
                )
                if discovery_evidence
                else {},
                "sealed_disclosure": evidence.disclosure,
            },
            capital_context={},
            risk_summary={"execution_risk_owner": "NAUTILUS_TRADER"},
            cost_summary={"simulation_owner": "NAUTILUS_TRADER"},
            capacity_summary={},
            changes_summary={"source": "NAUTILUS_SEALED_EVIDENCE"},
        )
        session.add(approval)
        program = session.get(ResearchProgram, experiment.program_id)
        if program is not None:
            program.state = "APPROVAL_PENDING"
    append_event(
        session,
        kind="NAUTILUS_ALPHA_PROMOTED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=experiment.program_id,
        payload={
            "experiment_id": str(experiment.id),
            "alpha_qualification_id": str(alpha.id),
            "portfolio_candidate_id": str(candidate.id),
        },
    )


def _begin_experiment(
    session: Session,
    experiment_id: UUID,
) -> ExperimentLease | None:
    experiment = session.execute(
        select(QuantExperiment)
        .where(QuantExperiment.id == experiment_id)
        .with_for_update()
    ).scalar_one_or_none()
    if experiment is None:
        raise QfError(
            "QUANT_EXPERIMENT_NOT_FOUND",
            "Quant experiment does not exist.",
            404,
        )
    if experiment.state == "SUCCEEDED":
        return None
    if experiment.state not in {"READY", "FAILED"}:
        raise QfError(
            "QUANT_EXPERIMENT_STATE_CONFLICT",
            "Quant experiment cannot run in its current state.",
            409,
            {"state": experiment.state},
        )
    experiment.state = "RUNNING"
    experiment.started_at = _now()
    experiment.finished_at = None
    experiment.error_code = None
    experiment.error_detail = None
    session.flush()
    zone: Literal["DISCOVERY", "SEALED"] = (
        "DISCOVERY" if experiment.zone == "DISCOVERY" else "SEALED"
    )
    return ExperimentLease(
        experiment_id=experiment.id,
        zone=zone,
        contract=ExperimentContract.model_validate(experiment.contract_json),
    )


def _finish_experiment(
    session: Session,
    *,
    experiment_id: UUID,
    evidence: BacktestEvidence,
) -> None:
    experiment = session.execute(
        select(QuantExperiment)
        .where(QuantExperiment.id == experiment_id)
        .with_for_update()
    ).scalar_one()
    if experiment.state == "SUCCEEDED":
        return
    if experiment.state != "RUNNING":
        raise QfError(
            "QUANT_EXPERIMENT_STATE_CONFLICT",
            "Only a RUNNING experiment may accept runtime evidence.",
            409,
            {"state": experiment.state},
        )
    if evidence.runtime_version != PINNED_NAUTILUS_VERSION:
        raise QfError(
            "NAUTILUS_RUNTIME_VERSION_MISMATCH",
            "Remote runtime version does not match the pinned contract.",
            409,
            {
                "expected": PINNED_NAUTILUS_VERSION,
                "actual": evidence.runtime_version,
            },
        )
    if evidence.partition != experiment.zone:
        raise QfError(
            "NAUTILUS_RUNTIME_CONTRACT_INVALID",
            "Runtime evidence partition does not match the experiment zone.",
            502,
        )
    experiment.state = "SUCCEEDED"
    experiment.runtime_version = evidence.runtime_version
    experiment.evidence_json = evidence.model_dump(mode="json")
    experiment.disclosure_json = dict(evidence.disclosure)
    experiment.finished_at = _now()
    _ledger(
        session,
        experiment=experiment,
        outcome="SUCCEEDED",
        evidence_summary=_evidence_summary(evidence),
    )
    if experiment.zone == "DISCOVERY":
        _create_sealed_experiment(session, experiment)
    else:
        _promote_from_sealed(session, experiment, evidence)
    append_event(
        session,
        kind="QUANT_EXPERIMENT_SUCCEEDED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=experiment.program_id,
        payload={
            "experiment_id": str(experiment.id),
            "zone": experiment.zone,
            "runtime_version": evidence.runtime_version,
        },
    )


def _fail_experiment(
    session: Session,
    *,
    experiment_id: UUID,
    error: Exception,
) -> None:
    experiment = session.execute(
        select(QuantExperiment)
        .where(QuantExperiment.id == experiment_id)
        .with_for_update()
    ).scalar_one_or_none()
    if experiment is None or experiment.state == "SUCCEEDED":
        return
    experiment.state = "FAILED"
    experiment.error_code = str(
        getattr(error, "code", type(error).__name__)
    )[:100]
    experiment.error_detail = str(error)[-4000:]
    experiment.finished_at = _now()
    _ledger(
        session,
        experiment=experiment,
        outcome="FAILED",
        evidence_summary={"error_code": experiment.error_code},
    )
    append_event(
        session,
        kind="QUANT_EXPERIMENT_FAILED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=experiment.program_id,
        payload={
            "experiment_id": str(experiment.id),
            "zone": experiment.zone,
            "error_code": experiment.error_code,
        },
    )


def execute_experiment(
    factory: SessionFactory,
    *,
    experiment_id: UUID,
    runtime: Any | None = None,
) -> None:
    """Run remote work outside every PostgreSQL transaction and row lock."""
    with factory.begin() as session:
        lease = _begin_experiment(session, experiment_id)
    if lease is None:
        return
    owned_runtime = runtime is None
    client = runtime or _runtime(lease.zone)
    try:
        evidence = (
            client.run_backtest(lease.contract)
            if lease.zone == "DISCOVERY"
            else client.run_sealed_backtest(lease.contract)
        )
        with factory.begin() as session:
            _finish_experiment(
                session,
                experiment_id=lease.experiment_id,
                evidence=evidence,
            )
    except Exception as exc:
        with factory.begin() as session:
            _fail_experiment(
                session,
                experiment_id=lease.experiment_id,
                error=exc,
            )
        raise
    finally:
        if owned_runtime and hasattr(client, "close"):
            client.close()


def run_job(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory() as session:
            job = session.get(Job, job_id)
            if job is None or job.kind not in {
                "NAUTILUS_DISCOVERY_BACKTEST",
                "NAUTILUS_SEALED_BACKTEST",
            }:
                raise QfError(
                    "JOB_NOT_FOUND",
                    "Nautilus experiment job does not exist.",
                    404,
                )
            experiment_id = job.resource_id
        execute_experiment(factory, experiment_id=experiment_id)
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one remote Nautilus experiment")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    run_job(Settings.from_env(), UUID(args.job_id))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
'''

REMOTE_TEST = r'''from __future__ import annotations

import math
import os
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import httpx
import pytest

from quant_runtime import (
    CatalogReference,
    ExperimentContract,
    RemoteNautilusQuantRuntime,
    StrategyArtifact,
)

pytestmark = pytest.mark.nautilus


def _ingest(
    *,
    base_url: str,
    token: str,
    catalog_key: str,
    dataset_id: object,
    quotes: list[dict[str, object]],
    partition: str,
) -> dict[str, object]:
    with httpx.Client(
        base_url=base_url,
        headers={"Authorization": f"Bearer {token}"},
        timeout=120,
    ) as client:
        response = client.post(
            "/v1/catalogs/ingest",
            json={
                "catalog_key": catalog_key,
                "dataset_revision_id": str(dataset_id),
                "provider": f"CI independent {partition.lower()} fixture",
                "source_license": "CI_TEST",
                "instrument": "EUR/USD",
                "quotes": quotes,
                "schema_revision": "quote-v1",
                "partition": partition,
            },
            headers={"Idempotency-Key": catalog_key},
        )
        response.raise_for_status()
        first = response.json()
        replay = client.post(
            "/v1/catalogs/ingest",
            json={
                "catalog_key": catalog_key,
                "dataset_revision_id": str(dataset_id),
                "provider": f"CI independent {partition.lower()} fixture",
                "source_license": "CI_TEST",
                "instrument": "EUR/USD",
                "quotes": quotes,
                "schema_revision": "quote-v1",
                "partition": partition,
            },
            headers={"Idempotency-Key": catalog_key},
        )
        replay.raise_for_status()
        assert replay.json() == first
        return first


def test_remote_research_and_sealed_runtimes_are_independent() -> None:
    research_url = os.environ.get("QUAZONAI_NAUTILUS_TEST_RESEARCH_URL")
    research_token = os.environ.get("QUAZONAI_NAUTILUS_TEST_RESEARCH_TOKEN")
    sealed_url = os.environ.get("QUAZONAI_NAUTILUS_TEST_SEALED_URL")
    sealed_token = os.environ.get("QUAZONAI_NAUTILUS_TEST_SEALED_TOKEN")
    if not all((research_url, research_token, sealed_url, sealed_token)):
        pytest.skip("independent remote Nautilus runtimes are not configured")
    assert research_url != sealed_url
    assert research_token != sealed_token

    start = datetime(2026, 1, 5, 0, 0, tzinfo=UTC)
    quotes: list[dict[str, object]] = []
    for index in range(720):
        timestamp = start + timedelta(seconds=index * 10)
        mid = 1.10 + 0.006 * math.sin(index / 22.0) + 0.00001 * index
        quotes.append(
            {
                "timestamp": timestamp.isoformat(),
                "bid_price": round(mid - 0.00005, 5),
                "ask_price": round(mid + 0.00005, 5),
            }
        )

    discovery_dataset_id = uuid4()
    sealed_dataset_id = uuid4()
    discovery_metadata = _ingest(
        base_url=research_url,
        token=research_token,
        catalog_key=f"issue22-discovery-{discovery_dataset_id.hex}",
        dataset_id=discovery_dataset_id,
        quotes=quotes,
        partition="DISCOVERY",
    )
    sealed_metadata = _ingest(
        base_url=sealed_url,
        token=sealed_token,
        catalog_key=f"issue22-sealed-{sealed_dataset_id.hex}",
        dataset_id=sealed_dataset_id,
        quotes=quotes,
        partition="SEALED",
    )
    assert discovery_metadata["catalog_uri"] != sealed_metadata["catalog_uri"]

    discovery_catalog = CatalogReference(
        dataset_revision_id=discovery_dataset_id,
        catalog_uri=str(discovery_metadata["catalog_uri"]),
        nautilus_data_type=str(discovery_metadata["nautilus_data_type"]),
        instrument_ids=list(discovery_metadata["instrument_scope"]),
        partition="DISCOVERY",
        start_time=str(quotes[0]["timestamp"]),
        end_time=str(quotes[-1]["timestamp"]),
    )
    contract = ExperimentContract(
        catalog=discovery_catalog,
        strategy=StrategyArtifact(
            strategy_path="nautilus_trader.examples.strategies.ema_cross:EMACross",
            config_path=(
                "nautilus_trader.examples.strategies.ema_cross:EMACrossConfig"
            ),
            config={
                "fast_ema_period": 2,
                "slow_ema_period": 5,
                "trade_size": "10000",
            },
        ),
    )
    research = RemoteNautilusQuantRuntime(
        base_url=research_url,
        token=research_token,
        timeout_seconds=120,
    )
    sealed = RemoteNautilusQuantRuntime(
        base_url=sealed_url,
        token=sealed_token,
        timeout_seconds=120,
    )
    try:
        assert research.health()["runtime_version"] == "1.231.0"
        assert sealed.health()["runtime_version"] == "1.231.0"
        validation = research.validate_catalog(discovery_catalog)
        assert validation.valid is True
        discovery = research.run_backtest(contract)
        assert discovery.runtime_version == "1.231.0"
        assert discovery.total_events > 0
        assert discovery.total_orders > 0
        assert discovery.reports["orders"]

        sealed_catalog = CatalogReference(
            dataset_revision_id=sealed_dataset_id,
            catalog_uri=str(sealed_metadata["catalog_uri"]),
            nautilus_data_type=str(sealed_metadata["nautilus_data_type"]),
            instrument_ids=list(sealed_metadata["instrument_scope"]),
            partition="SEALED",
            start_time=str(quotes[0]["timestamp"]),
            end_time=str(quotes[-1]["timestamp"]),
        )
        sealed_contract = contract.model_copy(
            update={"run_id": uuid4(), "catalog": sealed_catalog}
        )
        sealed_evidence = sealed.run_sealed_backtest(sealed_contract)
        assert sealed_evidence.catalog_uri != discovery.catalog_uri
        assert sealed_evidence.partition == "SEALED"
        assert sealed_evidence.reports == {}
        assert sealed_evidence.statistics == {}
        assert sealed_evidence.total_events == 0
        assert sealed_evidence.total_orders == 0
        assert sealed_evidence.total_positions == 0
        assert sealed_evidence.disclosure["decision"] == "PASS"
        verification = sealed.verify_candidate(
            {
                "runtime": {
                    "name": "NAUTILUS_TRADER",
                    "version": "1.231.0",
                },
                "required_files": [
                    "manifest.json",
                    "requirements.lock",
                    "lineage.json",
                ],
            }
        )
        assert verification.valid is True
    finally:
        research.close()
        sealed.close()
'''


def update_pipeline_test() -> None:
    path = Path("backend/tests/integration/test_quant_experiment_pipeline.py")
    text = path.read_text(encoding="utf-8")
    text = text.replace(
        "from runners.quant_experiments import process_experiment, submit_experiment",
        "from runners.quant_experiments import execute_experiment, submit_experiment",
    )
    text = text.replace(
        '''    with factory.begin() as session:
        process_experiment(
            session,
            experiment_id=discovery_id,
            runtime=FakeRemoteRuntime(),
        )
''',
        '''    execute_experiment(
        factory,
        experiment_id=discovery_id,
        runtime=FakeRemoteRuntime(),
    )
''',
    )
    text = text.replace(
        '''    with factory.begin() as session:
        process_experiment(
            session,
            experiment_id=sealed_id,
            runtime=FakeRemoteRuntime(),
        )
''',
        '''    execute_experiment(
        factory,
        experiment_id=sealed_id,
        runtime=FakeRemoteRuntime(),
    )
''',
    )
    path.write_text(text, encoding="utf-8")


def update_worker() -> None:
    path = Path("backend/src/runners/finite_worker.py")
    text = path.read_text(encoding="utf-8")
    text = text.replace(
        '''def _child_handler(
    module: str,
    *fixed_arguments: str,
    timeout_attribute: str = "plugin_job_timeout_seconds",
) -> Handler:
''',
        '''def _child_handler(
    module: str,
    *fixed_arguments: str,
    timeout_attribute: str = "plugin_job_timeout_seconds",
    clear_environment: tuple[str, ...] = (),
) -> Handler:
''',
        1,
    )
    text = text.replace(
        '''        try:
            subprocess.run(
                [sys.executable, "-m", module, *fixed_arguments, str(job.id)],
''',
        '''        environment = os.environ.copy()
        for name in clear_environment:
            environment[name] = ""
        try:
            subprocess.run(
                [sys.executable, "-m", module, *fixed_arguments, str(job.id)],
''',
        1,
    )
    text = text.replace("                env=os.environ.copy(),\n", "                env=environment,\n", 1)
    text = text.replace(
        '''    "NAUTILUS_DISCOVERY_BACKTEST": _child_handler(
        "runners.quant_experiments",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
    ),
''',
        '''    "NAUTILUS_DISCOVERY_BACKTEST": _child_handler(
        "runners.quant_experiments",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
        clear_environment=(
            "QUAZONAI_NAUTILUS_SEALED_URL",
            "QUAZONAI_NAUTILUS_SEALED_TOKEN",
        ),
    ),
''',
        1,
    )
    text = text.replace(
        '''    "NAUTILUS_SEALED_BACKTEST": _child_handler(
        "runners.quant_experiments",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
    ),
''',
        '''    "NAUTILUS_SEALED_BACKTEST": _child_handler(
        "runners.quant_experiments",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
        clear_environment=(
            "QUAZONAI_NAUTILUS_RESEARCH_URL",
            "QUAZONAI_NAUTILUS_RESEARCH_TOKEN",
            "CODEX_HOME",
        ),
    ),
''',
        1,
    )
    path.write_text(text, encoding="utf-8")


def update_ci() -> None:
    path = Path(".github/workflows/ci.yml")
    text = path.read_text(encoding="utf-8")
    start = text.index("      - name: Remote Nautilus integration\n")
    end = text.index("      - name: Compose configuration\n", start)
    block = '''      - name: Independent remote Nautilus integration
        env:
          QUAZONAI_NAUTILUS_TEST_RESEARCH_URL: http://127.0.0.1:8011
          QUAZONAI_NAUTILUS_TEST_RESEARCH_TOKEN: ci-research-runtime-token
          QUAZONAI_NAUTILUS_TEST_SEALED_URL: http://127.0.0.1:8012
          QUAZONAI_NAUTILUS_TEST_SEALED_TOKEN: ci-sealed-runtime-token
        run: |
          rm -rf /tmp/quazonai-nautilus-research /tmp/quazonai-nautilus-sealed
          QUAZONAI_NAUTILUS_RUNTIME_TOKEN=ci-research-runtime-token \\
          QUAZONAI_NAUTILUS_CATALOG_ROOT=/tmp/quazonai-nautilus-research \\
          QUAZONAI_NAUTILUS_HOST=127.0.0.1 \\
          QUAZONAI_NAUTILUS_PORT=8011 \\
          quazonai-nautilus-runtime > /tmp/nautilus-research.log 2>&1 &
          research_pid=$!
          QUAZONAI_NAUTILUS_RUNTIME_TOKEN=ci-sealed-runtime-token \\
          QUAZONAI_NAUTILUS_CATALOG_ROOT=/tmp/quazonai-nautilus-sealed \\
          QUAZONAI_NAUTILUS_HOST=127.0.0.1 \\
          QUAZONAI_NAUTILUS_PORT=8012 \\
          quazonai-nautilus-runtime > /tmp/nautilus-sealed.log 2>&1 &
          sealed_pid=$!
          cleanup() {
            kill "$research_pid" "$sealed_pid" 2>/dev/null || true
            cat /tmp/nautilus-research.log
            cat /tmp/nautilus-sealed.log
          }
          trap cleanup EXIT
          for endpoint in \\
            '8011 ci-research-runtime-token' \\
            '8012 ci-sealed-runtime-token'; do
            set -- $endpoint
            port=$1
            token=$2
            for attempt in $(seq 1 60); do
              if curl --fail --silent \\
                -H "Authorization: Bearer $token" \\
                "http://127.0.0.1:$port/v1/health" > "/tmp/health-$port.json"; then
                break
              fi
              if [ "$attempt" = 60 ]; then
                exit 1
              fi
              sleep 1
            done
          done
          pytest -q -m nautilus \\
            backend/tests/integration/test_remote_nautilus_runtime.py
'''
    text = text[:start] + block + text[end:]
    boundary = "          ! grep -R -n -E 'TradingNode|LiveNode|submit_order|cancel_order|modify_order' backend/src/api backend/src/quant_runtime.py backend/src/runners/quant_experiments.py\n"
    addition = boundary + "          ! grep -R -n -E 'Research Engine 与 NautilusTrader完全无关|feature_pipeline\\.whl|expected_alpha\\.arrow' DESIGN.md AGENTS.md OPERATIONS.md CLI.md README.md\n"
    if boundary in text and "expected_alpha\\.arrow" not in text:
        text = text.replace(boundary, addition, 1)
    path.write_text(text, encoding="utf-8")


def restore_agents() -> None:
    path = Path("AGENTS.md")
    current = path.read_text(encoding="utf-8")
    if "## 7. Runtime Plugin 边界" in current:
        return
    import subprocess

    base = subprocess.run(
        ["git", "show", "origin/main:AGENTS.md"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    insert_at = base.index("## 1. 事实源与读取顺序")
    addendum = '''## 0. Nautilus-first 远程运行边界

NautilusTrader `1.231.0` 是 Canonical Quant Runtime。QuaZonai 是 AI 研究与治理 Control Plane，通过类型化 HTTP contract 调用独立 Remote Research Runtime 与独立 Sealed Runtime；Core 不 import NautilusTrader、不共享 Catalog 文件系统、不启动 LiveNode。

- QZ：Idea/Charter/Mission/Search Ledger/Evaluation/Alpha/Portfolio/Approval/Handoff/Forward Evidence；
- NautilusTrader：Instrument/Data/Catalog/Strategy/Backtest/Matching/Order lifecycle/Fee/Fill/Latency/Account/Position/PnL/Execution Risk/Paper/Live；
- Research 与 Sealed 必须使用独立 endpoint、token、Catalog 和 Dataset Revision；
- Codex child 不得获得 DB、runtime token、Sealed raw data 或 broker credential；
- Sealed 只返回 deterministic controlled disclosure；
- Candidate Package 的正式协议是 Nautilus-native Candidate Bundle；
- Paper/Live 仍为独立 downstream，QZ 不 stop/cancel/flatten/recover；
- 新增量化基础设施前先确认 NautilusTrader 是否已可靠提供，若是则复用。

'''
    base = base[:insert_at] + addendum + base[insert_at:]
    base = base.replace(
        "NautilusTrader、LEAN 或自定义交易系统都是独立 downstream consumer。",
        "独立 Nautilus Paper/Live runtime 是 downstream consumer；Remote Nautilus Research/Sealed runtime 是 QZ 的 Canonical Quant Runtime 边界。",
    )
    base = base.replace(
        "- Candidate Package 只输出 TargetPortfolioFrame，不输出订单；",
        "- Nautilus-native Candidate Bundle 冻结 Strategy/config/data/runtime/evidence/lineage，不包含真实 broker credential，也不赋予 QZ 下单权限；",
    )
    base = base.replace(
        "- Research Engine：Arrow/Polars/evaluator/Optuna/CVXPY；",
        "- Quant Runtime Adapter：受控调用 Remote Nautilus Research/Sealed runtime；CVXPY 只补足 NautilusTrader 没有的 target-weight optimization；",
    )
    path.write_text(base, encoding="utf-8")


def update_notices() -> None:
    path = Path("THIRD_PARTY_NOTICES.md")
    text = path.read_text(encoding="utf-8")
    if "NautilusTrader" not in text:
        text += '''

## NautilusTrader

- Component: `nautilus_trader==1.231.0` (remote quant runtime image only)
- Project: NautilusTrader by Nautech Systems
- License: GNU Lesser General Public License v3.0 or later (LGPL-3.0-or-later)
- QuaZonai Core images do not bundle this component; it is pinned in the independently deployed remote runtime image.
'''
    path.write_text(text, encoding="utf-8")


def main() -> None:
    Path("backend/src/runners/nautilus_remote_runtime.py").write_text(
        REMOTE_RUNTIME,
        encoding="utf-8",
    )
    Path("backend/src/runners/quant_experiments.py").write_text(
        QUANT_EXPERIMENTS,
        encoding="utf-8",
    )
    Path("backend/tests/integration/test_remote_nautilus_runtime.py").write_text(
        REMOTE_TEST,
        encoding="utf-8",
    )
    update_pipeline_test()
    update_worker()
    update_ci()
    restore_agents()
    update_notices()


if __name__ == "__main__":
    main()
