# Hyperliquid public history

## Full-history derived cohort research

The explicit `EVENT_TIME_COHORT_CACHE_V1` representation avoids retaining the
complete trade history or factor observations in memory. It is a derived cache,
not a native TradeTick catalog: source rows still pass through the same native
decimal conversion and exact BigDecimal reduction before the derived VWAP and
quantity marks are stored as their original IEEE f64 bits. Each instrument file
contains 32-byte little-endian frames: event nanoseconds, VWAP bits, quantity
bits and original trade count. Availability is the explicitly unverified event
proxy, not an extra observed receive clock.

```sh
target/debug/market-history import \
  --manifest /absolute/data/market-data-processing.json \
  --instruments /absolute/data/original-hyperliquid-instruments.json \
  --cohort-cache --native-precision \
  --path-prefix perp_fills_all/ --path-prefix spot_fills/ \
  --max-trade-rows 1308222597 --max-output-bytes 25769803776 \
  --output /absolute/catalogs/hyperliquid-cohorts
```

The row budget above is the measured candidate count for the pinned archive and
its declared processing filters, not a general default or a profitability claim.
Set an explicit budget for the actual source. Cache mode keeps at most one
unfinished cohort per instrument, up to 100,000 trades per cohort and 1,000,000
unfinished source rows in aggregate, under the existing 2 GiB address-space
limit. It retains cohorts across file boundaries and rejects source time reversal
or repeated/conflicting identities within an event. This source's published
stream/date file order must therefore remain chronological; arbitrary unsorted
archives are not silently sorted or sampled. Completed cache frames are read
back bit-for-bit in bounded batches. Output limits include data and reports.
An interrupted cache has no completed `cohort-cache.json`; use a new output
directory rather than overwriting or admitting it.

Use `catalog-prepare prepare-ticks` with a request whose input explicitly declares
`"representation":"EVENT_TIME_COHORT_CACHE_V1"` and
`"maximum_source_rows":"1308222597"`, omitting the legacy `maximum_rows`.
The old native TradeTick request and its million-row limit remain unchanged.
Sampling, fixed-cohort horizon and the seven feature slots remain the same.

```sh
target/debug/catalog-prepare prepare-ticks \
  --catalog /absolute/catalogs/hyperliquid-cohorts/catalog \
  --request /absolute/catalogs/hyperliquid-research/cohort-request.json \
  --maximum-output-bytes 51539607552 \
  --output /absolute/catalogs/hyperliquid-cohort-prepared

node --experimental-strip-types apps/service/src/dataset-export.ts \
  --prepared /absolute/catalogs/hyperliquid-cohort-prepared/catalog-metadata.json \
  --request /absolute/catalogs/hyperliquid-research/cohort-request.json \
  --id hyperliquid-cohort-research --name 'Hyperliquid full-history cohort research' \
  --output /absolute/catalogs/hyperliquid-cohort-dataset.json --ready
```

Preparation creates explicit primary and independent byte-range views, linking
only newly produced immutable derived files. The primary range ends before the
validation cutoff; boundary checks prevent it from omitting an in-range cohort
or expanding into held-out observations. Reads never decode prices outside the
declared view; neighboring frame clocks are used only to check completeness.
The preparation byte budget counts both logical view file sets and metadata,
even though their data files share physical blocks. Publication clones or copies
each shared inode once into its owned revision, then links aliases only within
that revision. Exported research inputs point to that owned immutable copy.
Allow room for one physical cache plus one publication copy when reflinks are
unavailable; the 614,393,279-cohort census estimates 18.310 GiB per physical copy,
but actual output and free disk must be measured.

Keep the cache, prepared views and exported JSON under a catalog root mounted
into the native worker. On the installed local deployment, use an owned run
directory below `/home/zzy/.local/share/quazonai-ts/data/catalogs/`; publication
creates `.qz-source-revisions` beside the exported JSON, so placing that JSON
under an unmounted `imports` or temporary directory is insufficient.

Load the exported JSON through the existing `qz.data.local-config` data plugin.
The existing native worker's `--catalog` binding and `evaluate-tick-factor`
operation handle this explicit representation; no new worker path argument is
introduced. Cache evaluation requires `--summary-only`. EMA, VM state, ordinals,
fuel and pending labels continue across source units and partitions. Partition
and asset metrics use stable online pooled covariance, with unavailable statuses
preserved; local IC values are never averaged into a portfolio or global IC.

This path provides exploratory price/quantity/cohort-intensity controls only.
It does not add signed-flow features, fixed-clock labels, network-jitter
simulation, tick portfolios, historical funding settlement or execution
qualification. A cohort horizon is not a fixed holding time, and a VWAP mark is
not an executable quote. `ready` records operational preparation only.

## Native TradeTick preparation

