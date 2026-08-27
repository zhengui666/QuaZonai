from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from sqlalchemy import Engine, select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    DatasetRevision,
    MarketUniverseVersion,
    PortfolioCandidate,
    PortfolioMandate,
    PortfolioProgram,
    ResearchBranch,
    ResearchCharter,
    ResearchProgram,
    SearchLedgerEntry,
)
from db.session import create_session_factory
from quant_runtime.contracts import (
    PINNED_NAUTILUS_VERSION,
    BacktestExperimentRequest,
    ExperimentMode,
    StrategyArtifact,
)
from quant_runtime.ledger import ExperimentCoordinator
from quant_runtime.promotion import qualify_alpha, simulate_portfolio_candidate

_STRATEGY = StrategyArtifact(
    artifact_id="promotion-source-bundle-v1",
    kind="SOURCE_BUNDLE",
    strategy_path="alpha_strategy:CandidateStrategy",
    config_path="alpha_strategy:CandidateConfig",
    config={"instrument_id": "EUR/USD.SIM", "trade_size": "100000"},
    source_files={
        "alpha_strategy.py": (
            "from nautilus_trader.examples.strategies.ema_cross import "
            "EMACross as CandidateStrategy, EMACrossConfig as CandidateConfig\n"
        )
    },
    requirements=[f"nautilus_trader=={PINNED_NAUTILUS_VERSION}"],
)


def _evidence(experiment_id: object) -> dict:
    return {
        "protocol_version": "2",
        "runtime_version": PINNED_NAUTILUS_VERSION,
        "experiment_id": str(experiment_id),
        "remote_run_id": f"remote-{experiment_id}",
        "mode": "DISCOVERY",
        "orders": [{"order_id": "O-1", "status": "FILLED"}],
        "fills": [{"trade_id": "T-1", "order_id": "O-1"}],
        "positions": [{"position_id": "P-1", "side": "LONG"}],
        "balances": [{"currency": "USD", "total": "1000000"}],
        "pnl": {"USD": {"PnL (total)": 12.5}},
        "statistics": {
            "total_events": 2,
            "total_orders": 1,
            "total_positions": 1,
            "iterations": 360,
        },
        "diagnostics": {"loaded_instrument_count": 1},
    }


