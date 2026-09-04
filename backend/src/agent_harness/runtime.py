"""Durable App Server thread and observable turn bookkeeping."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Protocol

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from db.models import AgentSession, AgentTurn, ResearchMission
from errors import QfError


class ThreadLike(Protocol):
    id: str


@dataclass(frozen=True, slots=True)
class ThreadAdmission:
    thread: ThreadLike
    resumed: bool


def open_durable_thread(
    existing_thread_id: str | None,
    *,
    start: Callable[[], ThreadLike],
    resume: Callable[[str], ThreadLike],
) -> ThreadAdmission:
    """Use exactly one durable Mission thread, resuming it after a retry."""
    if existing_thread_id:
        return ThreadAdmission(thread=resume(existing_thread_id), resumed=True)
    return ThreadAdmission(thread=start(), resumed=False)


def now() -> datetime:
    return datetime.now(UTC)


def record_session_admission(
    session: Session,
    mission: ResearchMission,
    *,
    thread_id: str,
    codex_version: str,
    model: str | None,
    reasoning_effort: str | None,
    service_tier: str | None,
) -> AgentSession:
    """Persist a new/resumed App Server context after QZ admits the Mission."""
    if mission.state not in {"READY", "INTERRUPTED"}:
        raise QfError(
            "MISSION_STATE_CONFLICT",
            "Only READY or INTERRUPTED Missions may be admitted.",
            409,
            {"state": mission.state},
        )
    resumed = mission.state == "INTERRUPTED"
    existing = session.execute(
        select(AgentSession).where(AgentSession.mission_id == mission.id).with_for_update()
    ).scalar_one_or_none()
    timestamp = now()
    if existing is None:
        agent_session = AgentSession(
            mission_id=mission.id,
            role_profile=mission.role_profile or "UNKNOWN",
            codex_thread_id=thread_id,
            codex_version=codex_version,
            model=model,
            reasoning_effort=reasoning_effort,
            service_tier=service_tier,
            state="RUNNING",
            started_at=timestamp,
            last_event_at=timestamp,
        )
        session.add(agent_session)
    else:
        if existing.codex_thread_id != thread_id:
            raise QfError(
                "MISSION_THREAD_CONFLICT",
                "A Mission must retain its durable Codex Thread.",
                409,
            )
        existing.state = "RUNNING"
        existing.finished_at = None
        existing.last_event_at = timestamp
        agent_session = existing
    mission.codex_thread_id = thread_id
    mission.state = "RUNNING"
    mission.started_at = mission.started_at or timestamp
    mission.finished_at = None
    mission.error_code = None
    if resumed:
        mission.attempt += 1
    mission.revision += 1
    return agent_session


def begin_turn(session: Session, agent_session: AgentSession, *, kind: str, codex_turn_id: str) -> AgentTurn:
    """Record one observable App Server turn; never persist hidden reasoning."""
    ordinal = int(
        session.scalar(
            select(func.coalesce(func.max(AgentTurn.ordinal), 0)).where(
                AgentTurn.agent_session_id == agent_session.id
            )
        )
        or 0
    ) + 1
    turn = AgentTurn(
        agent_session_id=agent_session.id,
        ordinal=ordinal,
        kind=kind,
        codex_turn_id=codex_turn_id,
        state="RUNNING",
        input_artifact_ids=[],
        output_artifact_ids=[],
        tool_call_count=0,
        started_at=now(),
    )
    session.add(turn)
    agent_session.last_event_at = turn.started_at
    return turn


def finish_turn(
    turn: AgentTurn,
    agent_session: AgentSession,
    *,
    summary: str | None,
    tool_call_count: int = 0,
    error_code: str | None = None,
) -> None:
    if tool_call_count < 0:
        raise QfError("AGENT_TURN_INVALID", "Tool call count cannot be negative.", 422)
    timestamp = now()
    turn.state = "FAILED" if error_code else "SUCCEEDED"
    turn.finished_at = timestamp
    turn.observable_summary = summary[-12_000:] if summary else None
    turn.tool_call_count = tool_call_count
    turn.error_code = error_code
    agent_session.last_event_at = timestamp


def mark_session_interrupted(agent_session: AgentSession, *, error_code: str) -> None:
    timestamp = now()
    agent_session.state = "INTERRUPTED"
    agent_session.finished_at = timestamp
    agent_session.last_event_at = timestamp


def mark_session_finished(agent_session: AgentSession, *, succeeded: bool) -> None:
    timestamp = now()
    agent_session.state = "SUCCEEDED" if succeeded else "FAILED"
    agent_session.finished_at = timestamp
    agent_session.last_event_at = timestamp
