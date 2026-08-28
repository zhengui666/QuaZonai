from __future__ import annotations

from quazonai_nautilus_gateway.engine import _sealed_performance_disclosure


def test_sealed_level1_disclosure_never_exposes_exact_metrics_or_counts() -> None:
    disclosure = _sealed_performance_disclosure(
        {
            "orders": [{"order_id": "O-1"}],
            "fills": [{"trade_id": "T-1"}],
            "positions": [{"position_id": "P-1"}],
            "pnl": {
                "USD": {
                    "PnL (total)": 12.5,
                    "Profit Factor": 1.8,
                }
            },
            "statistics": {
                "returns": {
                    "Sharpe Ratio (252 days)": 1.4,
                    "Max Drawdown": -0.08,
                },
                "general": {},
            },
        }
    )

    assert disclosure == {
        "passed": True,
        "quality_tier": "QUALIFIED",
        "reason_codes": ["SEALED_POLICY_PASSED"],
        "policy_checks": {
            "transaction_evidence": True,
            "positive_total_pnl": True,
            "non_negative_sharpe_when_available": True,
            "max_drawdown_floor": True,
            "profit_factor_floor_when_available": True,
        },
        "policy": "SEALED_LEVEL1_POLICY_V1",
    }


def test_sealed_level1_rejection_is_reason_code_only() -> None:
    disclosure = _sealed_performance_disclosure(
        {
            "orders": [],
            "fills": [],
            "positions": [],
            "pnl": {"USD": {"PnL (total)": -99.0}},
            "statistics": {"returns": {}, "general": {}},
        }
    )

    assert disclosure["passed"] is False
    assert disclosure["quality_tier"] == "REJECTED"
    assert "TRANSACTION_EVIDENCE_MISSING" in disclosure["reason_codes"]
    assert "TOTAL_PNL_POLICY_FAILED" in disclosure["reason_codes"]
    for forbidden in (
        "quality_score",
        "performance",
        "order_count",
        "fill_count",
        "position_count",
        "statistics",
        "pnl_summary",
    ):
        assert forbidden not in disclosure
