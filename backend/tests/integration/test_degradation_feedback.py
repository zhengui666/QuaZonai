from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from sqlalchemy import Engine, func, select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidateBundle,
    DownstreamSystem,
    ForwardEvidenceEpisode,
    HandoffOffer,
    Job,
    PortfolioCandidate,
    PortfolioProgram,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)
from db.session import create_session_factory
from quant_runtime.degradation import schedule_degradation_missions


def test_explicit_degradation_feedback_queues_research_only(engine: Engine) -> None:
    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    with factory() as session, session.begin():
        charter = ResearchCharter(
            original_idea_text="Research a governed signal.",
            research_question="Does the signal remain robust?",
            market_scope="FX",
            universe_version_ids=[],
            prediction_horizon="1D",
            allowed_data_domains=["quotes"],
            explicit_exclusions=[],
            material_assumptions=[],
            system_assumptions=[],
            created_at=now,
        )
        session.add(charter)
        session.flush()
        program = ResearchProgram(charter_id=charter.id, title="Forward degradation test", state="ACTIVE")
        session.add(program)
        session.flush()
        root_branch = ResearchBranch(
            program_id=program.id,
            parent_branch_id=None,
            derivation_type="ROOT",
            hypothesis="Initial governed hypothesis.",
            changed_assumptions=[],
            preserved_constraints=[],
            state="ACTIVE",
            created_at=now,
        )
        alpha = AlphaQualification(
            program_id=program.id,
            universe="FX",
            horizon="1D",
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            name="Qualified alpha",
            scope_json={},
            degradation_state="HEALTHY",
            metrics={"search_adjusted_quality": 0.8},
            lineage=[],
            created_at=now,
        )
        portfolio_program = PortfolioProgram(
            mandate_version_id=uuid4(),
            mandate_name="Research Portfolio",
            state="CANDIDATE_READY",
        )
        downstream = DownstreamSystem(
            name="Independent Paper Runtime",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="2",
            feedback_contract_version="1",
            compatibility=[],
            preflight_state="READY",
            public_config={},
        )
        session.add_all([root_branch, alpha, portfolio_program, downstream])
        session.flush()
        candidate = PortfolioCandidate(
            portfolio_program_id=portfolio_program.id,
            mandate_version_id=portfolio_program.mandate_version_id,
            mandate_name=portfolio_program.mandate_name,
            state="READY",
            members=[
                {
                    "alpha_qualification_id": str(alpha.id),
                    "target_weight": 1.0,
                    "instrument_ids": ["EUR/USD.SIM"],
                }
            ],
            metrics={},
            created_at=now,
        )
        session.add(candidate)
        session.flush()
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="APPROVED",
            valid_until=now + timedelta(days=7),
            recommendation_rationale="Approved for independent paper validation.",
            human_report={},
            evidence_summary={},
            capital_context={},
            risk_summary={},
            cost_summary={},
            capacity_summary={},
            changes_summary={},
        )
        session.add(approval)
        session.flush()
        bundle = CandidateBundle(
            approval_id=approval.id,
            candidate_id=candidate.id,
            contract_version="2",
            state="AVAILABLE",
            manifest_json={},
            relative_path="fixture/candidate-bundle.zip",
            payload={},
            created_at=now,
        )
        session.add(bundle)
        session.flush()
        handoff = HandoffOffer(
            approval_id=approval.id,
            candidate_bundle_id=bundle.id,
            candidate_id=candidate.id,
            purpose="PAPER",
            downstream_system_id=downstream.id,
            state="FEEDBACK_COMPLETE",
            feedback_state="FEEDBACK_COMPLETE",
            feedback_contract_snapshot={},
        )
        session.add(handoff)
        session.flush()
        episode = ForwardEvidenceEpisode(
            handoff_id=handoff.id,
            state="FEEDBACK_COMPLETE",
            evidence={
                "return": -0.15,
                "degraded": True,
                "degradation_state": "DEGRADED",
            },
            observation_start=now - timedelta(days=3),
            observation_end=now,
            sample_size=100,
            created_at=now,
        )
        session.add(episode)
        session.flush()
        episode_id = episode.id
        alpha_id = alpha.id
        program_id = program.id

    with factory() as session, session.begin():
        assert schedule_degradation_missions(session) == 1

    with factory() as session:
        alpha = session.get(AlphaQualification, alpha_id)
        mission = session.scalar(
            select(ResearchMission).where(
                ResearchMission.program_id == program_id,
                ResearchMission.type == "ALPHA_DEGRADATION_RESEARCH",
            )
        )
        assert alpha is not None
        assert mission is not None
        assert alpha.degradation_state == "DEGRADED"
        assert alpha.metrics["degradation_followup_episode_id"] == str(episode_id)
        assert mission.state == "READY"
        branch = session.get(ResearchBranch, mission.branch_id)
        assert branch is not None
        assert branch.derivation_type == "FORWARD_DEGRADATION"
        assert "NO_LIVE_CONTROL" in branch.preserved_constraints
        job = session.scalar(
            select(Job).where(Job.kind == "RESEARCH_MISSION", Job.resource_id == mission.id)
        )
        assert job is not None
        assert job.state == "READY"
        # Re-scanning the same feedback is idempotent: no duplicate branch/Mission/job.
        assert session.scalar(
            select(func.count())
            .select_from(ResearchMission)
            .where(ResearchMission.type == "ALPHA_DEGRADATION_RESEARCH")
        ) == 1

    with factory() as session, session.begin():
        assert schedule_degradation_missions(session) == 0
