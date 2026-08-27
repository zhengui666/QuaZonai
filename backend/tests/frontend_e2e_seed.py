"""Seed deterministic browser-test facts into the real test database.

This module lives under tests and is never imported by production runtime code.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

from sqlalchemy import delete

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidateBundle,
    DatasetRevision,
    DegradationFollowup,
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
DATA_SOURCE_ID = UUID("10000000-0000-0000-0000-000000000002")
DISCOVERY_DATASET_ID = UUID("10000000-0000-0000-0000-000000000003")
MANDATE_ID = UUID("20000000-0000-0000-0000-000000000001")
MANDATE_VERSION_ID = UUID("20000000-0000-0000-0000-000000000002")
PORTFOLIO_PROGRAM_ID = UUID("30000000-0000-0000-0000-000000000001")
CANDIDATE_ID = UUID("40000000-0000-0000-0000-000000000001")
ALPHA_ID = UUID("50000000-0000-0000-0000-000000000001")
PAPER_DOWNSTREAM_ID = UUID("60000000-0000-0000-0000-000000000001")
LIVE_DOWNSTREAM_ID = UUID("60000000-0000-0000-0000-000000000002")
APPROVAL_ID = UUID("70000000-0000-0000-0000-000000000001")


def _nautilus_candidate_metrics() -> dict:
    experiment_id = uuid4()
    strategy_source = (
        "from nautilus_trader.examples.strategies.ema_cross import "
        "EMACross as CandidateStrategy, EMACrossConfig as CandidateConfig\n"
    )
    return {
        "search_adjusted_quality": 0.78,
        "nautilus": {
            "strategy_artifact": {
                "artifact_id": "candidate-ema-cross-v1",
                "kind": "SOURCE_BUNDLE",
                "strategy_path": "candidate_strategy:CandidateStrategy",
                "config_path": "candidate_strategy:CandidateConfig",
                "config": {
                    "instrument_id": "AAPL.XNAS",
                    "bar_type": "AAPL.XNAS-1-MINUTE-BID-INTERNAL",
                    "trade_size": "100000",
                    "fast_ema_period": 3,
                    "slow_ema_period": 8,
                },
                "source_files": {"candidate_strategy.py": strategy_source},
                "requirements": ["nautilus_trader==1.231.0"],
            },
            "evidence": {
                "experiment_id": str(experiment_id),
                "orders": [
                    {
                        "order_id": "O-E2E-1",
                        "instrument_id": "AAPL.XNAS",
                        "side": "BUY",
                        "order_type": "MARKET",
                        "status": "FILLED",
                        "quantity": "100000",
                        "filled_quantity": "100000",
                    }
                ],
                "fills": [
                    {
                        "trade_id": "T-E2E-1",
                        "order_id": "O-E2E-1",
                        "instrument_id": "AAPL.XNAS",
                        "side": "BUY",
                        "quantity": "100000",
                        "price": "1.10000",
                    }
                ],
                "positions": [
                    {
                        "position_id": "P-E2E-1",
                        "instrument_id": "AAPL.XNAS",
                        "side": "LONG",
                        "quantity": "100000",
                    }
                ],
                "pnl": {"realized": "250 USD"},
                "statistics": {"total_orders": 1, "total_fills": 1, "total_positions": 1},
            },
            "dataset_revision_ids": [str(DISCOVERY_DATASET_ID)],
            "alpha_qualification_ids": [str(ALPHA_ID)],
            "instrument_scope": ["AAPL.XNAS"],
            "data_requirements": {"nautilus_data_type": "QuoteTick"},
            "backtest_run_config": {
                "catalog_uri": "nautilus-catalog://frontend-e2e",
                "mode": "PORTFOLIO",
            },
            "venue_config": {"name": "SIM", "oms_type": "HEDGING", "account_type": "MARGIN"},
            "risk_config": {},
            "discovery_summary": {"source": "search-ledger"},
            "sealed_summary": {"raw_evidence_withheld": True},
            "robustness_summary": {"status": "PASS"},
        },
    }


def main() -> None:
    settings = Settings.from_env()
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    with factory() as session, session.begin():
        for model in (
            DegradationFollowup,
            ForwardEvidenceEpisode,
            HandoffOffer,
            CandidateBundle,
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
        data_source = GovernedDataSource(
            id=DATA_SOURCE_ID,
            name="Seeded executable PIT quotes",
            provider="CI generated fixture",
            state="ACTIVE",
            universe_scope=["US Equities"],
            fields=[
                "timestamp",
                "available_at",
                "bid_price",
                "ask_price",
                "volume",
            ],
            update_cadence="STATIC_FIXTURE",
            preflight_state="READY",
            public_config={"data_domains": ["quotes", "market_data"]},
        )
        discovery = DatasetRevision(
            id=DISCOVERY_DATASET_ID,
            data_source_id=DATA_SOURCE_ID,
            universe_version_id=UNIVERSE_ID,
            universe_name="US Equities",
            revision_no=1,
            schema_version="nautilus.quote_tick.v2",
            event_start=now - timedelta(days=30),
            event_end=now - timedelta(days=1),
            available_start=now - timedelta(days=30) + timedelta(seconds=2),
            available_end=now - timedelta(days=1) + timedelta(seconds=2),
            row_count=360,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=now,
            provider_name="CI generated fixture",
            source_license="CC0-1.0",
            catalog_uri="nautilus-catalog://frontend-e2e",
            nautilus_data_type="QuoteTick",
            instrument_scope=["AAPL.XNAS"],
            schema_revision="nautilus.quote_tick.v2",
            quality_result={"state": "VALID", "sorted": True},
            point_in_time_result={
                "state": "VALID",
                "replay_order": "TS_INIT",
                "event_time_preserved": True,
                "availability_time_preserved": True,
            },
            ingested_at=now,
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
            package_contract_version="2",
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
            package_contract_version="2",
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
        session.add_all(
            [universe, data_source, discovery, mandate, paper, live, portfolio_program]
        )
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
                    "instrument_id": "AAPL.XNAS",
                    "alpha_name": "PEAD residual drift",
                    "role": "PRIMARY_ALPHA",
                    "target_weight": 1.0,
                    "universe": "US Equities",
                }
            ],
            metrics=_nautilus_candidate_metrics(),
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
