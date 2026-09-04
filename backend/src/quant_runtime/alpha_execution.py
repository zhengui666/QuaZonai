"""Minimal, execution-free adapter for AlphaModelV1."""

from __future__ import annotations

from collections.abc import Iterable
from typing import cast

from quant_runtime.alpha_contracts import (
    AlphaContext,
    AlphaModelV1,
    AlphaPoint,
    AlphaSignalFrame,
    MarketBar,
)

_FORBIDDEN_API_PARTS = ("order", "broker", "account", "position")
_REQUIRED_METHODS = ("initialize", "on_bar", "finalize")


def validate_alpha_model(model: object) -> AlphaModelV1:
    """Fail closed when an Alpha exposes execution or account capability."""
    forbidden = sorted(
        name
        for name in dir(model)
        if not name.startswith("_")
        and any(part in name.casefold() for part in _FORBIDDEN_API_PARTS)
    )
    if forbidden:
        raise ValueError("Alpha models may not expose execution APIs: " + ", ".join(forbidden))
    missing = [name for name in _REQUIRED_METHODS if not callable(getattr(model, name, None))]
    if missing:
        raise TypeError("AlphaModelV1 is missing methods: " + ", ".join(missing))
    return cast(AlphaModelV1, model)


def run_alpha(
    model: AlphaModelV1,
    context: AlphaContext,
    bars: Iterable[MarketBar],
) -> AlphaSignalFrame:
    """Run a pure Alpha and validate its complete output frame."""
    checked_model = validate_alpha_model(model)
    checked_model.initialize(context)
    points: list[AlphaPoint] = []
    for bar in bars:
        point = checked_model.on_bar(bar)
        if point is not None:
            points.append(AlphaPoint.model_validate(point))
    final_points = checked_model.finalize()
    if not isinstance(final_points, list):
        raise TypeError("AlphaModelV1.finalize must return a list")
    points.extend(AlphaPoint.model_validate(point) for point in final_points)
    return AlphaSignalFrame(points=tuple(points))


__all__ = ["run_alpha", "validate_alpha_model"]
