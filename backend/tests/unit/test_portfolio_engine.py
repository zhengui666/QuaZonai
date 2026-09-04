from __future__ import annotations

import numpy as np
import pytest

from portfolio_engine import (
    CapacityEstimate,
    CapacityInput,
    CostModel,
    EligibleAlpha,
    OptimizationInput,
    OptimizationStatus,
    PortfolioConstraints,
    estimate_capacity,
    estimate_transaction_cost,
    ewma_shrinkage_covariance,
    optimize_portfolio,
)
from portfolio_engine import engine


def test_ewma_shrinkage_covariance_is_psd() -> None:
    estimate = ewma_shrinkage_covariance(
        (
            (0.01, 0.02),
            (0.02, 0.01),
            (-0.01, 0.00),
            (0.03, 0.01),
            (0.00, -0.02),
            (0.01, 0.03),
        ),
        minimum_observations=4,
    )

    assert estimate.observations == 6
    assert 0.0 <= estimate.shrinkage <= 1.0
    assert np.linalg.eigvalsh(np.asarray(estimate.covariance)).min() >= -1e-12


def test_covariance_rejects_insufficient_history() -> None:
    with pytest.raises(ValueError, match="RISK_MODEL_INSUFFICIENT_DATA"):
        ewma_shrinkage_covariance(((0.01, 0.02),), minimum_observations=2)


def test_cost_and_capacity_are_recomputable() -> None:
    capacity = estimate_capacity(
        CapacityInput(
            average_daily_volume=2_000_000,
            half_spread_rate=0.001,
            volatility=0.02,
            holding_horizon_days=5,
            turnover=0.25,
            deployable_capital=1_000_000,
            max_participation_rate=0.1,
            stress_multiplier=1.5,
        )
    )
    cost = estimate_transaction_cost(
        (0.6, 0.4),
        (0.0, 0.0),
        1_000_000,
        CostModel(
            commission_rate=0.001,
            half_spread_rate=0.002,
            slippage_rate=0.001,
            impact_rate=0.01,
            impact_breakpoint=0.1,
        ),
    )

    assert capacity.max_trade_notional > 0
    assert capacity.max_position_notional > capacity.max_trade_notional
    assert capacity.stressed_capacity <= capacity.max_position_notional
    assert cost.impact > 0
    assert cost.total == pytest.approx(
        cost.commission + cost.half_spread + cost.slippage + cost.impact
    )


def test_optimizer_returns_multiple_target_weights_and_recomputable_diagnostics() -> None:
    result = optimize_portfolio(
        OptimizationInput(
            eligible_alphas=(
                EligibleAlpha("alpha-a", expected_return=0.06, uncertainty=0.01),
                EligibleAlpha("alpha-b", expected_return=0.05, uncertainty=0.02),
            ),
            covariance=((0.04, 0.01), (0.01, 0.03)),
            capital=1_000_000,
            constraints=PortfolioConstraints(minimum_weight=0.1, maximum_weight=0.75),
            cost_model=CostModel(commission_rate=0.001, impact_rate=0.01, impact_breakpoint=0.05),
        )
    )

    assert result.status is OptimizationStatus.OPTIMAL
    assert len(result.target_weights) == 2
    assert sum(weight.target_weight for weight in result.target_weights) == pytest.approx(1.0)
    assert all(weight.target_weight >= 0.1 - 1e-6 for weight in result.target_weights)
    assert max(weight.target_weight for weight in result.target_weights) < 1.0
    assert sum(item.fraction for item in result.risk_contributions) == pytest.approx(1.0)
    assert {item.name for item in result.constraint_slacks} >= {
        "gross_exposure",
        "net_exposure_residual",
    }


def test_optimizer_returns_infeasible_for_one_alpha_without_a_fallback() -> None:
    result = optimize_portfolio(
        OptimizationInput(
            eligible_alphas=(EligibleAlpha("only", expected_return=0.05, uncertainty=0.01),),
            covariance=((0.04,),),
            capital=1_000_000,
        )
    )

    assert result.status is OptimizationStatus.INFEASIBLE
    assert not result.target_weights
    assert {diagnostic.code for diagnostic in result.diagnostics} >= {
        "INSUFFICIENT_ELIGIBLE_ALPHAS"
    }


