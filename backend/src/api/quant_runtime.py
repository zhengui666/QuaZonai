"""Operator API for remote Nautilus catalogs and governed run evidence."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID

from fastapi import APIRouter, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError

from db.models import (
    DatasetRevision,
    GovernedDataSource,
    MarketUniverseVersion,
    NautilusCatalogBinding,
    ArchiveManifest,
    ArchiveManifestShard,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
    QuantRuntimeRun,
    SearchLedgerEntry,
)
from errors import QfError
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import ArchiveManifestSpec, CatalogIngestSpec
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
    universe_version_id: UUID | None = None
    plugin_release_id: UUID | None = None
    plugin_runtime_bundle_id: UUID | None = None


class ArchiveManifestInspectInput(StrictModel):
    manifest_name: str = Field(pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}$")
    provider: str = Field(min_length=1, max_length=200)
    source_license: str = Field(min_length=1, max_length=500)
    universe_name: str = Field(min_length=1, max_length=200)
    source_spec: dict[str, Any]
    plugin_release_id: UUID | None = None
    plugin_runtime_bundle_id: UUID | None = None


class ArchiveManifestMaterializeInput(StrictModel):
    catalog_name: str = Field(pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}$")
    instrument: str = Field(min_length=1, max_length=200)
    instrument_symbol: str | None = Field(default=None, min_length=1, max_length=120)
    start: datetime
    end: datetime


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


class ArchiveManifestView(StrictModel):
    id: UUID
    manifest_uri: str
    data_source_id: UUID
    universe_version_id: UUID
    provider: str
    source_license: str
    source_spec: dict[str, Any]
    coverage_start: str
    coverage_end: str
    scanned_until: str
    shard_count: int
    total_bytes: int
    missing_shard_count: int
    probe_error_count: int
    schema_revision: str
    state: str
    point_in_time_result: dict[str, Any]
    created_at: str | None = None


class ArchiveManifestShardView(StrictModel):
    id: UUID
    manifest_id: UUID
    shard_key: str
    source_url: str
    coverage_start: str
    coverage_end: str
    size_bytes: int | None
    state: str
    observed_at: str


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


def _archive_materialization_timeout(shard_count: int) -> float:
    """Allow one bounded request to cover the runtime's per-shard download deadline."""

    return max(120.0, 180.0 * shard_count + 60.0)


def _validate_archive_range_shards(
    shards: list[ArchiveManifestShard],
    *,
    start: datetime,
    requested_hours: int,
) -> None:
    for index, shard in enumerate(shards):
        expected_start = start + timedelta(hours=index)
        expected_end = expected_start + timedelta(hours=1)
        if shard.coverage_start != expected_start or shard.coverage_end != expected_end:
            raise QfError(
                "ARCHIVE_MANIFEST_RANGE_INVALID",
                "The manifest must contain exactly one UTC-hour descriptor per requested hour.",
                409,
                {
                    "index": index,
                    "expected_start": expected_start.isoformat(),
                    "expected_end": expected_end.isoformat(),
                    "actual_start": shard.coverage_start.isoformat(),
                    "actual_end": shard.coverage_end.isoformat(),
                    "requested_hours": requested_hours,
                },
            )


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


def _remote_manifest_name(payload: ArchiveManifestInspectInput) -> str:
    return f"research-{payload.manifest_name}"


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


def _manifest_view(item: ArchiveManifest) -> ArchiveManifestView:
    return ArchiveManifestView(
        id=item.id,
        manifest_uri=item.manifest_uri,
        data_source_id=item.data_source_id,
        universe_version_id=item.universe_version_id,
        provider=item.provider,
        source_license=item.source_license,
        source_spec=item.source_spec,
        coverage_start=item.coverage_start.isoformat(),
        coverage_end=item.coverage_end.isoformat(),
        scanned_until=item.scanned_until.isoformat(),
        shard_count=item.shard_count,
        total_bytes=item.total_bytes,
        missing_shard_count=item.missing_shard_count,
        probe_error_count=item.probe_error_count,
        schema_revision=item.schema_revision,
        state=item.state,
        point_in_time_result=item.point_in_time_result,
        created_at=item.created_at.isoformat() if item.created_at else None,
    )


