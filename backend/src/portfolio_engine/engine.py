"""Numerical helpers and a fail-closed CVXPY portfolio optimizer."""

from __future__ import annotations

from math import isfinite
from typing import Any, Sequence

import numpy as np

from .contracts import (
    CapacityEstimate,
    CapacityInput,
    ConstraintSlack,
    CostEstimate,
    CostModel,
    CovarianceEstimate,
    Diagnostic,
    OptimizationInput,
    OptimizationResult,
    OptimizationStatus,
    RiskContribution,
    TargetWeight,
)


cp: Any
try:
    import cvxpy as cp
except ImportError:  # pragma: no cover - exercised where the optional solver is absent.
    cp = None


def _vector(values: Sequence[float], name: str) -> np.ndarray:
    result = np.asarray(values, dtype=float)
    if result.ndim != 1 or not np.isfinite(result).all():
        raise ValueError(f"{name} must be a finite one-dimensional vector")
    return result


def _nonnegative(value: float, name: str) -> None:
    if not isfinite(value) or value < 0:
        raise ValueError(f"{name} must be finite and non-negative")


def _positive(value: float, name: str) -> None:
    if not isfinite(value) or value <= 0:
        raise ValueError(f"{name} must be finite and positive")


def _validate_cost_model(model: CostModel) -> None:
    for name in (
        "commission_rate",
        "half_spread_rate",
        "slippage_rate",
        "impact_rate",
        "impact_breakpoint",
    ):
        _nonnegative(float(getattr(model, name)), name)


def project_psd(
    covariance: Sequence[Sequence[float]], *, eigenvalue_floor: float = 1e-12
) -> np.ndarray:
    """Symmetrize and clip eigenvalues so the covariance is positive semidefinite."""
    _nonnegative(eigenvalue_floor, "eigenvalue_floor")
    matrix = np.asarray(covariance, dtype=float)
    if matrix.ndim != 2 or matrix.shape[0] != matrix.shape[1] or not np.isfinite(matrix).all():
        raise ValueError("covariance must be a finite square matrix")
    symmetric = (matrix + matrix.T) / 2.0
    values, vectors = np.linalg.eigh(symmetric)
    repaired = (vectors * np.maximum(values, eigenvalue_floor)) @ vectors.T
    return (repaired + repaired.T) / 2.0


def ewma_shrinkage_covariance(
    returns: Sequence[Sequence[float]],
    *,
    decay: float = 0.94,
    minimum_observations: int = 20,
    eigenvalue_floor: float = 1e-12,
) -> CovarianceEstimate:
    """Return EWMA covariance with analytic Ledoit-Wolf shrinkage and PSD repair."""
    if not isfinite(decay) or not 0 < decay < 1:
        raise ValueError("decay must be finite and in (0, 1)")
    if minimum_observations < 2:
        raise ValueError("minimum_observations must be at least two")
    samples = np.asarray(returns, dtype=float)
    if samples.ndim != 2 or samples.shape[1] == 0 or not np.isfinite(samples).all():
        raise ValueError("returns must be a finite two-dimensional matrix")
    observations, assets = samples.shape
    if observations < minimum_observations:
        raise ValueError("RISK_MODEL_INSUFFICIENT_DATA")

    weights = np.power(decay, np.arange(observations - 1, -1, -1, dtype=float))
    weights /= weights.sum()
    centered = samples - weights @ samples
    covariance = (centered * weights[:, None]).T @ centered

    target_scale = float(np.trace(covariance) / assets)
    target = np.eye(assets) * target_scale
    delta = float(np.sum((covariance - target) ** 2))
    if delta == 0.0:
        shrinkage = 1.0
    else:
        outer_norm_squared = np.sum(centered * centered, axis=1) ** 2
        phi = max(float(weights @ outer_norm_squared - np.sum(covariance**2)), 0.0)
        effective_observations = 1.0 / float(np.sum(weights**2))
        shrinkage = min(phi / effective_observations / delta, 1.0)
    repaired = project_psd(
        (1.0 - shrinkage) * covariance + shrinkage * target, eigenvalue_floor=eigenvalue_floor
    )
    return CovarianceEstimate(
        covariance=tuple(tuple(float(value) for value in row) for row in repaired),
        observations=observations,
        decay=decay,
        shrinkage=float(shrinkage),
    )


