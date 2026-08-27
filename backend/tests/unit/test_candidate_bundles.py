from __future__ import annotations

from dataclasses import dataclass
import io
import json
from uuid import uuid4
import zipfile

import pytest

from candidate_bundles import build_candidate_bundle, validate_candidate_bundle
from errors import QfError


@dataclass
class Member:
    alpha_id: object
    weight: str


@dataclass
class Candidate:
    id: object
    program_id: object
    branch_id: object
    simulation_experiment_id: object
    members: list[Member]
    metrics: dict
    lineage: dict


def _candidate() -> Candidate:
    experiment_id = uuid4()
    strategy_source = """from nautilus_trader.config import StrategyConfig\nfrom nautilus_trader.trading.strategy import Strategy\n\nclass ExampleConfig(StrategyConfig, frozen=True):\n    pass\n\nclass ExampleStrategy(Strategy):\n    def __init__(self, config: ExampleConfig):\n        super().__init__(config)\n"""
    return Candidate(
        id=uuid4(),
        program_id=uuid4(),
        branch_id=uuid4(),
        simulation_experiment_id=experiment_id,
        members=[Member(alpha_id=uuid4(), weight="1.0")],
        lineage={"optimizer": "MEAN_VARIANCE"},
        metrics={
            "nautilus": {
                "strategy_artifact": {
                    "artifact_id": "strategy-v1",
                    "kind": "SOURCE_BUNDLE",
                    "strategy_path": "candidate_strategy:ExampleStrategy",
                    "config_path": "candidate_strategy:ExampleConfig",
                    "config": {},
                    "source_files": {"candidate_strategy.py": strategy_source},
                    "requirements": ["nautilus_trader==1.231.0"],
                },
                "evidence": {
                    "experiment_id": str(experiment_id),
                    "orders": [{"order_id": "O-1"}],
                    "fills": [{"trade_id": "T-1"}],
                    "positions": [{"position_id": "P-1"}],
                    "pnl": {"USD": {"PnL": "12.5"}},
                    "statistics": {"Sharpe Ratio (252 days)": 1.2},
                },
                "instrument_scope": ["EUR/USD.SIM"],
                "data_requirements": {"data_type": "QuoteTick"},
                "backtest_run_config": {"catalog_key": "prices-v1"},
                "venue_config": {"name": "SIM"},
                "risk_config": {"max_notional": "100000"},
            }
        },
    )


def test_bundle_contains_same_strategy_artifact_and_transaction_evidence() -> None:
    candidate = _candidate()
    built = build_candidate_bundle(object(), candidate=candidate)
    assert built.validation_summary["valid"] is True
    assert validate_candidate_bundle(built.archive_bytes)["valid"] is True

    with zipfile.ZipFile(io.BytesIO(built.archive_bytes)) as archive:
        names = set(archive.namelist())
        assert "strategy.whl" in names
        assert "requirements.lock" in names
        assert "evidence/portfolio-simulation.json" in names
        manifest = json.loads(archive.read("MANIFEST.json"))
        assert manifest["runtime"]["name"] == "NAUTILUS_TRADER"
        assert manifest["runtime"]["version"] == "1.231.0"
        assert manifest["strategy"]["artifact_id"] == "strategy-v1"
        assert manifest["lineage"]["portfolio_simulation_experiment_id"] == str(
            candidate.simulation_experiment_id
        )
        assert b"broker_token" not in built.archive_bytes


def test_bundle_rejects_candidate_without_runtime_evidence() -> None:
    candidate = _candidate()
    candidate.metrics = {}
    with pytest.raises(QfError) as raised:
        build_candidate_bundle(object(), candidate=candidate)
    assert raised.value.code == "CANDIDATE_NAUTILUS_EVIDENCE_MISSING"