def _manifest_shard_view(item: ArchiveManifestShard) -> ArchiveManifestShardView:
    return ArchiveManifestShardView(
        id=item.id,
        manifest_id=item.manifest_id,
        shard_key=item.shard_key,
        source_url=item.source_url,
        coverage_start=item.coverage_start.isoformat(),
        coverage_end=item.coverage_end.isoformat(),
        size_bytes=item.size_bytes,
        state=item.state,
        observed_at=item.observed_at.isoformat(),
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
    plugin_binding: dict[str, str] | None,
) -> bool:
    revision = session.get(DatasetRevision, item.dataset_revision_id)
    source = (
        session.get(GovernedDataSource, revision.data_source_id)
        if revision is not None and revision.data_source_id is not None
        else None
    )
    public_config = source.public_config if source is not None else {}
    stored_source_spec = public_config.get("source_spec") if isinstance(public_config, dict) else None
    stored_plugin_binding = (
        public_config.get("plugin_binding") if isinstance(public_config, dict) else None
    )
    return any(
        (
            revision is None,
            revision is not None and revision.universe_version_id != universe_id,
            revision is not None and revision.universe_name != payload.universe_name,
            item.provider != payload.provider,
            item.source_license != payload.source_license,
            item.sealed != payload.sealed,
            not isinstance(stored_source_spec, dict) or stored_source_spec != payload.source_spec,
            stored_plugin_binding != plugin_binding,
        )
    )


def _manifest_input_conflict(
    session: Any,
    item: ArchiveManifest,
    payload: ArchiveManifestInspectInput,
    universe_id: UUID,
    plugin_binding: dict[str, str] | None,
) -> bool:
    source = session.get(GovernedDataSource, item.data_source_id)
    public_config = source.public_config if source is not None else {}
    stored_plugin_binding = (
        public_config.get("plugin_binding") if isinstance(public_config, dict) else None
    )
    return any(
        (
            item.universe_version_id != universe_id,
            item.provider != payload.provider,
            item.source_license != payload.source_license,
            item.source_spec != payload.source_spec,
            stored_plugin_binding != plugin_binding,
        )
    )


def _resolve_plugin_binding(
    session: Any,
    payload: CatalogIngestInput | ArchiveManifestInspectInput,
    *,
    allow_draining: bool = False,
) -> dict[str, str] | None:
    source_kind = payload.source_spec.get("kind")
    supplied_ids = (payload.plugin_release_id, payload.plugin_runtime_bundle_id)
    if source_kind != "plugin":
        if any(value is not None for value in supplied_ids):
            raise QfError(
                "PLUGIN_BINDING_INVALID",
                "A plugin release and runtime bundle are only valid for source_spec.kind=plugin.",
                422,
            )
        return None
    if payload.plugin_release_id is None or payload.plugin_runtime_bundle_id is None:
        raise QfError(
            "PLUGIN_BINDING_REQUIRED",
            "Plugin Catalog ingest requires an active release and a ready runtime bundle.",
            422,
        )
    release = session.get(PluginRelease, payload.plugin_release_id)
    allowed_release_states = {"ACTIVE"}
    if allow_draining:
        allowed_release_states.add("DRAINING")
    if release is None or release.state not in allowed_release_states:
        raise QfError(
            "PLUGIN_RELEASE_NOT_ACTIVE",
            "Catalog ingest requires an ACTIVE plugin release.",
            409,
            {"plugin_release_id": str(payload.plugin_release_id)},
        )
    capabilities = release.descriptor_snapshot.get("capabilities", [])
    if "HISTORICAL_IMPORT" not in capabilities:
        raise QfError(
            "PLUGIN_CAPABILITY_FORBIDDEN",
            "The selected plugin release does not provide historical import.",
            422,
            {"plugin_id": release.plugin_id, "version": release.version},
        )
    bundle = session.get(PluginRuntimeBundle, payload.plugin_runtime_bundle_id)
    if bundle is None or bundle.state != "READY":
        raise QfError(
            "PLUGIN_BUNDLE_NOT_READY",
            "Catalog ingest requires a READY plugin runtime bundle.",
            409,
            {"plugin_runtime_bundle_id": str(payload.plugin_runtime_bundle_id)},
        )
    member = session.scalar(
        select(PluginRuntimeBundleMember).where(
            PluginRuntimeBundleMember.runtime_bundle_id == bundle.id,
            PluginRuntimeBundleMember.plugin_release_id == release.id,
            PluginRuntimeBundleMember.member_role == "IMPORTER",
        )
    )
    if member is None:
        raise QfError(
            "PLUGIN_BUNDLE_MEMBER_REQUIRED",
            "The READY plugin runtime bundle must contain the release as an IMPORTER.",
            422,
            {"plugin_release_id": str(release.id), "bundle_id": str(bundle.id)},
        )
    return {
        "plugin_release_id": str(release.id),
        "plugin_runtime_bundle_id": str(bundle.id),
        "plugin_id": release.plugin_id,
        "plugin_version": release.version,
        "plugin_bundle_path": bundle.environment_path,
    }


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
    return _ingest_catalog(payload, request)