def estimate_transaction_cost(
    target_weights: Sequence[float],
    previous_weights: Sequence[float],
    capital: float,
    model: CostModel,
) -> CostEstimate:
    """Estimate fee, spread, slippage, and convex excess-participation impact."""
    _positive(capital, "capital")
    _validate_cost_model(model)
    target = _vector(target_weights, "target_weights")
    previous = _vector(previous_weights, "previous_weights")
    if target.shape != previous.shape:
        raise ValueError("target_weights and previous_weights must have the same shape")
    trade_fraction = np.abs(target - previous)
    traded_notional = float(capital * trade_fraction.sum())
    commission = model.commission_rate * traded_notional
    half_spread = model.half_spread_rate * traded_notional
    slippage = model.slippage_rate * traded_notional
    excess = np.maximum(trade_fraction - model.impact_breakpoint, 0.0)
    impact = float(capital * model.impact_rate * np.sum(excess**2))
    total = commission + half_spread + slippage + impact
    return CostEstimate(
        traded_notional=traded_notional,
        commission=float(commission),
        half_spread=float(half_spread),
        slippage=float(slippage),
        impact=impact,
        total=float(total),
    )


def estimate_capacity(values: CapacityInput) -> CapacityEstimate:
    """Derive conservative liquidity capacity without treating capital as capacity."""
    _positive(values.average_daily_volume, "average_daily_volume")
    _nonnegative(values.half_spread_rate, "half_spread_rate")
    _nonnegative(values.volatility, "volatility")
    _positive(values.holding_horizon_days, "holding_horizon_days")
    _nonnegative(values.turnover, "turnover")
    _positive(values.deployable_capital, "deployable_capital")
    if not isfinite(values.max_participation_rate) or not 0 < values.max_participation_rate <= 1:
        raise ValueError("max_participation_rate must be finite and in (0, 1]")
    if not isfinite(values.stress_multiplier) or values.stress_multiplier < 1:
        raise ValueError("stress_multiplier must be finite and at least one")

    liquidity_haircut = 1.0 + values.stress_multiplier * (
        values.half_spread_rate + values.volatility
    )
    max_trade = values.average_daily_volume * values.max_participation_rate / liquidity_haircut
    max_position = max_trade * values.holding_horizon_days / (1.0 + values.turnover)
    days_to_liquidate = values.deployable_capital / max_trade
    stressed_capacity = max_position / values.stress_multiplier
    return CapacityEstimate(
        max_trade_notional=float(max_trade),
        max_position_notional=float(max_position),
        max_participation_rate=values.max_participation_rate,
        days_to_liquidate=float(days_to_liquidate),
        stressed_capacity=float(stressed_capacity),
    )


def _diagnostic(code: str, message: str, **context: str | int | float) -> Diagnostic:
    return Diagnostic(code, message, tuple(context.items()))


