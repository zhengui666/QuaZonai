from datetime import UTC, datetime

import pytest
import pmxt_archive_plugin as plugin_module

from pmxt_archive_plugin import (
    PMXTArchivePlugin,
    _kalshi_quotes,
    _polymarket_quotes,
    validate_archive_url,
)


def test_descriptor_is_historical_import_only() -> None:
    descriptor = PMXTArchivePlugin.descriptor()

    assert descriptor.plugin_id == "pmxt_archive"
    assert descriptor.version == "1.1.2"
    assert descriptor.capabilities == {"HISTORICAL_IMPORT"}
    assert descriptor.secret_config_schema["properties"] == {}


def test_validate_archive_url_requires_fixed_venue_host_and_path() -> None:
    parsed = validate_archive_url(
        "kalshi",
        "https://r2kalshi.pmxt.dev/kalshi_orderbook_2026-06-11T03.parquet",
    )
    assert parsed.hostname == "r2kalshi.pmxt.dev"

    with pytest.raises(ValueError, match="fixed direct PMXT Archive"):
        validate_archive_url(
            "kalshi",
            "https://example.com/kalshi_orderbook_2026-06-11T03.parquet",
        )


def test_kalshi_quotes_use_received_time_when_event_time_is_missing() -> None:
    received = datetime(2026, 6, 11, 3, 31, tzinfo=UTC)
    quotes, stats = _kalshi_quotes(
        [
            {
                "timestamp_received": received,
                "timestamp": None,
                "yes_bids": [{"1": "0.45", "2": "200"}],
                "no_bids": [{"1": "0.35", "2": "100"}],
            }
        ]
    )

    assert quotes[0]["bid_price"] == 0.45
    assert quotes[0]["ask_price"] == 0.65
    assert quotes[0]["bid_size"] == 200.0
    assert stats["event_timestamp_fallback_count"] == 1


def test_polymarket_quotes_keep_top_of_book_sizes() -> None:
    timestamp = datetime(2026, 8, 10, 0, 1, tzinfo=UTC)
    quotes, stats = _polymarket_quotes(
        [
            {
                "timestamp_received": timestamp,
                "timestamp": timestamp,
                "bids": '[["0.42", "12"]]',
                "asks": '[["0.58", "8"]]',
                "best_bid": None,
                "best_ask": None,
                "size": None,
            }
        ]
    )

    assert quotes[0]["bid_price"] == 0.42
    assert quotes[0]["ask_price"] == 0.58
    assert quotes[0]["bid_size"] == 12.0
    assert quotes[0]["ask_size"] == 8.0
    assert stats["skipped_rows"] == 0


def test_plugin_rejects_provider_secrets() -> None:
    with pytest.raises(ValueError, match="does not accept provider secrets"):
        PMXTArchivePlugin().build_catalog_importer(
            {
                "venue": "polymarket_v2",
                "archive_url": "https://r2v2.pmxt.dev/polymarket_orderbook_2026-08-10T00.parquet",
                "instrument": "asset-id",
            },
            {"api_key": "should-not-be-used"},
        )


def test_all_market_config_generates_a_bounded_manifest_range() -> None:
    importer = PMXTArchivePlugin().build_catalog_importer(
        {
            "venue": "polymarket_v2",
            "selection": "all_markets",
            "archive_start": "2026-04-13T19:00:00Z",
            "archive_end": "2026-04-13T21:00:00Z",
        }
    )

    assert importer.config["selection"] == "all_markets"
    assert importer.config["archive_start"].startswith("2026-04-13T19:00:00")
    assert importer.config["archive_end"].startswith("2026-04-13T21:00:00")


def test_all_market_config_rejects_instrument_and_url() -> None:
    with pytest.raises(ValueError, match="cannot include an instrument"):
        PMXTArchivePlugin().build_catalog_importer(
            {
                "venue": "polymarket_v2",
                "selection": "all_markets",
                "archive_start": "2026-04-13T19:00:00Z",
                "archive_end": "2026-04-13T21:00:00Z",
                "instrument": "asset-id",
            }
        )


def test_instrument_history_import_validates_manifest_selected_shards(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path,
) -> None:
    config = {
        "venue": "polymarket_v2",
        "selection": "instrument_history",
        "instrument": "asset-id",
        "instrument_symbol": "PMXT-ASSET",
        "archive_start": "2026-08-10T00:00:00Z",
        "archive_end": "2026-08-10T02:00:00Z",
    }
    importer = PMXTArchivePlugin().build_catalog_importer(config)
    captured: dict[str, object] = {}

    def fake_download(url: str, destination, *, max_bytes: int) -> None:
        captured.setdefault("urls", []).append(url)
        destination.write_bytes(b"test")

    monkeypatch.setattr(plugin_module, "_download", fake_download)
    monkeypatch.setattr(importer, "_read_rows", lambda path: [])
    monkeypatch.setattr(
        importer,
        "_catalog_from_rows",
        lambda **kwargs: captured.update(kwargs) or {"ok": True},
    )
    shards = [
        {
            "shard_key": "2026-08-10T00:00:00Z",
            "source_url": "https://r2v2.pmxt.dev/polymarket_orderbook_2026-08-10T00.parquet",
            "coverage_start": "2026-08-10T00:00:00+00:00",
            "coverage_end": "2026-08-10T01:00:00+00:00",
            "size_bytes": 10,
            "state": "AVAILABLE",
            "observed_at": "2026-08-31T00:00:00+00:00",
        },
        {
            "shard_key": "2026-08-10T01:00:00Z",
            "source_url": "https://r2v2.pmxt.dev/polymarket_orderbook_2026-08-10T01.parquet",
            "coverage_start": "2026-08-10T01:00:00+00:00",
            "coverage_end": "2026-08-10T02:00:00+00:00",
            "size_bytes": 11,
            "state": "AVAILABLE",
            "observed_at": "2026-08-31T00:00:00+00:00",
        },
    ]

    result = importer.import_sources(
        source_shards=shards,
        catalog_path=str(tmp_path),
        instrument_id="asset-id",
        metadata={
            "catalog_uri": "catalog://research-slice",
            "provider": "PMXT Archive",
            "source_license": "public",
            "source_spec": {
                "kind": "plugin",
                "config": config,
                "manifest_uri": "manifest://research-pmxt",
                "shard_keys": [item["shard_key"] for item in shards],
                "materialization": {
                    "source_shard_count": 2,
                    "missing_shard_count": 0,
                },
            },
            "sealed": False,
        },
    )

    assert result == {"ok": True}
    assert captured["urls"] == [item["source_url"] for item in shards]
