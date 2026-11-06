# CryptoHFTData Hyperliquid history

The Hyperliquid source plugin uses existing free published files from
[CryptoHFTData](https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data).
It registers the original archive in place and converts explicit selections with
the existing Nautilus `market-history` importer. No live collector or account
credentials run inside the plugin.

## Register the archive

In **Settings → Data Plugins → `qz.data.cryptohftdata`**, set `archiveDirectory`
to the downloaded directory, save, enable and select **Load Data**. The directory
must contain its completed `dataset.json` and the declared market directories:

```text
archive/
  dataset.json
  hyperliquid_spot/YYYY-MM-DD/HH/SYMBOL_orderbook.parquet[.zst]
  hyperliquid_spot/YYYY-MM-DD/HH/SYMBOL_trades.parquet[.zst]
  hyperliquid_futures/YYYY-MM-DD/HH/SYMBOL_orderbook.parquet[.zst]
  hyperliquid_futures/YYYY-MM-DD/HH/SYMBOL_mark_price.parquet[.zst]
  hyperliquid_futures/YYYY-MM-DD/HH/SYMBOL_trades.parquet[.zst]
```

Loading reads the manifest and date directories without scanning millions of
files or copying the archive. The source manifest records an inclusive end date;
the registered dataset exposes the following midnight as its exclusive end.
The completed local selection covers **2026-02-12 through 2026-10-13 UTC**:
3,368,701 objects, 279,118,890,859 bytes (279.12 GB), 244 complete selected dates.
These totals describe the downloaded books/context selection, not gap-free
exchange coverage for every symbol and hour. Trades are separately published
hourly files; the original 279 GB selection did not download them.

The former `qz.data.hyperliquid` HF provider and its bundled source profile are
removed. Existing stored research references retain their original dataset owner;
new archive IDs start with `cryptohftdata-hyperliquid-`.

## Inspect, plan and convert

Inspect is offline and reads only the archive descriptor:

```sh
python3 -B runtimes/data/source_plugins.py inspect cryptohftdata-hyperliquid \
  --archive /absolute/archive
```

Choose exact source filename symbols, a market, and a half-open UTC date range.
Repeat `--symbol`, `--hour` or `--stream` to narrow the selection. Perpetual input
defaults to books and context, spot to books. Omit `--hour` for all 24 hourly paths.
Select `--stream trades` explicitly to read existing published trade files.
Missing paths remain reported in the selection; a listing or collector gap is
never filled with invented rows. `coverage: OBSERVED_PUBLISHED_FILES` means only
the listed existing files, not a complete hourly grid or exchange history.
`selection.symbols` retains every requested identity; `unobserved_symbols` lists
those with no matching files. The first or last published file is not a listing
or delisting timestamp. `IMPORTED` means the observed files converted, while
missing hours and economic valuation gaps remain unknown.

For raw book replay, write a source manifest and use its parent directory as the
book source root. Planning runs offline, references only existing files in the
archive, and embeds the unchanged JSON supplied through `--instruments`:

```sh
python3 -B runtimes/data/source_plugins.py plan cryptohftdata-hyperliquid \
  --archive /absolute/archive --market perpetual --symbol BTC \
  --start-date 2026-10-13 --end-date 2026-10-14 --hour 23 --stream books \
  --instruments /absolute/original-instruments.json \
  --output /absolute/book-source/book-snapshots.source.json
```

This creates only the selected file manifest; raw replay reads those Parquet
files directly without preparing a second full book catalog. `plan` does not
accept native conversion or trade cache options.

```sh
python3 -B runtimes/data/source_plugins.py convert cryptohftdata-hyperliquid \
  --archive /absolute/archive --market perpetual --symbol BTC \
  --start-date 2026-10-13 --end-date 2026-10-14 --hour 23 \
  --instruments /absolute/original-instruments.json \
  --native-bin /absolute/matching/market-history \
  --output /absolute/native/btc-20261013-23
```

Use original Nautilus instrument definitions when available. The native raw
precision path preserves records whose historical definitions are unavailable
and reports the missing definitions; it does not synthesize fees or tick sizes.
Spot source symbols and official instrument identifiers may differ. Use the
importer's explicit symbol mapping when definitions require it.

