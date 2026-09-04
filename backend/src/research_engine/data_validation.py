"""Fail-closed dataset quality and point-in-time validation."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum
from math import isfinite
from typing import Iterable


class DatasetPartition(StrEnum):
    DISCOVERY = "DISCOVERY"
    VALIDATION = "VALIDATION"
    SEALED = "SEALED"
    FORWARD = "FORWARD"


class DatasetClass(StrEnum):
    SYNTHETIC = "SYNTHETIC"
    FIXTURE = "FIXTURE"
    VENDOR = "VENDOR"
    PRODUCTION = "PRODUCTION"


class ShardState(StrEnum):
    AVAILABLE = "AVAILABLE"
    MISSING = "MISSING"
    PROBE_ERROR = "PROBE_ERROR"


@dataclass(frozen=True)
class MarketObservation:
    event_time: datetime
    available_time: datetime
    instrument_id: str


@dataclass(frozen=True)
class SourceShard:
    state: ShardState


@dataclass(frozen=True)
class DatasetValidationResult:
    quality_state: str
    point_in_time_state: str
    promotability: str
    reason_codes: tuple[str, ...]
    row_count: int
    coverage_ratio: float
    duplicate_count: int
    out_of_order_count: int
    invalid_timestamp_count: int

    @property
    def is_valid(self) -> bool:
        return self.quality_state == "VALID" and self.point_in_time_state == "VALID"


def validate_dataset(
    *,
    partition: DatasetPartition,
    data_class: DatasetClass,
    observations: Iterable[MarketObservation],
    source_shards: Iterable[SourceShard],
    minimum_coverage: float = 1.0,
) -> DatasetValidationResult:
    """Return data-quality evidence; never classify a dataset failure as Alpha failure."""
    if not isfinite(minimum_coverage) or not 0 < minimum_coverage <= 1:
        raise ValueError("minimum_coverage must be finite and in (0, 1]")
    if not isinstance(partition, DatasetPartition):
        raise ValueError("partition must be a DatasetPartition")

    rows = tuple(observations)
    shards = tuple(source_shards)
    reason_codes: list[str] = []
    seen: set[tuple[datetime, str]] = set()
    last_event_time: dict[str, datetime] = {}
    duplicates = out_of_order = invalid_timestamps = 0
    pit_invalid = False

    for row in rows:
        if row.event_time.tzinfo is None or row.available_time.tzinfo is None:
            invalid_timestamps += 1
            continue
        if row.event_time > row.available_time:
            pit_invalid = True
        key = (row.event_time, row.instrument_id)
        if key in seen:
            duplicates += 1
        seen.add(key)
        previous = last_event_time.get(row.instrument_id)
        if previous is not None and row.event_time < previous:
            out_of_order += 1
        last_event_time[row.instrument_id] = row.event_time

    available_shards = sum(shard.state is ShardState.AVAILABLE for shard in shards)
    coverage = available_shards / len(shards) if shards else 0.0
    if not rows:
        reason_codes.append("DATA_EMPTY")
    if any(shard.state is ShardState.MISSING for shard in shards):
        reason_codes.append("DATA_SHARD_MISSING")
    if any(shard.state is ShardState.PROBE_ERROR for shard in shards):
        reason_codes.append("DATA_SOURCE_PROBE_ERROR")
    if coverage < minimum_coverage:
        reason_codes.append("DATA_COVERAGE_INSUFFICIENT")
    if duplicates:
        reason_codes.append("DATA_DUPLICATE_TIMESTAMP")
    if out_of_order:
        reason_codes.append("DATA_OUT_OF_ORDER")
    if invalid_timestamps:
        reason_codes.append("DATA_TIMESTAMP_INVALID")
    if pit_invalid:
        reason_codes.append("POINT_IN_TIME_VIOLATION")
    if data_class in {DatasetClass.SYNTHETIC, DatasetClass.FIXTURE}:
        reason_codes.append("DATA_CLASS_NON_PROMOTABLE")

    quality_valid = not any(
        code.startswith("DATA_") and code != "DATA_CLASS_NON_PROMOTABLE" for code in reason_codes
    )
    pit_valid = not pit_invalid and not invalid_timestamps
    promotability = "PROMOTABLE" if quality_valid and pit_valid and data_class in {
        DatasetClass.VENDOR,
        DatasetClass.PRODUCTION,
    } else "NON_PROMOTABLE"
    return DatasetValidationResult(
        quality_state="VALID" if quality_valid else "INVALID",
        point_in_time_state="VALID" if pit_valid else "INVALID",
        promotability=promotability,
        reason_codes=tuple(reason_codes),
        row_count=len(rows),
        coverage_ratio=coverage,
        duplicate_count=duplicates,
        out_of_order_count=out_of_order,
        invalid_timestamp_count=invalid_timestamps,
    )
