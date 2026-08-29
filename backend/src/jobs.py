"""Durable PostgreSQL job queue primitives."""

from __future__ import annotations

from collections.abc import Collection
from datetime import UTC, datetime, timedelta
from typing import Any, cast
from uuid import UUID

from sqlalchemy import select, update
from sqlalchemy.engine import CursorResult
from sqlalchemy.orm import Session

from db.models import Job


def enqueue_job(
    session: Session,
    *,
    kind: str,
    resource_type: str,
    resource_id: UUID,
    payload: dict[str, object] | None = None,
    available_at: datetime | None = None,
) -> Job:
    job = Job(
        kind=kind,
        resource_type=resource_type,
        resource_id=resource_id,
        payload=payload or {},
        available_at=available_at or datetime.now(UTC),
    )
    session.add(job)
    session.flush()
    return job


def release_expired_leases(session: Session, *, now: datetime | None = None) -> int:
    current = now or datetime.now(UTC)
    result = cast(
        CursorResult[Any],
        session.execute(
            update(Job)
            .where(
                Job.state == "LEASED",
                Job.lease_expires_at.is_not(None),
                Job.lease_expires_at < current,
            )
            .values(
                state="READY",
                lease_owner=None,
                lease_expires_at=None,
                updated_at=current,
            )
        ),
    )
    return int(result.rowcount or 0)


def claim_next_job(
    session: Session,
    *,
    owner: str,
    lease_seconds: int,
    now: datetime | None = None,
    kinds: Collection[str] | None = None,
) -> Job | None:
    """Claim one ready job, optionally constrained to an explicit worker capability set."""
    current = now or datetime.now(UTC)
    query = select(Job).where(Job.state == "READY", Job.available_at <= current)
    if kinds is not None:
        allowed = tuple(sorted(set(kinds)))
        if not allowed:
            return None
        query = query.where(Job.kind.in_(allowed))
    job = session.execute(
        query.order_by(Job.available_at.asc(), Job.created_at.asc())
        .limit(1)
        .with_for_update(skip_locked=True)
    ).scalar_one_or_none()
    if job is None:
        return None

    job.state = "LEASED"
    job.lease_owner = owner
    job.lease_expires_at = current + timedelta(seconds=lease_seconds)
    job.attempt += 1
    session.flush()
    return job


def renew_job_lease(
    session: Session,
    *,
    job_id: UUID,
    owner: str,
    lease_seconds: int,
    now: datetime | None = None,
) -> bool:
    current = now or datetime.now(UTC)
    result = cast(
        CursorResult[Any],
        session.execute(
            update(Job)
            .where(
                Job.id == job_id,
                Job.state == "LEASED",
                Job.lease_owner == owner,
            )
            .values(
                lease_expires_at=current + timedelta(seconds=lease_seconds),
                updated_at=current,
            )
        ),
    )
    return int(result.rowcount or 0) == 1


def complete_job(session: Session, job: Job) -> None:
    job.state = "SUCCEEDED"
    job.lease_owner = None
    job.lease_expires_at = None
    job.last_error = None
    session.flush()


def retry_job(
    session: Session,
    job: Job,
    message: str,
    *,
    delay_seconds: float = 0.0,
    now: datetime | None = None,
) -> None:
    current = now or datetime.now(UTC)
    job.state = "READY"
    job.lease_owner = None
    job.lease_expires_at = None
    job.last_error = message
    job.available_at = current + timedelta(seconds=max(0.0, float(delay_seconds)))
    session.flush()


def fail_job(session: Session, job: Job, message: str) -> None:
    job.state = "FAILED"
    job.lease_owner = None
    job.lease_expires_at = None
    job.last_error = message
    session.flush()
