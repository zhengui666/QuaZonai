"""Bind one pre-provisioned sealed catalog without importing a connector plugin."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any
from uuid import UUID

from pydantic import ValidationError
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError

from db.models import (
    DataQualityResult,
    DatasetRevision,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    NautilusCatalogBinding,
)
from db.session import SessionFactory, create_database_engine
from errors import QfError
from events import append_event
from jobs import JobLease, create_lease_fenced_session_factory
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import CatalogDescriptor
from quant_runtime.remote import NautilusQuantRuntime
from runtime_config import load_effective_settings
from settings import Settings

_CATALOG_URI = re.compile(r"catalog://[A-Za-z0-9][A-Za-z0-9._-]{0,119}\Z")


@dataclass(frozen=True)
class _ProvisionContext:
    dataset_id: UUID
    source_id: UUID
    universe_id: UUID
    catalog_uri: str
    provider: str
    source_license: str
    schema_version: str
    data_type: str
    instrument_scope: tuple[str, ...]
    event_start: datetime
    event_end: datetime
    available_start: datetime
    available_end: datetime
    data_class: str | None


def _runtime() -> NautilusQuantRuntime:
    config = RemoteNautilusConfig.from_env(required=True, profile="sealed")
    assert config is not None
    return NautilusQuantRuntime(config)


def _utc(value: datetime | None) -> datetime | None:
    if value is None:
        return None
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _pending_revision(session: Any, job: Job) -> DatasetRevision:
    if job.kind != "SEALED_CATALOG_PROVISION":
        raise QfError("JOB_NOT_FOUND", "Sealed catalog provision job does not exist.", 404)
    if job.state != "LEASED":
        raise QfError("JOB_STATE_CONFLICT", "Sealed catalog provision job is not leased.", 409)
    if job.resource_type != "dataset_revision" or job.payload:
        raise QfError(
            "SEALED_CATALOG_PROVISION_RESOURCE_INVALID",
            "Sealed catalog provision has an invalid resource or payload.",
            409,
        )
    revision = session.get(DatasetRevision, job.resource_id)
    if revision is None:
        raise QfError("DATASET_NOT_FOUND", "Dataset Revision was not found.", 404)
    if (
        revision.partition != "SEALED"
        or revision.quality_state != "PENDING"
        or revision.point_in_time_state != "PENDING"
    ):
        raise QfError(
            "SEALED_CATALOG_PROVISION_STATE_CONFLICT",
            "Dataset Revision is not pending sealed catalog provision.",
            409,
        )
    return revision


def _prepare_provision(factory: SessionFactory, job_id: UUID) -> _ProvisionContext:
    with factory() as session:
        job = session.get(Job, job_id)
        if job is None:
            raise QfError("JOB_NOT_FOUND", "Sealed catalog provision job does not exist.", 404)
        revision = _pending_revision(session, job)
        if revision.data_source_id is None or revision.universe_version_id is None:
            raise QfError(
                "SEALED_CATALOG_PROVISION_SOURCE_INVALID",
                "Dataset Revision must reference a governed Data Source and Universe Version.",
                409,
            )
        source = session.get(GovernedDataSource, revision.data_source_id)
        universe = session.get(MarketUniverseVersion, revision.universe_version_id)
        if (
            source is None
            or universe is None
            or source.state != "ACTIVE"
            or source.preflight_state != "READY"
            or universe.state != "ACTIVE"
            or str(universe.id) not in source.universe_scope
            or not source.provider
            or not source.license_classification
        ):
            raise QfError(
                "SEALED_CATALOG_PROVISION_SOURCE_NOT_READY",
                "Sealed catalog provision requires an ACTIVE, preflight-READY governed source.",
                409,
            )
        request = revision.materialization_request
        catalog_uri = request.get("sealed_catalog_uri") if isinstance(request, dict) else None
        instruments = request.get("instrument_scope") if isinstance(request, dict) else None
        data_type = request.get("data_type") if isinstance(request, dict) else None
        times = (
            _utc(revision.event_start),
            _utc(revision.event_end),
            _utc(revision.available_start),
            _utc(revision.available_end),
        )
        if (
            not isinstance(catalog_uri, str)
            or _CATALOG_URI.fullmatch(catalog_uri) is None
            or not isinstance(instruments, list)
            or not instruments
            or not all(isinstance(item, str) and item for item in instruments)
            or not isinstance(data_type, str)
            or not data_type
            or revision.schema_version is None
            or any(value is None for value in times)
        ):
            raise QfError(
                "SEALED_CATALOG_PROVISION_REQUEST_INVALID",
                "Sealed catalog provision requires a complete immutable catalog declaration.",
                409,
            )
        assert all(value is not None for value in times)
        event_start, event_end, available_start, available_end = times
        assert event_start is not None
        assert event_end is not None
        assert available_start is not None
        assert available_end is not None
        return _ProvisionContext(
            dataset_id=revision.id,
            source_id=source.id,
            universe_id=universe.id,
            catalog_uri=catalog_uri,
            provider=source.provider,
            source_license=source.license_classification,
            schema_version=revision.schema_version,
            data_type=data_type,
            instrument_scope=tuple(instruments),
            event_start=event_start,
            event_end=event_end,
            available_start=available_start,
            available_end=available_end,
            data_class=revision.data_class,
        )


def _verify_identity(context: _ProvisionContext, descriptor: CatalogDescriptor) -> None:
    expected_times = (
        context.event_start,
        context.event_end,
        context.available_start,
        context.available_end,
    )
    actual_times = tuple(
        _utc(value)
        for value in (
            descriptor.event_start,
            descriptor.event_end,
            descriptor.available_start,
            descriptor.available_end,
        )
    )
    mismatches: list[str] = []
    if descriptor.catalog_uri != context.catalog_uri:
        mismatches.append("catalog_uri")
    if not descriptor.sealed:
        mismatches.append("sealed")
    if descriptor.provider != context.provider:
        mismatches.append("provider")
    if descriptor.source_license != context.source_license:
        mismatches.append("source_license")
    if descriptor.schema_revision != context.schema_version:
        mismatches.append("schema_revision")
    if descriptor.nautilus_data_type != context.data_type:
        mismatches.append("data_type")
    if tuple(sorted(descriptor.instrument_scope)) != tuple(sorted(context.instrument_scope)):
        mismatches.append("instrument_scope")
    if actual_times != expected_times:
        mismatches.append("time_range")
    if mismatches:
        raise QfError(
            "SEALED_CATALOG_DESCRIPTOR_MISMATCH",
            "The sealed runtime descriptor does not match the immutable Dataset Revision.",
            409,
            {"fields": mismatches},
        )


def _validated_result(result: dict[str, Any]) -> dict[str, bool]:
    return {"valid": result.get("valid") is True}


def _outcome(
    context: _ProvisionContext,
    *,
    row_count: int,
    quality_valid: bool,
    point_in_time_valid: bool,
) -> tuple[str, str, str, list[str], list[str]]:
    quality_reasons: list[str] = []
    pit_reasons: list[str] = []
    if row_count <= 0:
        quality_reasons.append("DATA_EMPTY")
    if not quality_valid:
        quality_reasons.append("REMOTE_QUALITY_INVALID")
    if not point_in_time_valid:
        pit_reasons.append("REMOTE_POINT_IN_TIME_INVALID")
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


def _record_rejection(factory: SessionFactory, job_id: UUID, error: QfError) -> None:
    with factory.begin() as session:
        job = session.get(Job, job_id)
        if job is None:
            return
        revision = session.get(DatasetRevision, job.resource_id)
        if revision is None or (
            revision.quality_state != "PENDING" or revision.point_in_time_state != "PENDING"
        ):
            return
        summary = {"reason_codes": [error.code], "message": error.message}
        quality = DataQualityResult(
            dataset_revision_id=revision.id,
            check_kind="QUALITY",
            revision_no=1,
            state="INVALID",
            summary=summary,
            checker_version="sealed-catalog-provision-v1",
        )
        point_in_time = DataQualityResult(
            dataset_revision_id=revision.id,
            check_kind="POINT_IN_TIME",
            revision_no=1,
            state="INVALID",
            summary=summary,
            checker_version="sealed-catalog-provision-v1",
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
            kind="SEALED_CATALOG_PROVISION_REJECTED",
            aggregate_type="DATASET_REVISION",
            aggregate_id=revision.id,
            payload={"reason_code": error.code},
        )


def _is_terminal_rejection(error: QfError) -> bool:
    return error.code not in {
        "JOB_NOT_FOUND",
        "JOB_STATE_CONFLICT",
        "DATASET_NOT_FOUND",
        "SEALED_CATALOG_PROVISION_RESOURCE_INVALID",
        "SEALED_CATALOG_PROVISION_STATE_CONFLICT",
    } and not error.code.startswith("NAUTILUS_RUNTIME_")


def _persist_descriptor(
    factory: SessionFactory, context: _ProvisionContext, descriptor: CatalogDescriptor
) -> None:
    _verify_identity(context, descriptor)
    quality_result = _validated_result(descriptor.quality_result)
    point_in_time_result = _validated_result(descriptor.point_in_time_result)
    quality_state, pit_state, promotability, quality_reasons, pit_reasons = _outcome(
        context,
        row_count=descriptor.row_count,
        quality_valid=quality_result["valid"],
        point_in_time_valid=point_in_time_result["valid"],
    )
    try:
        with factory.begin() as session:
            revision = session.execute(
                select(DatasetRevision)
                .where(DatasetRevision.id == context.dataset_id)
                .with_for_update()
            ).scalar_one_or_none()
            source = session.execute(
                select(GovernedDataSource)
                .where(GovernedDataSource.id == context.source_id)
                .with_for_update()
            ).scalar_one_or_none()
            if revision is None or source is None or (
                revision.quality_state != "PENDING"
                or revision.point_in_time_state != "PENDING"
                or revision.partition != "SEALED"
                or source.state != "ACTIVE"
                or source.preflight_state != "READY"
                or source.provider != context.provider
                or source.license_classification != context.source_license
                or str(context.universe_id) not in source.universe_scope
            ):
                raise QfError(
                    "SEALED_CATALOG_PROVISION_STATE_CONFLICT",
                    "Dataset Revision or Data Source changed while provision was running.",
                    409,
                )
            if session.scalar(
                select(NautilusCatalogBinding).where(
                    NautilusCatalogBinding.dataset_revision_id == revision.id
                )
            ) is not None or session.scalar(
                select(NautilusCatalogBinding).where(
                    NautilusCatalogBinding.catalog_uri == descriptor.catalog_uri
                )
            ) is not None:
                raise QfError(
                    "SEALED_CATALOG_BINDING_CONFLICT",
                    "The sealed catalog reference is already bound immutably.",
                    409,
                )
            quality = DataQualityResult(
                dataset_revision_id=revision.id,
                check_kind="QUALITY",
                revision_no=1,
                state=quality_state,
                summary={
                    "remote_result": quality_result,
                    "reason_codes": quality_reasons,
                    "promotability_reason_codes": _promotability_reasons(context.data_class),
                },
                checker_version="sealed-catalog-provision-v1",
            )
            point_in_time = DataQualityResult(
                dataset_revision_id=revision.id,
                check_kind="POINT_IN_TIME",
                revision_no=1,
                state=pit_state,
                summary={
                    "remote_result": point_in_time_result,
                    "reason_codes": pit_reasons,
                },
                checker_version="sealed-catalog-provision-v1",
            )
            session.add_all((quality, point_in_time))
            session.flush()
            revision.row_count = descriptor.row_count
            revision.ingested_at = datetime.now(UTC)
            revision.quality_state = quality_state
            revision.point_in_time_state = pit_state
            revision.promotability = promotability
            revision.quality_result_id = quality.id
            revision.point_in_time_result_id = point_in_time.id
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
                quality_state=quality_state,
                quality_result=quality_result,
                point_in_time_state=pit_state,
                point_in_time_result=point_in_time_result,
                sealed=True,
            )
            session.add(binding)
            session.flush()
            append_event(
                session,
                kind="SEALED_CATALOG_PROVISIONED",
                aggregate_type="DATASET_REVISION",
                aggregate_id=revision.id,
                payload={
                    "catalog_id": str(binding.id),
                    "quality_state": quality_state,
                    "point_in_time_state": pit_state,
                    "promotability": promotability,
                },
            )
    except IntegrityError as exc:
        raise QfError(
            "SEALED_CATALOG_BINDING_CONFLICT",
            "The sealed catalog reference is already bound immutably.",
            409,
        ) from exc


def run_sealed_catalog_provision(settings: Settings, lease: JobLease) -> None:
    """Validate a pre-provisioned sealed catalog and bind its immutable evidence."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        try:
            context = _prepare_provision(factory, lease.job_id)
            try:
                descriptor = _runtime().validate_catalog(context.catalog_uri)
            except ValidationError:
                raise QfError(
                    "SEALED_CATALOG_DESCRIPTOR_INVALID",
                    "The sealed runtime returned an invalid catalog descriptor.",
                    502,
                ) from None
            _persist_descriptor(factory, context, descriptor)
        except QfError as exc:
            if _is_terminal_rejection(exc):
                _record_rejection(factory, lease.job_id, exc)
            raise
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Bind one pre-provisioned sealed catalog")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    run_sealed_catalog_provision(
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
