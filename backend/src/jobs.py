"""Durable PostgreSQL job queue primitives."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Any, cast
from uuid import UUID

from sqlalchemy import Engine, event, select, update
from sqlalchemy.engine import Connection, CursorResult
from sqlalchemy.orm import Session, sessionmaker
from sqlalchemy.sql.elements import ColumnElement

from db.models import AgentSession, AgentTurn, Event, Job, ResearchMission
from errors import QfError


@dataclass(frozen=True, slots=True)
class JobLease:
    """The exact fencing identity issued by one successful job claim."""

    job_id: UUID
    owner: str
    attempt: int


def _now() -> datetime:
    return datetime.now(UTC)


def _lease_conditions(lease: JobLease, current: datetime) -> tuple[ColumnElement[bool], ...]:
    return (
        Job.id == lease.job_id,
        Job.state == "LEASED",
        Job.lease_owner == lease.owner,
        Job.attempt == lease.attempt,
        Job.lease_expires_at.is_not(None),
        Job.lease_expires_at > current,
    )


def _require_current_lease(
    connection: Connection,
    lease: JobLease,
    *,
    now: datetime | None = None,
) -> None:
    current = now or _now()
    job_id = connection.scalar(
        select(Job.id)
        .where(*_lease_conditions(lease, current))
        .with_for_update()
    )
    if job_id is None:
        raise QfError(
            "JOB_LEASE_LOST",
            "Job lease is no longer current for this worker attempt.",
            409,
        )


class _LeaseFencedSession(Session):
    pass


@event.listens_for(_LeaseFencedSession, "after_begin")
def _fence_transaction(
    session: Session,
    transaction: object,
    connection: Connection,
) -> None:
    if getattr(transaction, "nested", False):
        return
    lease = cast(JobLease, session.info["job_lease"])
    _require_current_lease(connection, lease)


@event.listens_for(_LeaseFencedSession, "before_commit")
def _fence_commit(session: Session) -> None:
    # SQLAlchemy dispatches before_commit before its automatic flush. Flush here
    # then recheck the already-held Job lock immediately before COMMIT.
    session.flush()
    lease = cast(JobLease, session.info["job_lease"])
    _require_current_lease(session.connection(), lease)


def create_lease_fenced_session_factory(
    engine: Engine,
    lease: JobLease,
) -> sessionmaker[Session]:
    """Create a child-only factory that fences every database transaction."""
    return cast(
        sessionmaker[Session],
        sessionmaker(
            bind=engine,
            class_=_LeaseFencedSession,
            expire_on_commit=False,
            autoflush=False,
            info={"job_lease": lease},
        ),
    )


def _terminal_time(current: datetime, started_at: datetime | None) -> datetime:
    if started_at is None:
        return current
    if started_at.tzinfo is None:
        started_at = started_at.replace(tzinfo=UTC)
    return max(current, started_at)


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
    current = now or _now()
    expired = list(
        session.scalars(
            select(Job)
            .where(
                Job.state == "LEASED",
                Job.lease_expires_at.is_not(None),
                Job.lease_expires_at <= current,
            )
            .with_for_update(skip_locked=True)
        )
    )
    for job in expired:
        if job.resource_type == "research_mission":
            mission = session.scalar(
                select(ResearchMission)
                .where(ResearchMission.id == job.resource_id)
                .with_for_update()
            )
            if mission is not None and mission.state == "RUNNING":
                interrupted_at = _terminal_time(current, mission.started_at)
                mission.state = "INTERRUPTED"
                mission.finished_at = interrupted_at
                mission.error_code = "JOB_LEASE_EXPIRED"
                mission.revision += 1
                agent_session = session.scalar(
                    select(AgentSession)
                    .where(AgentSession.mission_id == mission.id)
                    .with_for_update()
                )
                if agent_session is not None and agent_session.state == "RUNNING":
                    interrupted_at = _terminal_time(interrupted_at, agent_session.started_at)
                    agent_session.state = "INTERRUPTED"
                    agent_session.finished_at = interrupted_at
                    agent_session.last_event_at = interrupted_at
                    for turn in session.scalars(
                        select(AgentTurn).where(
                            AgentTurn.agent_session_id == agent_session.id,
                            AgentTurn.state == "RUNNING",
                        )
                    ):
                        turn_finished_at = _terminal_time(interrupted_at, turn.started_at)
                        turn.state = "INTERRUPTED"
                        turn.finished_at = turn_finished_at
                        turn.error_code = "JOB_LEASE_EXPIRED"
                session.add(
                    Event(
                        kind="MISSION_INTERRUPTED",
                        aggregate_type="RESEARCH_PROGRAM",
                        aggregate_id=mission.program_id,
                        actor_kind="SYSTEM",
                        actor_metadata={},
                        payload={
                            "mission_id": str(mission.id),
                            "error_code": "JOB_LEASE_EXPIRED",
                            "job_id": str(job.id),
                        },
                    )
                )
        job.state = "READY"
        job.lease_owner = None
        job.lease_expires_at = None
        job.updated_at = current
    session.flush()
    return len(expired)


def claim_next_job(
    session: Session,
    *,
    owner: str,
    lease_seconds: int,
    now: datetime | None = None,
    kind: str | None = None,
) -> Job | None:
    current = now or _now()
    conditions = [Job.state == "READY", Job.available_at <= current]
    if kind is not None:
        conditions.append(Job.kind == kind)
    job = session.execute(
        select(Job)
        .where(*conditions)
        .order_by(Job.available_at.asc(), Job.created_at.asc())
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
    lease: JobLease,
    lease_seconds: int,
    now: datetime | None = None,
) -> bool:
    current = now or _now()
    result = cast(
        CursorResult[Any],
        session.execute(
            update(Job)
            .where(
                *_lease_conditions(lease, current),
            )
            .values(
                lease_expires_at=current + timedelta(seconds=lease_seconds),
                updated_at=current,
            )
        ),
    )
    return int(result.rowcount or 0) == 1


def complete_job(
    session: Session,
    *,
    lease: JobLease,
    now: datetime | None = None,
) -> bool:
    current = now or _now()
    result = cast(
        CursorResult[Any],
        session.execute(
            update(Job)
            .where(*_lease_conditions(lease, current))
            .values(
                state="SUCCEEDED",
                lease_owner=None,
                lease_expires_at=None,
                last_error=None,
                updated_at=current,
            )
        ),
    )
    return int(result.rowcount or 0) == 1


def fail_job(
    session: Session,
    message: str,
    *,
    lease: JobLease,
    now: datetime | None = None,
) -> bool:
    current = now or _now()
    result = cast(
        CursorResult[Any],
        session.execute(
            update(Job)
            .where(*_lease_conditions(lease, current))
            .values(
                state="FAILED",
                lease_owner=None,
                lease_expires_at=None,
                last_error=message,
                updated_at=current,
            )
        ),
    )
    return int(result.rowcount or 0) == 1


def retry_job(
    session: Session,
    message: str,
    *,
    lease: JobLease,
    now: datetime | None = None,
    available_at: datetime | None = None,
) -> bool:
    """Return a still-leased job to READY without changing its frozen resource."""
    current = now or _now()
    result = cast(
        CursorResult[Any],
        session.execute(
            update(Job)
            .where(*_lease_conditions(lease, current))
            .values(
                state="READY",
                lease_owner=None,
                lease_expires_at=None,
                last_error=message,
                available_at=available_at or current,
                updated_at=current,
            )
        ),
    )
    return int(result.rowcount or 0) == 1
