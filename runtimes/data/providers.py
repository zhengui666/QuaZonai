"""Source plugins for bounded public acquisition, not native or qualified research data.

A plugin describes its capabilities, plans fixed public GET requests and interprets
original JSON numbers. The acquisition runner owns I/O, budgets and publication.
"""

from dataclasses import dataclass
from decimal import Decimal
import datetime
import json
import re
from typing import Protocol
import urllib.parse


MAX_REQUESTS = 128
MAX_RECORDS = 100_000
MAX_SECONDS = 253402300799  # Last representable UTC second in year 9999.


def integer(value, name, minimum=0, maximum=MAX_SECONDS):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ValueError(f"invalid {name}")
    return value


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON object member")
    return result


def invalid_constant(_):
    raise ValueError("invalid JSON numeric constant")


def read_json(body):
    try:
        return json.loads(body, parse_float=Decimal, object_pairs_hook=unique_object,
                          parse_constant=invalid_constant)
    except RecursionError:
        raise ValueError("source JSON nesting limit exceeded") from None


def decimal_text(value, positive=False):
    # Never accept a binary float or round a source amount to an assumed tick size.
    if type(value) not in (int, Decimal):
        raise ValueError("source amount must be an original JSON number")
    number = Decimal(value)
    if (not number.is_finite() or len(number.as_tuple().digits) > 100
            or abs(number.as_tuple().exponent) > 100
            or number < 0 or (positive and number == 0)):
        raise ValueError("invalid source amount")
    return format(number, "f")


@dataclass(frozen=True)
class Selection:
    instrument: str
    start_seconds: int
    end_seconds: int
    interval_seconds: int

    def validate(self):
        integer(self.start_seconds, "start_seconds")
        integer(self.end_seconds, "end_seconds")
        integer(self.interval_seconds, "interval_seconds", minimum=1)
        if self.start_seconds >= self.end_seconds:
            raise ValueError("selection requires start_seconds < end_seconds")


@dataclass(frozen=True)
class Request:
    url: str
    start_seconds: int
    end_seconds: int
    method: str = "GET"


class Provider(Protocol):
    """Implement this interface and insert one instance in PROVIDERS to extend."""

    descriptor: dict

    def plan(self, selection: Selection) -> list[Request]: ...

    def decode(self, body: bytes, selection: Selection) -> list[dict]: ...


def utc_seconds(value):
    return datetime.datetime.fromtimestamp(value, datetime.timezone.utc).isoformat().replace("+00:00", "Z")


def windows(selection, seconds):
    count = (selection.end_seconds - selection.start_seconds + seconds - 1) // seconds
    if count > MAX_REQUESTS:
        raise ValueError("selection exceeds 128 requests; split the acquisition")
    for start in range(selection.start_seconds, selection.end_seconds, seconds):
        yield start, min(start + seconds, selection.end_seconds)


def distinct(rows):
    seen = set()
    for row in rows:
        key = row["selection_time_seconds"]
        if key in seen:
            raise ValueError("duplicate or conflicting source timestamp")
        seen.add(key)
    return rows


class PolymarketPrices:
    descriptor = {
        "id": "polymarket-prices", "version": 1, "venue": "POLYMARKET",
        "hosts": ["clob.polymarket.com"],
        "record_kind": "PRICE_MARK", "authentication": "NONE", "access": "PUBLIC_FREE",
        "documentation": "https://docs.polymarket.com/api-reference/markets/get-prices-history",
        "terms_reference": "https://polymarket.com/tos",
        "license": None, "permission_status": "REQUIRES_INDEPENDENT_REVIEW",
        "selection_time": "SOURCE_MARK_TIMESTAMP", "native_conversion": "UNSUPPORTED",
        "limitations": ["Sampled price marks are not trades, quotes or OHLCV bars",
                        "No historical instrument, fee, settlement or reception-time evidence",
                        "Coverage and historical availability are unverified"],
    }

    def plan(self, selection):
        selection.validate()
        if (not isinstance(selection.instrument, str)
                or not re.fullmatch(r"(?:0|[1-9][0-9]{0,77})", selection.instrument)
                or int(selection.instrument) >= 2**256):
            raise ValueError("Polymarket instrument must be a uint256 outcome token ID")
        if selection.interval_seconds % 60 or not 60 <= selection.interval_seconds <= 86400:
            raise ValueError("Polymarket fidelity must be 1 to 1440 whole minutes")
        return [Request("https://clob.polymarket.com/prices-history?" + urllib.parse.urlencode({
            "market": selection.instrument, "startTs": start, "endTs": end,
            "fidelity": selection.interval_seconds // 60,
        }), start, end) for start, end in windows(selection, 86400)]

    def decode(self, body, selection):
        value = read_json(body)
        if not isinstance(value, dict) or set(value) != {"history"} or not isinstance(value["history"], list):
            raise ValueError("invalid Polymarket price-history response")
        if len(value["history"]) > MAX_RECORDS:
            raise ValueError("source record limit exceeded")
        rows = []
        for item in value["history"]:
            if not isinstance(item, dict) or set(item) != {"t", "p"}:
                raise ValueError("invalid Polymarket price-history row")
            timestamp = integer(item["t"], "mark timestamp")
            price = decimal_text(item["p"])
            if Decimal(price) > 1:
                raise ValueError("Polymarket mark outside [0,1]")
            rows.append({"selection_time_seconds": timestamp, "event_time_seconds": timestamp,
                         "price": price})
        return distinct(rows)


