"""Dedicated durable worker for independent sealed Alpha evaluation.

This process is intentionally separate from the API and ordinary finite worker. Its
production deployment receives the sealed Nautilus service credential but no Codex
home, Mission workspace, plugin storage, or research-runtime credential.
"""

from __future__ import annotations

import argparse

import httpx
import logging
import os
import signal
import socket
import threading
import time
from collections.abc import Iterator, Sequence
from contextlib import contextmanager
from uuid import UUID

from sqlalchemy import func, select

from db.models import (
    DatasetRevision,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    PublicMutationReceipt,
)
from db.session import SessionFactory, create_database_engine, create_session_factory, ping_database
from events import append_event
from jobs import (
    claim_next_job,
    complete_job,
    fail_job,
    release_expired_leases,
    renew_job_lease,
    retry_job,
)
from logging_utils import configure_logging
from quant_runtime.client import NautilusQuantRuntime, RemoteNautilusConfig
from quant_runtime.contracts import CatalogValidationRequest
from quant_runtime.promotion import qualify_alpha, simulate_portfolio_candidate
from settings import Settings

LOGGER = logging.getLogger("quazonai.sealed_worker")
SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"
SEALED_DATASET_REGISTRATION_JOB_KIND = "SEALED_DATASET_REGISTRATION"
SEALED_PORTFOLIO_PROMOTION_JOB_KIND = "SEALED_PORTFOLIO_PROMOTION"
SEALED_JOB_KINDS = {
    SEALED_JOB_KIND,
    SEALED_DATASET_REGISTRATION_JOB_KIND,
    SEALED_PORTFOLIO_PROMOTION_JOB_KIND,
}


class StopFlag:
    requested = False

    def request(self, *_: object) -> None:
        self.requested = True


def _uuid_payload(job: Job, key: str) -> UUID:
    raw = (job.payload or {}).get(key)
    try:
        return UUID(str(raw))
    except (TypeError, ValueError) as exc:
        raise RuntimeError(f"sealed job payload field {key!r} must be a UUID") from exc


def _execute_qualification(factory: SessionFactory, job: Job) -> dict[str, str]:
    payload = dict(job.payload or {})
    sealed_dataset_revision_id = _uuid_payload(job, "sealed_dataset_revision_id")
    sealed_experiment_id = _uuid_payload(job, "sealed_experiment_id")
    name_raw = payload.get("name")
    role_raw = payload.get("role", "PRIMARY_ALPHA")
    name = str(name_raw) if name_raw is not None else None
    role = str(role_raw)
    alpha = qualify_alpha(
        factory,
        source_experiment_id=job.resource_id,
        sealed_dataset_revision_id=sealed_dataset_revision_id,
        sealed_experiment_id=sealed_experiment_id,
        name=name,
        role=role,
    )
    if alpha.source_experiment_id is None:
        raise RuntimeError("qualified Alpha lost source experiment lineage")
    return {
        "alpha_qualification_id": str(alpha.id),
        "source_experiment_id": str(alpha.source_experiment_id),
        "state": alpha.state,
        "degradation_state": alpha.degradation_state,
    }