def _ingest_catalog(
    payload: CatalogIngestInput,
    request: Request,
    *,
    source_shards: list[dict[str, Any]] | None = None,
    allow_draining_plugin: bool = False,
) -> CatalogView:
    factory = request.app.state.session_factory
    remote_catalog_uri = f"catalog://{_remote_catalog_name(payload)}"
    with factory() as session:
        plugin_binding = _resolve_plugin_binding(
            session,
            payload,
            allow_draining=allow_draining_plugin,
        )
        if payload.universe_version_id is not None:
            universe = session.get(MarketUniverseVersion, payload.universe_version_id)
            if universe is None or universe.name != payload.universe_name:
                raise QfError(
                    "UNIVERSE_VERSION_NOT_FOUND",
                    "The pinned Universe Version does not exist for this catalog.",
                    422,
                    {"universe_version_id": str(payload.universe_version_id)},
                )
        else:
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
            if _catalog_binding_input_conflict(
                session, existing, payload, universe_id, plugin_binding
            ):
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
            source_shards=source_shards,
            plugin_id=plugin_binding["plugin_id"] if plugin_binding else None,
            plugin_version=plugin_binding["plugin_version"] if plugin_binding else None,
            plugin_bundle_path=plugin_binding["plugin_bundle_path"] if plugin_binding else None,
        ),
        timeout_seconds=(
            _archive_materialization_timeout(len(source_shards))
            if source_shards is not None
            else None
        ),
    )
    with factory.begin() as session:
        existing = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.catalog_uri == descriptor.catalog_uri
            )
        )
        if existing is not None:
            if _catalog_binding_input_conflict(
                session, existing, payload, universe_id, plugin_binding
            ):
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
                "plugin_binding": plugin_binding,
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