Settings → Data Plugins → `qz.data.hyperliquid` accepts an explicit
`inputModality`: `BAR` retains the existing default; `TRADE_TICK` prepares
native trades without any BAR aggregation. Tick mode rejects a simultaneous
`barMilliseconds` setting. Its declared source scope is the existing spot and
perpetual fill directories, selected by `research_trade_path_prefixes` in the
profile. Outcome fills, supplied candles, books and funding records are not
required for this selection. Existing broad acquisition manifests remain usable
offline: missing paths outside the explicit selection do not become missing
tick inputs, while missing selected paths still fail.

Select those source paths before acquisition, rather than downloading outcomes
as part of the legacy all-trades stream:

```sh
python3 -I runtimes/data/hub.py download \
  --profile runtimes/data/sources/hyperliquid.json \
  --start-date 2026-10-13 --end-date 2026-10-14 \
  --kind trades --path-prefix perp_fills_all/ --path-prefix spot_fills/ \
  --output /absolute/data/hyperliquid-spot-perpetual

target/debug/market-history import \
  --manifest /absolute/data/hyperliquid-spot-perpetual/market-data.json \
  --instruments /absolute/data/original-hyperliquid-instruments.json \
  --trade-ticks --path-prefix perp_fills_all/ --path-prefix spot_fills/ \
  --native-precision --start-seconds 1791850200 --end-seconds 1791850500 \
  --resume --output /absolute/catalogs/hyperliquid-ticks-unit-1
```

`--trade-ticks` is explicit and rejects non-trade selected streams and BAR
aggregation. Original prices, quantities, trade IDs and event/availability
clocks pass through the existing exact converter and official native readback.
The output contains only native trades plus definitions/currencies and separate
provenance. Source fill archives can also contain account conversion records: observed `Spot Dust Conversion` rows share ID zero at one timestamp with differing values. They require explicit record classification; adding event time to the identity does not make these conflicting rows valid market trades. Raw files remain unchanged. `catalog/import-provenance.json` carries the pinned revision,
selected streams/paths, event range, source coverage, identity/rejection counts
and availability limitations with the catalog. The final `import-report.json`
is published last. Missing selected native instrument definitions or duplicate
trade identities make a tick import incomplete;
conflicting identities are reported as rejected. No raw file is overwritten.

Tick units admit at most 1,000,000 selected source rows, 100,000 retained
same-availability records and 10 GiB of derived output including reports.
`--max-trade-rows` and `--max-output-bytes` can lower these limits. Exceeding a
limit fails without silently sampling or truncating. Use explicit smaller
event-time units via `--start-seconds`/`--end-seconds`; the plugin exposes these
as `eventStartSeconds`/`eventEndSeconds`. The plugin runs tick conversion under
Linux `prlimit` with a 2 GiB address-space limit; missing `prlimit` or an
unsupported platform fails instead of running unbounded. The direct
`market-history --trade-ticks` command also lowers its own address-space limit
through the pinned Rustix process API, preserving any stricter inherited limit.
The existing BAR command does not acquire this tick-only limit.

For a unit inside a longer acquisition, concrete existing file prefixes such as
`--path-prefix perp_fills_all/date=2026-10-13/` and
`--path-prefix spot_fills/date=2026-10-13/` select those files before decoding.
The event-time window then selects rows within them; it does not make the unit
a complete full-history research input. Top-level prefixes retain their existing
behavior, and concrete date selections must remain inside the original acquisition.

An explicitly separate processing manifest can declare scalar row exclusions,
for example `"not_equals": {"direction": "Spot Dust Conversion"}` on its trade
stream alongside `"equals": {"crossed": true}`. Every equality must match and
every exclusion must not match. The existing `filtered_rows` count and embedded
`source_manifest` retain the selection and its rules. Preserve the original
acquisition manifest and raw files; a processing manifest may sit beside it so
the original relative file paths remain usable. Use a matching operator build:
older builds can ignore unknown filter keys. The shipped profile is unchanged,
and other special record classes still require their own interpretation.

For the inspected spot/perpetual archive, an explicit candidate-market processing
view separates account/protocol events with these additional stream conditions:

```json
{
  "equals": { "crossed": true },
  "not_equals": { "liquidation_method": "backstop" },
  "not_in": {
    "direction": [
      "Net Child Vaults", "Settlement", "Spot Dust Conversion",
      "Backstop Borrow Liquidation", "Partial Borrow Liquidation"
    ]
  }
}
```

`not_in` lists contain scalar values: any match excludes the row, separate fields
remain conjunctive, and an empty list excludes nothing. Existing scalar conditions
retain their meaning. This declaration requires a matching operator build and
does not change the shipped acquisition profile.

