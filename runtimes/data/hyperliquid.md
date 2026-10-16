# Hyperliquid public history

The [MIT-declared research archive](https://huggingface.co/datasets/ProbioticFarmer/hyperliquid-augmented-copytrading/tree/064bc5ea76a6f0db8dd5c2b66f598e925857784d) was inspected on 2026-10-16. Use its actual published intervals, not the card's unsupported claim of complete genesis-to-present history:

| Stream | Published files | Native preparation |
| --- | --- | --- |
| Perpetual and spot fills | 443 dates, 2025-07-28–2026-10-13 | Trades selected by the declared `crossed=true` filter, retaining original trade IDs. Maker/taker wallet records must not double market volume. |
| Outcome fills | 165 dates, 2026-05-02–2026-10-13, using only the `all/` hierarchy | Same declared trade mapping; per-outcome copies are not imported again. Missing earlier paths are reported, not invented. |
| One-second OHLCV | 440 dates, 2025-07-31–2026-10-13 | Original candles, including spot; interval-start labels become interval-end BAR events. Empty intervals remain absent. |
| Book snapshots | 303 dates, 2025-12-15–2026-10-13; individual symbol files | Clear/Add snapshot deltas and actual two-sided quotes. Inspected examples have 20 levels per side; this is not proven full depth or continuous L2 replay. |
| Asset contexts | 1,242 daily CSV.lz4 files beginning 2023-05-20 | Funding-rate observations and mark prices. Open interest, oracle/mid/impact prices remain in the original files; they are not fabricated fills, index prices or funding payments. |

[Source mapping](sources/hyperliquid.json) contains file layouts, columns, time units and fill filters. The downloader and record converter do not contain exchange or coin-name branches. Every matching symbol file is selected, including newly listed names; missing daily paths are reported. File partitions alone do not establish per-market completeness. The checked book partition has 178 symbols; it does not establish book coverage of every spot, builder-deployed or outcome market. Attribution to Georgia Tech CSE6242 Team 176 / ProbioticFarmer and the stated Hyperliquid/Hydromancer provenance is retained with acquisition and import reports. No application, account or paid S3 access is needed.

## Acquire original files

The full publisher corpus is about 268 GB; the market-data streams alone exceed 200 GB. Select dates/types intentionally. A checked 2026-10-13 selection was about 656 MB, with 178 book files and perpetual, spot and outcome fills:

```bash
python3 -I runtimes/data/hub.py plan \
  --profile runtimes/data/sources/hyperliquid.json \
  --start-date 2026-10-13 --end-date 2026-10-14
```

```bash
python3 -I runtimes/data/hub.py download \
  --profile runtimes/data/sources/hyperliquid.json \
  --start-date 2026-10-13 --end-date 2026-10-14 \
  --output /absolute/data/hyperliquid-20261013
```

The end date is exclusive UTC. Omit `--kind` for all declared streams, or repeat it with `trades`, `bars`, `books` or `context`. `--revision` overrides the profile revision. Original URLs, revision, license, attribution, retrieval time, file sizes and missing paths go into `market-data.json` after all selected files download. Interrupted downloads retain completed files; rerunning the same selection checks sizes and reuses them. A conflicting selection or size fails without replacement. This path intentionally performs **no content hashing**: size checks cannot detect same-size local edits. Keep raw files unchanged and use another directory for a different selection. Do not execute code from downloaded repositories.

## Native conversion

Build the optional operator:

```bash
rustup run 1.98.1 cargo build --locked -p job \
  --features market-history,catalog-prepare --bin market-history --bin catalog-prepare
```

Obtain current instrument definitions and resolve the acquired archive's identities through the unmodified official Nautilus adapter:

```bash
target/debug/market-history hyperliquid-instruments \
  --manifest /absolute/data/hyperliquid-20261013/market-data.json \
  --output /absolute/data/hyperliquid-instruments.json
```

This saves native definitions, currency properties and `symbol_ids`. The optional `--manifest` discovers original symbols from all selected files; only the metadata entry invokes official Hyperliquid parsers. The official outcome parser can establish an expired contract's identity without inventing its absent parameters. Such identity-only records are counted in `missing_definitions`; they are not historical contract definitions. Unsupported identities still fail. No symbol list or symbol-format implementation is copied into the generic importer.

Actual definition timestamps are retained. Current definitions are useful for inspecting raw history, **not historical contract/fee evidence**; missing or changed historical definitions must come from an original source, not be inferred from prices. A legacy native `InstrumentAny` array is also accepted when its currencies are already known to Nautilus.

```bash
target/debug/market-history import \
  --manifest /absolute/data/hyperliquid-20261013/market-data.json \
  --instruments /absolute/data/hyperliquid-instruments.json \
  --native-precision --start-seconds 1791850200 --end-seconds 1791850500 \
  --output /absolute/catalogs/hyperliquid-20261013-5m
```

The operator enables Nautilus's official `high-precision` feature: the source contains genuine ten-decimal spot quantities which the standard nine-decimal representation cannot retain. These new catalogs use the native 16-byte format. Existing standard-format catalogs and default scientific images are unchanged. Consumers of the new format, including `job` and `catalog-prepare`, must be built with `--features high-precision` (also enabled by `market-history`); the two native binary formats are not interchangeable. Do not overwrite or relabel an existing catalog to change formats.

`--native-precision` preserves source prices and quantities at native encoding precision, independently of current tick/lot parameters. It never rounds a source value to fit a current contract. Omit it when importing against original contract precision; then definitions are required and nonrepresentable values are rejected. Neither mode proves historical tradability. Decimal fields never pass through floating point, and both modes retain the exact-value check.

The generic importer reads selected Parquet/CSV/LZ4 streams in batches and writes `catalog/` plus `import-report.json`. Trade IDs are retained across all files in one import to detect duplicates/conflicts; split long histories into dated catalogs to bound that ID memory. It reads every written native batch back and compares converted values, IDs, times and book levels. Native currency properties travel with the catalog in `currencies.json`; the shared reader and `catalog-prepare` preserve them across processes. `source_coverage` records each stream's actual first/last event and selected row count, including zero-count streams. Date partition names do not establish whole-day coverage: the checked books start at 00:09:29.972 UTC and contexts stop at 06:42 UTC, so the example uses 00:10–00:15 UTC. A BAR's final label can equal the input window end; account for that interval-end convention in the existing half-open BAR selection.

If acquiring only `trades`, `--bar-seconds 60` builds observed one-minute bars with Nautilus `BarBuilder`, omitting empty intervals. Both event boundaries must align to complete intervals. Do not combine supplied candles and aggregated candles in one output, or merge overlapping providers. Published candle volume is not verified trade volume: in the checked 00:00–00:05 window, it matched selected taker volume in 206 of 301 shared symbols, not all of them. Use the observed-trade aggregation path when that distinction matters, not source-specific volume corrections. Book block numbers remain raw provenance, not invented book sequence numbers; native sequence `0` means unknown. One-sided/empty snapshots do not produce invented quotes. Mark/index valuation prices are not forced onto executable tick grids; they must still be exactly representable in the native format.

## Actual verification and limits

The 2026-10-13 00:10–00:15 UTC run selected 52,053 original rows with no missing mappings or rejected records. All 89,681 native output records were read back exactly:

| Native records | Markets | Count |
| --- | ---: | ---: |
| Trades | 265 | 38,292 |
| Supplied BARs | 281 | 11,701 |
| Book snapshot deltas | 178 | 36,458 |
| Two-sided quotes | 178 | 890 |
| Funding-rate observations | 234 | 1,170 |
| Mark-price observations | 234 | 1,170 |

Original contract parameters remain missing for 173 selected rows across 30 expired outcome symbols; their identities come from official parsing. The other 51,880 selected rows use later-observed definitions. This verification covers all published records in the chosen window, not the entire corpus or independent exchange-wide completeness.

A missing native symbol, rejected record or absent expected partition produces an **INCOMPLETE report and unsuccessful exit**, retaining the original raw files and explicitly counted native subset. A process interrupted before its final report has not finished. Existing output directories are never overwritten. Do not relabel an incomplete import as all-market coverage.

Event time is an explicitly unverified native availability proxy unless the profile maps an original reception column. Definitions are never backdated. Native decoding/readback does not certify PIT, historical fees, listing membership, source completeness or research permission. BARs use the existing [catalog preparation and registration workflow](README.md#prepare-isolated-research-partitions) when original provenance supports it. The scientific API still consumes BARs; archived trades/books/funding do not enable an L2 strategy engine or perpetual funding-settlement backtest. Existing unsupported account/instrument capabilities remain unsupported.

The [task record](../../.opensdlc/tasks/hyperliquid-public-history/task.md#verification) distinguishes actual live observations and local checks from unperformed qualification, full-corpus acquisition, review and deployment.
