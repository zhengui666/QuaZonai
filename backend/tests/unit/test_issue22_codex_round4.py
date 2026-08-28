from __future__ import annotations

from dataclasses import dataclass
import io
import zipfile
from uuid import uuid4

from candidate_bundles import build_candidate_bundle
from quant_runtime.workspace import _degradation_disclosure


@dataclass
class _Member:
    alpha_id: object
    instrument_id: str
    weight: str


@dataclass
class _Candidate:
    id: object
    portfolio_program_id: object
    simulation_experiment_id: object
    members: list[_Member]
    metrics: dict


def _package_candidate() -> _Candidate:
    experiment_id = uuid4()
    dataset_revision_id = uuid4()
    strategy_source = """from nautilus_trader.config import StrategyConfig
from nautilus_trader.trading.strategy import Strategy

class PackageConfig(StrategyConfig, frozen=True):
    pass

class PackageStrategy(Strategy):
    def __init__(self, config: PackageConfig):
        super().__init__(config)
"""
    return _Candidate(
        id=uuid4(),
        portfolio_program_id=uuid4(),
        simulation_experiment_id=experiment_id,
        members=[_Member(alpha_id=uuid4(), instrument_id="EUR/USD.SIM", weight="1.0")],
        metrics={
            "nautilus": {
                "strategy_artifact": {
                    "artifact_id": "package-strategy-v1",
                    "kind": "SOURCE_BUNDLE",
                    "strategy_path": "package_strategy:PackageStrategy",
                    "config_path": "package_strategy:PackageConfig",
                    "config": {},
                    "source_files": {"package_strategy/__init__.py": strategy_source},
                    "requirements": ["nautilus_trader==1.231.0"],
                },
                "evidence": {
                    "experiment_id": str(experiment_id),
                    "orders": [{"order_id": "O-1"}],
                    "fills": [{"trade_id": "T-1"}],
                    "positions": [{"position_id": "P-1"}],
                    "pnl": {"USD": {"PnL": "12.5"}},
                    "statistics": {"total_orders": 1, "total_positions": 1},
                },
                "dataset_revision_ids": [str(dataset_revision_id)],
                "instrument_scope": ["EUR/USD.SIM"],
                "data_requirements": {"data_type": "QuoteTick"},
                "backtest_run_config": {
                    "catalog_key": "prices-v1",
                    "catalog_uri": "nautilus-catalog://prices-v1",
                },
                "venue_config": {},
                "risk_config": {},
            }
        },
    )


def test_candidate_bundle_supports_package_entry_points() -> None:
    candidate = _package_candidate()
    built = build_candidate_bundle(object(), candidate=candidate)
    assert built.validation_summary["valid"] is True
    with zipfile.ZipFile(io.BytesIO(built.archive_bytes)) as archive:
        wheel_name = built.manifest["strategy"]["wheel"]
        with zipfile.ZipFile(io.BytesIO(archive.read(wheel_name))) as wheel:
            assert "package_strategy/__init__.py" in wheel.namelist()


def test_degradation_disclosure_is_capability_filtered() -> None:
    disclosure = _degradation_disclosure(
        {
            "degraded": True,
            "degradation_state": "degraded",
            "return": -0.12,
            "max_drawdown": -0.31,
            "reason_codes": ["RETURN_BREAKDOWN", "DRAWDOWN_LIMIT", "sk-secret"],
            "api_key": "must-not-leak",
            "access_token": "must-not-leak",
            "private_payload": {"account": "must-not-leak"},
        }
    )
    assert disclosure == {
        "degradation_state": "DEGRADED",
        "degraded": True,
        "max_drawdown": -0.31,
        "return": -0.12,
        "reason_codes": ["RETURN_BREAKDOWN", "DRAWDOWN_LIMIT"],
    }
