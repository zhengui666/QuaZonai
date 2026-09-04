"""Small, deterministic Alpha evaluation primitives.

The trusted runtime supplies realized returns.  Missing evidence stays unavailable;
it is never converted to a favorable zero.
"""

from __future__ import annotations

from dataclasses import dataclass
from math import isfinite, sqrt
from statistics import fmean, stdev
from typing import Iterable, Sequence


@dataclass(frozen=True)
class Metric:
    value: float | None
    status: str


@dataclass(frozen=True)
class AlphaEvaluation:
    observation_count: int
    coverage: Metric
    ic_mean: Metric
    rank_ic_mean: Metric
    hit_rate: Metric
    net_return: Metric
    annualized_volatility: Metric
    sharpe_ratio: Metric
    max_drawdown: Metric
    trial_adjusted_sharpe: Metric


@dataclass(frozen=True)
class WalkForwardFold:
    train_indices: tuple[int, ...]
    test_indices: tuple[int, ...]


def _available(value: float | None) -> Metric:
    return Metric(value=value, status="AVAILABLE" if value is not None else "NOT_AVAILABLE")


def _pearson(left: Sequence[float], right: Sequence[float]) -> float | None:
    if len(left) < 2 or len(left) != len(right):
        return None
    left_mean, right_mean = fmean(left), fmean(right)
    numerator = sum(
        (x - left_mean) * (y - right_mean) for x, y in zip(left, right, strict=True)
    )
    left_scale = sum((x - left_mean) ** 2 for x in left)
    right_scale = sum((y - right_mean) ** 2 for y in right)
    if left_scale == 0 or right_scale == 0:
        return None
    return numerator / sqrt(left_scale * right_scale)


def _ranks(values: Sequence[float]) -> list[float]:
    ordered = sorted(enumerate(values), key=lambda pair: pair[1])
    ranks = [0.0] * len(values)
    cursor = 0
    while cursor < len(ordered):
        end = cursor + 1
        while end < len(ordered) and ordered[end][1] == ordered[cursor][1]:
            end += 1
        rank = (cursor + 1 + end) / 2
        for index, _ in ordered[cursor:end]:
            ranks[index] = rank
        cursor = end
    return ranks


def _max_drawdown(returns: Sequence[float]) -> float | None:
    if not returns:
        return None
    equity = peak = 1.0
    drawdown = 0.0
    for item in returns:
        equity *= 1 + item
        peak = max(peak, equity)
        drawdown = min(drawdown, equity / peak - 1)
    return drawdown


def evaluate_alpha(
    scores: Iterable[float],
    realized_returns: Iterable[float | None],
    *,
    annualization_factor: float,
    trial_count: int,
) -> AlphaEvaluation:
    """Compute honest metrics for paired score/realized-return observations."""
    if not isfinite(annualization_factor) or annualization_factor <= 0:
        raise ValueError("annualization_factor must be finite and positive")
    if trial_count < 1:
        raise ValueError("trial_count must be positive")

    score_rows = tuple(scores)
    return_rows = tuple(realized_returns)
    if len(score_rows) != len(return_rows):
        raise ValueError("scores and realized_returns must have equal length")
    if any(not isfinite(score) for score in score_rows):
        raise ValueError("scores must be finite")
    observed = [
        (score, value)
        for score, value in zip(score_rows, return_rows, strict=True)
        if value is not None
    ]
    if any(not isfinite(value) for _, value in observed):
        raise ValueError("realized returns must be finite when present")
    paired_scores = [score for score, _ in observed]
    paired_returns = [value for _, value in observed]
    coverage = len(observed) / len(score_rows) if score_rows else None
    ic = _pearson(paired_scores, paired_returns)
    rank_ic = _pearson(_ranks(paired_scores), _ranks(paired_returns)) if observed else None
    hit_rate = (
        sum(score * value > 0 for score, value in observed) / len(observed) if observed else None
    )
    net_return = sum(paired_returns) if observed else None
    volatility = stdev(paired_returns) * sqrt(annualization_factor) if len(observed) >= 2 else None
    sharpe = (
        fmean(paired_returns) / stdev(paired_returns) * sqrt(annualization_factor)
        if len(observed) >= 2 and stdev(paired_returns) > 0
        else None
    )
    return AlphaEvaluation(
        observation_count=len(observed),
        coverage=_available(coverage),
        ic_mean=_available(ic),
        rank_ic_mean=_available(rank_ic),
        hit_rate=_available(hit_rate),
        net_return=_available(net_return),
        annualized_volatility=_available(volatility),
        sharpe_ratio=_available(sharpe),
        max_drawdown=_available(_max_drawdown(paired_returns)),
        trial_adjusted_sharpe=_available(sharpe / sqrt(trial_count) if sharpe is not None else None),
    )


def purged_walk_forward(
    *,
    observation_count: int,
    train_size: int,
    test_size: int,
    purge_size: int,
    embargo_size: int,
) -> tuple[WalkForwardFold, ...]:
    """Build finite, index-based folds for pre-sorted point-in-time observations."""
    if observation_count < 1 or train_size < 1 or test_size < 1 or purge_size < 0 or embargo_size < 0:
        raise ValueError("invalid walk-forward sizes")
    folds: list[WalkForwardFold] = []
    test_start = train_size + purge_size
    while test_start + test_size <= observation_count:
        folds.append(
            WalkForwardFold(
                train_indices=tuple(range(0, test_start - purge_size)),
                test_indices=tuple(range(test_start, test_start + test_size)),
            )
        )
        test_start += test_size + embargo_size
    return tuple(folds)


def benjamini_hochberg(p_values: Iterable[float], *, false_discovery_rate: float) -> tuple[bool, ...]:
    """Return per-input BH-FDR acceptance decisions without inventing a family winner."""
    values = tuple(p_values)
    if not 0 < false_discovery_rate < 1:
        raise ValueError("false_discovery_rate must be in (0, 1)")
    if any(not isfinite(value) or not 0 <= value <= 1 for value in values):
        raise ValueError("p_values must be finite values in [0, 1]")
    accepted = [False] * len(values)
    cutoff = -1
    for rank, (_, value) in enumerate(sorted(enumerate(values), key=lambda item: item[1]), start=1):
        if value <= false_discovery_rate * rank / len(values):
            cutoff = rank
    if cutoff < 0:
        return tuple(accepted)
    for rank, (index, _) in enumerate(sorted(enumerate(values), key=lambda item: item[1]), start=1):
        if rank <= cutoff:
            accepted[index] = True
    return tuple(accepted)
