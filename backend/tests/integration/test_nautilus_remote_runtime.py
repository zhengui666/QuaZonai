from __future__ import annotations

import io
import json
import os
import zipfile
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import Engine, func, select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CapitalContextVersion,
    DownstreamSystem,
    ForwardEvidenceEpisode,
    Job,
    PortfolioCandidate,
    PortfolioMandate,
    QuantRuntimeRun,
    SearchLedgerEntry,
    MarketUniverseVersion,
)
from db.session import create_session_factory
from downstream_auth import install_service_token, issue_service_token
from main import create_app
from quant_runtime.mission_execution import execute_mission_experiments
from runners.sealed_evaluator import run_sealed_evaluation
from settings import Settings

pytestmark = [
    pytest.mark.nautilus,
    pytest.mark.skipif(
        not os.environ.get("QUAZONAI_NAUTILUS_RUNTIME_URL")
        or not os.environ.get("QUAZONAI_NAUTILUS_SEALED_RUNTIME_URL"),
        reason="real remote Research and Sealed NautilusTrader runtimes are not configured",
    ),
]

_STRATEGY_SOURCE = '''from decimal import Decimal

from nautilus_trader.config import StrategyConfig
from nautilus_trader.indicators import ExponentialMovingAverage
from nautilus_trader.model.data import Bar, BarType
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.trading.strategy import Strategy


class EMACrossConfig(StrategyConfig, frozen=True):
    instrument_id: InstrumentId
    bar_type: BarType
    trade_size: Decimal
    fast_ema_period: int = 3
    slow_ema_period: int = 8


class EMACross(Strategy):
    def __init__(self, config: EMACrossConfig):
        super().__init__(config)
        self.fast = ExponentialMovingAverage(config.fast_ema_period)
        self.slow = ExponentialMovingAverage(config.slow_ema_period)

    def on_start(self):
        self.register_indicator_for_bars(self.config.bar_type, self.fast)
        self.register_indicator_for_bars(self.config.bar_type, self.slow)
        self.subscribe_bars(self.config.bar_type)

    def on_bar(self, bar: Bar):
        if not self.indicators_initialized():
            return
        if self.fast.value >= self.slow.value:
            if self.portfolio.is_flat(self.config.instrument_id):
                self._market(OrderSide.BUY)
            elif self.portfolio.is_net_short(self.config.instrument_id):
                self.close_all_positions(self.config.instrument_id)
                self._market(OrderSide.BUY)
        elif self.portfolio.is_flat(self.config.instrument_id):
            self._market(OrderSide.SELL)
        elif self.portfolio.is_net_long(self.config.instrument_id):
            self.close_all_positions(self.config.instrument_id)
            self._market(OrderSide.SELL)

    def _market(self, side: OrderSide):
        instrument = self.cache.instrument(self.config.instrument_id)
        order = self.order_factory.market(
            self.config.instrument_id,
            side,
            instrument.make_qty(self.config.trade_size),
        )
        self.submit_order(order)

    def on_stop(self):
        self.close_all_positions(self.config.instrument_id)
'''


def _client(engine: Engine, settings: Settings) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def _ingest(
    client: TestClient,
    *,
    name: str,
    seed: int,
    sealed: bool,
) -> dict[str, object]:
    response = client.post(
        "/api/v1/quant-runtime/catalogs/ingest",
        json={
            "catalog_name": name,
            "provider": "QuaZonai CI synthetic provider",
            "source_license": "CI-only generated data",
            "universe_name": "FX",
            "sealed": sealed,
            "source_spec": {
                "kind": "synthetic_fx_quotes",
                "instrument": "EUR/USD",
                "rows": 3000,
                "seed": seed,
            },
        },
    )
    assert response.status_code == 201, response.text
    body = response.json()
    assert body["quality_state"] == "VALID"
    assert body["point_in_time_state"] == "VALID"
    assert body["sealed"] is sealed
    return body