Actual market liquidations remain: the venue's [liquidation rules](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/liquidations)
distinguish market orders from backstop position transfers, and [ADL](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/auto-deleveraging)
uses the previous mark price. [Borrow takeovers](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/portfolio-margin)
precede subsequent TWAP executions; [delisting settlement](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/delisting)
is a cashflow/position event. Preserve those original records for separate event
and accounting consumers. `crossed`, positive order IDs or an account's liquidation
flag alone do not establish an ordinary book execution in this archive. Source
classification still does not establish original receive times, historical rules,
exchange-wide completeness or executable backtest results.

`--resume` verifies a completed immutable unit by re-reading its source,
definitions, currencies and native records, and comparing its final receipt;
it does not rewrite that catalog. A changed source/selection/definition or
native result fails verification. An interrupted or incomplete native output
must use a new directory; completed dated units can be reused while the failed
unit is retried. The plugin's optional `nativeOutputDirectory` selects this
verified reuse path. Original acquisition retains its existing size-based
reuse and no-content-hashing limitation; native readback is not independent
source or market-completeness evidence.

Results expose `nativeModality` and separate preparation facts
(`archiveLoaded`, `nativePrepared`, `scientificQualification`). The source plugin
keeps `ready:false` and qualification `UNVERIFIED`, including after successful
tick conversion. Research still needs an explicitly declared supported
instrument/time/sampling/target selection and original partition policy through
the research preparation/export and existing `qz.data.local-config` workflow.
Conversion does not invent those declarations or PIT evidence. New high-precision
catalogs require matching scientific/preparation binaries; see the native-format
limitations below.

## Settings plugin and millisecond history

`qz.data.hyperliquid` is built into **Settings → Data Plugins**, disabled initially.
It uses existing historical files, never a live market-data connection. Configure
an acquisition directory containing `market-data.json` for offline loading. To
download a date selection from the existing archive instead, supply both UTC
start and exclusive end dates. The plugin reuses `hub.py` and the fixed profile
below; all matching markets are selected, without a hard-coded coin list.

Downloads select the original `trades` streams. Existing mixed-stream archives
can load offline; native trade aggregation requires a trades-only selection.
Optional native conversion requires the built `market-history` executable and a
local native instrument-definition file. Set **BAR 毫秒间隔** (`barMilliseconds`):
the default is **1ms**, and **100ms** is supported. The plugin invokes
`import --bar-milliseconds` with that value and retains native readback and
coverage reports in a new output directory.
Existing files are not rewritten. A missing partition, rejected record or failed
native conversion fails loading and preserves the previous successful source.
Disabling the plugin or closing the service cancels its active operator process.

The result is deliberately `ready: false`: acquisition/native decoding does not
provide original research partitions, contract/fee history or qualification.
Use the existing preparation workflow and load its resulting DatasetConfig via
`qz.data.local-config` for research admission. No online instrument fetch is
performed by this plugin. Current definitions remain later observations.

The new native option constructs **millisecond observed-trade BARs**, including
1ms or 100ms, with the official Nautilus `BarBuilder`, exact decimal values and
original trade IDs. It preserves
event/availability clocks, omits empty intervals and never upsamples one-second
candles or minute books. Granularity is not a promise of a trade in every 100ms
window, transport latency or exchange-wide historical completeness.

Keep original millisecond trade timestamps regardless of an execution host's
network RTT. The [TradingVPS investigation](tradingvps-latency.md) separates
published claims from the missing on-instance latency/jitter measurements.

```sh
target/debug/market-history import \
  --manifest /absolute/data/hyperliquid-trades/market-data.json \
  --instruments /absolute/data/original-hyperliquid-instruments.json \
  --native-precision --bar-milliseconds 100 \
  --output /absolute/catalogs/new-hyperliquid-100ms
```

Do not combine supplied candles with trade aggregation. As with second BARs,
selected event-time boundaries must align with complete intervals. The importer
supports supplied `bar_milliseconds` mappings independently of its aggregation
CLI; seconds/milliseconds declarations are mutually exclusive.

### Historical source comparison

Sources can be official, GitHub or Hugging Face; third-party publishers are valid
candidates. The existing fixed archive was chosen for its inspected file layout
and previous real native readback, not because its publisher is Hyperliquid.

