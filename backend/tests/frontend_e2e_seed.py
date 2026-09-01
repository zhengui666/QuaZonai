"""Seed deterministic browser-test facts into the real test database.

This module lives under tests and is never imported by production runtime code.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import UUID

from sqlalchemy import delete

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidatePackage,
    DatasetRevision,
    DownstreamSystem,
    ForwardEvidenceEpisode,
    GovernedDataSource,
    HandoffOffer,
    IdeaContribution,
    MarketUniverseVersion,
    MobileOperatorDevice,
    PortfolioCandidate,
    PortfolioMandate,
    PortfolioProgram,
    PublicMutationReceipt,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)
from db.session import create_database_engine, create_session_factory
from downstream_auth import install_service_token, issue_service_token
from settings import Settings

UNIVERSE_ID = UUID("10000000-0000-0000-0000-000000000001")
MANDATE_ID = UUID("20000000-0000-0000-0000-000000000001")
MANDATE_VERSION_ID = UUID("20000000-0000-0000-0000-000000000002")
PORTFOLIO_PROGRAM_ID = UUID("30000000-0000-0000-0000-000000000001")
CANDIDATE_ID = UUID("40000000-0000-0000-0000-000000000001")
ALPHA_ID = UUID("50000000-0000-0000-0000-000000000001")
PAPER_DOWNSTREAM_ID = UUID("60000000-0000-0000-0000-000000000001")
LIVE_DOWNSTREAM_ID = UUID("60000000-0000-0000-0000-000000000002")
APPROVAL_ID = UUID("70000000-0000-0000-0000-000000000001")
REJECT_APPROVAL_ID = UUID("70000000-0000-0000-0000-000000000002")
RESEARCH_CHARTER_ID = UUID("80000000-0000-0000-0000-000000000001")
RESEARCH_PROGRAM_ID = UUID("80000000-0000-0000-0000-000000000002")
RESEARCH_BRANCH_ID = UUID("80000000-0000-0000-0000-000000000003")
ACTIVE_MISSION_ID = UUID("80000000-0000-0000-0000-000000000004")
FINISHED_MISSION_ID = UUID("80000000-0000-0000-0000-000000000005")
HANDOFF_PACKAGE_ID = UUID("90000000-0000-0000-0000-000000000001")
HANDOFF_ID = UUID("90000000-0000-0000-0000-000000000002")
MOBILE_DEVICE_ID = UUID("a0000000-0000-0000-0000-000000000001")


def main() -> None:
    settings = Settings.from_env()
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    with factory() as session, session.begin():
        for model in (
            MobileOperatorDevice,
            ForwardEvidenceEpisode,
            HandoffOffer,
            CandidatePackage,
            ApprovalSnapshot,
            PortfolioCandidate,
            PortfolioProgram,
            PortfolioMandate,
            AlphaQualification,
            DatasetRevision,
            GovernedDataSource,
            ResearchMission,
            IdeaContribution,
            ResearchBranch,
            ResearchProgram,
            ResearchCharter,
            DownstreamSystem,
            MarketUniverseVersion,
            PublicMutationReceipt,
        ):
            session.execute(delete(model))

        universe = MarketUniverseVersion(
            id=UNIVERSE_ID,
            universe_key="US_EQUITIES",
            version_no=1,
            name="US Equities",
            state="ACTIVE",
            spec_json={"calendar": "XNYS", "currency": "USD"},
            created_at=now,
        )
        charter = ResearchCharter(
            id=RESEARCH_CHARTER_ID,
            original_idea_text="Test post-earnings drift in liquid US equities.",
            research_question="Does post-earnings drift persist after realistic costs?",
            market_scope=["US_EQUITIES"],
            universe_version_ids=[str(UNIVERSE_ID)],
            prediction_horizon="1D",
            allowed_data_domains=["market"],
            explicit_exclusions=[],
            material_assumptions=[],
            system_assumptions=[],
            created_at=now,
        )
        research_program = ResearchProgram(
            id=RESEARCH_PROGRAM_ID,
            charter_id=RESEARCH_CHARTER_ID,
            title="Fixture research program",
            state="ACTIVE",
        )
        research_branch = ResearchBranch(
            id=RESEARCH_BRANCH_ID,
            program_id=RESEARCH_PROGRAM_ID,
            parent_branch_id=None,
            derivation_type="ROOT",
            hypothesis="Post-earnings drift survives realistic transaction costs.",
            changed_assumptions=[],
            preserved_constraints=[],
            state="ACTIVE",
            created_at=now,
        )
        active_mission = ResearchMission(
            id=ACTIVE_MISSION_ID,
            program_id=RESEARCH_PROGRAM_ID,
            branch_id=RESEARCH_BRANCH_ID,
            type="DISCOVERY",
            role="PRIMARY",
            state="READY",
            objective="Run the fixture discovery mission.",
            dependencies=[],
        )
        finished_mission = ResearchMission(
            id=FINISHED_MISSION_ID,
            program_id=RESEARCH_PROGRAM_ID,
            branch_id=RESEARCH_BRANCH_ID,
            type="VALIDATION",
            role="SECONDARY",
            state="SUCCEEDED",
            objective="Historical mission excluded from active count.",
            dependencies=[],
            finished_at=now - timedelta(hours=1),
        )
        mandate = PortfolioMandate(
            id=MANDATE_ID,
            key="CORE_GROWTH",
            name="Core Growth",
            enabled=True,
            latest_version_id=MANDATE_VERSION_ID,
            spec_json={"objective": "Risk-adjusted long-term growth"},
            state="ACTIVE",
        )
        paper = DownstreamSystem(
            id=PAPER_DOWNSTREAM_ID,
            name="Paper Lab",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            compatibility=["US_EQUITIES", "NAUTILUS_TRADER_1.231.0"],
            preflight_state="READY",
            public_config={},
        )
        live = DownstreamSystem(
            id=LIVE_DOWNSTREAM_ID,
            name="Live Primary",
            environment_type="LIVE",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            compatibility=["US_EQUITIES", "NAUTILUS_TRADER_1.231.0"],
            preflight_state="READY",
            public_config={},
        )
        install_service_token(paper, issue_service_token(settings, PAPER_DOWNSTREAM_ID))
        install_service_token(live, issue_service_token(settings, LIVE_DOWNSTREAM_ID))
        portfolio_program = PortfolioProgram(
            id=PORTFOLIO_PROGRAM_ID,
            mandate_version_id=MANDATE_VERSION_ID,
            mandate_name="Core Growth",
            state="CANDIDATE_READY",
            current_candidate_id=CANDIDATE_ID,
        )
        session.add(universe)
        session.flush()
        session.add(charter)
        session.flush()
        session.add(research_program)
        session.flush()
        session.add(research_branch)
        session.flush()
        session.add_all([
            active_mission,
            finished_mission,
            mandate,
            paper,
            live,
            portfolio_program,
        ])
        session.flush()

        alpha = AlphaQualification(
            id=ALPHA_ID,
            universe_version_id=UNIVERSE_ID,
            universe="US Equities",
            horizon="1D",
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            name="PEAD residual drift",
            degradation_state="HEALTHY",
            metrics={"search_adjusted_quality": 0.72},
            lineage=[],
            scope_json={},
            created_at=now,
        )
        candidate = PortfolioCandidate(
            id=CANDIDATE_ID,
            portfolio_program_id=PORTFOLIO_PROGRAM_ID,
            mandate_version_id=MANDATE_VERSION_ID,
            mandate_name="Core Growth",
            state="READY",
            universe_set_json=["US Equities"],
            members=[
                {
                    "alpha_qualification_id": str(ALPHA_ID),
                    "instrument_id": "AAPL",
                    "alpha_name": "PEAD residual drift",
                    "role": "PRIMARY_ALPHA",
                    "target_weight": 1.0,
                    "universe": "US Equities",
                }
            ],
            metrics={
                "search_adjusted_quality": 0.78,
                "nautilus": {
                    "strategy_artifact": {
                        "strategy_path": "strategy.example:ExampleStrategy",
                        "config_path": "strategy.example:ExampleConfig",
                        "config": {"instrument_id": "AAPL.SIM", "trade_size": "1"},
                        "source_files": {
                            "strategy/__init__.py": "",
                            "strategy/example.py": (
                                "class ExampleConfig:\n    pass\n\n"
                                "class ExampleStrategy:\n    pass\n"
                            ),
                        },
                        "requirements": ["nautilus-trader==1.231.0"],
                    },
                    "portfolio_evidence": {
                        "external_run_id": "fixture-portfolio-run",
                        "state": "SUCCEEDED",
                        "mode": "PORTFOLIO",
                        "runtime_name": "NautilusTrader",
                        "nautilus_version": "1.231.0",
                        "contract_version": "1",
                        "catalog_uri": "catalog://frontend-fixture",
                        "strategy_artifact": {
                            "strategy_path": "strategy.example:ExampleStrategy",
                            "config_path": "strategy.example:ExampleConfig",
                            "config": {"instrument_id": "AAPL.SIM", "trade_size": "1"},
                            "source_files": {
                                "strategy/__init__.py": "",
                                "strategy/example.py": (
                                    "class ExampleConfig:\n    pass\n\n"
                                    "class ExampleStrategy:\n    pass\n"
                                ),
                            },
                            "requirements": ["nautilus-trader==1.231.0"],
                        },
                        "orders": [{"instrument_id": "AAPL.SIM", "side": "BUY", "account_id": "SIM-001"}],
                        "fills": [{"instrument_id": "AAPL.SIM"}],
                        "positions": [{"instrument_id": "AAPL.SIM", "account_id": "SIM-001"}],
                        "account": [{"currency": "USD", "balance": "100000"}],
                        "statistics": {"total_orders": 1, "sharpe_ratio": 0.8},
                    },
                    "discovery_run_id": "fixture-discovery-run",
                    "sealed_run_id": "fixture-sealed-run",
                    "portfolio_run_id": "fixture-portfolio-run",
                },
            },
            created_at=now,
        )
        session.add_all([alpha, candidate])
        session.flush()

        approval = ApprovalSnapshot(
            id=APPROVAL_ID,
            candidate_id=CANDIDATE_ID,
            purpose="PAPER",
            state="PENDING",
            valid_until=now + timedelta(days=7),
            recommendation_rationale=(
                "Independent evidence is stable and the candidate materially improves the current frontier."
            ),
            human_report={"summary": "Paper validation is the next governed step."},
            evidence_summary={"search_adjusted_quality": 0.78},
            risk_summary={"tail_dependence": 0.23},
            cost_summary={"turnover_cost_bps": 7},
            capacity_summary={"capacity_ratio": 0.72},
            changes_summary={"changed": "Risk-adjusted edge improved"},
            capital_context={
                "base_currency": "USD",
                "deployable_capital": 100000,
                "observed_at": now.isoformat(),
            },
        )
        session.add(approval)
        reject_approval = ApprovalSnapshot(
            id=REJECT_APPROVAL_ID,
            candidate_id=CANDIDATE_ID,
            purpose="PAPER",
            state="PENDING",
            valid_until=now + timedelta(days=7),
            recommendation_rationale="Fixture approval reserved for the reject action test.",
            human_report={"summary": "Reject action fixture."},
            evidence_summary={"search_adjusted_quality": 0.7},
            risk_summary={},
            cost_summary={},
            capacity_summary={},
            changes_summary={},
            capital_context={"base_currency": "USD", "deployable_capital": 100000},
        )
        package = CandidatePackage(
            id=HANDOFF_PACKAGE_ID,
            approval_id=REJECT_APPROVAL_ID,
            candidate_id=CANDIDATE_ID,
            contract_version="1",
            state="AVAILABLE",
            manifest_json={},
            relative_path="fixture/candidate-bundle.zip",
            payload={},
            created_at=now,
        )
        handoff = HandoffOffer(
            id=HANDOFF_ID,
            approval_id=REJECT_APPROVAL_ID,
            candidate_package_id=HANDOFF_PACKAGE_ID,
            candidate_id=CANDIDATE_ID,
            purpose="PAPER",
            downstream_system_id=PAPER_DOWNSTREAM_ID,
            state="AVAILABLE",
            claim_deadline=now + timedelta(days=7),
            feedback_state="PENDING",
            feedback_contract_snapshot={},
        )
        device = MobileOperatorDevice(
            id=MOBILE_DEVICE_ID,
            installation_id="a0000000-0000-0000-0000-000000000002",
            display_name="Fixture iPhone",
            device_family="IPHONE",
            credential_generation=1,
            last_seen_at=now,
            refresh_expires_at=now + timedelta(days=30),
            client_version="1.0.0",
            app_build="100",
            os_version="18.0",
        )
        session.add_all([reject_approval, package, handoff, device])

    engine.dispose()


if __name__ == "__main__":
    main()