def _execute_portfolio_promotion(factory: SessionFactory, job: Job) -> dict[str, str]:
    payload = dict(job.payload or {})
    simulation_experiment_id = _uuid_payload(job, "simulation_experiment_id")
    portfolio_sealed_experiment_id = _uuid_payload(job, "portfolio_sealed_experiment_id")
    alpha_ids_raw = payload.get("alpha_ids", [])
    if not isinstance(alpha_ids_raw, list):
        raise RuntimeError("sealed portfolio job alpha_ids must be a list")
    try:
        alpha_ids = [UUID(str(value)) for value in alpha_ids_raw]
    except ValueError as exc:
        raise RuntimeError("sealed portfolio job alpha_ids must contain UUIDs") from exc
    result = simulate_portfolio_candidate(
        factory,
        portfolio_program_id=job.resource_id,
        alpha_ids=alpha_ids,
        simulation_experiment_id=simulation_experiment_id,
        portfolio_sealed_experiment_id=portfolio_sealed_experiment_id,
    )
    response = {
        "job_id": str(job.id),
        "state": "SUCCEEDED",
        "candidate_id": str(result.candidate_id),
        "approval_id": str(result.approval_id),
        "simulation_experiment_id": str(result.simulation_experiment_id),
        "portfolio_sealed_experiment_id": str(portfolio_sealed_experiment_id),
        "selected_alpha_id": str(result.selected_alpha_id),
    }
    key = str(payload.get("idempotency_key") or "")
    operation = str(payload.get("idempotency_operation") or "")
    normalized = payload.get("idempotency_normalized")
    if not key or not operation or not isinstance(normalized, dict):
        raise RuntimeError("sealed portfolio job lost its public idempotency receipt identity")
    with factory.begin() as session:
        receipt = session.execute(
            select(PublicMutationReceipt)
            .where(PublicMutationReceipt.idempotency_key == key)
            .with_for_update()
        ).scalar_one_or_none()
        if receipt is None:
            raise RuntimeError("sealed portfolio job public idempotency receipt is missing")
        if receipt.operation_name != operation or receipt.normalized_request != normalized:
            raise RuntimeError("sealed portfolio job public idempotency receipt identity changed")
        persisted_simulation = str((receipt.response_json or {}).get("simulation_experiment_id") or "")
        persisted_sealed = str((receipt.response_json or {}).get("portfolio_sealed_experiment_id") or "")
        if persisted_simulation != str(simulation_experiment_id) or persisted_sealed != str(portfolio_sealed_experiment_id):
            raise RuntimeError("sealed portfolio job experiment identity changed")
        receipt.response_json = response
        receipt.status_code = 200
        session.flush()
    return response


