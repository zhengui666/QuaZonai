"""PMXT Archive historical DATA_CONNECTOR plugin."""

from __future__ import annotations

import json
import re
from concurrent.futures import ThreadPoolExecutor
from datetime import UTC, datetime, timedelta
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener, urlopen
from urllib.parse import SplitResult, urlsplit

from plugins.contract import Capability, DescriptorSnapshot
from quant_runtime.contracts import CatalogDescriptor

_MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
_MAX_MATERIALIZATION_ARCHIVE_BYTES = 2 * 1024 * 1024 * 1024
_MAX_MATERIALIZATION_TOTAL_BYTES = 20 * 1024 * 1024 * 1024
_MAX_SOURCE_ROWS = 2_000_000
_MAX_PARQUET_BATCH_ROWS = 16_384
_MAX_DECODED_BATCH_BYTES = 64 * 1024 * 1024
_MAX_DECODED_SOURCE_BYTES = 4 * 1024 * 1024 * 1024
_MAX_MANIFEST_HOURS = 24 * 370
_MAX_MATERIALIZATION_SHARDS = 168
_MANIFEST_SCHEMA_REVISION = "pmxt-archive-manifest-v1"
_VENUE_RULES: dict[str, tuple[str, re.Pattern[str], str]] = {
    "polymarket_v2": (
        "r2v2.pmxt.dev",
        re.compile(r"/polymarket_orderbook_\d{4}-\d{2}-\d{2}T\d{2}\.parquet"),
        "asset_id",
    ),
    "kalshi": (
        "r2kalshi.pmxt.dev",
        re.compile(r"/kalshi_orderbook_\d{4}-\d{2}-\d{2}T\d{2}\.parquet"),
        "market_ticker",
    ),
}
_INSTRUMENT_SYMBOL = re.compile(r"[A-Za-z0-9][-A-Za-z0-9._:]{0,119}")
_KALSHI_TICKER = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,119}")


class _SourceResourceLimit(ValueError):
    pass


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_: Any, **__: Any) -> None:
        return None


def validate_archive_url(venue: str, archive_url: str) -> SplitResult:
    rule = _VENUE_RULES.get(venue)
    if rule is None:
        raise ValueError("venue must be polymarket_v2 or kalshi")
    host, path_pattern, _ = rule
    try:
        parsed = urlsplit(archive_url)
        parsed_hostname = parsed.hostname
        parsed_port = parsed.port
    except ValueError as exc:
        raise ValueError("archive_url is invalid") from exc
    if (
        parsed.scheme != "https"
        or parsed_hostname != host
        or parsed_port is not None
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or path_pattern.fullmatch(parsed.path) is None
    ):
        raise ValueError("archive_url must be a fixed direct PMXT Archive parquet URL")
    return parsed


def _as_float(value: Any) -> float | None:
    if value is None:
        return None
    try:
        number = float(value)
    except (TypeError, ValueError, InvalidOperation):
        return None
    if not 0.0 <= number <= 1.0:
        return None
    return number


def _as_size(value: Any) -> float | None:
    if value is None:
        return None
    try:
        number = float(value)
    except (TypeError, ValueError, InvalidOperation):
        return None
    return number if number >= 0.0 else None


def _timestamp_ns(value: Any) -> int | None:
    if value is None:
        return None
    if isinstance(value, datetime):
        if value.tzinfo is None:
            value = value.replace(tzinfo=UTC)
        return int(value.timestamp() * 1_000_000_000)
    if isinstance(value, int):
        return value
    try:
        parsed = datetime.fromisoformat(str(value).replace("Z", "+00:00"))
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=UTC)
    return int(parsed.timestamp() * 1_000_000_000)


def _top_json_level(value: Any, *, bids: bool) -> tuple[float, float] | None:
    if value is None:
        return None
    if isinstance(value, str):
        try:
            value = json.loads(value)
        except json.JSONDecodeError:
            return None
    if not isinstance(value, list):
        return None
    levels: list[tuple[float, float]] = []
    for level in value:
        if isinstance(level, dict):
            price = _as_float(level.get("price"))
            size = _as_size(level.get("size"))
        elif isinstance(level, (list, tuple)) and len(level) >= 2:
            price = _as_float(level[0])
            size = _as_size(level[1])
        else:
            continue
        if price is not None and size is not None:
            levels.append((price, size))
    if not levels:
        return None
    return max(levels) if bids else min(levels, key=lambda item: item[0])