def _request_diagnostics(request: OptimizationInput) -> tuple[Diagnostic, ...]:
    diagnostics: list[Diagnostic] = []
    alphas = request.eligible_alphas
    constraints = request.constraints
    required_count = max(2, constraints.minimum_alpha_count)
    if constraints.minimum_alpha_count < 2:
        diagnostics.append(
            _diagnostic(
                "MINIMUM_ALPHA_COUNT_INVALID",
                "minimum_alpha_count must be at least two.",
                minimum_alpha_count=constraints.minimum_alpha_count,
            )
        )
    if len(alphas) < required_count:
        diagnostics.append(
            _diagnostic(
                "INSUFFICIENT_ELIGIBLE_ALPHAS",
                "At least two eligible alphas are required.",
                eligible_alpha_count=len(alphas),
                required_alpha_count=required_count,
            )
        )
    if len({alpha.alpha_id for alpha in alphas}) != len(alphas) or any(
        not alpha.alpha_id for alpha in alphas
    ):
        diagnostics.append(
            _diagnostic("ALPHA_IDS_INVALID", "Alpha identifiers must be unique and non-empty.")
        )
    if any(
        not alpha.eligible
        or not alpha.role
        or not isfinite(alpha.expected_return)
        or not isfinite(alpha.uncertainty)
        or alpha.uncertainty < 0
        for alpha in alphas
    ):
        diagnostics.append(
            _diagnostic(
                "ELIGIBLE_ALPHA_INVALID",
                "Every selected alpha must be eligible with finite return and uncertainty.",
            )
        )
    if not isfinite(request.capital) or request.capital <= 0:
        diagnostics.append(_diagnostic("CAPITAL_INVALID", "capital must be finite and positive."))
    if any(
        not isfinite(value) or value < 0
        for value in (request.risk_aversion, request.cost_aversion, request.uncertainty_aversion)
    ):
        diagnostics.append(
            _diagnostic("AVERSION_INVALID", "Objective aversions must be finite and non-negative.")
        )
    try:
        _validate_cost_model(request.cost_model)
    except ValueError as error:
        diagnostics.append(_diagnostic("COST_MODEL_INVALID", str(error)))

    try:
        covariance = np.asarray(request.covariance, dtype=float)
        if covariance.shape != (len(alphas), len(alphas)) or not np.isfinite(covariance).all():
            raise ValueError
    except TypeError, ValueError:
        diagnostics.append(
            _diagnostic(
                "COVARIANCE_INVALID",
                "covariance must be finite and match the selected alpha count.",
            )
        )
    previous = request.previous_weights or (0.0,) * len(alphas)
    if len(previous) != len(alphas) or any(not isfinite(value) or value < 0 for value in previous):
        diagnostics.append(
            _diagnostic(
                "PREVIOUS_WEIGHTS_INVALID",
                "previous_weights must be non-negative, finite, and match the selected alpha count.",
            )
        )
    if request.capacities and len(request.capacities) != len(alphas):
        diagnostics.append(
            _diagnostic(
                "CAPACITY_INVALID", "capacities must be empty or match the selected alpha count."
            )
        )
    for capacity in request.capacities:
        if any(
            not isfinite(value) or value < 0
            for value in (
                capacity.max_trade_notional,
                capacity.max_position_notional,
                capacity.max_participation_rate,
                capacity.days_to_liquidate,
                capacity.stressed_capacity,
            )
        ):
            diagnostics.append(
                _diagnostic("CAPACITY_INVALID", "capacity limits must be finite and non-negative.")
            )
            break

    numeric_limits = (
        constraints.minimum_weight,
        constraints.maximum_weight,
        constraints.gross_exposure_limit,
        constraints.cash_reserve,
    )
    if any(not isfinite(value) for value in numeric_limits) or any(
        value < 0 for value in numeric_limits
    ):
        diagnostics.append(
            _diagnostic("CONSTRAINTS_INVALID", "Exposure limits must be finite and non-negative.")
        )
        return tuple(diagnostics)
    if constraints.minimum_weight > constraints.maximum_weight:
        diagnostics.append(
            _diagnostic("CONSTRAINTS_INVALID", "minimum_weight exceeds maximum_weight.")
        )
    if constraints.minimum_weight <= 0:
        diagnostics.append(
            _diagnostic(
                "MINIMUM_WEIGHT_REQUIRED",
                "minimum_weight must be positive to guarantee multiple active alphas.",
            )
        )
    if constraints.cash_reserve >= 1:
        diagnostics.append(
            _diagnostic("CONSTRAINTS_INVALID", "cash_reserve must be less than one.")
        )
    target = 1.0 - constraints.cash_reserve
    if constraints.net_exposure_target is not None:
        if not isfinite(constraints.net_exposure_target) or constraints.net_exposure_target < 0:
            diagnostics.append(
                _diagnostic(
                    "CONSTRAINTS_INVALID", "net_exposure_target must be finite and non-negative."
                )
            )
        elif abs(constraints.net_exposure_target - target) > 1e-9:
            diagnostics.append(
                _diagnostic(
                    "CASH_NET_EXPOSURE_CONFLICT",
                    "net_exposure_target must equal one minus cash_reserve for this long-only engine.",
                )
            )
        else:
            target = constraints.net_exposure_target
    if constraints.gross_exposure_limit < target:
        diagnostics.append(
            _diagnostic("CONSTRAINTS_INFEASIBLE", "gross exposure cannot satisfy net exposure.")
        )
    if len(alphas) and constraints.minimum_weight * len(alphas) > target + 1e-12:
        diagnostics.append(
            _diagnostic("CONSTRAINTS_INFEASIBLE", "minimum weights exceed net exposure.")
        )
    if len(alphas) and constraints.maximum_weight * len(alphas) < target - 1e-12:
        diagnostics.append(
            _diagnostic("CONSTRAINTS_INFEASIBLE", "maximum weights cannot satisfy net exposure.")
        )
    for name, value in (
        ("turnover_limit", constraints.turnover_limit),
        ("variance_limit", constraints.variance_limit),
    ):
        if value is not None and (not isfinite(value) or value < 0):
            diagnostics.append(
                _diagnostic("CONSTRAINTS_INVALID", f"{name} must be finite and non-negative.")
            )
    return tuple(diagnostics)


