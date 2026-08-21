"""Deterministic multi-objective result selection."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Literal

from quantfoundry.errors import QfError

Direction = Literal["minimize", "maximize"]


@dataclass(frozen=True, slots=True)
class TrialPoint:
    trial_no: int
    values: tuple[float, ...]


def _validate(points: list[TrialPoint], directions: tuple[Direction, ...]) -> None:
    if not points:
        raise QfError("OPTIMIZATION_FAILED", "Optimization produced no completed trials.", 422)
    if len(directions) not in {2, 3}:
        raise QfError("OPTIMIZATION_FAILED", "Optimization requires 2 or 3 objectives.", 422)
    for point in points:
        if len(point.values) != len(directions):
            raise QfError(
                "OPTIMIZATION_FAILED",
                "Trial objective count does not match objective directions.",
                422,
                {"trial_no": point.trial_no},
            )
        if any(not math.isfinite(value) for value in point.values):
            raise QfError(
                "OPTIMIZATION_FAILED",
                "Trial objectives must be finite numbers.",
                422,
                {"trial_no": point.trial_no},
            )


def dominates(
    left: TrialPoint,
    right: TrialPoint,
    directions: tuple[Direction, ...],
) -> bool:
    no_worse = True
    strictly_better = False
    for left_value, right_value, direction in zip(
        left.values, right.values, directions, strict=True
    ):
        if direction == "maximize":
            no_worse = no_worse and left_value >= right_value
            strictly_better = strictly_better or left_value > right_value
        else:
            no_worse = no_worse and left_value <= right_value
            strictly_better = strictly_better or left_value < right_value
    return no_worse and strictly_better


def pareto_front(
    points: list[TrialPoint],
    directions: tuple[Direction, ...],
) -> list[TrialPoint]:
    _validate(points, directions)
    front = [
        point
        for point in points
        if not any(
            other.trial_no != point.trial_no and dominates(other, point, directions)
            for other in points
        )
    ]
    return sorted(front, key=lambda point: point.trial_no)


def select_compromise(
    points: list[TrialPoint],
    directions: tuple[Direction, ...],
) -> TrialPoint:
    front = pareto_front(points, directions)
    transformed = [
        tuple(
            value if direction == "maximize" else -value
            for value, direction in zip(point.values, directions, strict=True)
        )
        for point in front
    ]
    minima = [min(values[index] for values in transformed) for index in range(len(directions))]
    maxima = [max(values[index] for values in transformed) for index in range(len(directions))]

    ranked: list[tuple[float, int, TrialPoint]] = []
    for point, values in zip(front, transformed, strict=True):
        normalized: list[float] = []
        for index, value in enumerate(values):
            low = minima[index]
            high = maxima[index]
            normalized.append(1.0 if high == low else (value - low) / (high - low))
        distance = math.sqrt(sum((1.0 - value) ** 2 for value in normalized))
        ranked.append((distance, point.trial_no, point))
    return min(ranked, key=lambda item: (item[0], item[1]))[2]