| Source | Evidence and limits |
| --- | --- |
| [Official history](https://hyperliquid.gitbook.io/hyperliquid-docs/historical-data) | Requester-pays S3 node fills and separate book/context history; documentation does not guarantee continuous complete history. |
| [Official historical GitHub](https://github.com/hyperliquid-dex/historical_data) | Some market-maker trades are excluded; not a complete market tape. |
| [ProbioticFarmer archive](https://huggingface.co/datasets/ProbioticFarmer/hyperliquid-augmented-copytrading/tree/064bc5ea76a6f0db8dd5c2b66f598e925857784d) | Fixed revision and actual inspected intervals below; millisecond fills support 100ms aggregation. The 2026-10-16 observations are prior evidence, not a new full-corpus verification. |
| [Chainticks/perp-data](https://huggingface.co/datasets/Chainticks/perp-data) | Public CC-BY-4.0 candidate with multiple daily shards; latest-date pointer observed on 2026-10-26 says 2026-08-11. Not established to exceed the selected source's coverage. |
| [gionuibk node fills](https://huggingface.co/datasets/gionuibk/hyperliquid-node-fills-by-block) | Large node-fill archive; samples retain ms times and string decimals, with spot/perpetual examples. Exact coverage and dataset licensing remain unverified. |

Current environment access to the selected HF API is blocked by proxy CONNECT
403. The fixed source was not re-downloaded or refreshed on 2026-10-26. New local
protocol/native checks must not be presented as fresh real-source verification.

## Existing inspected archive

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
  --features market-history,catalog-prepare --bin market-history --bin catalog-prepare --bin job
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

The generic importer reads selected Parquet/CSV/LZ4 streams in batches and writes `catalog/` plus `import-report.json`. Original definition precision changes create contiguous, time-disjoint native encoding segments, including changes back to an earlier precision. Readback uses fresh, file-scoped native queries (`optimize_file_loading=false`), retaining each file's metadata rather than one directory schema. Trade identity uses the native instrument, original event timestamp and original trade ID across all files in one import. A source ID reused at another event time remains a separate record; same-time duplicates/conflicts retain their existing handling. Split long histories into dated catalogs to bound that identity memory. It reads every written native batch back and compares converted values, IDs, times and book levels. Native currency properties travel with the catalog in `currencies.json`; the shared reader and `catalog-prepare` preserve them across processes. `source_coverage` records each stream's actual first/last event and selected row count, including zero-count streams. Date partition names do not establish whole-day coverage: the checked books start at 00:09:29.972 UTC and contexts stop at 06:42 UTC, so the example uses 00:10–00:15 UTC. A BAR's final label can equal the input window end; account for that interval-end convention in the existing half-open BAR selection.

If acquiring only `trades`, `--bar-seconds 60` builds observed one-minute bars with Nautilus `BarBuilder`, omitting empty intervals. It builds with one exact native representation, then publishes each interval at the maximum price/size precision actually observed in that interval; a definition change cannot round an earlier trade or leave mixed OHLC precisions. The next interval starts its own precision range. BARs with the same availability timestamp use that complete group's maximum precision so they share one native file without changing raw values or clocks. This is encoding, not a new contract tick/lot claim, and scientific definition-transition restrictions remain unchanged. Both event boundaries must align to complete intervals. Do not combine supplied candles and aggregated candles in one output, or merge overlapping providers. Published candle volume is not verified trade volume: in the checked 00:00–00:05 window, it matched selected taker volume in 206 of 301 shared symbols, not all of them. Use the observed-trade aggregation path when that distinction matters, not source-specific volume corrections. Book block numbers remain raw provenance, not invented book sequence numbers; native sequence `0` means unknown. One-sided/empty snapshots do not produce invented quotes. Mark/index valuation prices are not forced onto executable tick grids; they must still be exactly representable in the native format.

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

A separate trade-only run over the same window produced 1,053 observed one-minute BARs across all 265 traded markets, alongside the 38,292 original trades. Every output record read back exactly. A separate inspection matched each persisted BAR's exact OHLC, summed trade volume, interval-end label and availability against its contributing native trades; empty buckets stayed absent.

Original contract parameters remain missing for 173 selected rows across 30 expired outcome symbols; their identities come from official parsing. The other 51,880 selected rows use later-observed definitions. This verification covers all published records in the chosen window, not the entire corpus or independent exchange-wide completeness.

A missing native symbol, rejected record or absent expected partition produces an **INCOMPLETE report and unsuccessful exit**, retaining the original raw files and explicitly counted native subset. A process interrupted before its final report has not finished. Existing output directories are never overwritten. Do not relabel an incomplete import as all-market coverage.

Event time is an explicitly unverified native availability proxy unless the profile maps an original reception column. Definitions are never backdated. Native decoding/readback does not certify PIT, historical fees, listing membership, source completeness or research permission. BARs use the existing [catalog preparation and registration workflow](README.md#prepare-isolated-research-partitions) when original provenance supports it. Native TradeTick research requires the separate explicit tick input and supported feature/target contract; archived books/funding do not enable an L2 strategy engine or perpetual funding-settlement backtest. Existing unsupported account/instrument capabilities remain unsupported.

The [task record](../../.opensdlc/tasks/hyperliquid-public-history/task.md#verification) distinguishes actual live observations and local checks from unperformed qualification, full-corpus acquisition, review and deployment.
