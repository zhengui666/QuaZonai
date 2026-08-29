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


def main() -> None:
    settings = Settings.from_env()
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    with factory() as session, session.begin():
        for model in (
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
            compatibility=["US_EQUITIES"],
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
            compatibility=["US_EQUITIES"],
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
        session.add_all([universe, mandate, paper, live, portfolio_program])
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

    engine.dispose()


if __name__ == "__main__":
    main()