def _net_exposure(request: OptimizationInput) -> float:
    return 1.0 - request.constraints.cash_reserve


def _infeasible(*diagnostics: Diagnostic) -> OptimizationResult:
    return OptimizationResult(status=OptimizationStatus.INFEASIBLE, diagnostics=diagnostics)


def _risk_contributions(
    alpha_ids: tuple[str, ...], weights: np.ndarray, covariance: np.ndarray, variance: float
) -> tuple[RiskContribution, ...]:
    if variance <= 0:
        return tuple(RiskContribution(alpha_id, 0.0) for alpha_id in alpha_ids)
    contributions = weights * (covariance @ weights) / variance
    return tuple(
        RiskContribution(alpha_id, float(contribution))
        for alpha_id, contribution in zip(alpha_ids, contributions, strict=True)
    )


def _constraint_slacks(
    request: OptimizationInput,
    weights: np.ndarray,
    previous: np.ndarray,
    variance: float,
) -> tuple[ConstraintSlack, ...]:
    constraints = request.constraints
    slacks = [
        ConstraintSlack(
            "gross_exposure", float(constraints.gross_exposure_limit - np.abs(weights).sum())
        ),
        ConstraintSlack("net_exposure_residual", float(_net_exposure(request) - weights.sum())),
        ConstraintSlack(
            "cash_reserve_residual", float(1.0 - constraints.cash_reserve - weights.sum())
        ),
    ]
    for alpha, weight in zip(request.eligible_alphas, weights, strict=True):
        slacks.extend(
            (
                ConstraintSlack(
                    f"minimum_weight:{alpha.alpha_id}", float(weight - constraints.minimum_weight)
                ),
                ConstraintSlack(
                    f"maximum_weight:{alpha.alpha_id}", float(constraints.maximum_weight - weight)
                ),
            )
        )
    if constraints.turnover_limit is not None:
        slacks.append(
            ConstraintSlack(
                "turnover", float(constraints.turnover_limit - np.abs(weights - previous).sum())
            )
        )
    if constraints.variance_limit is not None:
        slacks.append(ConstraintSlack("variance", float(constraints.variance_limit - variance)))
    if request.capacities:
        for alpha, capacity, weight, previous_weight in zip(
            request.eligible_alphas, request.capacities, weights, previous, strict=True
        ):
            position_ceiling = min(capacity.max_position_notional, capacity.stressed_capacity)
            slacks.extend(
                (
                    ConstraintSlack(
                        f"capacity_position:{alpha.alpha_id}",
                        float(position_ceiling - request.capital * weight),
                    ),
                    ConstraintSlack(
                        f"capacity_trade:{alpha.alpha_id}",
                        float(
                            capacity.max_trade_notional
                            - request.capital * abs(weight - previous_weight)
                        ),
                    ),
                )
            )
    return tuple(slacks)


