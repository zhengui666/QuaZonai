from __future__ import annotations

import sys
import tomllib
from contextlib import contextmanager
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from pathlib import Path
from types import SimpleNamespace
from uuid import uuid4

import pytest
from sqlalchemy import select
from sqlalchemy.orm import Session

import runners.research_missions as research_missions
import jobs
from agent_harness.contracts import MissionType
from agent_harness.mcp_server import load_mission_contract
from db.models import (
    AgentSession,
    AgentTurn,
    Base,
    Job,
    MarketUniverseVersion,
    MissionArtifact,
    ResearchMission,
    ResearchProgram,
)
from db.session import create_database_engine
from errors import QfError
from jobs import (
    JobLease,
    claim_next_job,
    create_lease_fenced_session_factory,
    release_expired_leases,
)
from research_lifecycle import answer_draft, create_draft, draft_questions, start_draft
from runners.research_missions import _prepare_worktree


def test_degradation_missions_use_only_their_bounded_visible_turns() -> None:
    diagnose = research_missions._execution_turns(MissionType.DEGRADATION_DIAGNOSIS)
    replan = research_missions._execution_turns(MissionType.REPLAN)

    assert tuple(kind for kind, _ in diagnose) == ("DIAGNOSE",)
    assert "RESULT.md" in diagnose[0][1]
    assert tuple(kind for kind, _ in replan) == ("REPLAN",)
    assert research_missions._required_turn_kinds(MissionType.REPLAN) == ("REPLAN", "REVIEW")


def test_prepare_worktree_reuses_the_durable_mission_workspace(settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    configured = replace(settings, mission_root=tmp_path / "mission-root")
    program_id, mission_id = uuid4(), uuid4()
    workspace = _prepare_worktree(configured, program_id, mission_id)
    evidence = workspace / "evidence.txt"
    evidence.write_text("preserve me", encoding="utf-8")

    assert _prepare_worktree(configured, program_id, mission_id) == workspace
    assert evidence.read_text(encoding="utf-8") == "preserve me"


def test_prepare_worktree_refuses_a_deleted_recorded_workspace(settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    configured = replace(settings, mission_root=tmp_path / "mission-root")
    program_id, mission_id = uuid4(), uuid4()
    workspace = _prepare_worktree(configured, program_id, mission_id)
    workspace.rename(tmp_path / "removed-worktree")

    with pytest.raises(QfError) as exc_info:
        _prepare_worktree(
            configured,
            program_id,
            mission_id,
            recorded_workspace=str(workspace),
            resuming=True,
        )

    assert exc_info.value.code == "MISSION_WORKSPACE_MISSING"
    assert not workspace.exists()


def _leased_mission(settings, tmp_path):  # type: ignore[no-untyped-def]
    configured = replace(
        settings,
        database_url=f"sqlite+pysqlite:///{tmp_path / 'runner.db'}",
        alembic_url=f"sqlite+pysqlite:///{tmp_path / 'runner.db'}",
        mission_root=tmp_path / "mission-root",
    )
    engine = create_database_engine(configured)
    Base.metadata.create_all(engine)
    with Session(engine) as session, session.begin():
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
        program = start_draft(session, draft.id, expected_revision=draft.revision)
        mission = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.mission_type == "PLAN_RESEARCH",
            )
        )
        assert mission is not None
        session.add(
            MissionArtifact(
                mission_id=mission.id,
                kind="RESEARCH_PLAN",
                schema_version="v1",
                revision=1,
                state="VALIDATED",
                storage_uri="artifact://plans/validated",
                metadata_json={},
                created_at=datetime.now(UTC),
            )
        )
        claimed = claim_next_job(session, owner="worker", lease_seconds=60)
        assert claimed is not None and claimed.resource_id == mission.id
        assert claimed.lease_owner is not None
        lease = JobLease(claimed.id, claimed.lease_owner, claimed.attempt)
        return configured, engine, lease, mission.id


