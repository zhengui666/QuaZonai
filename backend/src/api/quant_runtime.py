"""Operator API for remote Nautilus catalogs and governed run evidence."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any
from uuid import UUID

from fastapi import APIRouter, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import select

from db.models import (
    DatasetRevision,
    GovernedDataSource,
    MarketUniverseVersion,
    NautilusCatalogBinding,
    QuantRuntimeRun,
    SearchLedgerEntry,
)
from errors import QfError
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import CatalogIngestSpec
from quant_runtime.remote import NautilusQuantRuntime

router = APIRouter(prefix="/api/v1", tags=["quant-runtime"])


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class CatalogIngestInput(StrictModel):
    catalog_name: str = Field(pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}$")
    provider: str = Field(min_length=1, max_length=200)
    source_license: str = Field(min_length=1, max_length=500)
    universe_name: str = Field(min_length=1, max_length=200)
    sealed: bool = False
    source_spec: dict[str, Any]


class CatalogView(StrictModel):
    id: UUID
    dataset_revision_id: UUID
    catalog_uri: str
    provider: str
    source_license: str
    nautilus_data_type: str
    instrument_scope: list[str]
    event_time_range: dict[str, Any]
    available_time_range: dict[str, Any]
    schema_revision: str
    quality_state: str
    quality_result: dict[str, Any]
    point_in_time_state: str
    point_in_time_result: dict[str, Any]
    sealed: bool
    created_at: str | None = None


class RunView(StrictModel):
    id: UUID
    program_id: UUID
    branch_id: UUID
    mission_id: UUID | None
    mode: str
    state: str
    experiment_key: str
    family: str
    catalog_uri: str
    runtime_name: str
    runtime_version: str | None
    contract_version: str | None
    parameters: dict[str, Any]
    evidence: dict[str, Any]
    error_code: str | None
    error_message: str | None
    created_at: str | None


class SearchLedgerView(StrictModel):
    id: UUID
    run_id: UUID
    family: str
    parameters: dict[str, Any]
    outcome: str
    failure_code: str | None
    disclosure_level: str
    evidence_summary: dict[str, Any]
    created_at: str


def _normalized_time(value: object) -> str | None:
    if value is None:
        return None
    try:
        parsed = datetime.fromisoformat(value) if isinstance(value, str) else value
        if not isinstance(parsed, datetime):
            return str(value)
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=UTC)
        return parsed.astimezone(UTC).isoformat()
    except (TypeError, ValueError):
        return str(value)


def _normalized_range(start: object, end: object) -> dict[str, str | None]:
    return {"start": _normalized_time(start), "end": _normalized_time(end)}


def _runtime(*, sealed: bool = False) -> NautilusQuantRuntime:
    config = RemoteNautilusConfig.from_env(
        required=True,
        profile="sealed" if sealed else "research",
    )
    assert config is not None
    return NautilusQuantRuntime(config)


def _remote_catalog_name(payload: CatalogIngestInput) -> str:
    profile = "sealed" if payload.sealed else "research"
    return f"{profile}-{payload.catalog_name}"


def _catalog_view(item: NautilusCatalogBinding) -> CatalogView:
    return CatalogView(
        id=item.id,
        dataset_revision_id=item.dataset_revision_id,
        catalog_uri=item.catalog_uri,
        provider=item.provider,
        source_license=item.source_license,
        nautilus_data_type=item.nautilus_data_type,
        instrument_scope=item.instrument_scope,
        event_time_range=item.event_time_range,
        available_time_range=item.available_time_range,
        schema_revision=item.schema_revision,
        quality_state=item.quality_state,
        quality_result=item.quality_result,
        point_in_time_state=item.point_in_time_state,
        point_in_time_result=item.point_in_time_result,
        sealed=item.sealed,
        created_at=item.created_at.isoformat() if item.created_at else None,
    )


def _safe_run_evidence(item: QuantRuntimeRun) -> dict[str, Any]:
    if item.mode == "SEALED":
        return {
            "sealed": True,
            "disclosure": "Raw sealed evidence is restricted to the evaluator boundary.",
        }
    return item.evidence


def _catalog_binding_input_conflict(
    session: Any,
    item: NautilusCatalogBinding,
    payload: CatalogIngestInput,
    universe_id: UUID,
) -> bool:
    revision = session.get(DatasetRevision, item.dataset_revision_id)
    source = (
        session.get(GovernedDataSource, revision.data_source_id)
        if revision is not None and revision.data_source_id is not None
        else None
    )
    public_config = source.public_config if source is not None else {}
    stored_source_spec = public_config.get("source_spec") if isinstance(public_config, dict) else None
    return any(
        (
            revision is None,
            revision is not None and revision.universe_version_id != universe_id,
            revision is not None and revision.universe_name != payload.universe_name,
            item.provider != payload.provider,
            item.source_license != payload.source_license,
            item.sealed != payload.sealed,
            not isinstance(stored_source_spec, dict) or stored_source_spec != payload.source_spec,
        )
    )


def _run_view(item: QuantRuntimeRun) -> RunView:
    error_code = item.error_code
    error_message = item.error_message
    if item.mode == "SEALED":
        persisted = item.evidence
        state = str(persisted.get("state", item.state)) if isinstance(persisted, dict) else item.state
        error_code, error_message = (
            ("SEALED_RUNTIME_FAILURE", "Sealed runtime failure; disclosure withheld.")
            if state == "FAILED"
            else (None, None)
        )
    return RunView(
        id=item.id,
        program_id=item.program_id,
        branch_id=item.branch_id,
        mission_id=item.mission_id,
        mode=item.mode,
        state=item.state,
        experiment_key=item.experiment_key,
        family=item.family,
        catalog_uri=item.catalog_uri,
        runtime_name=item.runtime_name,
        runtime_version=item.runtime_version,
        contract_version=item.contract_version,
        parameters=item.parameters,
        evidence=_safe_run_evidence(item),
        error_code=error_code,
        error_message=error_message,
        created_at=item.created_at.isoformat() if item.created_at else None,
    )


@router.get("/quant-runtime/capabilities")
def quant_runtime_capabilities() -> dict[str, Any]:
    return _runtime().capabilities().model_dump(mode="json")


@router.post("/quant-runtime/catalogs/ingest", response_model=CatalogView, status_code=201)
def ingest_catalog(payload: CatalogIngestInput, request: Request) -> CatalogView:
    factory = request.app.state.session_factory
    remote_catalog_uri = f"catalog://{_remote_catalog_name(payload)}"
    with factory() as session:
        universe = session.scalar(
            select(MarketUniverseVersion)
            .where(
                MarketUniverseVersion.name == payload.universe_name,
                MarketUniverseVersion.state == "ACTIVE",
            )
            .order_by(MarketUniverseVersion.version_no.desc())
        )
        if universe is None:
            raise QfError(
                "UNIVERSE_NOT_GOVERNED",
                "Catalog ingestion requires an active governed Universe Version.",
                422,
                {"universe_name": payload.universe_name},
            )
        universe_id = universe.id
        existing = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.catalog_uri == remote_catalog_uri
            )
        )
        if existing is not None:
            if _catalog_binding_input_conflict(session, existing, payload, universe_id):
                raise QfError(
                    "CATALOG_REVISION_INPUT_CONFLICT",
                    "Catalog identity is already bound to different immutable ingestion inputs.",
                    409,
                    {"catalog_uri": remote_catalog_uri},
                )
            return _catalog_view(existing)

    descriptor = _runtime(sealed=payload.sealed).ingest(
        CatalogIngestSpec(
            catalog_name=_remote_catalog_name(payload),
            provider=payload.provider,
            source_license=payload.source_license,
            sealed=payload.sealed,
            source_spec=payload.source_spec,
        )
    )
    with factory.begin() as session:
        existing = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.catalog_uri == descriptor.catalog_uri
            )
        )
        if existing is not None:
            if _catalog_binding_input_conflict(session, existing, payload, universe_id):
                raise QfError(
                    "CATALOG_REVISION_INPUT_CONFLICT",
                    "Catalog identity is already bound to different immutable ingestion inputs.",
                    409,
                    {"catalog_uri": descriptor.catalog_uri},
                )
            return _catalog_view(existing)

        source = GovernedDataSource(
            name=f"Nautilus {'sealed' if payload.sealed else 'research'} catalog {payload.catalog_name}",
            provider=descriptor.provider,
            state="ACTIVE",
            universe_scope=[payload.universe_name] if payload.universe_name else [],
            fields=[descriptor.nautilus_data_type],
            update_cadence="REMOTE_RUNTIME_MANAGED",
            preflight_state="READY",
            public_config={
                "catalog_uri": descriptor.catalog_uri,
                "source_license": descriptor.source_license,
                "source_spec": payload.source_spec,
                "remote_runtime": "NautilusTrader",
                "runtime_profile": "sealed" if payload.sealed else "research",
            },
        )
        session.add(source)
        session.flush()
        revision = DatasetRevision(
            data_source_id=source.id,
            universe_version_id=universe_id,
            universe_name=payload.universe_name,
            revision_no=1,
            schema_version=descriptor.schema_revision,
            event_start=descriptor.event_start,
            event_end=descriptor.event_end,
            available_start=descriptor.available_start,
            available_end=descriptor.available_end,
            row_count=descriptor.row_count,
            quality_state="VALID" if descriptor.quality_result.get("valid") else "INVALID",
            point_in_time_state=(
                "VALID" if descriptor.point_in_time_result.get("valid") else "INVALID"
            ),
            partition="SEALED" if payload.sealed else "DISCOVERY",
            created_at=datetime.now(UTC),
        )
        session.add(revision)
        session.flush()
        binding = NautilusCatalogBinding(
            dataset_revision_id=revision.id,
            catalog_uri=descriptor.catalog_uri,
            provider=descriptor.provider,
            source_license=descriptor.source_license,
            nautilus_data_type=descriptor.nautilus_data_type,
            instrument_scope=descriptor.instrument_scope,
            event_time_range={
                "start": descriptor.event_start.isoformat() if descriptor.event_start else None,
                "end": descriptor.event_end.isoformat() if descriptor.event_end else None,
            },
            available_time_range={
                "start": (
                    descriptor.available_start.isoformat()
                    if descriptor.available_start
                    else None
                ),
                "end": descriptor.available_end.isoformat() if descriptor.available_end else None,
            },
            schema_revision=descriptor.schema_revision,
            quality_state=revision.quality_state,
            quality_result=descriptor.quality_result,
            point_in_time_state=revision.point_in_time_state,
            point_in_time_result=descriptor.point_in_time_result,
            sealed=payload.sealed,
        )
        session.add(binding)
        session.flush()
        return _catalog_view(binding)


@router.get("/quant-runtime/catalogs", response_model=list[CatalogView])
def list_catalogs(request: Request) -> list[CatalogView]:
    factory = request.app.state.session_factory
    with factory() as session:
        items = session.scalars(
            select(NautilusCatalogBinding).order_by(NautilusCatalogBinding.created_at.desc())
        ).all()
        return [_catalog_view(item) for item in items]


def _catalog_validation_changed(
    item: NautilusCatalogBinding,
    revision: DatasetRevision | None,
    descriptor: Any,
) -> bool:
    if revision is None:
        return True
    expected_event = _normalized_range(revision.event_start, revision.event_end)
    expected_available = _normalized_range(revision.available_start, revision.available_end)
    stored_event = _normalized_range(
        item.event_time_range.get("start"), item.event_time_range.get("end")
    )
    stored_available = _normalized_range(
        item.available_time_range.get("start"), item.available_time_range.get("end")
    )
    actual_event = _normalized_range(descriptor.event_start, descriptor.event_end)
    actual_available = _normalized_range(descriptor.available_start, descriptor.available_end)
    return any(
        (
            item.catalog_uri != descriptor.catalog_uri,
            item.provider != descriptor.provider,
            item.source_license != descriptor.source_license,
            item.instrument_scope != descriptor.instrument_scope,
            item.nautilus_data_type != descriptor.nautilus_data_type,
            item.schema_revision != descriptor.schema_revision,
            stored_event != expected_event,
            stored_available != expected_available,
            actual_event != expected_event,
            actual_available != expected_available,
            revision.row_count != descriptor.row_count,
            item.quality_result != descriptor.quality_result,
            item.point_in_time_result != descriptor.point_in_time_result,
            item.quality_state != ("VALID" if descriptor.quality_result.get("valid") else "INVALID"),
            item.point_in_time_state
            != ("VALID" if descriptor.point_in_time_result.get("valid") else "INVALID"),
        )
    )


@router.post("/quant-runtime/catalogs/{catalog_id}/validate", response_model=CatalogView)
def validate_catalog(catalog_id: UUID, request: Request) -> CatalogView:
    factory = request.app.state.session_factory
    with factory.begin() as session:
        item = session.get(NautilusCatalogBinding, catalog_id)
        if item is None:
            raise QfError("CATALOG_NOT_FOUND", "Nautilus catalog binding does not exist.", 404)
        descriptor = _runtime(sealed=item.sealed).validate_catalog(item.catalog_uri)
        revision = session.get(DatasetRevision, item.dataset_revision_id)
        if _catalog_validation_changed(item, revision, descriptor):
            raise QfError(
                "CATALOG_REVISION_CHANGED",
                "Validation facts changed; ingest a new named catalog revision instead of mutating this Dataset Revision.",
                409,
                {"catalog_uri": item.catalog_uri, "dataset_revision_id": str(item.dataset_revision_id)},
            )
        return _catalog_view(item)


@router.get(
    "/research-programs/{program_id}/quant-runs",
    response_model=list[RunView],
)
def list_quant_runs(program_id: UUID, request: Request) -> list[RunView]:
    factory = request.app.state.session_factory
    with factory() as session:
        items = session.scalars(
            select(QuantRuntimeRun)
            .where(QuantRuntimeRun.program_id == program_id)
            .order_by(QuantRuntimeRun.created_at.asc())
        ).all()
        return [_run_view(item) for item in items]


@router.get(
    "/research-programs/{program_id}/search-ledger",
    response_model=list[SearchLedgerView],
)
def list_search_ledger(program_id: UUID, request: Request) -> list[SearchLedgerView]:
    factory = request.app.state.session_factory
    with factory() as session:
        items = session.scalars(
            select(SearchLedgerEntry)
            .where(SearchLedgerEntry.program_id == program_id)
            .order_by(SearchLedgerEntry.created_at.asc())
        ).all()
        return [
            SearchLedgerView(
                id=item.id,
                run_id=item.run_id,
                family=item.family,
                parameters=item.parameters,
                outcome=item.outcome,
                failure_code=item.failure_code,
                disclosure_level=item.disclosure_level,
                evidence_summary=item.evidence_summary,
                created_at=item.created_at.isoformat(),
            )
            for item in items
        ]
