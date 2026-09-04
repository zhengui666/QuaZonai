"""Pure Alpha signal contracts with no trading-runtime capability."""

from __future__ import annotations

import math
from datetime import UTC, datetime, timedelta
from typing import Any, Protocol, SupportsFloat, SupportsIndex, cast, runtime_checkable

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator


def _pyarrow() -> Any:
    try:
        import pyarrow
    except ModuleNotFoundError as exc:
        raise RuntimeError("pyarrow is required for Arrow Alpha signals") from exc
    return pyarrow


@runtime_checkable
class AlphaContext(Protocol):
    """Read-only context supplied to an Alpha model."""


@runtime_checkable
class MarketBar(Protocol):
    """Minimal point-in-time market input available to an Alpha model."""

    event_time: datetime
    available_time: datetime
    instrument_id: str


class AlphaPoint(BaseModel):
    """One finite, point-in-time-valid Alpha prediction."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    event_time: datetime
    available_time: datetime
    instrument_id: str = Field(min_length=1, max_length=200)
    score: float
    expected_return: float | None = None
    uncertainty: float | None = Field(default=None, ge=0)
    horizon_ns: int = Field(gt=0, le=2**63 - 1)

    @field_validator("event_time", "available_time")
    @classmethod
    def require_utc(cls, value: datetime) -> datetime:
        if value.tzinfo is None or value.utcoffset() != timedelta(0):
            raise ValueError("timestamps must be UTC")
        return value.astimezone(UTC)

    @field_validator("instrument_id")
    @classmethod
    def require_instrument_id(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("instrument_id must not be blank")
        return value

    @field_validator("score", "expected_return", "uncertainty", mode="before")
    @classmethod
    def reject_boolean_or_nonfinite(cls, value: object) -> object:
        if isinstance(value, bool):
            raise ValueError("numeric values must not be boolean")
        if value is not None:
            try:
                number = float(cast(str | SupportsFloat | SupportsIndex, value))
            except (TypeError, ValueError, OverflowError):
                if isinstance(value, str):
                    raise
            else:
                if not math.isfinite(number):
                    raise ValueError("numeric values must be finite")
        return value

    @field_validator("horizon_ns", mode="before")
    @classmethod
    def reject_boolean_horizon(cls, value: object) -> object:
        if isinstance(value, bool):
            raise ValueError("horizon_ns must not be boolean")
        return value

    @field_validator("score", "expected_return", "uncertainty")
    @classmethod
    def require_finite(cls, value: float | None) -> float | None:
        if value is not None and not math.isfinite(value):
            raise ValueError("numeric values must be finite")
        return value

    @model_validator(mode="after")
    def require_point_in_time_order(self) -> AlphaPoint:
        if self.available_time < self.event_time:
            raise ValueError("available_time must be at or after event_time")
        return self


class AlphaSignalFrame(BaseModel):
    """A validated collection of Alpha predictions."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    points: tuple[AlphaPoint, ...] = Field(default_factory=tuple)

    @model_validator(mode="after")
    def require_unique_event_instrument(self) -> AlphaSignalFrame:
        keys = {(point.event_time, point.instrument_id) for point in self.points}
        if len(keys) != len(self.points):
            raise ValueError("(event_time, instrument_id) pairs must be unique")
        return self

    @staticmethod
    def arrow_schema() -> object:
        """Return the exact V1 Arrow representation on demand."""
        pyarrow = _pyarrow()
        return pyarrow.schema(
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

    def to_arrow(self) -> object:
        """Serialize a validated frame into its V1 Arrow schema."""
        pyarrow = _pyarrow()
        return pyarrow.Table.from_pylist(
            [point.model_dump(mode="python") for point in self.points],
            schema=self.arrow_schema(),
        )

    @classmethod
    def from_arrow(cls, table: object) -> AlphaSignalFrame:
        """Deserialize only the exact V1 Arrow schema, then revalidate every point."""
        pyarrow = _pyarrow()
        if not isinstance(table, pyarrow.Table):
            raise TypeError("AlphaSignalFrame requires a pyarrow.Table")
        if not table.schema.equals(cls.arrow_schema(), check_metadata=True):
            raise ValueError("AlphaSignalFrame Arrow schema does not match V1")
        return cls.model_validate({"points": table.to_pylist()})


AlphaSignalFrameV1 = AlphaSignalFrame


@runtime_checkable
class AlphaModelV1(Protocol):
    """The complete Alpha model surface: produce signals, never orders."""

    def initialize(self, context: AlphaContext) -> None: ...

    def on_bar(self, bar: MarketBar) -> AlphaPoint | None: ...

    def finalize(self) -> list[AlphaPoint]: ...


__all__ = [
    "AlphaContext",
    "AlphaModelV1",
    "AlphaPoint",
    "AlphaSignalFrame",
    "AlphaSignalFrameV1",
    "MarketBar",
]
