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
  hyperliquid_futures/YYYY-MM-DD/HH/SYMBOL_orderbook.parquet[.zst]
  hyperliquid_futures/YYYY-MM-DD/HH/SYMBOL_mark_price.parquet[.zst]
```

Loading reads the manifest and date directories without scanning millions of
files or copying the archive. The source manifest records an inclusive end date;
the registered dataset exposes the following midnight as its exclusive end.
The completed local selection covers **2026-02-12 through 2026-10-13 UTC**:
3,368,701 objects, 279,118,890,859 bytes (279.12 GB), 244 complete selected dates.
These totals describe the downloaded selection, not gap-free exchange coverage
for every symbol and hour.

The former `qz.data.hyperliquid` HF provider and its bundled source profile are
removed. Existing stored research references retain their original dataset owner;
new archive IDs start with `cryptohftdata-hyperliquid-`.

## Inspect and convert

Inspect is offline and reads only the archive descriptor:

```sh
python3 -B runtimes/data/source_plugins.py inspect cryptohftdata-hyperliquid \
  --archive /absolute/archive
```

Choose exact source filename symbols, a market, and a half-open UTC date range.
Repeat `--symbol`, `--hour` or `--stream` to narrow the selection. Perpetual input
defaults to books and context, spot to books. Omit `--hour` for all 24 hourly paths.
Missing paths remain reported in the selection; a listing or collector gap is
never filled with invented rows.

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
These files do not provide an incremental exchange sequence or market fills.

Orderbook `event_time` is milliseconds and `received_time` is nanoseconds. The
native adapter retains separate exchange-event and collector-receive clocks.
Perpetual context uses the original nanosecond `received_time` as its observation
clock because older and newer `event_time` fields use different units and both
describe the collector observation. Original raw fields remain in the archive.
Mark/index prices and hourly fractional funding-rate observations remain their
own native record types; a funding-rate observation is not a funding payment.

The loaded archive remains `ready: false` with `nativePrepared: false`. Native
conversion supplies book/quote/price catalogs, while the current factor research
API accepts BAR or TRADE_TICK inputs. The source does not contain those fills or
candles, and no such records are fabricated. Historical instrument parameters,
receiver-clock quality, source rights and research suitability remain their
original unknowns where the publisher files do not establish them.
