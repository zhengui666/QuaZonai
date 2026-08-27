from __future__ import annotations

from dataclasses import dataclass
import io
import json
from pathlib import Path
from types import SimpleNamespace
from uuid import uuid4
import zipfile

import pytest

from candidate_bundles import build_candidate_bundle, resolve_bundle_archive, validate_candidate_bundle
from errors import QfError


@dataclass
class Member:
    alpha_id: object
    instrument_id: str
    weight: str


@dataclass
class Candidate:
    id: object
    portfolio_program_id: object
    simulation_experiment_id: object
    members: list[Member]
    metrics: dict


def _candidate() -> Candidate:
    experiment_id = uuid4()
    strategy_source = """from nautilus_trader.config import StrategyConfig\nfrom nautilus_trader.trading.strategy import Strategy\n\nclass ExampleConfig(StrategyConfig, frozen=True):\n    pass\n\nclass ExampleStrategy(Strategy):\n    def __init__(self, config: ExampleConfig):\n        super().__init__(config)\n"""
    return Candidate(
        id=uuid4(),
        portfolio_program_id=uuid4(),
        simulation_experiment_id=experiment_id,
        members=[Member(alpha_id=uuid4(), instrument_id="EUR/USD.SIM", weight="1.0")],
        metrics={
            "robustness": {"stress_passed": True},
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
                "custom_data_schemas": {"signal": {"type": "object"}},
                "backtest_run_config": {"catalog_uri": "catalog://prices-v1"},
                "venue_config": {"name": "SIM"},
                "risk_config": {"max_notional": "100000"},
                "discovery_summary": {"accepted_runs": 4},
                "sealed_summary": {"passed": True},
            }
        },
    )


def test_bundle_matches_issue_22_nautilus_native_contract(tmp_path: Path) -> None:
    candidate = _candidate()
    settings = SimpleNamespace(package_root=tmp_path)
    built = build_candidate_bundle(settings, candidate=candidate)
    assert built.validation_summary["valid"] is True
    assert validate_candidate_bundle(built.archive_bytes)["valid"] is True
    assert resolve_bundle_archive(settings, built.relative_path).read_bytes() == built.archive_bytes

    required = {
        "manifest.json",
        "requirements.lock",
        "strategy/strategy.whl",
        "strategy/strategy-config.json",
        "strategy/actor-config.json",
        "data/requirements.json",
        "data/instrument-scope.json",
        "data/custom-data-schemas/index.json",
        "data/custom-data-schemas/signal.json",
        "runtime/nautilus-version.json",
        "runtime/backtest-run-config.json",
        "runtime/venue-config.json",
        "runtime/risk-config.json",
        "runtime/live-node-template.json",
        "validation/fixture-catalog/manifest.json",
        "validation/expected-orders.json",
        "validation/expected-positions.json",
        "validation/expected-statistics.json",
        "evidence/discovery-summary.json",
        "evidence/sealed-summary.json",
        "evidence/robustness-summary.json",
        "lineage.json",
    }
    with zipfile.ZipFile(io.BytesIO(built.archive_bytes)) as archive:
        names = set(archive.namelist())
        assert required.issubset(names)
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["contract_version"] == "2"
        assert manifest["runtime"]["name"] == "NAUTILUS_TRADER"
        assert manifest["runtime"]["version"] == "1.231.0"
        assert manifest["strategy"]["artifact_id"] == "strategy-v1"
        assert manifest["strategy"]["wheel"] == "strategy/strategy.whl"
        assert archive.read("requirements.lock") == b"nautilus_trader==1.231.0\n"
        lineage = json.loads(archive.read("lineage.json"))
        assert lineage["portfolio_simulation_experiment_id"] == str(
            candidate.simulation_experiment_id
        )
        assert b"broker_token" not in built.archive_bytes


def test_bundle_supports_real_candidate_member_dicts() -> None:
    candidate = _candidate()
    candidate.members = [
        {"alpha_id": str(uuid4()), "instrument_id": "AAPL.XNAS", "target_weight": 0.6},
        {"alpha_id": str(uuid4()), "symbol": "MSFT.XNAS", "weight": 0.4},
    ]  # type: ignore[assignment]
    built = build_candidate_bundle(object(), candidate=candidate)
    manifest = built.manifest
    assert [row["instrument_id"] for row in manifest["target_weights"]] == [
        "AAPL.XNAS",
        "MSFT.XNAS",
    ]


def test_bundle_rejects_candidate_without_runtime_evidence() -> None:
    candidate = _candidate()
    candidate.metrics = {}
    with pytest.raises(QfError) as raised:
        build_candidate_bundle(object(), candidate=candidate)
    assert raised.value.code == "CANDIDATE_NAUTILUS_EVIDENCE_MISSING"


def test_bundle_rejects_embedded_broker_secret() -> None:
    candidate = _candidate()
    candidate.metrics["nautilus"]["risk_config"] = {"broker_token": "must-not-ship"}
    with pytest.raises(QfError) as raised:
        build_candidate_bundle(object(), candidate=candidate)
    assert raised.value.code == "CANDIDATE_BUNDLE_INVALID"