def test_runner_persists_a_real_turn_without_legacy_strategy_execution(
    settings, tmp_path, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    configured, engine, lease, mission_id = _leased_mission(settings, tmp_path)

    class FakeTurn:
        def __init__(self, ordinal: int, prompt: str) -> None:
            self.id = f"turn-{ordinal}"
            self._prompt = prompt

        def run(self):
            if self._prompt.startswith("RESULT.md is missing"):
                workspace = configured.mission_root / "worktrees" / str(mission_id)
                (workspace / "RESULT.md").write_text("Bounded conclusion.", encoding="utf-8")
            return SimpleNamespace(id=self.id, final_response="Bounded conclusion.")

    class FakeThread:
        id = "thread-1"

        def __init__(self) -> None:
            self._turns = 0

        def turn(self, prompt):
            self._turns += 1
            return FakeTurn(self._turns, prompt)

    captured: dict[str, Path] = {}
    auth_factories: list[object] = []

    @contextmanager
    def fake_thread(*_args, **_kwargs):
        auth_factories.append(_args[2])
        captured["contract_file"] = _kwargs["mission_contract_file"]
        yield FakeThread()

    monkeypatch.setattr(research_missions.importlib.util, "find_spec", lambda _: object())
    monkeypatch.setattr(research_missions, "codex_sandbox_preflight", lambda: None)
    monkeypatch.setattr(research_missions, "_mission_codex_thread", fake_thread)

    research_missions.run_mission(configured, lease)

    with Session(engine) as session:
        mission = session.get(ResearchMission, mission_id)
        assert mission is not None and mission.state == "SUCCEEDED"
        assert mission.max_turns == 7
        assert mission.runtime_snapshot["effective_codex_runtime"] == {
            "model": None,
            "reasoning_effort": None,
            "fast_mode": False,
            "base_url": None,
            "api_key_configured": False,
        }
        agent_session = session.query(AgentSession).filter_by(mission_id=mission_id).one()
        assert agent_session.codex_thread_id == "thread-1"
        turns = session.scalars(
            select(AgentTurn)
            .where(AgentTurn.agent_session_id == agent_session.id)
            .order_by(AgentTurn.ordinal)
        ).all()
        assert [(turn.kind, turn.state) for turn in turns] == [
            ("PLAN", "SUCCEEDED"),
            ("IMPLEMENT", "SUCCEEDED"),
            ("VALIDATE", "SUCCEEDED"),
            ("EXECUTE", "SUCCEEDED"),
            ("REPAIR", "SUCCEEDED"),
            ("REVIEW", "SUCCEEDED"),
        ]
    contract = load_mission_contract(captured["contract_file"])
    assert contract.mission_id == mission_id
    assert contract.charter_snapshot["research_question"]
    auth_factory = auth_factories[0]
    job_fence_calls: list[object] = []
    monkeypatch.setattr(jobs, "_require_current_lease", lambda *args, **kwargs: job_fence_calls.append(args))
    with auth_factory() as session:
        assert isinstance(session, Session)
        assert "job_lease" not in session.info
        session.execute(select(Job.id)).all()
        session.commit()
    assert not job_fence_calls
    engine.dispose()


def test_interrupted_runner_reuses_its_thread_workspace_and_frozen_runtime(
    settings, tmp_path, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    configured, engine, lease, mission_id = _leased_mission(settings, tmp_path)
    frozen = replace(
        configured,
        codex_model="frozen-model",
        codex_reasoning_effort="high",
        codex_fast_mode=False,
    )
    with Session(engine) as session:
        mission = session.get(ResearchMission, mission_id)
        assert mission is not None
        workspace = _prepare_worktree(frozen, mission.program_id, mission.id)
        (workspace / "evidence.txt").write_text("keep", encoding="utf-8")
        timestamp = datetime.now(UTC)
        mission.state = "INTERRUPTED"
        mission.started_at = timestamp
        mission.finished_at = timestamp
        mission.error_code = "JOB_LEASE_EXPIRED"
        mission.codex_thread_id = "thread-durable"
        mission.workspace_path = str(workspace)
        mission.runtime_snapshot = {
            **mission.runtime_snapshot,
            "effective_codex_runtime": research_missions._effective_codex_runtime(frozen),
        }
        session.add(
            AgentSession(
                mission_id=mission.id,
                role_profile=mission.role_profile or "UNKNOWN",
                codex_thread_id="thread-durable",
                codex_version="openai_codex",
                model=frozen.codex_model,
                reasoning_effort=frozen.codex_reasoning_effort,
                service_tier=None,
                state="INTERRUPTED",
                started_at=timestamp,
                finished_at=timestamp,
                last_event_at=timestamp,
            )
        )
        session.commit()

    class FakeTurn:
        def __init__(self, ordinal: int, prompt: str) -> None:
            self.id = f"turn-resume-{ordinal}"
            self._prompt = prompt

        def run(self):
            if self._prompt.startswith("RESULT.md is missing"):
                (workspace / "RESULT.md").write_text("Resumed conclusion.", encoding="utf-8")
            return SimpleNamespace(id=self.id, final_response="Resumed conclusion.")

    class FakeThread:
        id = "thread-durable"

        def __init__(self) -> None:
            self._turns = 0

        def turn(self, prompt):
            self._turns += 1
            return FakeTurn(self._turns, prompt)

    captured: dict[str, object] = {}

    @contextmanager
    def fake_thread(runtime_settings, current_workspace, *_args, **kwargs):
        captured["settings"] = runtime_settings
        captured["workspace"] = current_workspace
        captured["existing_thread_id"] = kwargs["existing_thread_id"]
        yield FakeThread()

    monkeypatch.setattr(research_missions.importlib.util, "find_spec", lambda _: object())
    monkeypatch.setattr(research_missions, "codex_sandbox_preflight", lambda: None)
    monkeypatch.setattr(research_missions, "_mission_codex_thread", fake_thread)

    research_missions.run_mission(
        replace(
            configured,
            codex_model="changed-model",
            codex_reasoning_effort="minimal",
            codex_fast_mode=True,
        ),
        lease,
    )

    resumed_settings = captured["settings"]
    assert isinstance(resumed_settings, type(configured))
    assert resumed_settings.codex_model == "frozen-model"
    assert resumed_settings.codex_reasoning_effort == "high"
    assert resumed_settings.codex_fast_mode is False
    assert captured["workspace"] == workspace
    assert captured["existing_thread_id"] == "thread-durable"
    assert (workspace / "evidence.txt").read_text(encoding="utf-8") == "keep"
    with Session(engine) as session:
        mission = session.get(ResearchMission, mission_id)
        assert mission is not None and mission.state == "SUCCEEDED"
        assert mission.attempt == 2
        assert (
            session.query(AgentSession).filter_by(mission_id=mission_id).one().codex_thread_id
            == "thread-durable"
        )
    engine.dispose()


def test_reclaimed_mission_cannot_finish_a_stale_turn(
    settings, tmp_path, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    configured, engine, lease, mission_id = _leased_mission(settings, tmp_path)

    class FakeTurn:
        id = "turn-stale"

        def run(self):
            current = datetime.now(UTC)
            with Session(engine) as session, session.begin():
                job = session.get(Job, lease.job_id)
                assert job is not None
                job.lease_expires_at = current - timedelta(seconds=1)
            with Session(engine) as session, session.begin():
                assert release_expired_leases(session, now=current) == 1
                reclaimed = claim_next_job(
                    session,
                    owner=lease.owner,
                    lease_seconds=60,
                    now=current,
                )
                assert reclaimed is not None and reclaimed.attempt == lease.attempt + 1
            return SimpleNamespace(id=self.id, final_response="stale completion")

    class FakeThread:
        id = "thread-stale"

        def turn(self, _prompt):
            return FakeTurn()

    @contextmanager
    def fake_thread(*_args, **_kwargs):
        yield FakeThread()

    monkeypatch.setattr(research_missions.importlib.util, "find_spec", lambda _: object())
    monkeypatch.setattr(research_missions, "codex_sandbox_preflight", lambda: None)
    monkeypatch.setattr(research_missions, "_mission_codex_thread", fake_thread)

    with pytest.raises(QfError, match="JOB_LEASE_LOST"):
        research_missions.run_mission(configured, lease)

    with Session(engine) as session:
        mission = session.get(ResearchMission, mission_id)
        job = session.get(Job, lease.job_id)
        agent_session = session.scalar(select(AgentSession).where(AgentSession.mission_id == mission_id))
        assert mission is not None and mission.state == "INTERRUPTED"
        assert job is not None
        assert agent_session is not None and agent_session.state == "INTERRUPTED"
        assert (job.state, job.lease_owner, job.attempt) == (
            "LEASED",
            lease.owner,
            lease.attempt + 1,
        )
        turns = list(
            session.scalars(
                select(AgentTurn).where(AgentTurn.agent_session_id == agent_session.id)
            )
        )
        assert [turn.state for turn in turns] == ["INTERRUPTED"]
    engine.dispose()


def test_frozen_runtime_refuses_provider_route_drift(settings) -> None:  # type: ignore[no-untyped-def]
    snapshot = {"effective_codex_runtime": research_missions._effective_codex_runtime(settings)}

    with pytest.raises(QfError) as exc_info:
        research_missions._frozen_mission_settings(
            replace(settings, codex_base_url="https://provider.example/v1"), snapshot
        )

    assert exc_info.value.code == "MISSION_RUNTIME_CONFIGURATION_CHANGED"


def test_runner_rejects_a_paused_program_before_admission(settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    configured, engine, lease, mission_id = _leased_mission(settings, tmp_path)
    with Session(engine) as session, session.begin():
        mission = session.get(ResearchMission, mission_id)
        assert mission is not None
        program = session.get(ResearchProgram, mission.program_id)
        assert program is not None
        program.state = "PAUSED"

    with pytest.raises(QfError, match="Paused or archived"):
        research_missions._load_mission_context(
            configured,
            create_lease_fenced_session_factory(engine, lease),
            lease,
        )

    engine.dispose()


def test_codex_launch_config_has_one_frozen_contract_mcp_server(
    settings, tmp_path: Path
) -> None:  # type: ignore[no-untyped-def]
    contract_file = tmp_path / "contracts" / "mission.json"
    config, _ = research_missions._codex_launch_configuration(
        settings,
        tmp_path,
        mission_contract_file=contract_file,
    )
    override = next(value for value in config.config_overrides if value.startswith("mcp_servers ="))
    mcp_servers = tomllib.loads(override)["mcp_servers"]

    assert mcp_servers == {
        "quazonai_mission": {
            "command": sys.executable,
            "args": [
                "-I",
                "-m",
                "agent_harness.mcp_server",
                "--contract-file",
                str(contract_file),
            ],
            "cwd": str(research_missions.Path(__file__).resolve().parents[2]),
        }
    }