def _seed_mandate_and_downstream(
    engine: Engine,
    settings: Settings,
) -> tuple[str, str]:
    factory = create_session_factory(engine)
    with factory.begin() as session:
        mandate = PortfolioMandate(
            key="nautilus-ci",
            name="Nautilus CI Mandate",
            enabled=True,
            latest_version_id=uuid4(),
            spec_json={"max_weight": 1.0},
            state="ACTIVE",
        )
        downstream_id = uuid4()
        issued = issue_service_token(settings, downstream_id)
        downstream = DownstreamSystem(
            id=downstream_id,
            name="Remote Nautilus Paper CI",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            compatibility=["NAUTILUS_TRADER_1.231.0"],
            preflight_state="READY",
            public_config={
                "feedback_contract": {
                    "minimum_observation_duration_seconds": 60,
                    "minimum_valid_sample_size": 10,
                    "required_fields": ["return"],
                }
            },
        )
        install_service_token(downstream, issued)
        session.add_all([
            mandate,
            downstream,
            CapitalContextVersion(
                source_type="ADMIN",
                base_currency="USD",
                deployable_capital=100_000,
                observed_at=datetime.now(UTC),
                valid_until=datetime.now(UTC) + timedelta(days=7),
            ),
        ])
        return str(downstream.id), issued.token


