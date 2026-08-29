from __future__ import annotations

import json
import zipfile
from datetime import UTC, datetime
from types import SimpleNamespace
from uuid import uuid4

from candidate_packages import build_candidate_package
from settings import Settings


def test_candidate_bundle_is_nautilus_native_and_secret_free(settings: Settings) -> None:
    artifact = {
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
    }
    portfolio_evidence = {
        "external_run_id": "fixture-portfolio-run",
        "state": "SUCCEEDED",
        "mode": "PORTFOLIO",
        "runtime_name": "NautilusTrader",
        "nautilus_version": "1.231.0",
        "contract_version": "1",
        "catalog_uri": "catalog://discovery-eurusd",
        "strategy_artifact": artifact,
        "orders": [{"instrument_id": "AAPL.SIM", "side": "BUY", "account_id": "SIM-001"}],
        "fills": [{"instrument_id": "AAPL.SIM"}],
        "positions": [{"instrument_id": "AAPL.SIM", "account_id": "SIM-001"}],
        "account": [],
        "statistics": {"total_orders": 1, "summary": {"account.SIM.id": "SIM-001"}},
    }
    candidate = SimpleNamespace(
        id=uuid4(),
        candidate_family_id=uuid4(),
        portfolio_program_id=uuid4(),
        mandate_version_id=uuid4(),
        capital_context_version_id=None,
        universe_set_json=[str(uuid4())],
        evaluation_episode_id=uuid4(),
        policy_version="POLICY_V1",
        risk_model_version="RISK_V1",
        cost_model_version="COST_V1",
        capacity_model_version="CAPACITY_V1",
        constraint_set_version="CONSTRAINTS_V1",
        rebalance_policy_version="REBALANCE_V1",
        members=[{"instrument_id": "AAPL.SIM", "target_weight": 1.0}],
        metrics={
            "nautilus": {
                "strategy_artifact": artifact,
                "portfolio_evidence": portfolio_evidence,
                "discovery_run_id": "fixture-discovery-run",
                "sealed_run_id": "fixture-sealed-run",
                "portfolio_run_id": "fixture-portfolio-run",
            },
            "sealed_statistics": {"sharpe_ratio": 1.0},
        },
    )
    approval = SimpleNamespace(
        id=uuid4(),
        purpose="PAPER",
        evidence_summary={"decision": "PASS"},
        capital_context={},
        risk_summary={},
        cost_summary={},
        capacity_summary={},
        changes_summary={},
    )
    downstream = SimpleNamespace(
        package_contract_version="1",
        compatibility=["NAUTILUS_TRADER_1.231.0"],
    )

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
            "validation/target-portfolio-frame.json",
            "evidence/discovery-summary.json",
            "evidence/sealed-summary.json",
            "lineage.json",
        } <= names
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["candidate_bundle_contract_version"] == "1"
        assert manifest["canonical_runtime"] == {
            "name": "NautilusTrader",
            "version": "1.231.0",
            "quant_contract_version": "1",
        }
        assert manifest["execution_secret_material"] == "excluded"
        assert b"api_key" not in archive.read("manifest.json")
        assert b"account.SIM.id" not in archive.read("validation/expected-statistics.json")
        assert b"order" not in archive.read("validation/target-portfolio-frame.json").lower()
        assert b"position" not in archive.read("validation/target-portfolio-frame.json").lower()
        assert archive.read("requirements.lock") == b"nautilus-trader==1.231.0\n"

    assert built.operator_summary["approval_id"] == str(approval.id)
    assert datetime.now(UTC).year >= 2026
