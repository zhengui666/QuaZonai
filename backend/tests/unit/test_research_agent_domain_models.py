from __future__ import annotations

import importlib.util
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import UUID, uuid4

import pytest
from alembic.migration import MigrationContext
from alembic.operations import Operations
from sqlalchemy import Column, DateTime, Integer, MetaData, String, Table, Uuid, inspect, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from db.models import (
    AgentSession,
    AgentTurn,
    ClarificationAnswer,
    ClarificationQuestion,
    IdeaDraft,
    MissionArtifact,
    MissionDependency,
    PreflightReceipt,
    ProgramRelationship,
    ResearchBranch,
    ResearchCharter,
    ResearchCycle,
    ResearchMission,
    ResearchProgram,
)


def _now() -> datetime:
    return datetime.now(UTC)


def _seed_research(
    session: Session, *, idea_draft_id: UUID | None = None
) -> tuple[ResearchProgram, ResearchMission, ResearchMission]:
    now = _now()
    charter = ResearchCharter(
        idea_draft_id=idea_draft_id,
        original_idea_text="Test a bounded research hypothesis.",
        research_question="Does the bounded hypothesis hold?",
        market_scope=[],
        universe_version_ids=[],
        prediction_horizon="1D",
        allowed_data_domains=[],
        explicit_exclusions=[],
        material_assumptions=[],
        system_assumptions=[],
        clarification_transcript=[],
        created_at=now,
    )
    session.add(charter)
    session.flush()
    program = ResearchProgram(charter_id=charter.id, title="bounded research")
    session.add(program)
    session.flush()
    cycle = ResearchCycle(
        program_id=program.id,
        cycle_no=1,
        trigger="IDEA_START",
        state="PLANNED",
        mission_budget=3,
        replan_budget=1,
        runtime_configuration_revision=1,
        summary={},
        created_at=now,
    )
    session.add(cycle)
    session.flush()
    program.current_cycle_id = cycle.id
    branch = ResearchBranch(
        program_id=program.id,
        cycle_id=cycle.id,
        derivation_type="ROOT",
        hypothesis="Test the bounded hypothesis.",
        changed_assumptions=[],
        preserved_constraints=[],
        state="ACTIVE",
        revision_no=1,
        created_at=now,
    )
    session.add(branch)
    session.flush()
    first = ResearchMission(
        program_id=program.id,
        cycle_id=cycle.id,
        branch_id=branch.id,
        type="PLAN_RESEARCH",
        role="RESEARCH_PLANNER",
        state="PLANNED",
        objective="Plan the bounded research.",
        contract_version="1",
        input_snapshot={},
        capability_snapshot={},
        runtime_snapshot={},
        prompt_version="1",
        max_turns=3,
        max_tool_calls=5,
        attempt=1,
        revision=1,
    )
    second = ResearchMission(
        program_id=program.id,
        cycle_id=cycle.id,
        branch_id=branch.id,
        mission_type="DATA_REQUIREMENT",
        role_profile="DATA_STEWARD",
        state="PLANNED",
        objective="Validate data requirements.",
        contract_version="1",
        input_snapshot={},
        capability_snapshot={},
        runtime_snapshot={},
        prompt_version="1",
        max_turns=3,
        max_tool_calls=5,
        attempt=1,
        revision=1,
    )
    session.add_all([first, second])
    session.flush()
    return program, first, second


def test_research_agent_facts_persist_as_normalized_rows(engine) -> None:
    now = _now()
    with Session(engine) as session:
        draft = IdeaDraft(original_idea_text="Find a researchable market effect.")
        session.add(draft)
        session.flush()
        question = ClarificationQuestion(
            idea_draft_id=draft.id,
            ordinal=1,
            question_text="Which market scope changes the research boundary?",
            created_at=now,
        )
        session.add(question)
        session.flush()
        session.add(
            ClarificationAnswer(
                question_id=question.id,
                answer_text="US equities.",
                created_at=now,
            )
        )
        program, first, second = _seed_research(session, idea_draft_id=draft.id)
        session.add(
            MissionDependency(
                mission_id=second.id,
                depends_on_mission_id=first.id,
                required_outcome="SUCCEEDED",
            )
        )
        agent_session = AgentSession(
            mission_id=first.id,
            role_profile="RESEARCH_PLANNER",
            codex_thread_id="thread-test-1",
            codex_version="0.144.4",
            state="RUNNING",
            started_at=now,
        )
        session.add(agent_session)
        session.flush()
        turn = AgentTurn(
            agent_session_id=agent_session.id,
            ordinal=1,
            kind="PLAN",
            codex_turn_id="turn-test-1",
            state="RUNNING",
            input_artifact_ids=[],
            output_artifact_ids=[],
            tool_call_count=0,
            started_at=now,
        )
        session.add(turn)
        session.flush()
        session.add(
            MissionArtifact(
                mission_id=first.id,
                turn_id=turn.id,
                kind="MISSION_GRAPH_PROPOSAL",
                schema_version="1",
                revision=1,
                state="DRAFT",
                storage_uri="missions/test/graph.json",
                metadata_json={},
                created_at=now,
            )
        )
        other = ResearchProgram(charter_id=program.charter_id, title="related research")
        session.add(other)
        session.flush()
        session.add(
            ProgramRelationship(
                from_program_id=program.id,
                to_program_id=other.id,
                relationship_type="RELATED_PROGRAM",
                created_at=now,
            )
        )
        session.add(
            PreflightReceipt(
                resource_type="DOWNSTREAM_SYSTEM",
                resource_id=uuid4(),
                resource_revision=1,
                revision=1,
                status="READY",
                reason_codes=[],
                capabilities=["TARGET_PORTFOLIO"],
                contract_version="1",
                checked_at=now,
                valid_until=now + timedelta(minutes=5),
                checker_version="1",
            )
        )
        session.commit()

        assert first.mission_type == "PLAN_RESEARCH"
        assert first.type == "PLAN_RESEARCH"
        assert first.role_profile == "RESEARCH_PLANNER"
        assert first.role == "RESEARCH_PLANNER"
        assert session.scalar(select(AgentTurn).where(AgentTurn.agent_session_id == agent_session.id))
        assert session.scalar(select(MissionArtifact).where(MissionArtifact.mission_id == first.id))