The selection manifest is written beside the output as `OUTPUT.source.json`.
Its `source_root` points at the existing archive, so conversion reads original
files without copying them. The native catalog and `import-report.json` are
written under the new output. Work in bounded selections to control disk use;
outer Zstandard files are decompressed one at a time.

For a trade-only catalog, add `--stream trades --trade-ticks`. A derived factor
cache uses `--cohort-cache` or `--cohort-flow-cache` instead. These modes require
only the trade stream. Original exchange/receive clocks remain distinct:

```sh
python3 -B runtimes/data/source_plugins.py convert cryptohftdata-hyperliquid \
  --archive /absolute/archive --market perpetual --symbol BTC \
  --start-date 2026-10-13 --end-date 2026-10-14 --hour 23 --stream trades \
  --cohort-flow-cache --received-time --maximum-receive-delay-ns 2000000000 \
  --instruments /absolute/original-instruments.json \
  --native-bin /absolute/matching/market-history \
  --output /absolute/native/btc-received-20261013-23
```

`--received-time` selects the UTC window and forms cohorts by original
`received_time`; it never substitutes that value for the exchange event clock.
Its delay limit is explicit, in nanoseconds, and bounds receive minus event
time. The 2-second value above covers the checked BTC hour, not every market or
date. Rows with negative or excessive delay remain errors. Without this option,
the existing event-time path remains unchanged. Cache output is a derived cohort
representation, not a native TradeTick catalog; its report retains both clocks.

The matching remote Nautilus image packages the operator at
`/opt/quazonai/operator/source_plugins.py` with its adjacent native executables.
Mount the archive read-only at its declared absolute path and a separate writable
output parent, then invoke that entrypoint. See the
[installed operator guide](../../deploy/docker/README.md#source-preparation).

## Record semantics

The publisher's [perpetual wire format](https://www.cryptohftdata.com/docs/wire-format-hyperliquid-futures)
describes full `l2Book` snapshots, up to 20 levels per side. Flat rows sharing a
snapshot are grouped, sides are sorted and each snapshot starts with a native
book clear followed by its actual levels. Quotes use the actual best bid/ask.
Book files do not provide an incremental exchange sequence or market fills.

Published [trade files](https://www.cryptohftdata.com/docs/rest-trades) contain
real fills. `event_time` and `trade_time` are exchange milliseconds;
`received_time` is the collector's nanosecond clock. Exact price/quantity strings
and trade IDs are preserved. `is_buyer_maker=true` means seller aggressor, and
`false` means buyer aggressor; the constant `order_type` is not used. Replayed
trades deduplicate by native market, event time and trade ID, retaining the
earliest receive time when price, quantity and aggressor agree. Conflicting fills
remain errors. Reports retain `duplicate_trades`, with `deduplicated_trades`
and `deduplication_policy: EARLIEST_RECEIPT_IDENTICAL_TRADE` identifying resolved
CryptoHFTData copies. Hourly files are processed in arrival-hour order, with each
hour's deliveries sorted before writing; received-time imports retain seen IDs
within the declared delay window.
Include source files from at least that delay before the research window, so
an arrival before its start remains the first observation when replayed later.
In daily selections, load the preceding day and start research on the following
day. A missing earlier source hour cannot establish first-arrival identity.

Orderbook `event_time` is milliseconds and `received_time` is nanoseconds. The
native adapter retains separate exchange-event and collector-receive clocks.
Perpetual context uses the original nanosecond `received_time` as its observation
clock because older and newer `event_time` fields use different units and both
describe the collector observation. Original raw fields remain in the archive.
Mark/index prices and hourly fractional funding-rate observations remain their
own native record types; a funding-rate observation is not a funding payment.

The loaded raw archive remains `ready: false` with `nativePrepared: false`.
Explicit trade conversion supplies real TRADE_TICK input or a declared received-time
cohort cache; book/context conversion supplies book/quote/price catalogs.
No candles or market fills are fabricated from book snapshots. Historical instrument parameters,
receiver-clock quality, source rights and research suitability remain their
original unknowns where the publisher files do not establish them.