@router.post(
    "/quant-runtime/archive-manifests/inspect",
    response_model=ArchiveManifestView,
    status_code=201,
)
def inspect_archive_manifest(
    payload: ArchiveManifestInspectInput,
    request: Request,
) -> ArchiveManifestView:
    factory = request.app.state.session_factory
    manifest_uri = f"manifest://{_remote_manifest_name(payload)}"
    with factory() as session:
        plugin_binding = _resolve_plugin_binding(session, payload)
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
                "Archive manifest inspection requires an active governed Universe Version.",
                422,
                {"universe_name": payload.universe_name},
            )
        existing = session.scalar(
            select(ArchiveManifest).where(ArchiveManifest.manifest_uri == manifest_uri)
        )
        if existing is not None:
            if _manifest_input_conflict(session, existing, payload, universe.id, plugin_binding):
                raise QfError(
                    "ARCHIVE_MANIFEST_INPUT_CONFLICT",
                    "Manifest identity is already bound to different immutable inspection inputs.",
                    409,
                    {"manifest_uri": manifest_uri},
                )
            return _manifest_view(existing)

    descriptor = _runtime().inspect_archive_manifest(
        ArchiveManifestSpec(
            manifest_name=_remote_manifest_name(payload),
            provider=payload.provider,
            source_license=payload.source_license,
            source_spec=payload.source_spec,
            plugin_id=plugin_binding["plugin_id"] if plugin_binding else None,
            plugin_version=plugin_binding["plugin_version"] if plugin_binding else None,
            plugin_bundle_path=plugin_binding["plugin_bundle_path"] if plugin_binding else None,
        )
    )
    try:
        with factory.begin() as session:
            existing = session.scalar(
                select(ArchiveManifest).where(ArchiveManifest.manifest_uri == descriptor.manifest_uri)
            )
            if existing is not None:
                if _manifest_input_conflict(session, existing, payload, universe.id, plugin_binding):
                    raise QfError(
                        "ARCHIVE_MANIFEST_INPUT_CONFLICT",
                        "Manifest identity is already bound to different immutable inspection inputs.",
                        409,
                        {"manifest_uri": descriptor.manifest_uri},
                    )
                return _manifest_view(existing)

            source = GovernedDataSource(
                name=f"Archive manifest {payload.manifest_name}",
                provider=descriptor.provider,
                state="ACTIVE",
                universe_scope=[payload.universe_name],
                fields=["archive_shard_metadata"],
                update_cadence="on-demand manifest inspection",
                preflight_state="READY",
                public_config={
                    "source_spec": payload.source_spec,
                    "plugin_binding": plugin_binding,
                    "manifest_uri": descriptor.manifest_uri,
                },
            )
            session.add(source)
            session.flush()
            manifest = ArchiveManifest(
                manifest_uri=descriptor.manifest_uri,
                data_source_id=source.id,
                universe_version_id=universe.id,
                provider=descriptor.provider,
                source_license=descriptor.source_license,
                source_spec=descriptor.source_spec,
                coverage_start=descriptor.coverage_start,
                coverage_end=descriptor.coverage_end,
                scanned_until=descriptor.scanned_until,
                shard_count=descriptor.shard_count,
                total_bytes=descriptor.total_bytes,
                missing_shard_count=descriptor.missing_shard_count,
                probe_error_count=descriptor.probe_error_count,
                schema_revision=descriptor.schema_revision,
                state="ACTIVE",
                point_in_time_result=descriptor.point_in_time_result,
            )
            session.add(manifest)
            session.flush()
            session.add_all(
                [
                    ArchiveManifestShard(
                        manifest_id=manifest.id,
                        shard_key=shard.shard_key,
                        source_url=shard.source_url,
                        coverage_start=shard.coverage_start,
                        coverage_end=shard.coverage_end,
                        size_bytes=shard.size_bytes,
                        state=shard.state,
                        observed_at=shard.observed_at,
                    )
                    for shard in descriptor.shards
                ]
            )
            session.flush()
            return _manifest_view(manifest)
    except IntegrityError as exc:
        # Two operators may scan the same immutable manifest concurrently. The
        # unique URI is the database arbiter; reload after the losing transaction
        # rolls back, then verify the original immutable inputs before returning it.
        with factory() as session:
            existing = session.scalar(
                select(ArchiveManifest).where(ArchiveManifest.manifest_uri == descriptor.manifest_uri)
            )
            if existing is None:
                raise
            if _manifest_input_conflict(session, existing, payload, universe.id, plugin_binding):
                raise QfError(
                    "ARCHIVE_MANIFEST_INPUT_CONFLICT",
                    "Manifest identity is already bound to different immutable inspection inputs.",
                    409,
                    {"manifest_uri": descriptor.manifest_uri},
                ) from exc
            return _manifest_view(existing)


