from __future__ import annotations

from datetime import UTC, datetime
from uuid import uuid4

from sqlalchemy import Engine, func, select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    DatasetRevision,
    DownstreamSystem,
    Job,
    NautilusCatalogBinding,
    PortfolioCandidate,
    QuantExperiment,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
    SearchLedgerEntry,
    SealedEvaluation,
)
from db.session import create_session_factory
from quant_runtime import (
    BacktestEvidence,
    CatalogReference,
    ExperimentContract,
    StrategyArtifact,
)
from runners.quant_experiments import process_experiment, submit_experiment
from settings import Settings


class FakeRemoteRuntime:
    def run_backtest(self, contract: ExperimentContract) -> BacktestEvidence:
        return BacktestEvidence(
            run_id=contract.run_id,
            run_config_id=str(contract.run_id),
            runtime_version="1.231.0",
            catalog_uri=contract.catalog.catalog_uri,
            partition="DISCOVERY",
            total_events=100,
            total_orders=4,
            total_positions=2,
            statistics={"returns": {"Sharpe Ratio (252 days)": 1.2}},
            reports={
                "orders": [{"client_order_id": "O-1"}],
                "fills": [{"trade_id": "T-1"}],
                "positions": [{"instrument_id": "EUR/USD.SIM"}],
            },
        )

    def run_sealed_backtest(self, contract: ExperimentContract) -> BacktestEvidence:
        return BacktestEvidence(
            run_id=contract.run_id,
            run_config_id=str(contract.run_id),
            runtime_version="1.231.0",
            catalog_uri=contract.catalog.catalog_uri,
            partition="SEALED",
            total_events=80,
            total_orders=2,
            total_positions=1,
            statistics={"general": {"Profit Factor": 1.1}},
            reports={},
            disclosure={
                "decision": "PASS",
                "classification": "SEALED_RUNTIME_EVIDENCE_SUFFICIENT",
            },
        )


def _seed(engine: Engine) -> tuple[object, object]:
    factory = create_session_factory(engine)
    with factory.begin() as session:
        charter = ResearchCharter(
            original_idea_text="Test an executable EMA crossover in the canonical runtime.",
            research_question="Does an EMA crossover survive realistic simulation?",
            market_scope=["FX"],
            universe_version_ids=[],
            prediction_horizon="intraday",
            allowed_data_domains=["market_data"],
            explicit_exclusions=[],
            material_assumptions=[],
            system_assumptions=[],
            created_at=datetime.now(UTC),
        )
        session.add(charter)
        session.flush()
        program = ResearchProgram(charter_id=charter.id, title="EMA research", state="ACTIVE")
        session.add(program)
        session.flush()
        branch = ResearchBranch(
            program_id=program.id,
            derivation_type="ROOT",
            hypothesis="Fast EMA crossing slow EMA has predictive value.",
            changed_assumptions=[],
            preserved_constraints=[],
            state="ACTIVE",
            created_at=datetime.now(UTC),
        )
        session.add(branch)
        session.flush()
        mission = ResearchMission(
            program_id=program.id,
            branch_id=branch.id,
            type="ALPHA_DISCOVERY",
            role="ALPHA_RESEARCHER",
            state="READY",
            objective="Run a real NautilusTrader backtest.",
            dependencies=[],
        )
        dataset = DatasetRevision(
            universe_name="FX",
            revision_no=1,
            schema_version="quote-v1",
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=datetime.now(UTC),
        )
        downstream = DownstreamSystem(
            name="Nautilus Paper Lab",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="2",
            feedback_contract_version="1",
            compatibility=["NAUTILUS_NATIVE_CANDIDATE"],
            preflight_state="READY",
            public_config={},
        )
        session.add_all([mission, dataset, downstream])
        session.flush()
        binding = NautilusCatalogBinding(
            dataset_revision_id=dataset.id,
            provider="integration-fixture",
            source_license="TEST",
            catalog_uri="catalog://discovery-eurusd",
            nautilus_data_type="QuoteTick",
            instrument_scope=["EUR/USD.SIM"],
            event_time_range={},
            available_time_range={},
            schema_revision="quote-v1",
            quality_result={"state": "VALID"},
            point_in_time_result={"state": "VALID"},
            runtime_name="NAUTILUS_TRADER",
            runtime_version="1.231.0",
            ingested_at=datetime.now(UTC),
        )
        session.add(binding)
        session.flush()
        return mission.id, dataset.id


def test_real_evidence_chain_creates_alpha_candidate_and_human_approval(
    engine: Engine,
    settings: Settings,
) -> None:
    del settings
    mission_id, dataset_id = _seed(engine)
    factory = create_session_factory(engine)
    contract = ExperimentContract(
        catalog=CatalogReference(
            dataset_revision_id=dataset_id,
            catalog_uri="catalog://ignored-agent-value",
            nautilus_data_type="QuoteTick",
            instrument_ids=["EUR/USD.SIM"],
            partition="DISCOVERY",
        ),
        strategy=StrategyArtifact(
            strategy_path="nautilus_trader.examples.strategies.ema_cross:EMACross",
            config_path="nautilus_trader.examples.strategies.ema_cross:EMACrossConfig",
            config={"fast_ema_period": 3, "slow_ema_period": 8, "trade_size": "10000"},
        ),
    )
    with factory.begin() as session:
        discovery = submit_experiment(session, mission_id=mission_id, contract=contract)
        discovery_id = discovery.id
    with factory.begin() as session:
        process_experiment(
            session,
            experiment_id=discovery_id,
            runtime=FakeRemoteRuntime(),
        )
    with factory() as session:
        sealed = session.scalar(
            select(QuantExperiment).where(
                QuantExperiment.parent_experiment_id == discovery_id,
                QuantExperiment.zone == "SEALED",
            )
        )
        assert sealed is not None
        sealed_id = sealed.id
        assert session.scalar(
            select(func.count()).select_from(Job).where(Job.kind == "NAUTILUS_SEALED_BACKTEST")
        ) == 1
    with factory.begin() as session:
        process_experiment(
            session,
            experiment_id=sealed_id,
            runtime=FakeRemoteRuntime(),
        )
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(SealedEvaluation)) == 1
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 1
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 1
        assert session.scalar(select(func.count()).select_from(ApprovalSnapshot)) == 1
        approval = session.scalar(select(ApprovalSnapshot))
        assert approval is not None
        assert approval.purpose == "PAPER"
        assert approval.state == "PENDING"
        candidate = session.scalar(select(PortfolioCandidate))
        assert candidate is not None
        assert candidate.metrics["nautilus_runtime"]["version"] == "1.231.0"
        outcomes = session.scalars(select(SearchLedgerEntry.outcome)).all()
        assert outcomes.count("SUCCEEDED") == 2