def optimize_portfolio(request: OptimizationInput) -> OptimizationResult:
    """Optimize selected Alpha sleeves or return explicit INFEASIBLE diagnostics."""
    if cp is None:
        return _infeasible(
            _diagnostic("CVXPY_UNAVAILABLE", "CVXPY is required to optimize target weights.")
        )
    diagnostics = _request_diagnostics(request)
    if diagnostics:
        return _infeasible(*diagnostics)

    alphas = request.eligible_alphas
    alpha_ids = tuple(alpha.alpha_id for alpha in alphas)
    count = len(alphas)
    previous = _vector(request.previous_weights or (0.0,) * count, "previous_weights")
    covariance = project_psd(request.covariance)
    expected_returns = np.asarray([alpha.expected_return for alpha in alphas], dtype=float)
    uncertainty = np.asarray([alpha.uncertainty for alpha in alphas], dtype=float)
    constraints = request.constraints
    weights = cp.Variable(count)
    delta = weights - previous
    risk = cp.quad_form(weights, cp.psd_wrap(covariance))
    cvx_constraints: list[cp.Constraint] = [
        weights >= constraints.minimum_weight,
        weights <= constraints.maximum_weight,
        cp.sum(weights) == _net_exposure(request),
        cp.sum(weights) <= constraints.gross_exposure_limit,
    ]
    if constraints.turnover_limit is not None:
        cvx_constraints.append(cp.norm1(delta) <= constraints.turnover_limit)
    if constraints.variance_limit is not None:
        cvx_constraints.append(risk <= constraints.variance_limit)
    for index, capacity in enumerate(request.capacities):
        position_ceiling = min(capacity.max_position_notional, capacity.stressed_capacity)
        cvx_constraints.extend(
            (
                weights[index] * request.capital <= position_ceiling,
                cp.abs(delta[index]) * request.capital <= capacity.max_trade_notional,
            )
        )

    transaction_cost: cp.Expression | float = 0.0
    if request.cost_aversion and (request.cost_model.linear_rate or request.cost_model.impact_rate):
        traded = cp.Variable(count, nonneg=True)
        cvx_constraints.extend((traded >= delta, traded >= -delta))
        transaction_cost = request.cost_model.linear_rate * cp.sum(traded)
        if request.cost_model.impact_rate:
            excess = cp.Variable(count, nonneg=True)
            cvx_constraints.append(excess >= traded - request.cost_model.impact_breakpoint)
            transaction_cost += request.cost_model.impact_rate * cp.sum_squares(excess)

    objective = cp.Maximize(
        expected_returns @ weights
        - request.risk_aversion * risk
        - request.cost_aversion * transaction_cost
        - request.uncertainty_aversion * (uncertainty @ weights)
    )
    problem = cp.Problem(objective, cvx_constraints)
    try:
        problem.solve()
    except cp.SolverError as error:
        return _infeasible(
            _diagnostic("SOLVER_ERROR", "CVXPY could not solve the portfolio.", error=str(error))
        )
    if problem.status != cp.OPTIMAL or weights.value is None:
        status = str(problem.status or "unknown").upper()
        return _infeasible(
            _diagnostic(
                "SOLVER_INFEASIBLE", "Portfolio constraints are infeasible.", solver_status=status
            )
        )

    solved = np.asarray(weights.value, dtype=float).reshape(-1)
    if not np.isfinite(solved).all() or not np.isclose(
        solved.sum(), _net_exposure(request), atol=1e-6
    ):
        return _infeasible(
            _diagnostic("SOLVER_INVALID_RESULT", "CVXPY returned invalid target weights.")
        )
    active_count = int(np.count_nonzero(solved >= constraints.minimum_weight / 2.0))
    if active_count < max(2, constraints.minimum_alpha_count):
        return _infeasible(
            _diagnostic(
                "MINIMUM_ALPHA_COUNT_UNMET",
                "The solved portfolio does not contain the required number of active alphas.",
                active_alpha_count=active_count,
            )
        )

    variance = float(solved @ covariance @ solved)
    cost = estimate_transaction_cost(
        solved.tolist(), previous.tolist(), request.capital, request.cost_model
    )
    return OptimizationResult(
        status=OptimizationStatus.OPTIMAL,
        target_weights=tuple(
            TargetWeight(alpha_id, float(weight))
            for alpha_id, weight in zip(alpha_ids, solved, strict=True)
        ),
        portfolio_variance=variance,
        estimated_cost=cost,
        risk_contributions=_risk_contributions(alpha_ids, solved, covariance, variance),
        constraint_slacks=_constraint_slacks(request, solved, previous, variance),
    )
