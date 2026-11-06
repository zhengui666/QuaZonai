"""Read an existing CryptoHFTData Hyperliquid archive without copying its files."""

import datetime
import json
from pathlib import Path
import re


PLUGIN = "cryptohftdata-hyperliquid"
SOURCE = "https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data"
MARKETS = {"spot": "hyperliquid_spot", "perpetual": "hyperliquid_futures"}


def date(value):
    if not isinstance(value, str):
        raise ValueError("expected a UTC YYYY-MM-DD date")
    result = datetime.date.fromisoformat(value)
    if result.isoformat() != value:
        raise ValueError("expected a UTC YYYY-MM-DD date")
    return result


def archive(directory):
    root = Path(directory).resolve(strict=True)
    with (root / "dataset.json").open() as stream:
        manifest = json.load(stream)
    if (not isinstance(manifest, dict) or manifest.get("status") != "COMPLETE"
            or any(type(manifest.get(key)) is not int or manifest[key] <= 0
                   for key in ("objects", "bytes"))):
        raise ValueError("expected a completed CryptoHFTData dataset.json")
    start, end = date(manifest.get("start_date_utc")), date(manifest.get("end_date_utc"))
    if start > end or any(not (root / directory).is_dir() for directory in MARKETS.values()):
        raise ValueError("invalid CryptoHFTData archive dates or market directories")
    return root, manifest, start, end


def inspect(directory):
    root, manifest, start, end = archive(directory)
    return {"plugin": PLUGIN, "archive_directory": str(root),
            "source_reference": SOURCE, "start_date": start.isoformat(),
            "end_date": (end + datetime.timedelta(days=1)).isoformat(),
            "objects": manifest["objects"], "bytes": manifest["bytes"],
            "streams": ["spot orderbook", "perpetual orderbook", "perpetual mark_price"],
            "coverage": "PUBLISHED_ARCHIVE_FILES", "research_qualified": False}


def selection(directory, market, symbols, start_date, end_date, streams=None, hours=None):
    root, manifest, archive_start, archive_end = archive(directory)
    start, end = date(start_date), date(end_date)
    if not archive_start <= start < end <= archive_end + datetime.timedelta(days=1):
        raise ValueError("selection must lie within the downloaded UTC date range")
    if market not in MARKETS:
        raise ValueError("market must be spot or perpetual")
    symbols = sorted(set(symbols))
    if not symbols or any(not isinstance(symbol, str) or not symbol or symbol in (".", "..")
                          or re.search(r"[/\\\x00-\x1f]", symbol) for symbol in symbols):
        raise ValueError("expected exact source filename symbols")
    streams = sorted(set(streams or (["books", "context"] if market == "perpetual" else ["books"])))
    if not streams or set(streams) - {"books", "context"} or market == "spot" and "context" in streams:
        raise ValueError("spot provides books; perpetual provides books and context")
    hours = sorted(set(range(24) if hours is None else hours))
    if not hours or any(type(hour) is not int or not 0 <= hour < 24 for hour in hours):
        raise ValueError("hours must be integers from 0 through 23")
    files, missing, definitions, found = [], [], {}, set()
    day = start
    while day < end:
        for hour in hours:
            for symbol in symbols:
                for kind in streams:
                    suffix = "orderbook" if kind == "books" else "mark_price"
                    relative = f"{MARKETS[market]}/{day.isoformat()}/{hour:02}/{symbol}_{suffix}.parquet"
                    variants = [root / relative, root / (relative + ".zst")]
                    present = [path for path in variants if path.is_file()]
                    if len(present) > 1:
                        raise ValueError(f"ambiguous plain and compressed source file: {relative}")
                    if not present:
                        missing.append({"stream": kind, "date": day.isoformat(), "pattern": relative + "[.zst]"})
                        continue
                    path = present[0]
                    if not path.resolve().is_relative_to(root):
                        raise ValueError("source file resolves outside the archive directory")
                    compressed = path.suffix == ".zst"
                    name = kind + ("-zstd" if compressed else "")
                    files.append({"path": path.relative_to(root).as_posix(),
                                  "size": path.stat().st_size, "stream": name})
                    found.add(symbol)
                    if name not in definitions:
                        if kind == "books":
                            columns = {"symbol": "symbol", "time": "event_time", "available_at": "received_time",
                                       "side": "side", "price": "price", "size": "quantity", "event_type": "event_type"}
                        else:
                            # The publisher's old/new mark event_time units differ; received_time is always ns.
                            columns = {"symbol": "symbol", "time": "received_time", "available_at": "received_time",
                                       "mark_price": "mark_price", "index_price": "index_price", "funding_rate": "funding_rate"}
                        definitions[name] = {"name": name, "kind": "book_snapshots" if kind == "books" else "context",
                                             "format": "parquet-zstd" if compressed else "parquet",
                                             "time_unit": "ms" if kind == "books" else "ns",
                                             "available_time_unit": "ns", "columns": columns,
                                             "paths": [f"{MARKETS[market]}/{{date}}/{{hour}}/*_{suffix}.parquet" + (".zst" if compressed else "")]}
        day += datetime.timedelta(days=1)
    if not files:
        raise ValueError("no downloaded source files match the selection")
    if set(symbols) - found:
        raise ValueError("selected symbol has no downloaded files: " + ", ".join(sorted(set(symbols) - found)))
    return {"schema_version": 1, "repository": "CryptoHFTData",
            "revision": f"published-files-{manifest['start_date_utc']}-{manifest['end_date_utc']}",
            "source_root": str(root), "source_reference": SOURCE,
            "license": "UNSPECIFIED_BY_LOCAL_ARCHIVE", "attribution": "CryptoHFTData / Hyperliquid",
            "retrieved_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "start_date": start.isoformat(), "end_date": end.isoformat(),
            "profile": {"streams": list(definitions.values())}, "files": files, "missing": missing,
            "selected_path_prefixes": [MARKETS[market] + "/"],
            "selection": {"market": market, "symbols": symbols, "hours": hours, "streams": streams},
            "coverage": "SELECTED_PUBLISHED_FILES; missing hourly paths may include unlisted markets"}


def publish(path, manifest):
    with Path(path).open("x") as stream:
        json.dump(manifest, stream, indent=2)
        stream.write("\n")
    return manifest