class CoinbaseCandles:
    descriptor = {
        "id": "coinbase-candles", "version": 1, "venue": "COINBASE",
        "hosts": ["api.exchange.coinbase.com"],
        "record_kind": "OHLCV_CANDLE", "authentication": "NONE", "access": "PUBLIC_FREE",
        "documentation": "https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles",
        "terms_reference": "https://www.coinbase.com/legal/market_data",
        "license": None, "permission_status": "REQUIRES_INDEPENDENT_REVIEW",
        "selection_time": "BUCKET_START", "event_time": "BUCKET_END",
        "native_conversion": "UNSUPPORTED",
        "limitations": ["Missing buckets are not filled or evidence of zero trading",
                        "Historical rates may be incomplete or revised",
                        "No historical product-definition, fee or reception-time evidence",
                        "Public access does not grant redistribution rights"],
    }

    def plan(self, selection):
        selection.validate()
        if (not isinstance(selection.instrument, str)
                or not re.fullmatch(r"[A-Z0-9]{1,20}-[A-Z0-9]{1,20}", selection.instrument)):
            raise ValueError("Coinbase instrument must be an explicit BASE-QUOTE product ID")
        if selection.interval_seconds not in (60, 300, 900, 3600, 21600, 86400):
            raise ValueError("unsupported Coinbase granularity")
        if selection.start_seconds % selection.interval_seconds or selection.end_seconds % selection.interval_seconds:
            raise ValueError("candle boundaries must align to complete UTC buckets")
        # At most 299 selected buckets, leaving room for an inclusive endpoint row.
        return [Request(f"https://api.exchange.coinbase.com/products/{selection.instrument}/candles?"
                        + urllib.parse.urlencode({"start": utc_seconds(start), "end": utc_seconds(end),
                                                  "granularity": selection.interval_seconds}), start, end)
                for start, end in windows(selection, 299 * selection.interval_seconds)]

    def decode(self, body, selection):
        value = read_json(body)
        if not isinstance(value, list) or len(value) > 300:
            raise ValueError("invalid or oversized Coinbase candles response")
        rows = []
        for item in value:
            if not isinstance(item, list) or len(item) != 6:
                raise ValueError("invalid Coinbase candle row")
            start = integer(item[0], "candle timestamp", maximum=MAX_SECONDS - selection.interval_seconds)
            if start % selection.interval_seconds:
                raise ValueError("candle timestamp is not bucket-aligned")
            low, high, opening, close = [decimal_text(price, positive=True) for price in item[1:5]]
            volume = decimal_text(item[5])
            if not (Decimal(low) <= Decimal(opening) <= Decimal(high)
                    and Decimal(low) <= Decimal(close) <= Decimal(high)):
                raise ValueError("inconsistent candle OHLC bounds")
            rows.append({"selection_time_seconds": start, "event_time_seconds": start + selection.interval_seconds,
                         "open": opening, "high": high, "low": low, "close": close, "volume": volume})
        return distinct(rows)


PROVIDERS: dict[str, Provider] = {provider.descriptor["id"]: provider
                                  for provider in (PolymarketPrices(), CoinbaseCandles())}


def provider_by_id(provider_id):
    if provider_id not in PROVIDERS:
        raise ValueError("unknown public data provider")
    return PROVIDERS[provider_id]
