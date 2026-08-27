from pathlib import Path


path = Path("backend/src/runners/quant_experiments.py")
text = path.read_text(encoding="utf-8")
text = text.replace("from sqlalchemy.orm import Session, sessionmaker\n", "from sqlalchemy.orm import Session\n")
text = text.replace(
    "from db.session import create_database_engine, create_session_factory\n",
    "from db.session import SessionFactory, create_database_engine, create_session_factory\n",
)
text = text.replace("\nSessionFactory = sessionmaker[Session]\n", "\n")
path.write_text(text, encoding="utf-8")

pipeline = r'''from __future__ import annotations

from datetime import UTC, datetime
from uuid import UUID, uuid4

import pytest
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
from db.session import SessionFactory, create_session_factory
from errors import QfError
from quant_runtime import (
    BacktestEvidence,
    CatalogReference,
    ExperimentContract,
    StrategyArtifact,
)
from runners.quant_experiments import execute_experiment, submit_experiment
from settings import Settings


class FakeRemoteRuntime:
    def __init__(self, factory: SessionFactory) -> None:
        self.factory = factory
        self.observed_states: list[str] = []

    def _observe_committed_running_state(self, run_id: UUID) -> None:
        del run_id
        with self.factory() as session:
            state = session.scalar(
                select(QuantExperiment.state)
                .where(QuantExperiment.state == "RUNNING")
                .limit(1)
            )
        assert state == "RUNNING"
        self.observed_states.append(state)

    def run_backtest(self, contract: ExperimentContract) -> BacktestEvidence:
        self._observe_committed_running_state(contract.run_id)
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
                "account": [{"total": "1000000 USD"}],
            },
        )

    def run_sealed_backtest(self, contract: ExperimentContract) -> BacktestEvidence:
        self._observe_committed_running_state(contract.run_id)
        return BacktestEvidence(
            run_id=contract.run_id,
            run_config_id=str(contract.run_id),
            runtime_version="1.231.0",
            catalog_uri=contract.catalog.catalog_uri,
            partition="SEALED",
            total_events=0,
            total_orders=0,
            total_positions=0,
            statistics={},
            reports={},
            disclosure={
                "decision": "PASS",
                "classification": "SEALED_RUNTIME_EVIDENCE_SUFFICIENT",
                "total_orders_bucket": "NON_ZERO",
                "total_positions_bucket": "NON_ZERO",
            },
        )


class FailingRemoteRuntime:
    def run_backtest(self, contract: ExperimentContract) -> BacktestEvidence:
        del contract
        raise QfError(
            "NAUTILUS_RUNTIME_UNAVAILABLE",
            "The independent runtime is unavailable.",
            503,
        )


def _seed(
    engine: Engine,
    *,
    with_sealed: bool = True,
) -> tuple[UUID, UUID]:
    factory = create_session_factory(engine)
    with factory.begin() as session:
        charter = ResearchCharter(
            original_idea_text=(
                "Test an executable EMA crossover in the canonical runtime."
            ),
            research_question=(
                "Does an EMA crossover survive realistic transaction simulation?"
            ),
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
        program = ResearchProgram(
            charter_id=charter.id,
            title="EMA research",
            state="ACTIVE",
        )
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
        discovery_dataset = DatasetRevision(
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
        session.add_all([mission, discovery_dataset, downstream])
        session.flush()
        discovery_binding = NautilusCatalogBinding(
            dataset_revision_id=discovery_dataset.id,
            provider="integration-discovery-fixture",
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
        session.add(discovery_binding)
        if with_sealed:
            sealed_dataset = DatasetRevision(
                universe_name="FX",
                revision_no=2,
                schema_version="quote-v1",
                quality_state="VALID",
                point_in_time_state="VALID",
                partition="SEALED",
                created_at=datetime.now(UTC),
            )
            session.add(sealed_dataset)
            session.flush()
            session.add(
                NautilusCatalogBinding(
                    dataset_revision_id=sealed_dataset.id,
                    provider="integration-sealed-fixture",
                    source_license="TEST",
                    catalog_uri="catalog://sealed-eurusd",
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
            )
        session.flush()
        return mission.id, discovery_dataset.id


def _contract(dataset_id: UUID) -> ExperimentContract:
    return ExperimentContract(
        catalog=CatalogReference(
            dataset_revision_id=dataset_id,
            catalog_uri="catalog://agent-value-is-replaced",
            nautilus_data_type="QuoteTick",
            instrument_ids=["EUR/USD.SIM"],
            partition="DISCOVERY",
        ),
        strategy=StrategyArtifact(
            strategy_path=(
                "nautilus_trader.examples.strategies.ema_cross:EMACross"
            ),
            config_path=(
                "nautilus_trader.examples.strategies.ema_cross:EMACrossConfig"
            ),
            config={
                "fast_ema_period": 3,
                "slow_ema_period": 8,
                "trade_size": "10000",
            },
        ),
    )


def test_evidence_chain_creates_alpha_candidate_and_human_approval(
    engine: Engine,
    settings: Settings,
) -> None:
    del settings
    mission_id, dataset_id = _seed(engine)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        discovery_id = submit_experiment(
            session,
            mission_id=mission_id,
            contract=_contract(dataset_id),
        ).id

    runtime = FakeRemoteRuntime(factory)
    execute_experiment(
        factory,
        experiment_id=discovery_id,
        runtime=runtime,
    )
    with factory() as session:
        sealed = session.scalar(
            select(QuantExperiment).where(
                QuantExperiment.parent_experiment_id == discovery_id,
                QuantExperiment.zone == "SEALED",
            )
        )
        assert sealed is not None
        assert sealed.dataset_revision_id != dataset_id
        assert sealed.contract_json["catalog"]["catalog_uri"] == (
            "catalog://sealed-eurusd"
        )
        sealed_id = sealed.id
        assert session.scalar(
            select(func.count())
            .select_from(Job)
            .where(Job.kind == "NAUTILUS_SEALED_BACKTEST")
        ) == 1

    execute_experiment(
        factory,
        experiment_id=sealed_id,
        runtime=runtime,
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
        assert candidate.metrics["sealed_dataset_revision_id"] != str(dataset_id)
        outcomes = session.scalars(select(SearchLedgerEntry.outcome)).all()
        assert outcomes.count("SUCCEEDED") == 2
        assert runtime.observed_states == ["RUNNING", "RUNNING"]


def test_runtime_failure_is_durable_search_ledger_evidence(
    engine: Engine,
    settings: Settings,
) -> None:
    del settings
    mission_id, dataset_id = _seed(engine, with_sealed=False)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        experiment_id = submit_experiment(
            session,
            mission_id=mission_id,
            contract=_contract(dataset_id),
        ).id

    with pytest.raises(QfError, match="independent runtime is unavailable"):
        execute_experiment(
            factory,
            experiment_id=experiment_id,
            runtime=FailingRemoteRuntime(),
        )
    with factory() as session:
        experiment = session.get(QuantExperiment, experiment_id)
        assert experiment is not None
        assert experiment.state == "FAILED"
        assert experiment.error_code == "NAUTILUS_RUNTIME_UNAVAILABLE"
        outcomes = session.scalars(
            select(SearchLedgerEntry.outcome).where(
                SearchLedgerEntry.experiment_id == experiment_id
            )
        ).all()
        assert outcomes == ["QUEUED", "FAILED"]


def test_missing_sealed_catalog_blocks_promotion_without_faking_independence(
    engine: Engine,
    settings: Settings,
) -> None:
    del settings
    mission_id, dataset_id = _seed(engine, with_sealed=False)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        discovery_id = submit_experiment(
            session,
            mission_id=mission_id,
            contract=_contract(dataset_id),
        ).id

    execute_experiment(
        factory,
        experiment_id=discovery_id,
        runtime=FakeRemoteRuntime(factory),
    )
    with factory() as session:
        assert session.scalar(
            select(func.count()).select_from(QuantExperiment).where(
                QuantExperiment.parent_experiment_id == discovery_id
            )
        ) == 0
        assert session.scalar(select(func.count()).select_from(SealedEvaluation)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0
        outcomes = session.scalars(
            select(SearchLedgerEntry.outcome).where(
                SearchLedgerEntry.experiment_id == discovery_id
            )
        ).all()
        assert outcomes == ["QUEUED", "SUCCEEDED", "SEALED_DATA_BLOCKED"]
'''

Path("backend/tests/integration/test_quant_experiment_pipeline.py").write_text(
    pipeline,
    encoding="utf-8",
)