def test_research_agent_constraints_reject_invalid_normalized_facts(engine) -> None:
    now = _now()
    with Session(engine) as session:
        _, first, second = _seed_research(session)
        first_id = first.id
        second_id = second.id
        session.commit()

    with Session(engine) as session:
        with pytest.raises(IntegrityError):
            session.add(
                MissionDependency(
                    mission_id=first_id,
                    depends_on_mission_id=first_id,
                    required_outcome="SUCCEEDED",
                )
            )
            session.flush()
        session.rollback()

        session.add(
            AgentSession(
                mission_id=first_id,
                role_profile="RESEARCH_PLANNER",
                codex_thread_id="thread-unique",
                codex_version="0.144.4",
                state="PLANNED",
            )
        )
        session.flush()
        with pytest.raises(IntegrityError):
            session.add(
                AgentSession(
                    mission_id=first_id,
                    role_profile="RESEARCH_PLANNER",
                    codex_thread_id="thread-duplicate",
                    codex_version="0.144.4",
                    state="PLANNED",
                )
            )
            session.flush()
        session.rollback()

        session.add(
            PreflightReceipt(
                resource_type="DOWNSTREAM_SYSTEM",
                resource_id=uuid4(),
                resource_revision=1,
                revision=1,
                status="READY",
                reason_codes=[],
                capabilities=[],
                contract_version="1",
                checked_at=now,
                valid_until=now - timedelta(seconds=1),
                checker_version="1",
            )
        )
        with pytest.raises(IntegrityError):
            session.flush()
        session.rollback()

        assert second_id != first_id


def test_phase_a_tables_do_not_add_content_address_fields() -> None:
    prohibited = ("sha", "hash", "digest", "fingerprint")
    table_names = {
        "idea_drafts",
        "clarification_questions",
        "clarification_answers",
        "research_cycles",
        "mission_dependencies",
        "agent_sessions",
        "agent_turns",
        "mission_artifacts",
        "program_relationships",
        "preflight_receipts",
    }
    from db.models import Base

    for table_name in table_names:
        assert table_name in Base.metadata.tables
        assert not {
            column.name
            for column in Base.metadata.tables[table_name].columns
            if any(word in column.name.lower() for word in prohibited)
        }


def test_0016_keeps_legacy_mission_columns_as_the_single_source_of_truth() -> None:
    from sqlalchemy import create_engine

    engine = create_engine("sqlite+pysqlite:///:memory:")
    metadata = MetaData()
    Table("research_charters", metadata, Column("id", Uuid(), primary_key=True))
    Table(
        "research_programs",
        metadata,
        Column("id", Uuid(), primary_key=True),
        Column("charter_id", Uuid(), nullable=False),
        Column("state", String(40), nullable=False),
        Column("revision", Integer(), nullable=False),
    )
    Table(
        "research_branches",
        metadata,
        Column("id", Uuid(), primary_key=True),
        Column("program_id", Uuid(), nullable=False),
    )
    Table(
        "research_missions",
        metadata,
        Column("id", Uuid(), primary_key=True),
        Column("program_id", Uuid(), nullable=False),
        Column("branch_id", Uuid(), nullable=False),
        Column("type", String(100), nullable=False),
        Column("role", String(100), nullable=True),
        Column("state", String(40), nullable=False),
        Column("attempt", Integer(), nullable=False),
        Column("started_at", DateTime(timezone=True)),
        Column("finished_at", DateTime(timezone=True)),
    )
    metadata.create_all(engine)
    ids = (uuid4(), uuid4(), uuid4(), uuid4())
    with engine.begin() as connection:
        connection.execute(
            metadata.tables["research_missions"].insert().values(
                id=ids[3],
                program_id=ids[1],
                branch_id=ids[2],
                type="PLAN_RESEARCH",
                role="RESEARCH_PLANNER",
                state="PLANNED",
                attempt=1,
            )
        )
        module_path = Path(__file__).parents[2] / "alembic/versions/0016_research_agent_domain.py"
        spec = importlib.util.spec_from_file_location("migration_0016", module_path)
        assert spec and spec.loader
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            module.upgrade()

        columns = {column["name"] for column in inspect(connection).get_columns("research_missions")}
        assert {"type", "role", "cycle_id"} <= columns
        assert "mission_type" not in columns
        assert "role_profile" not in columns
        charter_columns = {
            column["name"] for column in inspect(connection).get_columns("research_charters")
        }
        assert "idea_draft_id" in charter_columns
        missions = Table("research_missions", MetaData(), autoload_with=connection)
        row = connection.execute(
            select(missions.c.type, missions.c.role).where(missions.c.id == ids[3])
        ).one()
        assert tuple(row) == ("PLAN_RESEARCH", "RESEARCH_PLANNER")
        assert "preflight_receipts" in inspect(connection).get_table_names()