def _execute_sealed_dataset_registration(
    factory: SessionFactory, job: Job
) -> dict[str, str]:
    payload = dict(job.payload or {})
    universe_version_id = job.resource_id
    data_source_id = _uuid_payload(job, "data_source_id")
    catalog_key = str(payload.get("catalog_key") or "")
    expected = [str(value) for value in payload.get("expected_instrument_ids", [])]
    if not catalog_key:
        raise RuntimeError("sealed registration catalog_key is required")
    with NautilusQuantRuntime(RemoteNautilusConfig.from_env(sealed=True)) as runtime:
        validation = runtime.validate_sealed_catalog(
            CatalogValidationRequest(
                catalog_key=catalog_key,
                instrument_ids=expected,
                nautilus_data_type="QuoteTick",
            )
        )
    if not validation.valid:
        raise RuntimeError("sealed catalog failed remote validation")
    required = (
        validation.catalog_uri, validation.gateway_instance_id, validation.catalog_release_id,
        validation.nautilus_data_type, validation.schema_revision,
        validation.event_time_start, validation.event_time_end,
        validation.available_time_start, validation.available_time_end, validation.ingested_at,
    )
    if any(value is None for value in required):
        raise RuntimeError("sealed catalog validation omitted immutable governance metadata")
    if str(validation.quality_result.get("state", "")).upper() != "VALID":
        raise RuntimeError("sealed catalog quality governance is not VALID")
    if str(validation.point_in_time_result.get("state", "")).upper() != "VALID":
        raise RuntimeError("sealed catalog point-in-time governance is not VALID")

    with factory.begin() as session:
        universe = session.execute(
            select(MarketUniverseVersion)
            .where(MarketUniverseVersion.id == universe_version_id)
            .with_for_update()
        ).scalar_one_or_none()
        source = session.execute(
            select(GovernedDataSource)
            .where(GovernedDataSource.id == data_source_id)
            .with_for_update()
        ).scalar_one_or_none()
        if universe is None or universe.state != "ACTIVE":
            raise RuntimeError("sealed registration Universe Version is no longer active")
        if source is None or source.state != "ACTIVE" or source.preflight_state != "READY":
            raise RuntimeError("sealed registration Data Source is no longer ready")
        source_scope = {str(value) for value in (source.universe_scope or []) if str(value)}
        if source_scope and universe.name not in source_scope:
            raise RuntimeError("sealed registration Data Source no longer covers this Universe")
        existing = session.scalar(
            select(DatasetRevision).where(DatasetRevision.catalog_uri == validation.catalog_uri)
        )
        if existing is not None:
            if (
                existing.partition != "SEALED"
                or existing.universe_version_id != universe.id
                or existing.data_source_id != source.id
                or existing.instrument_scope != validation.instrument_scope
                or existing.row_count != validation.row_count
                or existing.gateway_instance_id != validation.gateway_instance_id
                or existing.catalog_release_id != validation.catalog_release_id
            ):
                raise RuntimeError("sealed catalog URI is already bound to different governance facts")
            revision = existing
        else:
            revision_no = int(
                session.scalar(
                    select(func.max(DatasetRevision.revision_no)).where(
                        DatasetRevision.data_source_id == source.id,
                        DatasetRevision.universe_version_id == universe.id,
                        DatasetRevision.partition == "SEALED",
                    )
                )
                or 0
            ) + 1
            revision = DatasetRevision(
                data_source_id=source.id,
                universe_version_id=universe.id,
                universe_name=universe.name,
                revision_no=revision_no,
                schema_version=validation.schema_revision,
                event_start=validation.event_time_start,
                event_end=validation.event_time_end,
                available_start=validation.available_time_start,
                available_end=validation.available_time_end,
                row_count=validation.row_count,
                quality_state="VALID",
                point_in_time_state="VALID",
                partition="SEALED",
                provider_name=source.provider or source.name,
                source_license=(str(payload.get("source_license")) if payload.get("source_license") else None),
                catalog_uri=validation.catalog_uri,
                gateway_instance_id=validation.gateway_instance_id,
                catalog_release_id=validation.catalog_release_id,
                nautilus_data_type=validation.nautilus_data_type,
                instrument_scope=validation.instrument_scope,
                schema_revision=validation.schema_revision,
                quality_result=validation.quality_result,
                point_in_time_result=validation.point_in_time_result,
                ingested_at=validation.ingested_at,
                created_at=validation.ingested_at,
            )
            session.add(revision)
            session.flush()
            append_event(
                session,
                kind="SEALED_DATASET_REVISION_REGISTERED",
                aggregate_type="dataset_revision",
                aggregate_id=revision.id,
                payload={
                    "universe_version_id": str(universe.id),
                    "data_source_id": str(source.id),
                    "catalog_key": catalog_key,
                },
            )
        return {
            "dataset_revision_id": str(revision.id),
            "universe_version_id": str(universe.id),
            "state": "REGISTERED",
        }


@contextmanager
def _lease_heartbeat(
    settings: Settings,
    *,
    owner: str,
    job_id: UUID,
    factory: SessionFactory,
) -> Iterator[threading.Event]:
    stop = threading.Event()
    lost = threading.Event()
    interval = max(1.0, min(float(settings.job_lease_seconds) / 3.0, 15.0))

    def heartbeat() -> None:
        while not stop.wait(interval):
            try:
                with factory.begin() as session:
                    renewed = renew_job_lease(
                        session,
                        job_id=job_id,
                        owner=owner,
                        lease_seconds=settings.job_lease_seconds,
                    )
                if not renewed:
                    lost.set()
                    return
            except Exception:
                lost.set()
                LOGGER.exception("sealed job lease heartbeat failed", extra={"job_id": str(job_id)})
                return

    thread = threading.Thread(target=heartbeat, name=f"sealed-lease-{job_id}", daemon=True)
    thread.start()
    try:
        yield lost
    finally:
        stop.set()
        thread.join(timeout=max(2.0, interval + 1.0))