def test_idea_to_remote_nautilus_paper_feedback_vertical_e2e(
    engine: Engine,
    settings: Settings,
    tmp_path: Path,
) -> None:
    client = _client(engine, settings)
    capabilities = client.get("/api/v1/quant-runtime/capabilities")
    assert capabilities.status_code == 200, capabilities.text
    assert capabilities.json() == {
        "runtime_name": "NautilusTrader",
        "nautilus_version": "1.231.0",
        "contract_version": "2",
        "catalog_type": "ParquetDataCatalog",
        "supported_modes": ["DISCOVERY", "SEALED", "PORTFOLIO"],
        "candidate_contract_version": "1",
    }

    factory = create_session_factory(engine)
    with factory.begin() as session:
        session.add(
            MarketUniverseVersion(
                universe_key="FX",
                version_no=1,
                name="FX",
                state="ACTIVE",
                spec_json={"currency": "USD"},
                created_at=datetime.now(UTC),
            )
        )

    discovery = _ingest(client, name="ci-discovery", seed=41, sealed=False)
    changed_catalog_input = client.post(
        "/api/v1/quant-runtime/catalogs/ingest",
        json={
            "catalog_name": "ci-discovery",
            "provider": "QuaZonai CI synthetic provider",
            "source_license": "CI-only generated data",
            "universe_name": "FX",
            "sealed": False,
            "source_spec": {
                "kind": "synthetic_fx_quotes",
                "instrument": "EUR/USD",
                "rows": 3000,
                "seed": 42,
            },
        },
    )
    assert changed_catalog_input.status_code == 409, changed_catalog_input.text
    # Seed 1 produces a positive independent sealed result under the fixed
    # server-owned promotion policy; the test must not rely on a Mission gate.
    _ingest(client, name="ci-sealed", seed=1, sealed=True)
    validated = client.post(
        f"/api/v1/quant-runtime/catalogs/{discovery['id']}/validate"
    )
    assert validated.status_code == 200, validated.text

    downstream_id, downstream_token = _seed_mandate_and_downstream(engine, settings)
    program_response = client.post(
        "/api/v1/research-programs",
        headers={"Idempotency-Key": "nautilus-e2e-program"},
        json={
            "idea": (
                "Research a bounded EUR/USD EMA crossover using transaction-level "
                "NautilusTrader evidence and independent sealed evaluation."
            ),
            "answers": {},
        },
    )
    assert program_response.status_code == 201, program_response.text
    program = program_response.json()
    missions = client.get(f"/api/v1/research-programs/{program['id']}/missions")
    assert missions.status_code == 200
    mission_id = UUID(missions.json()[0]["id"])

    workspace = tmp_path / "mission"
    workspace.mkdir()
    artifact = {
        "strategy_path": "strategy.ema_cross:EMACross",
        "config_path": "strategy.ema_cross:EMACrossConfig",
        "config": {
            "instrument_id": "EUR/USD.SIM",
            "bar_type": "EUR/USD.SIM-5-MINUTE-BID-INTERNAL",
            "trade_size": "100000",
            "fast_ema_period": 3,
            "slow_ema_period": 8,
        },
        "source_files": {
            "strategy/__init__.py": "",
            "strategy/ema_cross.py": _STRATEGY_SOURCE,
        },
        "requirements": ["nautilus-trader==1.231.0"],
    }
    (workspace / "EXPERIMENTS.json").write_text(
        json.dumps(
            {
                "experiments": [
                    {
                        "experiment_key": "eurusd-ema-3-8",
                        "family": "EMA_CROSS",
                        "catalog_uri": discovery["catalog_uri"],
                        "strategy": artifact,
                        "parameters": {
                            "fast": 3,
                            "slow": 8,
                            "alpha_output_contract": {
                                "kind": "score",
                                "fields": ["score", "expected_return", "uncertainty"],
                            },
                        },
                    },
                    {
                        "experiment_key": "eurusd-ema-4-12",
                        "family": "EMA_CROSS",
                        "catalog_uri": discovery["catalog_uri"],
                        "strategy": {
                            **artifact,
                            "config": {
                                **artifact["config"],
                                "fast_ema_period": 4,
                                "slow_ema_period": 12,
                            },
                        },
                        "parameters": {
                            "fast": 4,
                            "slow": 12,
                            "alpha_output_contract": {
                                "kind": "score",
                                "fields": ["score", "expected_return", "uncertainty"],
                            },
                        },
                    },
                ]
            }
        ),
        encoding="utf-8",
    )

    result = execute_mission_experiments(
        settings,
        mission_id=mission_id,
        workspace=workspace,
    )
    assert result is not None
    assert result["promotion"] == "SEALED_EVALUATION_QUEUED"
    assert len(result["runs"]) == 2
    evidence_file = json.loads((workspace / "EVIDENCE.json").read_text())
    assert evidence_file["selected_discovery_run_id"]
    assert "catalog://ci-sealed" not in json.dumps(evidence_file)

    factory = create_session_factory(engine)
    with factory() as session:
        job = session.scalar(select(Job).where(Job.kind == "SEALED_EVALUATION"))
        assert job is not None
        sealed_job_id = job.id
        assert session.scalar(select(func.count()).select_from(SearchLedgerEntry)) == 2

    run_sealed_evaluation(settings, sealed_job_id)

    with factory() as session:
        modes = set(session.scalars(select(QuantRuntimeRun.mode)).all())
        assert modes == {"DISCOVERY", "SEALED", "PORTFOLIO"}
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 1
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 1
        approval = session.scalar(select(ApprovalSnapshot).where(ApprovalSnapshot.state == "PENDING"))
        assert approval is not None
        approval_id = str(approval.id)

    approved = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers={"Idempotency-Key": "nautilus-e2e-approve"},
        json={
            "downstream_system_id": downstream_id,
            "expected_state": "PENDING",
        },
    )
    assert approved.status_code == 200, approved.text

    handoffs = client.get("/api/v1/handoffs")
    assert handoffs.status_code == 200
    handoff = handoffs.json()[0]
    auth = {"Authorization": f"Bearer {downstream_token}"}
    claimed = client.post(
        f"/api/v1/handoffs/{handoff['id']}/claim",
        headers={**auth, "Idempotency-Key": "nautilus-e2e-claim"},
        json={"expected_state": "AVAILABLE"},
    )
    assert claimed.status_code == 200, claimed.text

    package = client.get(f"/api/v1/handoffs/{handoff['id']}/package", headers=auth)
    assert package.status_code == 200, package.text
    with zipfile.ZipFile(io.BytesIO(package.content)) as bundle:
        names = set(bundle.namelist())
        assert {
            "manifest.json",
            "requirements.lock",
            "strategy/strategy.whl",
            "strategy/strategy-config.json",
            "runtime/live-node-template.json",
            "validation/target-portfolio-frame.json",
            "validation/expected-statistics.json",
            "evidence/discovery-summary.json",
            "evidence/sealed-summary.json",
            "evidence/robustness-summary.json",
            "lineage.json",
        } <= names
        manifest = json.loads(bundle.read("manifest.json"))
        assert manifest["canonical_runtime"]["version"] == "1.231.0"
        assert manifest["same_strategy_artifact_for_backtest_paper_live"] is True
        assert b"nautilus-trader==1.231.0" in bundle.read("requirements.lock")

    accepted = client.post(
        f"/api/v1/handoffs/{handoff['id']}/accept",
        headers={**auth, "Idempotency-Key": "nautilus-e2e-accept"},
        json={"expected_state": "CLAIMED"},
    )
    assert accepted.status_code == 200, accepted.text

    start = datetime.now(UTC) - timedelta(minutes=10)
    feedback = client.post(
        f"/api/v1/handoffs/{handoff['id']}/feedback",
        headers={**auth, "Idempotency-Key": "nautilus-e2e-feedback"},
        json={
            "state": "FEEDBACK_COMPLETE",
            "observation_start": start.isoformat(),
            "observation_end": datetime.now(UTC).isoformat(),
            "sample_size": 30,
            "evidence": {"periods": 30, "return": 0.02},
        },
    )
    assert feedback.status_code == 200, feedback.text
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(ForwardEvidenceEpisode)) == 1
