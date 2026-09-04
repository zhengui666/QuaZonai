from datetime import UTC, datetime, timedelta

from research_engine.data_validation import (
    DatasetClass,
    DatasetPartition,
    MarketObservation,
    ShardState,
    SourceShard,
    validate_dataset,
)


def test_vendor_data_with_valid_pit_is_promotable() -> None:
    now = datetime(2026, 9, 3, tzinfo=UTC)
    result = validate_dataset(
        partition=DatasetPartition.DISCOVERY,
        data_class=DatasetClass.VENDOR,
        observations=[MarketObservation(now, now, "BTC-USD")],
        source_shards=[SourceShard(ShardState.AVAILABLE)],
    )

    assert result.is_valid
    assert result.promotability == "PROMOTABLE"
    assert result.reason_codes == ()


def test_quality_and_pit_failures_are_explicit_and_never_alpha_failures() -> None:
    now = datetime(2026, 9, 3, tzinfo=UTC)
    result = validate_dataset(
        partition=DatasetPartition.SEALED,
        data_class=DatasetClass.FIXTURE,
        observations=[
            MarketObservation(now, now - timedelta(seconds=1), "BTC-USD"),
            MarketObservation(now, now, "BTC-USD"),
            MarketObservation(now - timedelta(seconds=1), now, "BTC-USD"),
        ],
        source_shards=[SourceShard(ShardState.AVAILABLE), SourceShard(ShardState.PROBE_ERROR)],
    )

    assert not result.is_valid
    assert result.promotability == "NON_PROMOTABLE"
    assert {
        "POINT_IN_TIME_VIOLATION",
        "DATA_DUPLICATE_TIMESTAMP",
        "DATA_OUT_OF_ORDER",
        "DATA_SOURCE_PROBE_ERROR",
        "DATA_COVERAGE_INSUFFICIENT",
        "DATA_CLASS_NON_PROMOTABLE",
    } <= set(result.reason_codes)
    assert all("ALPHA" not in code for code in result.reason_codes)
