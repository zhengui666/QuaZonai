from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta

import pytest
from sqlalchemy import select
from sqlalchemy.orm import Session

from agent_harness.orchestrator import finish_mission
from agent_harness.runtime import begin_turn, finish_turn, open_durable_thread, record_session_admission
from db.models import (
    AgentSession,
    AgentTurn,
    Job,
    MarketUniverseVersion,
    MissionArtifact,
    ResearchMission,
    ResearchCycle,
)
from errors import QfError
from jobs import release_expired_leases
from research_lifecycle import answer_draft, create_draft, draft_questions, start_draft


@dataclass
class _Thread:
    id: str


def _program_with_graph(session: Session):
    session.add(
        MarketUniverseVersion(
            universe_key="TEST_UNIVERSE",
            version_no=1,
            name="Test Universe",
            spec_json={},
            created_at=datetime.now(UTC),
        )
    )
    draft = create_draft(session, "Research a bounded, liquid market signal.")
    questions = draft_questions(session, draft.id)
    answer_draft(
        session,
        draft.id,
        {
            question.id: ("1D" if question.ordinal == 2 else f"answer-{question.ordinal}")
            for question in questions
        },
        expected_revision=draft.revision,
    )
    return start_draft(session, draft.id, expected_revision=draft.revision)


def test_durable_thread_resumes_and_success_unlocks_only_dependants(engine) -> None:
    with Session(engine) as session, session.begin():
        program = _program_with_graph(session)
        plan = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.mission_type == "PLAN_RESEARCH",
            )
        )
        assert plan is not None and plan.state == "READY"
        admission = open_durable_thread(
            None,
            start=lambda: _Thread("thread-new"),
            resume=lambda _: (_ for _ in ()).throw(AssertionError("must not resume")),
        )
        assert not admission.resumed
        agent_session = record_session_admission(
            session,
            plan,
            thread_id=admission.thread.id,
            codex_version="0.144.4",
            model=None,
            reasoning_effort=None,
            service_tier=None,
        )
        turn = begin_turn(session, agent_session, kind="PLAN", codex_turn_id="turn-plan")
        finish_turn(turn, agent_session, summary="A bounded plan was submitted.")
        finish_mission(session, plan.id, succeeded=True, summary="Plan accepted.")

        data_mission = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.mission_type == "DATA_QUALITY",
            )
        )
        assert data_mission is not None and data_mission.state == "READY"
        assert session.scalar(select(Job).where(Job.resource_id == data_mission.id)) is not None
        assert agent_session.state == "SUCCEEDED"

    resumed = open_durable_thread(
        "thread-new",
        start=lambda: (_ for _ in ()).throw(AssertionError("must not start")),
        resume=lambda thread_id: _Thread(thread_id),
    )
    assert resumed.resumed and resumed.thread.id == "thread-new"

    with Session(engine) as session:
        assert session.scalar(select(AgentSession).where(AgentSession.codex_thread_id == "thread-new"))


def test_explicit_validated_output_gate_fails_closed_then_unlocks(engine) -> None:
    with Session(engine) as session, session.begin():
        program = _program_with_graph(session)
        plan = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.mission_type == "PLAN_RESEARCH",
            )
        )
        assert plan is not None
        plan.state = "RUNNING"
        plan.started_at = datetime.now(UTC)
        with pytest.raises(QfError) as exc_info:
            finish_mission(
                session,
                plan.id,
                succeeded=True,
                require_validated_output=True,
            )
        assert exc_info.value.code == "MISSION_VALIDATED_ARTIFACT_REQUIRED"
        assert plan.state == "RUNNING"

        session.add(
            MissionArtifact(
                mission_id=plan.id,
                kind="RESEARCH_PLAN",
                schema_version="v1",
                revision=1,
                state="VALIDATED",
                storage_uri="artifact://plans/validated",
                metadata_json={},
                created_at=datetime.now(UTC),
            )
        )
        finish_mission(
            session,
            plan.id,
            succeeded=True,
            require_validated_output=True,
        )
        data_mission = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.mission_type == "DATA_QUALITY",
            )
        )
        assert data_mission is not None and data_mission.state == "READY"


def test_ready_failure_closes_cycle_and_cools_program(engine) -> None:
    with Session(engine) as session, session.begin():
        program = _program_with_graph(session)
        plan = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.mission_type == "PLAN_RESEARCH",
            )
        )
        assert plan is not None and plan.state == "READY"
        finish_mission(session, plan.id, succeeded=False, error_code="PRE_ADMISSION_FAILED")
        session.flush()
        cycle = session.scalar(select(ResearchCycle).where(ResearchCycle.program_id == program.id))
        assert cycle is not None and cycle.state == "FAILED"
        session.refresh(program)
        assert program.state == "COOLING"


def test_expired_mission_lease_interrupts_then_allows_same_thread_resume(engine) -> None:
    current = datetime.now(UTC)
    with Session(engine) as session, session.begin():
        program = _program_with_graph(session)
        mission = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.mission_type == "PLAN_RESEARCH",
            )
        )
        assert mission is not None
        agent_session = record_session_admission(
            session,
            mission,
            thread_id="thread-durable",
            codex_version="0.144.4",
            model=None,
            reasoning_effort=None,
            service_tier=None,
        )
        begin_turn(session, agent_session, kind="PLAN", codex_turn_id="turn-running")
        job = session.scalar(select(Job).where(Job.resource_id == mission.id))
        assert job is not None
        job.state = "LEASED"
        job.lease_owner = "dead-worker"
        job.lease_expires_at = current - timedelta(seconds=1)
        assert release_expired_leases(session, now=current) == 1
        assert mission.state == "INTERRUPTED"
        assert agent_session.state == "INTERRUPTED"
        assert job.state == "READY"
        assert session.scalar(select(AgentTurn).where(AgentTurn.state == "INTERRUPTED")) is not None

        resumed = record_session_admission(
            session,
            mission,
            thread_id="thread-durable",
            codex_version="0.144.4",
            model=None,
            reasoning_effort=None,
            service_tier=None,
        )
        assert mission.state == "RUNNING"
        assert resumed.state == "RUNNING"
        assert mission.attempt == 2
