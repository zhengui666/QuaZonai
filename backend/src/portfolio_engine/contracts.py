"""Typed, execution-free contracts for deterministic portfolio construction."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum


class OptimizationStatus(StrEnum):
    OPTIMAL = "OPTIMAL"
    INFEASIBLE = "INFEASIBLE"


@dataclass(frozen=True, slots=True)
class EligibleAlpha:
    """A pre-qualified alpha selected by the Portfolio Architect."""

    alpha_id: str
    expected_return: float
    uncertainty: float
    role: str = "PRIMARY_ALPHA"
    eligible: bool = True


@dataclass(frozen=True, slots=True)
class PortfolioConstraints:
    """Long-only mandate limits for the selected alpha set."""

    minimum_alpha_count: int = 2
    minimum_weight: float = 0.01
    maximum_weight: float = 1.0
    gross_exposure_limit: float = 1.0
    cash_reserve: float = 0.0
    net_exposure_target: float | None = None
    turnover_limit: float | None = None
    variance_limit: float | None = None


@dataclass(frozen=True, slots=True)
class CostModel:
    """Rates applied to traded notional; impact starts above the breakpoint."""

    commission_rate: float = 0.0
    half_spread_rate: float = 0.0
    slippage_rate: float = 0.0
    impact_rate: float = 0.0
    impact_breakpoint: float = 0.0

    @property
    def linear_rate(self) -> float:
        return self.commission_rate + self.half_spread_rate + self.slippage_rate


@dataclass(frozen=True, slots=True)
class CostEstimate:
    traded_notional: float
    commission: float
    half_spread: float
    slippage: float
    impact: float
    total: float


@dataclass(frozen=True, slots=True)
class CapacityInput:
    average_daily_volume: float
    half_spread_rate: float
    volatility: float
    holding_horizon_days: float
    turnover: float
    deployable_capital: float
    max_participation_rate: float
    stress_multiplier: float = 1.0


@dataclass(frozen=True, slots=True)
class CapacityEstimate:
    max_trade_notional: float
    max_position_notional: float
    max_participation_rate: float
    days_to_liquidate: float
    stressed_capacity: float


@dataclass(frozen=True, slots=True)
class CovarianceEstimate:
    covariance: tuple[tuple[float, ...], ...]
    observations: int
    decay: float
    shrinkage: float


@dataclass(frozen=True, slots=True)
class OptimizationInput:
    """Inputs for alpha-sleeve weights. It intentionally contains no order fields."""

    eligible_alphas: tuple[EligibleAlpha, ...]
    covariance: tuple[tuple[float, ...], ...]
    capital: float
    previous_weights: tuple[float, ...] = ()
    constraints: PortfolioConstraints = field(default_factory=PortfolioConstraints)
    cost_model: CostModel = field(default_factory=CostModel)
    capacities: tuple[CapacityEstimate, ...] = ()
    risk_aversion: float = 1.0
    cost_aversion: float = 1.0
    uncertainty_aversion: float = 1.0


@dataclass(frozen=True, slots=True)
class TargetWeight:
    alpha_id: str
    target_weight: float


@dataclass(frozen=True, slots=True)
class Diagnostic:
    code: str
    message: str
    context: tuple[tuple[str, str | int | float], ...] = ()


@dataclass(frozen=True, slots=True)
class RiskContribution:
    alpha_id: str
    fraction: float


@dataclass(frozen=True, slots=True)
class ConstraintSlack:
    name: str
    value: float


@dataclass(frozen=True, slots=True)
class OptimizationResult:
    status: OptimizationStatus
    target_weights: tuple[TargetWeight, ...] = ()
    diagnostics: tuple[Diagnostic, ...] = ()
    portfolio_variance: float | None = None
    estimated_cost: CostEstimate | None = None
    risk_contributions: tuple[RiskContribution, ...] = ()
    constraint_slacks: tuple[ConstraintSlack, ...] = ()
