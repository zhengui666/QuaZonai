from __future__ import annotations

import json
import zipfile
from datetime import UTC, datetime
from types import SimpleNamespace
from uuid import uuid4

from candidate_packages import build_candidate_package
from settings import Settings


def test_candidate_bundle_is_nautilus_native_and_secret_free(settings: Settings) -> None:
    candidate = SimpleNamespace(
        id=uuid4(),
        candidate_family_id=uuid4(),
        portfolio_program_id=uuid4(),
        evaluation_episode_id=uuid4(),
        members=[{"instrument_id": "EUR/USD.SIM", "target_weight": 1.0}],
        metrics={
            "strategy_artifact": {
                "artifact_id": str(uuid4()),
                "strategy_path": "nautilus_trader.examples.strategies.ema_cross:EMACross",
                "config_path": "nautilus_trader.examples.strategies.ema_cross:EMACrossConfig",
                "config": {"fast_ema_period": 3, "slow_ema_period": 8},
            },
            "quant_evidence": {
                "discovery": {
                    "catalog_uri": "catalog://discovery-eurusd",
                    "statistics": {"returns": {"Sharpe Ratio (252 days)": 1.0}},
                    "reports": {
                        "orders": [{"client_order_id": "O-1"}],
                        "positions": [{"instrument_id": "EUR/USD.SIM"}],
                    },
                },
                "sealed": {
                    "disclosure": {"decision": "PASS"},
                    "reports": {},
                },
            },
        },
    )
    approval = SimpleNamespace(
        id=uuid4(),
        purpose="PAPER",
        evidence_summary={"decision": "PASS"},
        risk_summary={},
        cost_summary={},
        capacity_summary={},
    )
    downstream = SimpleNamespace(package_contract_version="2")

    built = build_candidate_package(
        settings,
        approval=approval,
        candidate=candidate,
        downstream=downstream,
    )
    archive_path = settings.package_root / built.relative_path
    with zipfile.ZipFile(archive_path) as archive:
        names = set(archive.namelist())
        assert {
            "manifest.json",
            "requirements.lock",
            "strategy/strategy.whl",
            "runtime/nautilus-version.json",
            "runtime/live-node-template.json",
            "validation/expected-orders.json",
            "evidence/discovery-summary.json",
            "evidence/sealed-summary.json",
            "lineage.json",
        } <= names
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["bundle_kind"] == "NAUTILUS_NATIVE_CANDIDATE"
        assert manifest["runtime"] == {"name": "NAUTILUS_TRADER", "version": "1.231.0"}
        assert manifest["contains_broker_credentials"] is False
        assert b"api_key" not in archive.read("manifest.json")
        assert archive.read("requirements.lock") == b"nautilus_trader==1.231.0\n"

    assert built.operator_summary["approval_id"] == str(approval.id)
    assert datetime.now(UTC).year >= 2026
