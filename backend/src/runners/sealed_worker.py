"""Dedicated durable worker for independent sealed Alpha evaluation.

This process is intentionally separate from the API and ordinary finite worker. Its
production deployment receives the sealed Nautilus service credential but no Codex
home, Mission workspace, plugin storage, or research-runtime credential.
"""

from __future__ import annotations

import argparse
import logging
import os
import signal
import socket
import threading
import time
from collections.abc import Sequence
from contextlib import contextmanager
from uuid import UUID

from db.models import Job
from db.session import SessionFactory, create_database_engine, create_session_factory, ping_database
from events import append_event
from jobs import (
    claim_next_job,
    complete_job,
    fail_job,
    release_expired_leases,
    renew_job_lease,
)
from logging_utils import configure_logging
from quant_runtime.client import RemoteNautilusConfig
from quant_runtime.promotion import qualify_alpha
from settings import Settings

LOGGER = logging.getLogger("quazonai.sealed_worker")
SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"


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
    name_raw = payload.get("name")
    role_raw = payload.get("role", "PRIMARY_ALPHA")
    name = str(name_raw) if name_raw is not None else None
    role = str(role_raw)
    alpha = qualify_alpha(
        factory,
        source_experiment_id=job.resource_id,
        sealed_dataset_revision_id=sealed_dataset_revision_id,
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


@contextmanager
def _lease_heartbeat(
    settings: Settings,
    *,
    owner: str,
    job_id: UUID,
    factory: SessionFactory,
):
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
            kinds={SEALED_JOB_KIND},
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
            result = _execute_qualification(factory, job)
        if lease_lost.is_set():
            LOGGER.error("sealed job lease ownership was lost", extra={"job_id": str(job.id)})
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
