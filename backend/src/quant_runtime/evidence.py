"""Shared validation and safe persistence helpers for remote run evidence."""

from __future__ import annotations

import math
from typing import Any, cast

from quant_runtime.contracts import RunEvidence


def _finite_number(value: object) -> float | None:
    if isinstance(value, bool):
        return None
    try:
        result = float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None
    return result if math.isfinite(result) else None


def _stat_value(
    stats: dict[str, Any],
    primary: str,
    *fallbacks: tuple[dict[str, Any], str],
) -> float | None:
    if primary in stats:
        return _finite_number(stats[primary])
    for source, key in fallbacks:
        if key in source:
            return _finite_number(source[key])
    return None


def extract_statistics(evidence: RunEvidence) -> tuple[float, float, float, int, int] | None:
    """Return only complete finite runtime metrics; never substitute favorable defaults."""
    stats = evidence.statistics
    raw_returns = stats.get("returns")
    raw_general = stats.get("general")
    returns = raw_returns if isinstance(raw_returns, dict) else {}
    general = raw_general if isinstance(raw_general, dict) else {}
    sharpe = _stat_value(
        stats,
        "sharpe_ratio",
        (returns, "Sharpe Ratio"),
        (returns, "SharpeRatio"),
    )
    drawdown = _stat_value(
        stats,
        "max_drawdown",
        (returns, "Max Drawdown"),
        (returns, "MaxDrawdown"),
    )
    turnover = _stat_value(stats, "turnover", (general, "Turnover"))
    total_orders_value = _stat_value(stats, "total_orders", (general, "Total Orders"))
    total_positions_value = _stat_value(
        stats,
        "total_positions",
        (general, "Total Positions"),
    )
    if (
        sharpe is None
        or drawdown is None
        or turnover is None
        or total_orders_value is None
        or total_positions_value is None
    ):
        return None
    if total_orders_value < 0 or total_positions_value < 0:
        return None
    if int(total_orders_value) != total_orders_value or int(total_positions_value) != total_positions_value:
        return None
    return (
        sharpe,
        abs(drawdown),
        turnover,
        int(total_orders_value),
        int(total_positions_value),
    )


def persistable_evidence(evidence: RunEvidence) -> dict[str, Any]:
    """Persist governed aggregate evidence without Nautilus execution/account reports."""
    payload = evidence.model_dump(mode="json")
    for key in ("orders", "fills", "positions", "account"):
        payload.pop(key, None)
    return cast(dict[str, Any], _without_account_fields(payload))


def _without_account_fields(value: object) -> object:
    if isinstance(value, dict):
        return {
            str(key): _without_account_fields(item)
            for key, item in value.items()
            if str(key).casefold() not in {"account", "account_id"}
            and not str(key).casefold().startswith("account.")
        }
    if isinstance(value, list):
        return [_without_account_fields(item) for item in value]
    return value


def sealed_error_fields(evidence: RunEvidence) -> tuple[str | None, str | None]:
    if evidence.state == "FAILED":
        return "SEALED_RUNTIME_FAILURE", "Sealed runtime failure; disclosure withheld."
    return None, None
