from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from sqlalchemy import Engine, func, select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidateBundle,
    DatasetRevision,
    DegradationFollowup,
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
    SearchLedgerEntry,
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
        session.add(root_branch)
        session.flush()
        dataset = DatasetRevision(
            universe_name="FX",
            revision_no=1,
            event_start=now - timedelta(days=30),
            event_end=now - timedelta(days=20),
            available_start=now - timedelta(days=30),
            available_end=now - timedelta(days=20),
            row_count=100,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=now,
            provider_name="test",
            source_license="test",
            catalog_uri="nautilus-catalog://degradation-source",
            nautilus_data_type="QuoteTick",
            instrument_scope=["EUR/USD.SIM"],
            schema_revision="quote-v2",
            quality_result={"state": "VALID"},
            point_in_time_result={"state": "VALID"},
            ingested_at=now,
        )
        session.add(dataset)
        session.flush()
        source_experiment_id = uuid4()
        source = SearchLedgerEntry(
            id=source_experiment_id,
            program_id=program.id,
            branch_id=root_branch.id,
            mission_id=None,
            dataset_revision_id=dataset.id,
            parent_entry_id=None,
            mode="DISCOVERY",
            state="SUCCEEDED",
            runtime_name="NAUTILUS_TRADER",
            runtime_version="1.231.0",
            remote_run_id="degradation-source",
            request_json={},
            evidence_json={"orders": [{}], "fills": [{}], "positions": [{}], "pnl": {}, "statistics": {}},
            disclosure_json={},
            started_at=now,
            finished_at=now,
        )
        session.add(source)
        session.flush()
        unrelated_branch = ResearchBranch(
            program_id=program.id,
            parent_branch_id=root_branch.id,
            derivation_type="ALTERNATIVE",
            hypothesis="Unrelated later branch.",
            changed_assumptions=[],
            preserved_constraints=[],
            state="ACTIVE",
            created_at=now + timedelta(seconds=1),
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
            source_experiment_id=source_experiment_id,
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
        session.add_all([unrelated_branch, alpha, portfolio_program, downstream])
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
        root_branch_id = root_branch.id
        handoff_id = handoff.id

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
        assert alpha.degradation_state == "HEALTHY"
        assert "degradation_followup_episode_id" not in alpha.metrics
        assert mission.state == "READY"
        assert mission.dependencies == [str(alpha.source_experiment_id), str(episode_id)]
        branch = session.get(ResearchBranch, mission.branch_id)
        assert branch is not None
        assert branch.derivation_type == "FORWARD_DEGRADATION"
        assert branch.parent_branch_id == root_branch_id
        assert "NO_LIVE_CONTROL" in branch.preserved_constraints
        followup = session.scalar(
            select(DegradationFollowup).where(
                DegradationFollowup.alpha_qualification_id == alpha_id,
                DegradationFollowup.forward_evidence_episode_id == episode_id,
            )
        )
        assert followup is not None
        assert followup.source_experiment_id == alpha.source_experiment_id
        assert followup.mission_id == mission.id
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
        second = ForwardEvidenceEpisode(
            handoff_id=handoff_id,
            state="FEEDBACK_COMPLETE",
            evidence={"degraded": True, "degradation_state": "DEGRADED"},
            observation_start=now - timedelta(days=1),
            observation_end=now,
            sample_size=50,
            created_at=now + timedelta(seconds=2),
        )
        session.add(second)

    with factory() as session, session.begin():
        assert schedule_degradation_missions(session) == 1
    with factory() as session, session.begin():
        assert schedule_degradation_missions(session) == 0
        assert session.scalar(select(func.count()).select_from(DegradationFollowup)) == 2
