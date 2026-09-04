from __future__ import annotations

from datetime import UTC, datetime, timedelta, timezone
from math import inf, nan

import pytest
from pydantic import ValidationError

from quant_runtime.alpha_contracts import AlphaPoint, AlphaSignalFrame, AlphaSignalFrameV1
from quant_runtime.alpha_execution import run_alpha, validate_alpha_model


def _point(**overrides: object) -> AlphaPoint:
    values: dict[str, object] = {
        "event_time": datetime(2026, 1, 1, tzinfo=UTC),
        "available_time": datetime(2026, 1, 1, tzinfo=UTC),
        "instrument_id": "PMXT-ASSET.PMXT",
        "score": 0.5,
        "expected_return": 0.01,
        "uncertainty": 0.1,
        "horizon_ns": 1_000_000_000,
    }
    values.update(overrides)
    return AlphaPoint.model_validate(values)


@pytest.mark.parametrize(
    ("field", "value"),
    [("score", nan), ("expected_return", inf), ("uncertainty", -inf)],
)
def test_alpha_point_rejects_nonfinite_values(field: str, value: float) -> None:
    with pytest.raises(ValidationError, match="finite"):
        _point(**{field: value})


def test_alpha_point_requires_utc_and_point_in_time_order() -> None:
    with pytest.raises(ValidationError, match="UTC"):
        _point(event_time=datetime(2026, 1, 1))
    with pytest.raises(ValidationError, match="UTC"):
        _point(event_time=datetime(2026, 1, 1, tzinfo=timezone(timedelta(hours=8))))
    with pytest.raises(ValidationError, match="available_time"):
        _point(available_time=datetime(2025, 12, 31, 23, 59, tzinfo=UTC))


def test_signal_frame_requires_unique_event_instrument_pairs() -> None:
    point = _point()
    with pytest.raises(ValidationError, match="unique"):
        AlphaSignalFrame(points=(point, point.model_copy(update={"score": 0.6})))
    assert AlphaSignalFrameV1 is AlphaSignalFrame


def test_signal_frame_arrow_round_trip_uses_v1_schema() -> None:
    pyarrow = pytest.importorskip("pyarrow")
    frame = AlphaSignalFrame(points=(_point(),))
    table = frame.to_arrow()

    assert table.schema == pyarrow.schema(
        [
            pyarrow.field("event_time", pyarrow.timestamp("ns", tz="UTC"), nullable=False),
            pyarrow.field("available_time", pyarrow.timestamp("ns", tz="UTC"), nullable=False),
            pyarrow.field("instrument_id", pyarrow.string(), nullable=False),
            pyarrow.field("score", pyarrow.float64(), nullable=False),
            pyarrow.field("expected_return", pyarrow.float64(), nullable=True),
            pyarrow.field("uncertainty", pyarrow.float64(), nullable=True),
            pyarrow.field("horizon_ns", pyarrow.int64(), nullable=False),
        ]
    )
    assert AlphaSignalFrame.from_arrow(table) == frame
    with pytest.raises(ValueError, match="schema"):
        AlphaSignalFrame.from_arrow(pyarrow.table({"score": [0.5]}))


class _PureAlpha:
    def initialize(self, context: object) -> None:
        self.context = context

    def on_bar(self, bar: object) -> AlphaPoint | None:
        return _point(instrument_id=str(bar))

    def finalize(self) -> list[AlphaPoint]:
        return []


class _OrderCapableAlpha(_PureAlpha):
    def submit_order(self) -> None:
        pass


def test_run_alpha_validates_the_complete_frame_and_rejects_execution_api() -> None:
    frame = run_alpha(_PureAlpha(), object(), ["A", "B"])
    assert [point.instrument_id for point in frame.points] == ["A", "B"]

    with pytest.raises(ValidationError, match="unique"):
        run_alpha(_PureAlpha(), object(), ["A", "A"])
    with pytest.raises(ValueError, match="execution APIs"):
        validate_alpha_model(_OrderCapableAlpha())
