"""Bounded, fail-closed Dataset Revision materialization worker."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from math import isfinite
from typing import Any
from uuid import UUID

from sqlalchemy import func, select

from db.models import (
    ArchiveManifest,
    ArchiveManifestShard,
    DataQualityResult,
    DatasetRevision,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    NautilusCatalogBinding,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from db.session import SessionFactory, create_database_engine
from errors import QfError
from events import append_event
from jobs import JobLease, create_lease_fenced_session_factory
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import CatalogDescriptor, CatalogIngestSpec
from quant_runtime.remote import NautilusQuantRuntime
from runtime_config import load_effective_settings
from settings import Settings


@dataclass(frozen=True)
class _MaterializationContext:
    dataset_id: UUID
    data_class: str | None
    catalog_name: str
    provider: str
    source_license: str
    source_spec: dict[str, Any]
    source_shards: list[dict[str, Any]]
    plugin_binding: dict[str, str]
    missing_shard_count: int
    probe_error_count: int
    requested_schema_version: str | None
    requested_data_type: str | None
    requested_instrument_scope: tuple[str, ...]
    requested_event_start: datetime
    requested_event_end: datetime
    requested_available_start: datetime
    requested_available_end: datetime
    quality_requirements: dict[str, Any]
    point_in_time_requirements: dict[str, Any]


def _utc_hour(value: datetime, field_name: str) -> datetime:
    normalized = value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)
    if normalized.minute or normalized.second or normalized.microsecond:
        raise QfError(
            "DATASET_MATERIALIZATION_RANGE_NOT_ALIGNED",
            f"{field_name} must be aligned to a UTC hour.",
            422,
        )
    return normalized


def _utc(value: datetime) -> datetime:
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _archive_materialization_timeout(shard_count: int) -> float:
    return max(120.0, 180.0 * shard_count + 60.0)


def _runtime() -> NautilusQuantRuntime:
    config = RemoteNautilusConfig.from_env(required=True, profile="research")
    assert config is not None
    return NautilusQuantRuntime(config)


def _validate_range_shards(
    shards: list[ArchiveManifestShard], *, start: datetime, requested_hours: int
) -> None:
    for index, shard in enumerate(shards):
        expected_start = start + timedelta(hours=index)
        expected_end = expected_start + timedelta(hours=1)
        if _utc(shard.coverage_start) != expected_start or _utc(shard.coverage_end) != expected_end:
            raise QfError(
                "DATASET_MATERIALIZATION_RANGE_INVALID",
                "The archive manifest must contain one contiguous descriptor per UTC hour.",
                409,
                {"index": index, "requested_hours": requested_hours},
            )


def _require_uuid(value: object, *, code: str, message: str) -> UUID:
    try:
        return UUID(str(value))
    except (TypeError, ValueError) as exc:
        raise QfError(code, message, 409) from exc


def _resolve_plugin_binding(session: Any, binding: dict[str, Any]) -> dict[str, str]:
    release_id = _require_uuid(
        binding.get("plugin_release_id"),
        code="DATASET_MATERIALIZATION_BINDING_INVALID",
        message="Data Source plugin release binding is invalid.",
    )
    bundle_id = _require_uuid(
        binding.get("plugin_runtime_bundle_id"),
        code="DATASET_MATERIALIZATION_BINDING_INVALID",
        message="Data Source plugin runtime bundle binding is invalid.",
    )
    release = session.get(PluginRelease, release_id)
    if release is None or release.state not in {"ACTIVE", "DRAINING"}:
        raise QfError(
            "DATASET_MATERIALIZATION_PLUGIN_RELEASE_NOT_ACTIVE",
            "Dataset materialization requires an ACTIVE or DRAINING plugin release.",
            409,
        )
    capabilities = release.descriptor_snapshot.get("capabilities", [])
    if not isinstance(capabilities, list) or "HISTORICAL_IMPORT" not in capabilities:
        raise QfError(
            "DATASET_MATERIALIZATION_PLUGIN_CAPABILITY_FORBIDDEN",
            "Dataset materialization requires historical-import capability.",
            422,
        )
    bundle = session.get(PluginRuntimeBundle, bundle_id)
    if bundle is None or bundle.state != "READY":
        raise QfError(
            "DATASET_MATERIALIZATION_PLUGIN_BUNDLE_NOT_READY",
            "Dataset materialization requires a READY plugin runtime bundle.",
            409,
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
            "DATASET_MATERIALIZATION_PLUGIN_BUNDLE_MEMBER_REQUIRED",
            "The plugin runtime bundle must include the release as an IMPORTER.",
            422,
        )
    return {
        "plugin_id": release.plugin_id,
        "plugin_version": release.version,
        "plugin_bundle_path": bundle.environment_path,
    }


def _source_shard_payload(shards: list[ArchiveManifestShard]) -> list[dict[str, Any]]:
    return [
        {
            "shard_key": item.shard_key,
            "source_url": item.source_url,
            "coverage_start": _utc(item.coverage_start).isoformat(),
            "coverage_end": _utc(item.coverage_end).isoformat(),
            "size_bytes": item.size_bytes,
            "state": item.state,
            "observed_at": _utc(item.observed_at).isoformat(),
        }
        for item in shards
    ]


def _prepare_materialization(factory: SessionFactory, job_id: UUID) -> _MaterializationContext:
    """Load only a registered, bounded archive selection before contacting the runtime."""
    with factory() as session:
        job = session.get(Job, job_id)
        if job is None or job.kind != "DATASET_MATERIALIZATION":
            raise QfError("JOB_NOT_FOUND", "Dataset materialization job does not exist.", 404)
        if job.state != "LEASED":
            raise QfError("JOB_STATE_CONFLICT", "Dataset materialization job is not leased.", 409)
        if job.resource_type != "dataset_revision":
            raise QfError(
                "DATASET_MATERIALIZATION_RESOURCE_INVALID",
                "Dataset materialization job has an invalid resource type.",
                409,
            )
        revision = session.get(DatasetRevision, job.resource_id)
        if revision is None:
            raise QfError("DATASET_NOT_FOUND", "Dataset Revision was not found.", 404)
        if revision.partition == "SEALED":
            raise QfError(
                "DATASET_MATERIALIZATION_SEALED_PARTITION_FORBIDDEN",
                "Plugin-backed research materialization cannot provision SEALED Dataset Revisions.",
                409,
            )
        if revision.quality_state != "PENDING" or revision.point_in_time_state != "PENDING":
            raise QfError(
                "DATASET_MATERIALIZATION_STATE_CONFLICT",
                "Dataset Revision is no longer pending materialization.",
                409,
            )
        if revision.data_source_id is None or revision.universe_version_id is None:
            raise QfError(
                "DATASET_MATERIALIZATION_SOURCE_INVALID",
                "Dataset Revision must reference a governed Data Source and Universe Version.",
                409,
            )
        source = session.get(GovernedDataSource, revision.data_source_id)
        universe = session.get(MarketUniverseVersion, revision.universe_version_id)
        if source is None or universe is None:
            raise QfError(
                "DATASET_MATERIALIZATION_SOURCE_INVALID",
                "Dataset materialization source or Universe Version is unavailable.",
                409,
            )
        if source.state != "ACTIVE" or source.preflight_state != "READY":
            raise QfError(
                "DATASET_MATERIALIZATION_SOURCE_NOT_READY",
                "Dataset materialization requires an ACTIVE, preflight-READY Data Source.",
                409,
            )
        config = source.public_config
        if not isinstance(config, dict):
            raise QfError(
                "DATASET_MATERIALIZATION_CONNECTOR_UNSUPPORTED",
                "Data Source does not provide a registered archive materialization contract.",
                409,
            )
        manifest_id = _require_uuid(
            config.get("archive_manifest_id"),
            code="DATASET_MATERIALIZATION_CONNECTOR_UNSUPPORTED",
            message="Data Source must pin archive_manifest_id for materialization.",
        )
        manifest = session.get(ArchiveManifest, manifest_id)
        if (
            manifest is None
            or manifest.state != "ACTIVE"
            or manifest.data_source_id != source.id
            or manifest.universe_version_id != universe.id
        ):
            raise QfError(
                "DATASET_MATERIALIZATION_MANIFEST_INVALID",
                "The pinned archive manifest is not active for this Data Source and Universe Version.",
                409,
            )
        if (
            not isinstance(manifest.source_spec, dict)
            or manifest.source_spec.get("kind") != "plugin"
            or not isinstance(manifest.source_spec.get("config"), dict)
        ):
            raise QfError(
                "DATASET_MATERIALIZATION_CONNECTOR_UNSUPPORTED",
                "Only registered plugin archive manifests can be materialized.",
                409,
            )
        binding = config.get("plugin_binding")
        if not isinstance(binding, dict):
            raise QfError(
                "DATASET_MATERIALIZATION_BINDING_INVALID",
                "Data Source is missing its registered plugin binding.",
                409,
            )
        plugin_binding = _resolve_plugin_binding(session, binding)

        request = revision.materialization_request
        quality_requirements = request.get("quality_requirements") if isinstance(request, dict) else None
        point_in_time_requirements = (
            request.get("point_in_time_requirements") if isinstance(request, dict) else None
        )
        if not isinstance(quality_requirements, dict) or not isinstance(
            point_in_time_requirements, dict
        ):
            raise QfError(
                "DATASET_MATERIALIZATION_REQUIREMENTS_INVALID",
                "Dataset materialization requires the typed quality and point-in-time requirements.",
                409,
            )
        if set(point_in_time_requirements) != {"available_at"} or point_in_time_requirements.get(
            "available_at"
        ) != "required":
            raise QfError(
                "DATASET_MATERIALIZATION_REQUIREMENTS_INVALID",
                "Point-in-time requirements must request available_at.",
                409,
            )
        if set(quality_requirements) == {"coverage"}:
            if quality_requirements.get("coverage") != "required":
                raise QfError(
                    "DATASET_MATERIALIZATION_REQUIREMENTS_INVALID",
                    "Coverage requirement must be required.",
                    409,
                )
        elif set(quality_requirements) == {"minimum_coverage"}:
            minimum_coverage = quality_requirements.get("minimum_coverage")
            if (
                isinstance(minimum_coverage, bool)
                or not isinstance(minimum_coverage, (int, float))
                or not isfinite(float(minimum_coverage))
                or not 0 <= float(minimum_coverage) <= 1
            ):
                raise QfError(
                    "DATASET_MATERIALIZATION_REQUIREMENTS_INVALID",
                    "minimum_coverage must be a finite ratio between zero and one.",
                    409,
                )
        else:
            raise QfError(
                "DATASET_MATERIALIZATION_REQUIREMENTS_INVALID",
                "Quality requirements must use one supported typed field.",
                409,
            )
        instruments = request.get("instrument_scope") if isinstance(request, dict) else None
        if (
            not isinstance(instruments, list)
            or len(instruments) != 1
            or not isinstance(instruments[0], str)
        ):
            raise QfError(
                "DATASET_MATERIALIZATION_INSTRUMENT_SCOPE_INVALID",
                "Archive materialization requires exactly one configured instrument.",
                422,
            )
        if revision.event_start is None or revision.event_end is None:
            raise QfError(
                "DATASET_MATERIALIZATION_RANGE_INVALID",
                "Dataset materialization requires an event-time range.",
                422,
            )
        start = _utc_hour(revision.event_start, "event_start")
        end = _utc_hour(revision.event_end, "event_end")
        if end <= start:
            raise QfError(
                "DATASET_MATERIALIZATION_RANGE_INVALID",
                "Dataset materialization requires a non-empty half-open time range.",
                422,
            )
        requested_hours = int((end - start) / timedelta(hours=1))
        if requested_hours > 168:
            raise QfError(
                "DATASET_MATERIALIZATION_TOO_LARGE",
                "One materialization may cover at most 168 UTC hours.",
                422,
            )
        if start < _utc(manifest.coverage_start) or end > _utc(manifest.coverage_end):
            raise QfError(
                "DATASET_MATERIALIZATION_RANGE_OUTSIDE_MANIFEST",
                "The requested range is outside the pinned archive manifest.",
                422,
            )
        range_shards = list(
            session.scalars(
                select(ArchiveManifestShard)
                .where(
                    ArchiveManifestShard.manifest_id == manifest.id,
                    ArchiveManifestShard.coverage_start >= start,
                    ArchiveManifestShard.coverage_end <= end,
                )
                .order_by(ArchiveManifestShard.coverage_start.asc())
            )
        )
        if len(range_shards) != requested_hours:
            raise QfError(
                "DATASET_MATERIALIZATION_RANGE_INCOMPLETE",
                "The archive manifest does not contain one descriptor per requested UTC hour.",
                409,
            )
        _validate_range_shards(range_shards, start=start, requested_hours=requested_hours)
        available = [item for item in range_shards if item.state == "AVAILABLE"]
        if not available:
            raise QfError(
                "DATASET_MATERIALIZATION_NO_AVAILABLE_SHARDS",
                "No available archive shards exist in the requested range.",
                422,
            )
        if any(item.size_bytes is None for item in available):
            raise QfError(
                "DATASET_MATERIALIZATION_SIZE_UNKNOWN",
                "Every selected archive shard must have a known size.",
                409,
            )
        estimated_bytes = sum(item.size_bytes for item in available if item.size_bytes is not None)
        if estimated_bytes > 20 * 1024 * 1024 * 1024:
            raise QfError(
                "DATASET_MATERIALIZATION_TOO_LARGE",
                "Selected archive shards exceed the 20 GiB materialization limit.",
                422,
            )
        source_spec = dict(manifest.source_spec)
        source_config = dict(manifest.source_spec["config"])
        source_config.update(
            {
                "selection": "instrument_history",
                "instrument": instruments[0],
                "archive_start": start.isoformat(),
                "archive_end": end.isoformat(),
            }
        )
        source_spec["config"] = source_config
        source_spec["manifest_uri"] = manifest.manifest_uri
        source_spec["shard_keys"] = [item.shard_key for item in available]
        source_spec["materialization"] = {
            "start": start.isoformat(),
            "end": end.isoformat(),
            "source_shard_count": len(available),
            "requested_shard_count": len(range_shards),
            "missing_shard_count": sum(item.state == "MISSING" for item in range_shards),
            "probe_error_count": sum(item.state == "PROBE_ERROR" for item in range_shards),
            "estimated_source_bytes": estimated_bytes,
        }
        return _MaterializationContext(
            dataset_id=revision.id,
            data_class=revision.data_class,
            catalog_name=f"dataset-{revision.id}",
            provider=manifest.provider,
            source_license=manifest.source_license,
            source_spec=source_spec,
            source_shards=_source_shard_payload(available),
            plugin_binding=plugin_binding,
            missing_shard_count=sum(item.state == "MISSING" for item in range_shards),
            probe_error_count=sum(item.state == "PROBE_ERROR" for item in range_shards),
            requested_schema_version=(
                str(request.get("schema_version"))
                if isinstance(request, dict) and request.get("schema_version")
                else revision.schema_version
            ),
            requested_data_type=(
                str(request.get("data_type"))
                if isinstance(request, dict) and request.get("data_type")
                else None
            ),
            requested_instrument_scope=tuple(instruments),
            requested_event_start=start,
            requested_event_end=end,
            requested_available_start=_utc(revision.available_start)
            if revision.available_start is not None
            else start,
            requested_available_end=_utc(revision.available_end)
            if revision.available_end is not None
            else end,
            quality_requirements=quality_requirements,
            point_in_time_requirements=point_in_time_requirements,
        )


def _terminal_states(
    context: _MaterializationContext, descriptor: CatalogDescriptor
) -> tuple[str, str, str, list[str], list[str]]:
    quality_reasons: list[str] = []
    pit_reasons: list[str] = []
    if descriptor.quality_result.get("valid") is not True:
        quality_reasons.append("REMOTE_QUALITY_INVALID")
    if "minimum_coverage" in context.quality_requirements:
        coverage = descriptor.quality_result.get("coverage")
        minimum_coverage = context.quality_requirements["minimum_coverage"]
        if (
            isinstance(coverage, bool)
            or not isinstance(coverage, (int, float))
            or not isfinite(float(coverage))
            or float(coverage) < float(minimum_coverage)
        ):
            quality_reasons.append("QUALITY_COVERAGE_BELOW_REQUIREMENT")
    if descriptor.row_count <= 0:
        quality_reasons.append("DATA_EMPTY")
    if context.missing_shard_count:
        quality_reasons.append("DATA_SHARD_MISSING")
    if context.probe_error_count:
        quality_reasons.append("DATA_SOURCE_PROBE_ERROR")
    if descriptor.point_in_time_result.get("valid") is not True:
        pit_reasons.append("REMOTE_POINT_IN_TIME_INVALID")
    values = (
        descriptor.event_start,
        descriptor.event_end,
        descriptor.available_start,
        descriptor.available_end,
    )
    if any(value is None or value.tzinfo is None for value in values):
        pit_reasons.append("DATA_TIMESTAMP_INVALID")
    elif (
        descriptor.event_start > descriptor.event_end  # type: ignore[operator]
        or descriptor.available_start > descriptor.available_end  # type: ignore[operator]
        or descriptor.event_start > descriptor.available_start  # type: ignore[operator]
        or descriptor.event_end > descriptor.available_end  # type: ignore[operator]
    ):
        pit_reasons.append("POINT_IN_TIME_VIOLATION")
    quality_state = "VALID" if not quality_reasons else "INVALID"
    pit_state = "VALID" if not pit_reasons else "INVALID"
    promotability = (
        "PROMOTABLE"
        if quality_state == "VALID"
        and pit_state == "VALID"
        and context.data_class in {"VENDOR", "PRODUCTION"}
        else "NON_PROMOTABLE"
    )
    return quality_state, pit_state, promotability, quality_reasons, pit_reasons


def _promotability_reasons(data_class: str | None) -> list[str]:
    if data_class in {"SYNTHETIC", "FIXTURE"}:
        return ["DATA_CLASS_NON_PROMOTABLE"]
    if data_class not in {"VENDOR", "PRODUCTION"}:
        return ["DATA_CLASS_INVALID"]
    return []


def _next_result_revision(session: Any, dataset_id: UUID) -> int:
    return (
        int(
            session.scalar(
                select(func.max(DataQualityResult.revision_no)).where(
                    DataQualityResult.dataset_revision_id == dataset_id
                )
            )
            or 0
        )
        + 1
    )


def _record_rejection(factory: SessionFactory, job_id: UUID, error: QfError) -> None:
    """Terminally record a local admission failure as data evidence, never success."""
    with factory.begin() as session:
        job = session.get(Job, job_id)
        if job is None:
            return
        revision = session.get(DatasetRevision, job.resource_id)
        if revision is None or (
            revision.quality_state != "PENDING" or revision.point_in_time_state != "PENDING"
        ):
            return
        revision_no = _next_result_revision(session, revision.id)
        summary = {"reason_codes": [error.code], "message": error.message}
        quality = DataQualityResult(
            dataset_revision_id=revision.id,
            check_kind="QUALITY",
            revision_no=revision_no,
            state="INVALID",
            summary=summary,
            checker_version="dataset-materialization-v1",
        )
        point_in_time = DataQualityResult(
            dataset_revision_id=revision.id,
            check_kind="POINT_IN_TIME",
            revision_no=revision_no,
            state="INVALID",
            summary=summary,
            checker_version="dataset-materialization-v1",
        )
        session.add_all((quality, point_in_time))
        session.flush()
        revision.quality_state = "INVALID"
        revision.point_in_time_state = "INVALID"
        revision.promotability = "NON_PROMOTABLE"
        revision.quality_result_id = quality.id
        revision.point_in_time_result_id = point_in_time.id
        append_event(
            session,
            kind="DATASET_MATERIALIZATION_REJECTED",
            aggregate_type="DATASET_REVISION",
            aggregate_id=revision.id,
            payload={"reason_code": error.code},
        )


def _is_terminal_rejection(error: QfError) -> bool:
    if error.code in {
        "DATASET_MATERIALIZATION_BINDING_CONFLICT",
        "DATASET_MATERIALIZATION_BINDING_INVALID",
        "DATASET_MATERIALIZATION_CONNECTOR_UNSUPPORTED",
        "DATASET_MATERIALIZATION_MANIFEST_INVALID",
        "DATASET_MATERIALIZATION_PLUGIN_BUNDLE_MEMBER_REQUIRED",
        "DATASET_MATERIALIZATION_PLUGIN_BUNDLE_NOT_READY",
        "DATASET_MATERIALIZATION_PLUGIN_CAPABILITY_FORBIDDEN",
        "DATASET_MATERIALIZATION_PLUGIN_RELEASE_NOT_ACTIVE",
        "DATASET_MATERIALIZATION_REQUIREMENTS_INVALID",
        "DATASET_MATERIALIZATION_SOURCE_INVALID",
        "DATASET_MATERIALIZATION_SOURCE_NOT_READY",
        "DATASET_MATERIALIZATION_WORKER_TIMEOUT_INSUFFICIENT",
    }:
        return False
    return error.code not in {
        "JOB_NOT_FOUND",
        "JOB_STATE_CONFLICT",
        "DATASET_NOT_FOUND",
        "DATASET_MATERIALIZATION_RESOURCE_INVALID",
        "DATASET_MATERIALIZATION_STATE_CONFLICT",
    }


def _persist_descriptor(
    factory: SessionFactory, context: _MaterializationContext, descriptor: CatalogDescriptor
) -> None:
    mismatches: list[str] = []
    if descriptor.catalog_uri != f"catalog://{context.catalog_name}":
        mismatches.append("catalog_uri")
    if context.requested_schema_version and descriptor.schema_revision != context.requested_schema_version:
        mismatches.append("schema_revision")
    if context.requested_data_type and descriptor.nautilus_data_type != context.requested_data_type:
        mismatches.append("data_type")
    if tuple(descriptor.instrument_scope) != context.requested_instrument_scope:
        mismatches.append("instrument_scope")
    expected_ranges = (
        (descriptor.event_start, context.requested_event_start, "event_start"),
        (descriptor.event_end, context.requested_event_end, "event_end"),
        (descriptor.available_start, context.requested_available_start, "available_start"),
        (descriptor.available_end, context.requested_available_end, "available_end"),
    )
    for actual, expected, field_name in expected_ranges:
        if actual is None or _utc(actual) != expected:
            mismatches.append(field_name)
    if mismatches:
        raise QfError(
            "DATASET_MATERIALIZATION_DESCRIPTOR_MISMATCH",
            "The materialized descriptor does not match the frozen Dataset Revision request.",
            409,
            {"fields": mismatches},
        )
    quality_state, pit_state, promotability, quality_reasons, pit_reasons = _terminal_states(
        context, descriptor
    )
    with factory.begin() as session:
        revision = session.execute(
            select(DatasetRevision)
            .where(DatasetRevision.id == context.dataset_id)
            .with_for_update()
        ).scalar_one_or_none()
        if revision is None:
            raise QfError("DATASET_NOT_FOUND", "Dataset Revision was not found.", 404)
        if revision.quality_state != "PENDING" or revision.point_in_time_state != "PENDING":
            raise QfError(
                "DATASET_MATERIALIZATION_STATE_CONFLICT",
                "Dataset Revision is no longer pending materialization.",
                409,
            )
        revision_no = _next_result_revision(session, revision.id)
        quality = DataQualityResult(
            dataset_revision_id=revision.id,
            check_kind="QUALITY",
            revision_no=revision_no,
            state=quality_state,
            summary={
                "remote_result": descriptor.quality_result,
                "reason_codes": quality_reasons,
                "promotability_reason_codes": _promotability_reasons(context.data_class),
                "source_shard_count": len(context.source_shards),
            },
            checker_version="dataset-materialization-v1",
        )
        point_in_time = DataQualityResult(
            dataset_revision_id=revision.id,
            check_kind="POINT_IN_TIME",
            revision_no=revision_no,
            state=pit_state,
            summary={
                "remote_result": descriptor.point_in_time_result,
                "reason_codes": pit_reasons,
            },
            checker_version="dataset-materialization-v1",
        )
        session.add_all((quality, point_in_time))
        session.flush()
        revision.schema_version = descriptor.schema_revision
        revision.event_start = descriptor.event_start
        revision.event_end = descriptor.event_end
        revision.available_start = descriptor.available_start
        revision.available_end = descriptor.available_end
        revision.row_count = descriptor.row_count
        revision.ingested_at = datetime.now(UTC)
        revision.quality_state = quality_state
        revision.point_in_time_state = pit_state
        revision.promotability = promotability
        revision.quality_result_id = quality.id
        revision.point_in_time_result_id = point_in_time.id
        binding = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == revision.id
            )
        )
        if binding is not None:
            raise QfError(
                "DATASET_MATERIALIZATION_BINDING_CONFLICT",
                "Dataset Revision already has an immutable catalog binding.",
                409,
            )
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
                "start": descriptor.available_start.isoformat()
                if descriptor.available_start
                else None,
                "end": descriptor.available_end.isoformat() if descriptor.available_end else None,
            },
            schema_revision=descriptor.schema_revision,
            quality_state=quality_state,
            quality_result=descriptor.quality_result,
            point_in_time_state=pit_state,
            point_in_time_result=descriptor.point_in_time_result,
            sealed=False,
        )
        session.add(binding)
        session.flush()
        append_event(
            session,
            kind="DATASET_MATERIALIZED",
            aggregate_type="DATASET_REVISION",
            aggregate_id=revision.id,
            payload={
                "catalog_id": str(binding.id),
                "quality_state": quality_state,
                "point_in_time_state": pit_state,
                "promotability": promotability,
            },
        )


def run_dataset_materialization(settings: Settings, lease: JobLease) -> None:
    """Materialize one immutable Dataset Revision through a registered connector only."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        try:
            context = _prepare_materialization(factory, lease.job_id)
        except QfError as exc:
            if _is_terminal_rejection(exc):
                _record_rejection(factory, lease.job_id, exc)
            raise
        timeout_seconds = _archive_materialization_timeout(len(context.source_shards))
        if timeout_seconds > settings.plugin_job_timeout_seconds:
            raise QfError(
                "DATASET_MATERIALIZATION_WORKER_TIMEOUT_INSUFFICIENT",
                "Configured plugin job timeout cannot complete this bounded materialization.",
                409,
                {
                    "required_timeout_seconds": int(timeout_seconds),
                    "configured_timeout_seconds": settings.plugin_job_timeout_seconds,
                },
            )
        descriptor = _runtime().ingest(
            CatalogIngestSpec(
                catalog_name=context.catalog_name,
                provider=context.provider,
                source_license=context.source_license,
                source_spec=context.source_spec,
                source_shards=context.source_shards,
                plugin_id=context.plugin_binding["plugin_id"],
                plugin_version=context.plugin_binding["plugin_version"],
                plugin_bundle_path=context.plugin_binding["plugin_bundle_path"],
            ),
            timeout_seconds=timeout_seconds,
        )
        _persist_descriptor(factory, context, descriptor)
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one bounded Dataset materialization")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    run_dataset_materialization(
        load_effective_settings(Settings.from_env()),
        JobLease(
            job_id=UUID(args.job_id),
            owner=args.lease_owner,
            attempt=args.lease_attempt,
        ),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