def _seed(engine: Engine) -> tuple[object, object, object]:
    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    universe_id = uuid4()
    mandate_version_id = uuid4()
    with factory() as session, session.begin():
        universe = MarketUniverseVersion(
            id=universe_id,
            universe_key="FX",
            version_no=1,
            name="FX",
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        charter = ResearchCharter(
            original_idea_text="Test a governed FX alpha.",
            research_question="Does the source-bundle strategy survive sealed evaluation?",
            market_scope=["FX"],
            universe_version_ids=[str(universe_id)],
            prediction_horizon="1D",
            allowed_data_domains=["quotes"],
            explicit_exclusions=[],
            material_assumptions=[],
            system_assumptions=[],
            created_at=now,
        )
        session.add_all([universe, charter])
        session.flush()
        program = ResearchProgram(charter_id=charter.id, title="Governed FX alpha", state="ACTIVE")
        session.add(program)
        session.flush()
        branch = ResearchBranch(
            program_id=program.id,
            derivation_type="ROOT",
            hypothesis="A deterministic executable hypothesis.",
            changed_assumptions=[],
            preserved_constraints=[],
            state="ACTIVE",
            created_at=now,
        )
        discovery = DatasetRevision(
            universe_version_id=universe_id,
            universe_name="FX",
            revision_no=1,
            event_start=now - timedelta(days=30),
            event_end=now - timedelta(days=20),
            available_start=now - timedelta(days=30) + timedelta(seconds=2),
            available_end=now - timedelta(days=20) + timedelta(seconds=2),
            row_count=360,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=now,
            provider_name="CI fixture provider",
            source_license="CC0-1.0",
            catalog_uri="nautilus-catalog://promotion-discovery",
            nautilus_data_type="QuoteTick",
            instrument_scope=["EUR/USD.SIM"],
            schema_revision="quote-v2",
            quality_result={"state": "VALID"},
            point_in_time_result={"state": "VALID", "replay_order": "TS_INIT"},
            ingested_at=now,
        )
        sealed = DatasetRevision(
            universe_version_id=universe_id,
            universe_name="FX",
            revision_no=2,
            event_start=now - timedelta(days=20),
            event_end=now - timedelta(days=10),
            available_start=now - timedelta(days=20) + timedelta(seconds=2),
            available_end=now - timedelta(days=10) + timedelta(seconds=2),
            row_count=360,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="SEALED",
            created_at=now,
            provider_name="CI fixture provider",
            source_license="CC0-1.0",
            catalog_uri="nautilus-catalog://promotion-sealed",
            nautilus_data_type="QuoteTick",
            instrument_scope=["EUR/USD.SIM"],
            schema_revision="quote-v2",
            quality_result={"state": "VALID"},
            point_in_time_result={"state": "VALID", "replay_order": "TS_INIT"},
            ingested_at=now,
        )
        mandate = PortfolioMandate(
            key=f"TEST-{mandate_version_id}",
            name="Research Portfolio",
            enabled=True,
            latest_version_id=mandate_version_id,
            spec_json={
                "constraints": {
                    "max_single_alpha_weight": 1.0,
                    "allowed_universe_version_ids": [str(universe_id)],
                }
            },
            state="ACTIVE",
        )
        portfolio = PortfolioProgram(
            mandate_version_id=mandate_version_id,
            mandate_name="Research Portfolio",
            state="ACTIVE",
        )
        session.add_all([branch, discovery, sealed, mandate, portfolio])
        session.flush()
        experiment_id = uuid4()
        request = BacktestExperimentRequest(
            experiment_id=experiment_id,
            mode=ExperimentMode.DISCOVERY,
            dataset_revision_id=discovery.id,
            catalog_key="promotion-discovery",
            instrument_ids=["EUR/USD.SIM"],
            strategy=_STRATEGY,
        )
        source = SearchLedgerEntry(
            id=experiment_id,
            program_id=program.id,
            branch_id=branch.id,
            mission_id=None,
            dataset_revision_id=discovery.id,
            parent_entry_id=None,
            mode=ExperimentMode.DISCOVERY.value,
            state="SUCCEEDED",
            runtime_name="NAUTILUS_TRADER",
            runtime_version=PINNED_NAUTILUS_VERSION,
            remote_run_id="remote-discovery",
            request_json=request.model_dump(mode="json"),
            evidence_json=_evidence(experiment_id),
            disclosure_json={},
            started_at=now,
            finished_at=now,
        )
        session.add(source)
        return source.id, sealed.id, portfolio.id


def test_real_evidence_promotes_through_alpha_and_portfolio(
    engine: Engine,
    monkeypatch,
) -> None:
    factory = create_session_factory(engine)
    source_id, sealed_dataset_id, portfolio_program_id = _seed(engine)

    def fake_execute(
        self: ExperimentCoordinator,
        *,
        mission_id,
        program_id,
        branch_id,
        request: BacktestExperimentRequest,
        sealed: bool = False,
        parent_entry_id=None,
    ) -> SearchLedgerEntry:
        del self, mission_id
        now = datetime.now(UTC)
        with factory() as session, session.begin():
            entry = SearchLedgerEntry(
                id=request.experiment_id,
                program_id=program_id,
                branch_id=branch_id,
                mission_id=None,
                dataset_revision_id=request.dataset_revision_id,
                parent_entry_id=parent_entry_id,
                mode=ExperimentMode.SEALED.value if sealed else request.mode.value,
                state="SUCCEEDED",
                runtime_name="NAUTILUS_TRADER",
                runtime_version=PINNED_NAUTILUS_VERSION,
                remote_run_id=f"remote-{request.experiment_id}",
                request_json=request.model_dump(mode="json"),
                evidence_json={} if sealed else _evidence(request.experiment_id),
                disclosure_json=(
                    {
                        "passed": True,
                        "order_count": 1,
                        "fill_count": 1,
                        "position_count": 1,
                        "policy": "AGGREGATES_ONLY_V1",
                    }
                    if sealed
                    else {}
                ),
                started_at=now,
                finished_at=now,
            )
            session.add(entry)
            session.flush()
            session.expunge(entry)
            return entry

    monkeypatch.setattr(ExperimentCoordinator, "execute", fake_execute)

    alpha = qualify_alpha(
        factory,
        source_experiment_id=source_id,
        sealed_dataset_revision_id=sealed_dataset_id,
        name="Qualified remote Nautilus alpha",
    )
    assert alpha.source_experiment_id == source_id
    assert alpha.metrics["sealed_disclosure"]["passed"] is True
    assert alpha.metrics["strategy_artifact"]["artifact_id"] == _STRATEGY.artifact_id

    with factory() as session:
        sealed_entry = session.scalar(
            select(SearchLedgerEntry).where(SearchLedgerEntry.mode == ExperimentMode.SEALED.value)
        )
        assert sealed_entry is not None
        assert sealed_entry.parent_entry_id == source_id
        assert sealed_entry.evidence_json == {}
        assert sealed_entry.disclosure_json["policy"] == "AGGREGATES_ONLY_V1"

    promoted = simulate_portfolio_candidate(
        factory,
        portfolio_program_id=portfolio_program_id,
        alpha_ids=[alpha.id],
    )
    assert promoted.selected_alpha_id == alpha.id

    with factory() as session:
        candidate = session.get(PortfolioCandidate, promoted.candidate_id)
        approval = session.get(ApprovalSnapshot, promoted.approval_id)
        simulation = session.get(SearchLedgerEntry, promoted.simulation_experiment_id)
        persisted_alpha = session.get(AlphaQualification, alpha.id)
        assert candidate is not None
        assert approval is not None
        assert simulation is not None
        assert persisted_alpha is not None
        assert simulation.mode == ExperimentMode.PORTFOLIO.value
        assert simulation.evidence_json["orders"]
        assert candidate.simulation_experiment_id == simulation.id
        assert candidate.members[0]["alpha_qualification_id"] == str(alpha.id)
        assert candidate.metrics["optimizer"]["selected_alpha_id"] == str(alpha.id)
        assert candidate.metrics["nautilus"]["strategy_artifact"]["artifact_id"] == _STRATEGY.artifact_id
        assert candidate.metrics["nautilus"]["evidence"]["fills"]
        assert approval.state == "PENDING"
        assert approval.purpose == "PAPER"