@router.get(
    "/quant-runtime/archive-manifests",
    response_model=list[ArchiveManifestView],
)
def list_archive_manifests(request: Request) -> list[ArchiveManifestView]:
    factory = request.app.state.session_factory
    with factory() as session:
        items = session.scalars(
            select(ArchiveManifest).order_by(ArchiveManifest.created_at.desc())
        ).all()
        return [_manifest_view(item) for item in items]


@router.get(
    "/quant-runtime/archive-manifests/{manifest_id}/shards",
    response_model=list[ArchiveManifestShardView],
)
def list_archive_manifest_shards(
    manifest_id: UUID,
    request: Request,
) -> list[ArchiveManifestShardView]:
    factory = request.app.state.session_factory
    with factory() as session:
        if session.get(ArchiveManifest, manifest_id) is None:
            raise QfError("ARCHIVE_MANIFEST_NOT_FOUND", "Archive manifest does not exist.", 404)
        items = session.scalars(
            select(ArchiveManifestShard)
            .where(ArchiveManifestShard.manifest_id == manifest_id)
            .order_by(ArchiveManifestShard.coverage_start.asc())
        ).all()
        return [_manifest_shard_view(item) for item in items]


@router.post(
    "/quant-runtime/archive-manifests/{manifest_id}/materialize",
    response_model=CatalogView,
    status_code=201,
)
def materialize_archive_manifest(
    manifest_id: UUID,
    payload: ArchiveManifestMaterializeInput,
    request: Request,
) -> CatalogView:
    """Materialize one bounded instrument/time slice from an immutable manifest."""

    def utc_hour(value: datetime, field_name: str) -> datetime:
        normalized = value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)
        if normalized.minute or normalized.second or normalized.microsecond:
            raise QfError(
                "ARCHIVE_RANGE_NOT_ALIGNED",
                f"{field_name} must be aligned to a UTC hour.",
                422,
            )
        return normalized

    start = utc_hour(payload.start, "start")
    end = utc_hour(payload.end, "end")
    if end <= start:
        raise QfError("ARCHIVE_RANGE_INVALID", "end must be after start.", 422)
    requested_hours = int((end - start) / timedelta(hours=1))
    if requested_hours > 168:
        raise QfError(
            "ARCHIVE_MATERIALIZATION_TOO_LARGE",
            "One materialization may cover at most 168 UTC hours.",
            422,
            {"requested_hours": requested_hours, "max_hours": 168},
        )

    factory = request.app.state.session_factory
    with factory() as session:
        manifest = session.get(ArchiveManifest, manifest_id)
        if manifest is None:
            raise QfError("ARCHIVE_MANIFEST_NOT_FOUND", "Archive manifest does not exist.", 404)
        if manifest.state != "ACTIVE":
            raise QfError("ARCHIVE_MANIFEST_NOT_ACTIVE", "Archive manifest is not active.", 409)
        if start < manifest.coverage_start or end > manifest.coverage_end:
            raise QfError(
                "ARCHIVE_RANGE_OUTSIDE_MANIFEST",
                "The requested UTC range is outside the immutable manifest coverage.",
                422,
                {
                    "manifest_start": manifest.coverage_start.isoformat(),
                    "manifest_end": manifest.coverage_end.isoformat(),
                },
            )
        source = session.get(GovernedDataSource, manifest.data_source_id)
        universe = session.get(MarketUniverseVersion, manifest.universe_version_id)
        public_config = source.public_config if source is not None else {}
        stored_binding = public_config.get("plugin_binding") if isinstance(public_config, dict) else None
        if universe is None or not isinstance(stored_binding, dict):
            raise QfError(
                "ARCHIVE_MANIFEST_BINDING_INVALID",
                "The manifest is missing its governed Universe or plugin binding.",
                409,
            )
        manifest_source_spec = manifest.source_spec
        if (
            not isinstance(manifest_source_spec, dict)
            or manifest_source_spec.get("kind") != "plugin"
            or not isinstance(manifest_source_spec.get("config"), dict)
        ):
            raise QfError(
                "ARCHIVE_MANIFEST_SOURCE_INVALID",
                "The manifest source specification is not a plugin source.",
                409,
            )
        range_shards = session.scalars(
            select(ArchiveManifestShard)
            .where(
                ArchiveManifestShard.manifest_id == manifest.id,
                ArchiveManifestShard.coverage_start >= start,
                ArchiveManifestShard.coverage_end <= end,
            )
            .order_by(ArchiveManifestShard.coverage_start.asc())
        ).all()
        if len(range_shards) != requested_hours:
            raise QfError(
                "ARCHIVE_MANIFEST_RANGE_INCOMPLETE",
                "The manifest does not contain one hourly descriptor for the requested range.",
                409,
                {"expected_hours": requested_hours, "described_hours": len(range_shards)},
            )
        _validate_archive_range_shards(
            range_shards,
            start=start,
            requested_hours=requested_hours,
        )
        available_shards = [item for item in range_shards if item.state == "AVAILABLE"]
        if not available_shards:
            raise QfError(
                "ARCHIVE_RANGE_HAS_NO_DATA",
                "No AVAILABLE archive shard exists in the requested range.",
                422,
            )
        if any(item.size_bytes is None for item in available_shards):
            raise QfError(
                "ARCHIVE_MATERIALIZATION_SIZE_UNKNOWN",
                "Bounded materialization requires a known size for every AVAILABLE shard.",
                409,
            )
        estimated_bytes = sum(item.size_bytes for item in available_shards if item.size_bytes is not None)
        if estimated_bytes > 20 * 1024 * 1024 * 1024:
            raise QfError(
                "ARCHIVE_MATERIALIZATION_TOO_LARGE",
                "The selected archive shards exceed the 20 GiB materialization limit.",
                422,
                {"estimated_source_bytes": estimated_bytes, "max_source_bytes": 20 * 1024 * 1024 * 1024},
            )
        try:
            plugin_release_id = UUID(str(stored_binding["plugin_release_id"]))
            plugin_bundle_id = UUID(str(stored_binding["plugin_runtime_bundle_id"]))
        except (KeyError, TypeError, ValueError) as exc:
            raise QfError(
                "ARCHIVE_MANIFEST_BINDING_INVALID",
                "The manifest plugin binding is malformed.",
                409,
            ) from exc
        base_config = dict(manifest_source_spec["config"])
        base_config["selection"] = "instrument_history"
        base_config["instrument"] = payload.instrument
        if payload.instrument_symbol is not None:
            base_config["instrument_symbol"] = payload.instrument_symbol
        base_config["archive_start"] = start.isoformat()
        base_config["archive_end"] = end.isoformat()
        selected_shards = [
            {
                "shard_key": item.shard_key,
                "source_url": item.source_url,
                "coverage_start": item.coverage_start.isoformat(),
                "coverage_end": item.coverage_end.isoformat(),
                "size_bytes": item.size_bytes,
                "state": item.state,
                "observed_at": item.observed_at.isoformat(),
            }
            for item in available_shards
        ]
        materialized_source_spec = {
            "kind": "plugin",
            "config": base_config,
            "manifest_uri": manifest.manifest_uri,
            "shard_keys": [item.shard_key for item in available_shards],
            "materialization": {
                "start": start.isoformat(),
                "end": end.isoformat(),
                "source_shard_count": len(available_shards),
                "requested_shard_count": len(range_shards),
                "missing_shard_count": sum(item.state == "MISSING" for item in range_shards),
                "probe_error_count": sum(item.state == "PROBE_ERROR" for item in range_shards),
                "estimated_source_bytes": estimated_bytes,
            },
        }
        catalog_payload = CatalogIngestInput(
            catalog_name=payload.catalog_name,
            provider=manifest.provider,
            source_license=manifest.source_license,
            universe_name=universe.name,
            universe_version_id=manifest.universe_version_id,
            sealed=False,
            source_spec=materialized_source_spec,
            plugin_release_id=plugin_release_id,
            plugin_runtime_bundle_id=plugin_bundle_id,
        )
    return _ingest_catalog(
        catalog_payload,
        request,
        source_shards=selected_shards,
        allow_draining_plugin=True,
    )


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