def _explicitly_empty_book_side(value: Any) -> bool:
    """Return whether a source row explicitly says that a book side is empty."""

    if isinstance(value, str):
        try:
            value = json.loads(value)
        except json.JSONDecodeError:
            return False
    return isinstance(value, list) and not value


def _top_kalshi_level(value: Any) -> tuple[float, float] | None:
    if not isinstance(value, list):
        return None
    levels: list[tuple[float, float]] = []
    for level in value:
        if not isinstance(level, dict):
            continue
        price = _as_float(level.get("1"))
        size = _as_size(level.get("2"))
        if price is not None and size is not None:
            levels.append((price, size))
    return max(levels) if levels else None


def _format_decimal(value: float, places: int = 6) -> str:
    decimal = Decimal(str(value)).quantize(Decimal(1).scaleb(-places))
    return format(decimal, "f")


def _quote_stats() -> dict[str, int]:
    return {
        "event_timestamp_fallback_count": 0,
        "event_after_received_count": 0,
        "skipped_rows": 0,
        "crossed_rows": 0,
        "order_book_state_reset_count": 0,
    }


def _polymarket_quotes(
    rows: list[dict[str, Any]],
    *,
    state: dict[str, tuple[float, float] | None] | None = None,
    stats: dict[str, int] | None = None,
) -> tuple[list[dict[str, Any]], dict[str, int]]:
    state = state if state is not None else {"bid": None, "ask": None}
    quote_rows: list[dict[str, Any]] = []
    stats = stats if stats is not None else _quote_stats()
    for row in rows:
        received_ns = _timestamp_ns(row.get("timestamp_received"))
        if received_ns is None:
            stats["skipped_rows"] += 1
            continue
        event_ns = _timestamp_ns(row.get("timestamp"))
        if event_ns is None:
            event_ns = received_ns
            stats["event_timestamp_fallback_count"] += 1
        elif event_ns > received_ns:
            stats["event_after_received_count"] += 1

        bid = _top_json_level(row.get("bids"), bids=True)
        ask = _top_json_level(row.get("asks"), bids=False)
        if _explicitly_empty_book_side(row.get("bids")):
            state["bid"] = None
        elif bid is not None:
            state["bid"] = bid
        elif row.get("best_bid") is not None:
            best_bid = _as_float(row.get("best_bid"))
            if best_bid is not None:
                state["bid"] = (best_bid, _as_size(row.get("size")) or 0.0)
        if _explicitly_empty_book_side(row.get("asks")):
            state["ask"] = None
        elif ask is not None:
            state["ask"] = ask
        elif row.get("best_ask") is not None:
            best_ask = _as_float(row.get("best_ask"))
            if best_ask is not None:
                state["ask"] = (best_ask, _as_size(row.get("size")) or 0.0)
        if state["bid"] is None or state["ask"] is None:
            stats["skipped_rows"] += 1
            continue
        bid_price, bid_size = state["bid"]
        ask_price, ask_size = state["ask"]
        if bid_price >= ask_price:
            stats["crossed_rows"] += 1
            stats["skipped_rows"] += 1
            continue
        quote_rows.append(
            {
                "event_ns": event_ns,
                "available_ns": received_ns,
                "bid_price": bid_price,
                "ask_price": ask_price,
                "bid_size": bid_size,
                "ask_size": ask_size,
            }
        )
    return quote_rows, stats


def _kalshi_quotes(rows: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], dict[str, int]]:
    quote_rows: list[dict[str, Any]] = []
    stats = _quote_stats()
    for row in rows:
        received_ns = _timestamp_ns(row.get("timestamp_received"))
        if received_ns is None:
            stats["skipped_rows"] += 1
            continue
        event_ns = _timestamp_ns(row.get("timestamp"))
        if event_ns is None:
            event_ns = received_ns
            stats["event_timestamp_fallback_count"] += 1
        elif event_ns > received_ns:
            stats["event_after_received_count"] += 1
        yes_bid = _top_kalshi_level(row.get("yes_bids"))
        no_bid = _top_kalshi_level(row.get("no_bids"))
        if yes_bid is None or no_bid is None:
            stats["skipped_rows"] += 1
            continue
        bid_price, bid_size = yes_bid
        ask_price = 1.0 - no_bid[0]
        ask_size = no_bid[1]
        if bid_price >= ask_price:
            stats["crossed_rows"] += 1
            stats["skipped_rows"] += 1
            continue
        quote_rows.append(
            {
                "event_ns": event_ns,
                "available_ns": received_ns,
                "bid_price": bid_price,
                "ask_price": ask_price,
                "bid_size": bid_size,
                "ask_size": ask_size,
            }
        )
    return quote_rows, stats


