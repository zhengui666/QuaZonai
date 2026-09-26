# Public historical data

Operator tools acquire immutable source files, then convert supported records into the existing Nautilus catalog. The downloader is venue independent. Source-specific interpretation stays in the optional `polymarket-history` binary; scientific jobs remain offline and the generic research/API/database contracts are unchanged.

A downloaded archive is not automatically a qualified research dataset. Public Polymarket sources collectively cover much of its history, but no verified, free source currently establishes gap-free trades, continuous depth, historical instrument/fee changes and timestamped settlements for every market from launch through today.

## Source selection

The following primary sources were checked on 2026-09-27. Dates and sizes are publisher or file-inventory observations, not an independent full-chain reconciliation. Lock a revision before each selection; a repository update date is not its data cutoff. Keep attribution with exported results. Personal use does not remove the source's attribution or additional terms.

| Source | Coverage and useful records | Published terms | Integration and limitations |
| --- | --- | --- | --- |
| [Moose / Envio v1](https://huggingface.co/datasets/moose-code/polymarket-onchain-v1) | FPMM from September 2020; CLOB from November 2022; chain events through **2026-04-24 07:43:41 UTC**, block 85,948,287. About 127 GB across events and state. | CC-BY-4.0; attribute Envio | Primary native `moose-fills` adapter. Also acquire split, merge, redemption, condition and FPMM files for separate investigation. `orderbook.parquet` contains cumulative statistics, not L2. State export is 2,185 seconds later than the event cutoff. |
| [Joseph3222 orderbook](https://huggingface.co/datasets/Joseph3222/polymarket-orderbook) | Raw events and minute-end full-depth snapshots, 2026-02-22–2026-08-10; about 1.37 TB overall / 192 GB minute snapshots. | CC-BY-4.0 | Native `joseph-books` adapter for minute snapshots. File inventory has **no 2026-06-12–2026-06-17 partitions**. Sparse active-minute observations are not a continuous book. |
| [TimeSeventeen v1](https://huggingface.co/datasets/TimeSeventeen/Polymarket-v1) | CLOB through 2026-04-28; raw ConditionalTokens lifecycle, including resolution events. About 53 GB. | CC-BY-4.0; attribute Boka Qin and Rui Yang | Acquisition supported; native schema conversion is not implemented. Resolution rows have block/log IDs but need block timestamps. Derived aligned tables contain retrospective labels. Partition timezone descriptions conflict; filter actual UTC timestamps. |
| [SII-WANGZJ](https://huggingface.co/datasets/SII-WANGZJ/Polymarket_data) | Raw events with transaction/log identity, linked trades and market metadata; card says 2022-11-21–2026-03-04. | README declares MIT; Hub license frontmatter absent | Acquisition supported with the verified README terms supplied explicitly. Large monolithic files. [Reported historical gaps](https://huggingface.co/datasets/SII-WANGZJ/Polymarket_data/discussions/8) were said to be repaired, but full reconciliation is not established here. `quant` normalizes NO to YES; `users` splits trades by wallet. |
| [PolyData capture](https://huggingface.co/datasets/PolyData/polymarket_trade_capture_5Mar2026) | Daily CLOB fill partitions, 2022-11-21–2026-03-05, about 22 GB. | CC-BY-4.0 | Acquisition supported; no native adapter. Missing log index makes transaction hash insufficient for deduplication. Do not use the card's `cash/(cash+tokens)` price example. |
| [Jon Becker](https://github.com/Jon-Becker/prediction-market-analysis) / [rhinot mirror](https://huggingface.co/datasets/rhinot/prediction-market-analysis) | CLOB, legacy FPMM, block timestamps and market snapshots; mirror is a 36 GB compressed archive. | MIT in project and mirror card | Independent cross-check candidate; whole archive is inconvenient for selective downloads. Verify collateral precision before interpreting legacy token amounts. |
| [Kacho crypto 5-minute markets](https://huggingface.co/datasets/kachoio/polymarket-5-minute-crypto-up-down-markets) | Narrow crypto universe, 1 Hz top-of-book, March–May 2026, about 725 MB. | CC0 | Acquisition supported; no native adapter. Collector gaps and a narrow universe. Price-derived outcomes are not oracle settlement evidence. |
| [Pancake history](https://github.com/usepancake/polymarket-history) | Small daily candle/metadata/resolution collection through June 2026. | Attribution plus explicitly stated noncommercial restriction | Download separately using its checksummed manifests. Its trade tape is **synthetic**; never import it as observed fills. |
| [wzsg v2](https://huggingface.co/datasets/wzsg/polymarket-orderfilled-v2) | New exchange ABI, 2026-04-28–2026-08-03 in the checked snapshot. | No explicit dataset license found | Candidate only. Do not invent a license or concatenate v1/v2 amount fields without decoding their different ABIs. |
| [warproxxx collector](https://github.com/warproxxx/poly_data) | Current v2 backfill/update pipeline. | GPL-3.0 code; separate data provenance | An external collector, not a licensed frozen data release. HyperSync requires a free account/token. The old Goldsky path no longer establishes complete coverage. |

Pinned defaults used for verification:

- Moose: `7eeb860dea5b79d5c74f3182b70bd08c85c8f833`.
- Joseph: `efe472f1f00a62f2cab1fed2851da439d9c1fe11`.
- TimeSeventeen: `5aa1b9d52316a8b2e789e81c8ae42c7ed532e8aa`.
- SII: `6d3c336c39cf1a2dfe53d702ad2c110ab5bdbfde`.
- PolyData: `804f6e173f6614710e9e91628f0a4897801047d2`.

## Plan and download

Run from the source checkout with Python 3.10+. No Python packages or account are required for public, ungated sources. The command prints the fixed revision, file list, declared license and total bytes before acquisition. An explicit `--include` is always required. The default maximum is 128 MiB; increase it only for an intentional larger selection.

```sh
python3 -B runtimes/data/snapshot.py plan \
  --dataset moose-code/polymarket-onchain-v1 \
  --revision 7eeb860dea5b79d5c74f3182b70bd08c85c8f833 \
  --include 'order_filled/year=2022/month=11.parquet'

python3 -B runtimes/data/snapshot.py download \
  --dataset moose-code/polymarket-onchain-v1 \
  --revision 7eeb860dea5b79d5c74f3182b70bd08c85c8f833 \
  --include 'order_filled/year=2022/month=11.parquet' \
  --output /absolute/data/moose-2022-11
```

Change the month/glob or repeat `--include` for further partitions. Preserve raw sources outside Git; `/.ai-bridge/` is already ignored for local verification. File selection reduces download size; **market/token filtering occurs during local conversion**, not inside the downloader. Monolithic vendor files still require their full bytes.

`README.md` and `SNAPSHOT.json` are retained when available. LFS content is checked against the upstream SHA-256; regular files against their Git blob identity. The local `snapshot.json` records resolved commit, source terms/reference, retrieval time, every original URL, size and SHA-256. It is published only after all selected files pass. Repeating the identical command verifies and reuses complete files. An interrupted file is fetched again; conflicting files or a different completed selection fail without replacement. Use a new output directory for a new snapshot. Missing license metadata requires independently verified source terms through `--license`; that option records terms, it does not grant permission.

For a bounded real book partition (about 98 MB):

```sh
python3 -B runtimes/data/snapshot.py download \
  --dataset Joseph3222/polymarket-orderbook \
  --revision efe472f1f00a62f2cab1fed2851da439d9c1fe11 \
  --include 'orderbook_1min/date=2026-06-06/data_0.parquet' \
  --output /absolute/data/joseph-2026-06-06
```

## Convert selected instruments and time

Build the optional operator binary using the repository-pinned Rust toolchain. It is not part of the offline scientific job entrypoint:

```sh
rustup run 1.98.1 cargo build --locked -p job \
  --features polymarket-history --bin polymarket-history

target/debug/polymarket-history archive \
  --snapshot /absolute/data/moose-2022-11/snapshot.json \
  --format moose-fills \
  --instruments /absolute/original-instruments.json \
  --start-seconds 1668988800 --end-seconds 1669852800 \
  --bar-seconds 60 \
  --output /absolute/catalogs/moose-2022-11
```

`--instruments` is a JSON array of the **original native Rust `InstrumentAny` definitions**, using the same representation as the existing native `import` command's `instruments` field. Their `raw_symbol` is the outcome token ID and their ID is `{condition}-{token}.POLYMARKET`. This selects the assets to import. Provide historical currency, price/size precision, tick/fee evidence and observation times from your source; do not rewrite today's Gamma definitions to look historical. v1 fills require `USDC.e` definitions for Polygon collateral `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`, not native `USDC` or today's `pUSD`. The [official migration](https://help.polymarket.com/en/articles/14762452-polymarket-exchange-upgrade-april-28-2026) distinguishes these regimes. Metadata can be prepared for inspection with its actual late observation time, but that does not admit a historical backtest. Missing metadata evidence remains missing.

`moose-fills` reads only `order_filled/` Parquet files and the half-open UTC event window `[start,end)`. It uses exact integer cash/token amounts, six-decimal shares and canonical chain/block/log identities. Matching emits both maker fills and an exchange-counterparty summary; the latter is excluded using the [original exchange contract](https://github.com/Polymarket/ctf-exchange/blob/main/src/exchange/mixins/Trading.sol). Direct fills and self-trades remain. Identical duplicate IDs count once; conflicting records fail. Aggressor is unknown, since complementary-outcome matches do not establish the counterparty's wallet intent. Price quantization to the supplied native precision is counted in the source quality report; sizes cannot silently lose precision. Fees and transaction identities remain in the immutable raw source.

`--bar-seconds` optionally invokes Nautilus's native `BarBuilder` on observed fills. Both selection boundaries must align to complete intervals. Supported intervals follow native SECOND/MINUTE/HOUR/DAY specifications; 60 produces `1-MINUTE-LAST-EXTERNAL`. Bars are labeled by interval end, so the last bar may have timestamp `end`. Empty intervals are omitted. Aggregation never establishes missing-trade coverage or executable liquidity.

For books, select `--format joseph-books`, omit `--bar-seconds`, and provide the matching native instrument definitions. The selection window is half-open on **minute-end event time**: source `minute_ts + 60`. Each complete book becomes native Clear/Add snapshot deltas; two-sided books also produce quotes with actual top sizes. Empty or one-sided books never generate quotes with invented zero prices. Sequence `0` means unknown, not a verified event sequence. These are snapshots, not continuous replay across missing minutes/days. Do not derive transaction volume or settlement from them.

Each conversion is bounded to the existing combined one-million-native-record limit and 256 instruments. Split larger work by non-overlapping time/asset selections into new catalog directories. Do not concatenate overlapping providers or reinterpret `orders_matched`, wallet-split records, YES-normalized rows or derived price marks as additional fills.

## Quality, research admission and recovery

The new output contains `catalog/`, detached `source-evidence.json` and a final `import-report.json`. Without the final report it is an interrupted, unpublished import. Existing outputs are never overwritten. Source evidence includes selected revision/files, native definition digest, UTC selection, rows scanned/selected, excluded summaries, duplicate counts, self-trades, rounded prices and one-sided/empty books. Archive import rechecks selected file sizes and hashes before conversion. Raw source files must remain available for audit.

`coverage=UNPROVEN`, `historical_availability=UNVERIFIED`, and `registered_in_quazonai=false` are deliberate. On-chain block time and book minute-end are **event-time proxies**, not measured exchange reception/finality latency. Snapshot market metadata, final payout state, redemption time, end dates and last prices do not establish when a resolution became available. Original source evidence is never mounted as researcher-readable catalog data.

To conduct admitted research, provide the original universe/membership, calendar, permission/license, historical parameter and availability provenance, partition and quality evidence required by [RuntimeCatalogMetadataV1](../../crates/contracts/src/catalogs.rs), then use the existing [catalog registration and Runtime workflow](../../.opensdlc/operations.md#scientific-runtime). Fresh `DATA_VALIDATE` must reopen the frozen catalog. Keep Discovery/Validation/Sealed/Forward isolated. The current scientific catalog contract accepts BAR data; archived trades/L2 are source material and do not enable a new L2 strategy engine or bypass that contract.

Remaining source gaps include pre-CLOB FPMM normalization, late-v1 lifecycle joins with block timestamps, licensed v2 continuity, historical fee/tick changes and actual reception/finality evidence. The generic downloader can preserve those public archives now, but this change does not label them converted, PIT-verified or complete.

## Checks

```sh
python3 -B -m unittest discover -s runtimes/data -v
rustup run 1.98.1 cargo test --locked -p job --features polymarket-history --bin polymarket-history
rustup run 1.98.1 cargo clippy --locked -p job --features polymarket-history --bin polymarket-history -- -D warnings
```

CI uses deterministic local Parquet fixtures and HTTP mocks. Live download/import observations are recorded in the [task verification](../../.opensdlc/tasks/polymarket-public-history/task.md#verification), separately from claims about full-source coverage.