def run_once(
    settings: Settings,
    *,
    owner: str,
    factory: SessionFactory,
) -> tuple[bool, float]:
    """Claim and execute at most one sealed-only durable job."""
    with factory.begin() as session:
        release_expired_leases(session)
        job = claim_next_job(
            session,
            owner=owner,
            lease_seconds=settings.job_lease_seconds,
            kinds=SEALED_JOB_KINDS,
        )
        if job is None:
            return False, settings.job_poll_seconds
        append_event(
            session,
            kind="SEALED_JOB_LEASED",
            aggregate_type="job",
            aggregate_id=job.id,
            payload={"kind": job.kind, "attempt": job.attempt},
        )
        session.expunge(job)

    try:
        with _lease_heartbeat(
            settings,
            owner=owner,
            job_id=job.id,
            factory=factory,
        ) as lease_lost:
            if job.kind == SEALED_JOB_KIND:
                result = _execute_qualification(factory, job)
            elif job.kind == SEALED_DATASET_REGISTRATION_JOB_KIND:
                result = _execute_sealed_dataset_registration(factory, job)
            elif job.kind == SEALED_PORTFOLIO_PROMOTION_JOB_KIND:
                result = _execute_portfolio_promotion(factory, job)
            else:
                raise RuntimeError(f"unsupported sealed job kind: {job.kind}")
        if lease_lost.is_set():
            LOGGER.error("sealed job lease ownership was lost", extra={"job_id": str(job.id)})
            return True, settings.job_poll_seconds
    except httpx.TransportError as exc:
        with factory.begin() as session:
            current = session.get(Job, job.id)
            if (
                current is not None
                and current.state == "LEASED"
                and current.lease_owner == owner
            ):
                retry_job(
                    session,
                    current,
                    "sealed remote result is transport-uncertain; retrying the same durable experiment",
                    delay_seconds=min(max(settings.job_poll_seconds, 1.0), 30.0),
                )
                append_event(
                    session,
                    kind="SEALED_JOB_RETRYABLE",
                    aggregate_type="job",
                    aggregate_id=current.id,
                    payload={
                        "error_code": type(exc).__name__,
                        "experiment_id": str((current.payload or {}).get("sealed_experiment_id") or (current.payload or {}).get("portfolio_sealed_experiment_id") or ""),
                    },
                )
        LOGGER.warning("sealed job remote result is uncertain; durable retry retained", extra={"job_id": str(job.id)})
        return True, settings.job_poll_seconds
    except Exception as exc:  # noqa: BLE001 - durable privileged job boundary
        with factory.begin() as session:
            current = session.get(Job, job.id)
            if (
                current is not None
                and current.state == "LEASED"
                and current.lease_owner == owner
            ):
                fail_job(session, current, str(exc)[-4000:])
                append_event(
                    session,
                    kind="SEALED_JOB_FAILED",
                    aggregate_type="job",
                    aggregate_id=current.id,
                    payload={"error_code": type(exc).__name__},
                )
        LOGGER.exception("sealed job failed", extra={"job_id": str(job.id)})
        return True, settings.job_poll_seconds

    with factory.begin() as session:
        current = session.get(Job, job.id)
        if (
            current is not None
            and current.state == "LEASED"
            and current.lease_owner == owner
        ):
            current.payload = {**dict(current.payload or {}), "result": result}
            complete_job(session, current)
            append_event(
                session,
                kind="SEALED_JOB_SUCCEEDED",
                aggregate_type="job",
                aggregate_id=current.id,
                payload=result,
            )
    return True, settings.job_poll_seconds


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="QuaZonai independent sealed evaluator")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--once", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    configure_logging()
    settings = Settings.from_env()
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    try:
        if args.check:
            ping_database(engine)
            RemoteNautilusConfig.from_env(sealed=True).validate()
            return 0

        # Fail closed before leasing a job if this process was deployed without the
        # independent sealed-runtime endpoint/credential contract.
        RemoteNautilusConfig.from_env(sealed=True).validate()
        owner = f"sealed:{socket.gethostname()}:{os.getpid()}"
        if args.once:
            run_once(settings, owner=owner, factory=factory)
            return 0

        stop = StopFlag()
        signal.signal(signal.SIGTERM, stop.request)
        signal.signal(signal.SIGINT, stop.request)
        LOGGER.info("sealed evaluator started")
        while not stop.requested:
            worked, poll_seconds = run_once(settings, owner=owner, factory=factory)
            if not worked:
                time.sleep(poll_seconds)
        LOGGER.info("sealed evaluator stopped")
        return 0
    finally:
        engine.dispose()


if __name__ == "__main__":
    raise SystemExit(main())
