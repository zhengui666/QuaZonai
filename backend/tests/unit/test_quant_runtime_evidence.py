from __future__ import annotations

import math

from quant_runtime.contracts import RunEvidence
from quant_runtime.evidence import extract_statistics, persistable_evidence


def _evidence(statistics: dict[str, object]) -> RunEvidence:
    return RunEvidence(
        external_run_id="fixture-run",
        state="SUCCEEDED",
        mode="DISCOVERY",
        runtime_name="NautilusTrader",
        nautilus_version="1.231.0",
        contract_version="1",
        catalog_uri="catalog://fixture",
        strategy_artifact={},
        orders=[{"account_id": "SIM-001"}],
        fills=[{"instrument_id": "EUR/USD"}],
        positions=[{"account_id": "SIM-001"}],
        account=[{"account_id": "SIM-001"}],
        statistics=statistics,
    )


def test_statistics_require_complete_finite_aggregate_values() -> None:
    valid = _evidence(
        {
            "sharpe_ratio": 0.4,
            "max_drawdown": 0.2,
            "turnover": 3.0,
            "total_orders": 2,
            "total_positions": 1,
        }
    )
    assert extract_statistics(valid) == (0.4, 0.2, 3.0, 2, 1)

    assert extract_statistics(
        _evidence(
            {
                "sharpe_ratio": math.nan,
                "max_drawdown": 0.2,
                "turnover": 3.0,
                "total_orders": 2,
                "total_positions": 1,
            }
        )
    ) is None
    assert extract_statistics(
        _evidence(
            {
                "sharpe_ratio": 0.4,
                "max_drawdown": 0.2,
                "turnover": 3.0,
                "total_orders": True,
                "total_positions": 1,
            }
        )
    ) is None


def test_persistable_evidence_excludes_execution_and_account_state() -> None:
    persisted = persistable_evidence(
        _evidence(
            {
                "sharpe_ratio": 0.4,
                "max_drawdown": 0.2,
                "turnover": 3.0,
                "total_orders": 2,
                "total_positions": 1,
                "summary": {"account.SIM.id": "SIM-001"},
            }
        )
    )
    assert not {"orders", "fills", "positions", "account"} & persisted.keys()
    assert "account.SIM.id" not in str(persisted)