def _polymarket_quotes_by_shard(
    shard_rows: list[tuple[datetime, list[dict[str, Any]]]],
) -> tuple[list[dict[str, Any]], dict[str, int]]:
    """Reconstruct each contiguous shard sequence without carrying state across gaps."""

    state: dict[str, tuple[float, float] | None] = {"bid": None, "ask": None}
    stats = _quote_stats()
    quote_rows: list[dict[str, Any]] = []
    previous_end: datetime | None = None
    for shard_start, rows in sorted(shard_rows, key=lambda item: item[0]):
        shard_end = shard_start + timedelta(hours=1)
        if previous_end is not None:
            if shard_start < previous_end:
                raise ValueError("materialization contains overlapping archive shards")
            if shard_start > previous_end:
                state = {"bid": None, "ask": None}
                stats["order_book_state_reset_count"] += 1
        ordered_rows = sorted(
            rows,
            key=lambda row: _timestamp_ns(row.get("timestamp_received")) or 0,
        )
        shard_quotes, _ = _polymarket_quotes(ordered_rows, state=state, stats=stats)
        quote_rows.extend(shard_quotes)
        previous_end = shard_end
    return quote_rows, stats


def _parse_utc_hour(value: Any, field_name: str) -> datetime:
    try:
        parsed = datetime.fromisoformat(str(value).replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValueError(f"{field_name} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=UTC)
    parsed = parsed.astimezone(UTC)
    if parsed.minute or parsed.second or parsed.microsecond:
        raise ValueError(f"{field_name} must be aligned to a UTC hour")
    return parsed


def _archive_url_for(venue: str, hour: datetime) -> str:
    host, _, _ = _VENUE_RULES[venue]
    prefix = "polymarket_orderbook" if venue == "polymarket_v2" else "kalshi_orderbook"
    return f"https://{host}/{prefix}_{hour:%Y-%m-%dT%H}.parquet"


def _probe_archive_url(url: str) -> tuple[str, int | None]:
    opener = build_opener(_NoRedirect)
    request = Request(url, method="HEAD", headers={"User-Agent": "QuaZonai-PMXT-Archive/1.1"})
    try:
        with opener.open(request, timeout=5) as response:
            if response.status != 200:
                return "PROBE_ERROR", None
            content_length = response.headers.get("Content-Length")
            if content_length is None:
                return "PROBE_ERROR", None
            try:
                size = int(content_length)
            except ValueError:
                return "PROBE_ERROR", None
            return ("AVAILABLE", size) if size >= 0 else ("PROBE_ERROR", None)
    except HTTPError as exc:
        return ("MISSING", None) if exc.code == 404 else ("PROBE_ERROR", None)
    except (URLError, TimeoutError, OSError):
        return "PROBE_ERROR", None


def _validate_public_config(config: dict[str, Any]) -> dict[str, Any]:
    allowed = {
        "venue",
        "selection",
        "archive_url",
        "instrument",
        "instrument_symbol",
        "archive_start",
        "archive_end",
    }
    extras = sorted(set(config) - allowed)
    if extras:
        raise ValueError(f"unexpected PMXT config fields: {', '.join(extras)}")
    venue = str(config.get("venue", ""))
    selection = str(config.get("selection", "single_instrument"))
    if selection == "all_markets":
        if any(config.get(key) for key in ("archive_url", "instrument", "instrument_symbol")):
            raise ValueError("all_markets selection cannot include an instrument or archive_url")
        start = _parse_utc_hour(config.get("archive_start"), "archive_start")
        end = _parse_utc_hour(config.get("archive_end"), "archive_end")
        if end <= start:
            raise ValueError("archive_end must be after archive_start")
        if (end - start) > timedelta(hours=_MAX_MANIFEST_HOURS):
            raise ValueError("archive range exceeds the one-year manifest limit")
        return {
            "venue": venue,
            "selection": selection,
            "archive_start": start.isoformat(),
            "archive_end": end.isoformat(),
        }
    if selection == "instrument_history":
        if config.get("archive_url"):
            raise ValueError("instrument_history selection cannot include archive_url")
        start = _parse_utc_hour(config.get("archive_start"), "archive_start")
        end = _parse_utc_hour(config.get("archive_end"), "archive_end")
        if end <= start:
            raise ValueError("archive_end must be after archive_start")
        if (end - start) > timedelta(hours=_MAX_MANIFEST_HOURS):
            raise ValueError("archive range exceeds the one-year materialization limit")
        target = str(config.get("instrument", ""))
        if not target or len(target) > 200:
            raise ValueError("instrument is required")
        if venue == "kalshi" and _KALSHI_TICKER.fullmatch(target) is None:
            raise ValueError("market_ticker contains unsupported characters")
        symbol = str(config.get("instrument_symbol") or f"{venue.upper()}-{target}")
        if _INSTRUMENT_SYMBOL.fullmatch(symbol) is None:
            raise ValueError("instrument_symbol contains unsupported characters")
        return {
            "venue": venue,
            "selection": selection,
            "instrument": target,
            "instrument_symbol": symbol,
            "archive_start": start.isoformat(),
            "archive_end": end.isoformat(),
        }
    if selection != "single_instrument":
        raise ValueError("selection must be single_instrument, instrument_history or all_markets")
    archive_url = str(config.get("archive_url", ""))
    target = str(config.get("instrument", ""))
    if not target or len(target) > 200:
        raise ValueError("instrument is required")
    validate_archive_url(venue, archive_url)
    if venue == "kalshi" and _KALSHI_TICKER.fullmatch(target) is None:
        raise ValueError("market_ticker contains unsupported characters")
    symbol = str(config.get("instrument_symbol") or f"{venue.upper()}-{target}")
    if _INSTRUMENT_SYMBOL.fullmatch(symbol) is None:
        raise ValueError("instrument_symbol contains unsupported characters")
    return {
        "venue": venue,
        "selection": selection,
        "archive_url": archive_url,
        "instrument": target,
        "instrument_symbol": symbol,
    }


def _download(
    url: str,
    destination: Path,
    *,
    max_bytes: int = _MAX_ARCHIVE_BYTES,
    expected_size: int | None = None,
) -> int:
    opener = build_opener(_NoRedirect)
    request = Request(url, headers={"User-Agent": "QuaZonai-PMXT-Archive/1.0"})
    try:
        with opener.open(request, timeout=180) as response:
            if response.status != 200:
                raise ValueError("PMXT Archive file download failed")
            content_length = response.headers.get("Content-Length")
            if content_length is not None:
                try:
                    advertised_size = int(content_length)
                except ValueError as exc:
                    raise ValueError("PMXT Archive file size header is invalid") from exc
                if advertised_size < 0 or advertised_size > max_bytes:
                    raise ValueError("PMXT Archive file exceeds the size limit")
                if expected_size is not None and advertised_size != expected_size:
                    raise ValueError("PMXT Archive file size changed after manifest inspection")
            downloaded = 0
            with destination.open("wb") as output:
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    downloaded += len(chunk)
                    if downloaded > max_bytes:
                        raise ValueError("PMXT Archive file exceeds the size limit")
                    output.write(chunk)
            if expected_size is not None and downloaded != expected_size:
                raise ValueError("PMXT Archive file size changed after manifest inspection")
            return downloaded
    except (HTTPError, URLError) as exc:
        if isinstance(exc, HTTPError) and exc.code in {301, 302, 303, 307, 308}:
            raise ValueError("PMXT Archive redirects are not allowed") from exc
        raise ValueError("PMXT Archive file download failed") from exc


class PMXTArchiveImporter:
    def __init__(self, public_config: dict[str, Any]):
        self.config = _validate_public_config(public_config)
        self._last_decoded_bytes = 0

    def preflight(self) -> None:
        _validate_public_config(self.config)

    def _read_rows(self, path: Path) -> list[dict[str, Any]]:
        import pyarrow as pa
        from pyarrow import dataset

        target_field = _VENUE_RULES[self.config["venue"]][2]
        columns = (
            [
                "timestamp_received",
                "timestamp",
                "asset_id",
                "event_type",
                "bids",
                "asks",
                "price",
                "size",
                "side",
                "best_bid",
                "best_ask",
            ]
            if self.config["venue"] == "polymarket_v2"
            else [
                "timestamp_received",
                "timestamp",
                "market_ticker",
                "event_type",
                "yes_bids",
                "no_bids",
                "price",
                "delta",
                "side",
            ]
        )
        try:
            source = dataset.dataset(path, format="parquet")
            scanner = source.scanner(
                columns=columns,
                filter=dataset.field(target_field) == self.config["instrument"],
                batch_size=_MAX_PARQUET_BATCH_ROWS,
                batch_readahead=1,
                fragment_readahead=1,
                use_threads=False,
                cache_metadata=False,
            )
            rows: list[dict[str, Any]] = []
            decoded_bytes = 0
            source_row_count = 0
            for batch in scanner.to_batches():
                batch_rows = int(batch.num_rows)
                if source_row_count + batch_rows > _MAX_SOURCE_ROWS:
                    raise _SourceResourceLimit("PMXT instrument slice exceeds the row limit")
                batch_bytes = int(batch.nbytes)
                if (
                    batch_bytes > _MAX_DECODED_BATCH_BYTES
                    or decoded_bytes + batch_bytes > _MAX_DECODED_SOURCE_BYTES
                ):
                    raise _SourceResourceLimit(
                        "PMXT instrument slice exceeds the decoded size limit"
                    )
                rows.extend(batch.to_pylist())
                source_row_count += batch_rows
                decoded_bytes += batch_bytes
            self._last_decoded_bytes = decoded_bytes
            return rows
        except _SourceResourceLimit:
            raise
        except (OSError, ValueError, RuntimeError, pa.ArrowException) as exc:
            raise ValueError("PMXT Archive Parquet schema is invalid") from exc

    def _catalog_from_rows(
        self,
        *,
        rows: list[dict[str, Any]],
        catalog_path: str,
        metadata: dict[str, Any],
        quote_result: tuple[list[dict[str, Any]], dict[str, int]] | None = None,
    ) -> dict[str, Any]:
        source_spec = metadata.get("source_spec")
        if not isinstance(source_spec, dict) or source_spec.get("kind") != "plugin":
            raise ValueError("plugin source metadata is invalid")
        catalog_uri = str(metadata.get("catalog_uri", ""))
        provider = str(metadata.get("provider", ""))
        source_license = str(metadata.get("source_license", ""))
        sealed = bool(metadata.get("sealed", False))
        if not catalog_uri.startswith("catalog://") or not provider or not source_license:
            raise ValueError("plugin catalog metadata is incomplete")
        if len(rows) > _MAX_SOURCE_ROWS:
            raise ValueError("PMXT instrument slice exceeds the row limit")
        rows.sort(key=lambda row: _timestamp_ns(row.get("timestamp_received")) or 0)
        if quote_result is not None:
            quote_rows, quality_stats = quote_result
        elif self.config["venue"] == "polymarket_v2":
            quote_rows, quality_stats = _polymarket_quotes(rows)
        else:
            quote_rows, quality_stats = _kalshi_quotes(rows)
        if not quote_rows:
            raise ValueError("PMXT instrument has no valid two-sided quotes")
        quote_rows.sort(key=lambda row: (row["event_ns"], row["available_ns"]))

        from nautilus_trader.model.data import QuoteTick
        from nautilus_trader.model.enums import AssetClass
        from nautilus_trader.model.identifiers import InstrumentId, Symbol, Venue
        from nautilus_trader.model.instruments import BinaryOption
        from nautilus_trader.model.objects import Currency, Price, Quantity
        from nautilus_trader.persistence.catalog import ParquetDataCatalog

        first_event_ns = quote_rows[0]["event_ns"]
        last_event_ns = quote_rows[-1]["event_ns"]
        instrument_id_value = InstrumentId(
            Symbol(self.config["instrument_symbol"]), Venue("PMXT")
        )
        instrument = BinaryOption(
            instrument_id=instrument_id_value,
            raw_symbol=Symbol(self.config["instrument_symbol"]),
            asset_class=AssetClass.ALTERNATIVE,
            currency=Currency.from_str("USD"),
            activation_ns=first_event_ns,
            expiration_ns=max(last_event_ns + 1, first_event_ns + 1),
            price_precision=6,
            size_precision=6,
            price_increment=Price.from_str("0.000001"),
            size_increment=Quantity.from_str("0.000001"),
            ts_event=first_event_ns,
            ts_init=first_event_ns,
            outcome="YES",
            description=f"PMXT Archive {self.config['venue']} {self.config['instrument']}",
        )
        ticks = [
            QuoteTick(
                instrument_id=instrument_id_value,
                bid_price=Price.from_str(_format_decimal(row["bid_price"])),
                ask_price=Price.from_str(_format_decimal(row["ask_price"])),
                bid_size=Quantity.from_str(_format_decimal(row["bid_size"])),
                ask_size=Quantity.from_str(_format_decimal(row["ask_size"])),
                ts_event=row["event_ns"],
                ts_init=row["available_ns"],
            )
            for row in quote_rows
        ]
        staging_path = Path(catalog_path)
        catalog = ParquetDataCatalog(staging_path)
        catalog.write_data([instrument])
        catalog.write_data(ticks)
        first_available_ns = min(row["available_ns"] for row in quote_rows)
        last_available_ns = max(row["available_ns"] for row in quote_rows)
        first_event = datetime.fromtimestamp(first_event_ns / 1_000_000_000, tz=UTC)
        last_event = datetime.fromtimestamp(last_event_ns / 1_000_000_000, tz=UTC)
        materialization = source_spec.get("materialization")
        materialization = materialization if isinstance(materialization, dict) else {}
        descriptor = CatalogDescriptor(
            catalog_uri=catalog_uri,
            provider=provider,
            source_license=source_license,
            source_spec=source_spec,
            nautilus_data_type="QuoteTick",
            instrument_scope=[instrument_id_value.value],
            event_start=first_event,
            event_end=last_event,
            available_start=datetime.fromtimestamp(first_available_ns / 1_000_000_000, tz=UTC),
            available_end=datetime.fromtimestamp(last_available_ns / 1_000_000_000, tz=UTC),
            row_count=len(ticks),
            schema_revision=f"pmxt-archive-{self.config['venue']}-quote-tick-v1",
            quality_result={
                "valid": True,
                "sorted": all(
                    left["event_ns"] <= right["event_ns"]
                    for left, right in zip(quote_rows, quote_rows[1:], strict=False)
                ),
                "unique_timestamps": len({row["event_ns"] for row in quote_rows})
                == len(quote_rows),
                "duplicate_event_timestamp_count": len(quote_rows)
                - len({row["event_ns"] for row in quote_rows}),
                "non_crossed_quotes": quality_stats["crossed_rows"] == 0,
                "source_row_count": len(rows),
                "quote_tick_count": len(ticks),
                "source_shard_count": materialization.get("source_shard_count", 1),
                "missing_shard_count": materialization.get("missing_shard_count", 0),
                "probe_error_count": materialization.get("probe_error_count", 0),
                "range_complete": (
                    materialization.get("missing_shard_count", 0) == 0
                    and materialization.get("probe_error_count", 0) == 0
                ),
                **quality_stats,
            },
            point_in_time_result={
                "valid": True,
                "available_time_preserved": True,
                "available_time_field": "timestamp_received",
            },
            sealed=sealed,
        )
        return {
            "descriptor": descriptor.model_dump(mode="json"),
            "source_row_count": len(rows),
            "quote_tick_count": len(ticks),
        }

    def import_source(
        self,
        *,
        source_url: str,
        catalog_path: str,
        instrument_id: str,
        metadata: dict[str, Any],
    ) -> dict[str, Any]:
        if self.config["selection"] != "single_instrument":
            raise ValueError("manifest PMXT config requires bounded import_sources()")
        if source_url != self.config["archive_url"]:
            raise ValueError("source URL does not match validated PMXT config")
        if instrument_id != self.config["instrument"]:
            raise ValueError("instrument does not match validated PMXT config")
        staging_path = Path(catalog_path)
        download_path = staging_path / "source.parquet"
        try:
            _download(source_url, download_path)
            self._last_decoded_bytes = 0
            rows = self._read_rows(download_path)
        finally:
            download_path.unlink(missing_ok=True)
        return self._catalog_from_rows(rows=rows, catalog_path=catalog_path, metadata=metadata)

    def import_sources(
        self,
        *,
        source_shards: list[dict[str, Any]],
        catalog_path: str,
        instrument_id: str,
        metadata: dict[str, Any],
    ) -> dict[str, Any]:
        if self.config["selection"] != "instrument_history":
            raise ValueError("bounded archive materialization requires selection=instrument_history")
        if instrument_id != self.config["instrument"]:
            raise ValueError("instrument does not match validated PMXT config")
        source_spec = metadata.get("source_spec")
        if not isinstance(source_spec, dict) or source_spec.get("kind") != "plugin":
            raise ValueError("plugin source metadata is invalid")
        public_config = source_spec.get("config")
        if not isinstance(public_config, dict) or _validate_public_config(public_config) != self.config:
            raise ValueError("materialization config does not match validated PMXT config")
        manifest_uri = source_spec.get("manifest_uri")
        shard_keys = source_spec.get("shard_keys")
        if not isinstance(manifest_uri, str) or not manifest_uri.startswith("manifest://"):
            raise ValueError("materialization must identify a manifest:// source")
        if not isinstance(shard_keys, list) or not shard_keys:
            raise ValueError("materialization must identify selected archive shards")
        if len(source_shards) == 0 or len(source_shards) > _MAX_MATERIALIZATION_SHARDS:
            raise ValueError("materialization shard count is outside the bounded limit")
        expected_keys = {str(item) for item in shard_keys}
        if len(expected_keys) != len(shard_keys) or expected_keys != {
            str(item.get("shard_key")) for item in source_shards if isinstance(item, dict)
        }:
            raise ValueError("materialization shard selection does not match its manifest input")
        start = _parse_utc_hour(self.config["archive_start"], "archive_start")
        end = _parse_utc_hour(self.config["archive_end"], "archive_end")
        requested_hours = int((end - start) / timedelta(hours=1))
        materialization = source_spec.get("materialization")
        if not isinstance(materialization, dict):
            raise ValueError("materialization evidence is required")
        try:
            requested_shard_count = int(materialization["requested_shard_count"])
            source_shard_count = int(materialization["source_shard_count"])
            missing_shard_count = int(materialization["missing_shard_count"])
            probe_error_count = int(materialization["probe_error_count"])
        except (KeyError, TypeError, ValueError) as exc:
            raise ValueError("materialization evidence is invalid") from exc
        if (
            requested_shard_count != requested_hours
            or source_shard_count != len(source_shards)
            or missing_shard_count < 0
            or probe_error_count < 0
            or source_shard_count + missing_shard_count + probe_error_count
            != requested_shard_count
        ):
            raise ValueError("materialization shard counts are inconsistent")
        rows: list[dict[str, Any]] = []
        shard_rows: list[tuple[datetime, list[dict[str, Any]]]] = []
        estimated_bytes = 0
        downloaded_bytes = 0
        decoded_bytes = 0
        staging_path = Path(catalog_path)
        seen_shard_keys: set[str] = set()
        seen_coverage: set[datetime] = set()
        for index, shard in enumerate(source_shards):
            if not isinstance(shard, dict) or shard.get("state") != "AVAILABLE":
                raise ValueError("materialization contains a non-available archive shard")
            shard_key = str(shard.get("shard_key", ""))
            if shard_key in seen_shard_keys:
                raise ValueError("materialization contains duplicate archive shards")
            seen_shard_keys.add(shard_key)
            shard_start = _parse_utc_hour(shard.get("coverage_start"), "coverage_start")
            shard_end = _parse_utc_hour(shard.get("coverage_end"), "coverage_end")
            if shard_end != shard_start + timedelta(hours=1) or shard_start < start or shard_end > end:
                raise ValueError("archive shard is outside the validated materialization range")
            if shard_start in seen_coverage:
                raise ValueError("materialization contains duplicate archive coverage")
            seen_coverage.add(shard_start)
            if shard_key != shard_start.strftime("%Y-%m-%dT%H:00:00Z"):
                raise ValueError("archive shard key does not match its UTC coverage hour")
            source_url = str(shard.get("source_url", ""))
            validate_archive_url(self.config["venue"], source_url)
            if source_url != _archive_url_for(self.config["venue"], shard_start):
                raise ValueError("archive shard URL does not match its UTC coverage hour")
            size_bytes = shard.get("size_bytes")
            if size_bytes is None:
                raise ValueError("bounded materialization requires known archive shard sizes")
            try:
                size_bytes = int(size_bytes)
            except (TypeError, ValueError) as exc:
                raise ValueError("archive shard size is invalid") from exc
            if size_bytes < 0 or size_bytes > _MAX_MATERIALIZATION_ARCHIVE_BYTES:
                raise ValueError("archive shard exceeds the materialization size limit")
            estimated_bytes += size_bytes
            if estimated_bytes > _MAX_MATERIALIZATION_TOTAL_BYTES:
                raise ValueError("materialization source estimate exceeds 20 GiB")
            download_path = staging_path / f"source-{index:03d}.parquet"
            try:
                downloaded = _download(
                    source_url,
                    download_path,
                    max_bytes=min(
                        _MAX_MATERIALIZATION_ARCHIVE_BYTES,
                        _MAX_MATERIALIZATION_TOTAL_BYTES - downloaded_bytes,
                    ),
                    expected_size=size_bytes,
                )
                downloaded_bytes += downloaded
                if downloaded_bytes > _MAX_MATERIALIZATION_TOTAL_BYTES:
                    raise ValueError("materialization download exceeds 20 GiB")
                self._last_decoded_bytes = 0
                shard_data = self._read_rows(download_path)
                decoded_bytes += self._last_decoded_bytes
                if decoded_bytes > _MAX_DECODED_SOURCE_BYTES:
                    raise ValueError("materialization decoded source exceeds 4 GiB")
                rows.extend(shard_data)
                shard_rows.append((shard_start, shard_data))
            finally:
                download_path.unlink(missing_ok=True)
            if len(rows) > _MAX_SOURCE_ROWS:
                raise ValueError("PMXT materialized instrument slice exceeds the row limit")
        quote_result = (
            _polymarket_quotes_by_shard(shard_rows)
            if self.config["venue"] == "polymarket_v2"
            else None
        )
        return self._catalog_from_rows(
            rows=rows,
            catalog_path=catalog_path,
            metadata=metadata,
            quote_result=quote_result,
        )

    def scan_manifest(self, *, metadata: dict[str, Any]) -> dict[str, Any]:
        if self.config["selection"] != "all_markets":
            raise ValueError("manifest scan requires selection=all_markets")
        source_spec = metadata.get("source_spec")
        if not isinstance(source_spec, dict) or source_spec.get("kind") != "plugin":
            raise ValueError("plugin source metadata is invalid")
        manifest_uri = str(metadata.get("manifest_uri", ""))
        provider = str(metadata.get("provider", ""))
        source_license = str(metadata.get("source_license", ""))
        start = _parse_utc_hour(self.config["archive_start"], "archive_start")
        end = _parse_utc_hour(self.config["archive_end"], "archive_end")
        hours = []
        current = start
        while current < end:
            hours.append(current)
            current += timedelta(hours=1)
        def probe(hour: datetime) -> dict[str, Any]:
            source_url = _archive_url_for(self.config["venue"], hour)
            state, size = _probe_archive_url(source_url)
            return {
                "shard_key": hour.strftime("%Y-%m-%dT%H:00:00Z"),
                "source_url": source_url,
                "coverage_start": hour,
                "coverage_end": hour + timedelta(hours=1),
                "size_bytes": size,
                "state": state,
                "observed_at": datetime.now(UTC),
            }

        with ThreadPoolExecutor(max_workers=64, thread_name_prefix="pmxt-probe") as pool:
            shards = list(pool.map(probe, hours))
        available = [item for item in shards if item["state"] == "AVAILABLE"]
        return {
            "manifest_uri": manifest_uri,
            "provider": provider,
            "source_license": source_license,
            "source_spec": source_spec,
            "coverage_start": start,
            "coverage_end": end,
            "scanned_until": datetime.now(UTC),
            "shard_count": len(shards),
            "total_bytes": sum(item["size_bytes"] or 0 for item in available),
            "missing_shard_count": sum(item["state"] == "MISSING" for item in shards),
            "probe_error_count": sum(item["state"] == "PROBE_ERROR" for item in shards),
            "schema_revision": _MANIFEST_SCHEMA_REVISION,
            "point_in_time_result": {
                "valid": True,
                "available_time_preserved": "deferred_to_materialization",
                "available_time_field": "timestamp_received",
                "missing_hours_retained": True,
            },
            "shards": shards,
        }


class PMXTArchivePlugin:
    @staticmethod
    def descriptor() -> DescriptorSnapshot:
        return DescriptorSnapshot(
            plugin_id="pmxt_archive",
            version="1.1.3",
            capabilities={Capability.HISTORICAL_IMPORT},
            compatibility_key="prediction-market-data-v1",
            requires_python=">=3.14,<3.15",
            requires_qf=">=0.1,<0.2",
            public_config_schema={
                "type": "object",
                "required": ["venue"],
                "properties": {
                    "venue": {"type": "string", "enum": ["polymarket_v2", "kalshi"]},
                    "selection": {
                        "type": "string",
                        "enum": ["single_instrument", "instrument_history", "all_markets"],
                        "default": "single_instrument",
                    },
                    "archive_url": {"type": "string", "minLength": 1},
                    "instrument": {"type": "string", "minLength": 1, "maxLength": 200},
                    "instrument_symbol": {"type": "string", "minLength": 1, "maxLength": 120},
                    "archive_start": {"type": "string", "format": "date-time"},
                    "archive_end": {"type": "string", "format": "date-time"},
                },
                "additionalProperties": False,
            },
            secret_config_schema={"type": "object", "properties": {}, "additionalProperties": False},
        )

    def build_catalog_importer(
        self,
        public_config: dict[str, Any],
        secret_config: dict[str, str] | None = None,
    ) -> PMXTArchiveImporter:
        if secret_config:
            raise ValueError("PMXT Archive does not accept provider secrets")
        return PMXTArchiveImporter(public_config)


plugin = PMXTArchivePlugin()