def test_optimizer_rejects_nonfinite_alpha_inputs() -> None:
    result = optimize_portfolio(
        OptimizationInput(
            eligible_alphas=(
                EligibleAlpha("alpha-a", expected_return=float("nan"), uncertainty=0.01),
                EligibleAlpha("alpha-b", expected_return=0.05, uncertainty=0.02),
            ),
            covariance=((0.04, 0.01), (0.01, 0.03)),
            capital=1_000_000,
        )
    )

    assert result.status is OptimizationStatus.INFEASIBLE
    assert {diagnostic.code for diagnostic in result.diagnostics} >= {"ELIGIBLE_ALPHA_INVALID"}


def test_optimizer_returns_explicit_diagnostic_without_cvxpy(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(engine, "cp", None)

    result = engine.optimize_portfolio(
        OptimizationInput(
            eligible_alphas=(
                EligibleAlpha("alpha-a", expected_return=0.06, uncertainty=0.01),
                EligibleAlpha("alpha-b", expected_return=0.05, uncertainty=0.02),
            ),
            covariance=((0.04, 0.01), (0.01, 0.03)),
            capital=1_000_000,
        )
    )

    assert result.status is OptimizationStatus.INFEASIBLE
    assert {diagnostic.code for diagnostic in result.diagnostics} == {"CVXPY_UNAVAILABLE"}


def test_optimizer_returns_infeasible_for_conflicting_weight_limits() -> None:
    result = optimize_portfolio(
        OptimizationInput(
            eligible_alphas=(
                EligibleAlpha("alpha-a", expected_return=0.06, uncertainty=0.01),
                EligibleAlpha("alpha-b", expected_return=0.05, uncertainty=0.02),
            ),
            covariance=((0.04, 0.01), (0.01, 0.03)),
            capital=1_000_000,
            constraints=PortfolioConstraints(minimum_weight=0.01, maximum_weight=0.4),
        )
    )

    assert result.status is OptimizationStatus.INFEASIBLE
    assert not result.target_weights
    assert {diagnostic.code for diagnostic in result.diagnostics} >= {"CONSTRAINTS_INFEASIBLE"}


def test_optimizer_returns_solver_infeasible_for_capacity_limits() -> None:
    result = optimize_portfolio(
        OptimizationInput(
            eligible_alphas=(
                EligibleAlpha("alpha-a", expected_return=0.06, uncertainty=0.01),
                EligibleAlpha("alpha-b", expected_return=0.05, uncertainty=0.02),
            ),
            covariance=((0.04, 0.01), (0.01, 0.03)),
            capital=1_000_000,
            capacities=(
                CapacityEstimate(1_000_000, 400_000, 0.1, 1.0, 400_000),
                CapacityEstimate(1_000_000, 400_000, 0.1, 1.0, 400_000),
            ),
        )
    )

    assert result.status is OptimizationStatus.INFEASIBLE
    assert not result.target_weights
    assert {diagnostic.code for diagnostic in result.diagnostics} >= {"SOLVER_INFEASIBLE"}


def test_optimizer_enforces_stressed_capacity_ceiling() -> None:
    result = optimize_portfolio(
        OptimizationInput(
            eligible_alphas=(
                EligibleAlpha("alpha-a", expected_return=0.06, uncertainty=0.01),
                EligibleAlpha("alpha-b", expected_return=0.05, uncertainty=0.02),
            ),
            covariance=((0.04, 0.01), (0.01, 0.03)),
            capital=1_000_000,
            capacities=(
                CapacityEstimate(1_000_000, 1_000_000, 0.1, 1.0, 100_000),
                CapacityEstimate(1_000_000, 1_000_000, 0.1, 1.0, 100_000),
            ),
        )
    )

    assert result.status is OptimizationStatus.INFEASIBLE
