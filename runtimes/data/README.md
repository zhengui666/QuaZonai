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
| [TimeSeventeen v2](https://huggingface.co/datasets/TimeSeventeen/Polymarket-v2) | 175 daily fill files from 2026-04-03 through 2026-09-25; 43.5 GB of fills. **2026-09-10 is absent**. Early April records precede the production migration. | CC-BY-4.0 | Native `time-seventeen-v2` adapter. Normalized float amounts must have a unique, exact six-decimal representation; no blind rounding. Original raw integers, emitting contract, transaction hashes and reception times are absent. This is not gap-free production coverage. |
| [lokima live BTC/ETH capture](https://huggingface.co/datasets/lokima/polymarket-btc-eth-5m-updown-8h) | 2026-07-29 20:40 through 2026-07-30 04:40 UTC; 96 five-minute windows, BTC and ETH, dual CLOB feeds, original Gamma responses and receive clocks. | ODC-BY; retain publisher attribution and provenance | Native `capture` path below. The first BTC window has matching trade messages, contemporaneous fees and an observed resolution. Other windows have feed differences; publisher `clean` flags and backfilled labels are not accepted as proof. |
| [SII-WANGZJ](https://huggingface.co/datasets/SII-WANGZJ/Polymarket_data) | Raw events with transaction/log identity, linked trades and market metadata; card says 2022-11-21–2026-03-04. | README declares MIT; Hub license frontmatter absent | Acquisition supported with the verified README terms supplied explicitly. Large monolithic files. [Reported historical gaps](https://huggingface.co/datasets/SII-WANGZJ/Polymarket_data/discussions/8) were said to be repaired, but full reconciliation is not established here. `quant` normalizes NO to YES; `users` splits trades by wallet. |
| [PolyData capture](https://huggingface.co/datasets/PolyData/polymarket_trade_capture_5Mar2026) | Daily CLOB fill partitions, 2022-11-21–2026-03-05, about 22 GB. | CC-BY-4.0 | Acquisition supported; no native adapter. Missing log index makes transaction hash insufficient for deduplication. Do not use the card's `cash/(cash+tokens)` price example. |
| [Jon Becker](https://github.com/Jon-Becker/prediction-market-analysis) / [rhinot mirror](https://huggingface.co/datasets/rhinot/prediction-market-analysis) | CLOB, legacy FPMM, block timestamps and market snapshots; mirror is a 36 GB compressed archive. | MIT in project and mirror card | Independent cross-check candidate; whole archive is inconvenient for selective downloads. Verify collateral precision before interpreting legacy token amounts. |
| [Kacho crypto 5-minute markets](https://huggingface.co/datasets/kachoio/polymarket-5-minute-crypto-up-down-markets) | Narrow crypto universe, 1 Hz top-of-book, March–May 2026, about 725 MB. | CC0 | Acquisition supported; no native adapter. Collector gaps and a narrow universe. Price-derived outcomes are not oracle settlement evidence. |
| [Pancake history](https://github.com/usepancake/polymarket-history) | Small daily candle/metadata/resolution collection through June 2026. | Attribution plus explicitly stated noncommercial restriction | Download separately using its checksummed manifests. Its trade tape is **synthetic**; never import it as observed fills. |
| [wzsg v2](https://huggingface.co/datasets/wzsg/polymarket-orderfilled-v2) | New exchange ABI, 2026-04-28–2026-08-03 in the checked snapshot. | No explicit dataset license found | Candidate only. Do not invent a license or concatenate v1/v2 amount fields without decoding their different ABIs. |
| [warproxxx collector](https://github.com/warproxxx/poly_data) | Current v2 backfill/update pipeline. | GPL-3.0 code; separate data provenance | An external collector, not a licensed frozen data release. HyperSync requires a free account/token. The old Goldsky path no longer establishes complete coverage. |
| [OpenMarket](https://huggingface.co/datasets/gregyoung14/openmarket-btc-polymarket/tree/74502466d1a7cef56395bfd8d0b465fbebc849cf) | BTC 15-minute quotes, 54 observed dates within 2026-02-12–05-15; 7.58 GB of unified quote partitions. | Apache-2.0 in the fixed card | Acquisition supported. Two complete date files inspected contain only 35 and 74 seconds. Price-change levels are not fills or mid prices. Actual metadata lacks historical fees/resolutions; the collector overwrites values while retaining first_seen. Do not backdate current fields. |
| [PMXT original capture](https://github.com/pmxt-dev/polymarket-orderbook-collector/tree/cb0f6631556bf460d03594fe20f9bbd020b47d19) | Public hourly v2 market-data objects, including trades and tick changes. | Archive advertises CC-BY-4.0; retain the original terms with each selected object | Candidate requiring a frozen object/terms manifest. The public collector sets timestamp_received at database insertion, and does not archive its market-definition/resolution control stream. This is not proof of original WebSocket arrival or a complete fee regime. |
| [Rocklabs](https://github.com/rocklabs-io/polymarket-dataset) | Publisher claims chain history from 2020 and extensive 2026 tick capture. | Free academic access by application | Not an anonymous public download. Individual access has not been established; no account, application or email was submitted. |

Pinned defaults used for verification:

- Moose: `7eeb860dea5b79d5c74f3182b70bd08c85c8f833`.
- Joseph: `efe472f1f00a62f2cab1fed2851da439d9c1fe11`.
- TimeSeventeen: `5aa1b9d52316a8b2e789e81c8ae42c7ed532e8aa`.
- TimeSeventeen v2: `f14b2977b23852380f828e0a03ec0ad31af710c2`.
- lokima: `97d05eac7324a410db4dabb4f06b25cc21aae23f`.
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

Each conversion is bounded to the existing combined one-million-native-record limit and 256 instrument definition records, including original versions. Split larger work by non-overlapping time/asset selections into new catalog directories. Do not concatenate overlapping providers or reinterpret `orders_matched`, wallet-split records, YES-normalized rows or derived price marks as additional fills. The import report distinguishes unique `instruments` from `instrument_versions`.

## Quality, research admission and recovery

The new output contains `catalog/`, detached `source-evidence.json` and a final `import-report.json`. Without the final report it is an interrupted, unpublished import. Existing outputs are never overwritten. Source evidence includes selected revision/files, native definition digest, UTC selection, rows scanned/selected, excluded summaries, duplicate counts, self-trades, rounded prices and one-sided/empty books. Archive import rechecks selected file sizes and hashes before conversion. Raw source files must remain available for audit.

`coverage=UNPROVEN`, `historical_availability=UNVERIFIED`, and `registered_in_quazonai=false` are deliberate. On-chain block time and book minute-end are **event-time proxies**, not measured exchange reception/finality latency. Snapshot market metadata, final payout state, redemption time, end dates and last prices do not establish when a resolution became available. Original source evidence is never mounted as researcher-readable catalog data.

To conduct admitted research, provide the original universe/membership, calendar, permission/license, historical parameter and availability provenance, partition and quality evidence required by [RuntimeCatalogMetadataV1](../../crates/contracts/src/catalogs.rs), then use the existing [catalog registration and Runtime workflow](../../.opensdlc/operations.md#scientific-runtime). Fresh `DATA_VALIDATE` must reopen the frozen catalog. Keep Discovery/Validation/Sealed/Forward isolated. The current scientific catalog contract accepts BAR data; archived trades/L2 are source material and do not enable a new L2 strategy engine or bypass that contract.

For v2 choose `--format time-seventeen-v2`, original **pUSD** definitions, and explicit `OrderFilled/YYYY_MM_DD.parquet` files. Canonical chain/block/log IDs, independent row deduplication and bar construction use the same path as v1. Both v2 exchange-summary addresses come from the [original contract deployments and matching implementation](https://github.com/Polymarket/ctf-exchange-v2/tree/ccc0596074f4dfd62c944fbca4de252893b82b4b). The amount check rejects values outside the exact recoverable range. In the actual 2026-08-09 file, 28,494 of 2,529,399 non-summary fills had at least one amount that failed this strict check (about 1.13%); a selected BTC market failed without publishing an output. Do not round or silently drop those rows to make a selection pass. The legacy column name `usdc_amount` does not authorize changing pUSD to USDC. Derived prices, aggressor directions and retrospective labels are not imported; chain corroboration additionally checks the original maker-side label.

## Original chain evidence

`evm.py` freezes original EVM headers and logs for explicit blocks, addresses and one event signature. It needs no account, Python package or paid API. Choose two to four distinct public HTTPS RPC hosts that retain the requested history; the [Polygon endpoint list](https://docs.polygon.technology/pos/reference/rpc-endpoints/) lists public options. Provider limits and pruned history can cause a request to fail. No credentials or automatic paid fallback are used.

This bounded example selects the eight blocks containing the rejected BTC amounts. It does **not** establish continuous coverage between them:

```sh
python3 -B runtimes/data/evm.py \
  --chain-id 137 \
  --rpc https://polygon-mainnet.gateway.tatum.io/ \
  --rpc https://polygon.drpc.org/ \
  --block 91705515 --block 91705517 --block 91705572 --block 91705647 \
  --block 91705673 --block 91705693 --block 91705701 --block 91705709 \
  --address 0xe111180000d2663c0091e4f400237545b87b996b \
  --address 0xe2222d279d744050d28e00520010520000310f59 \
  --topic0 0xd543adfd945773f1a62f74f0ee55a5e3b9b1a28262980ba90b1a89f2ea84d8ee \
  --output /absolute/data/v2-eight-blocks.json
```

For a continuous block selection replace the repeated `--block` arguments with inclusive `--from-block` and `--to-block` (at most 4,096 blocks per capture). The query covers all matching logs in each explicit block; native conversion then filters tokens and UTC time. Block selection does not freeze a historical market universe. The default total response-body and output limits are each 128 MiB, with 32 MiB per response. Connections can be attempted at most three times with short backoff before any response body is read; HTTP errors, interrupted bodies, malformed data and provider disagreement are not retried. There is no overwrite or resume; use a new output file. A failed acquisition publishes no final file.

The collector binds logs to block hashes, transaction hashes/indices and canonical log indices, checks parent links for adjacent blocks, and requires agreement between providers. The snapshot preserves each original response, endpoint and actual retrieval time. The native reader repeats identity/consistency checks and decodes the original ABI using the already pinned Alloy library. This is corroborated RPC evidence, not an independently verified receipt trie, a historical finality clock or a source permission grant. Raw blockchain facts do not imply a dataset redistribution license; preserve applicable provider terms.

Import chain fills directly, even when a vendor daily partition is absent:

```sh
target/debug/polymarket-history chain \
  --snapshot /absolute/data/v2-eight-blocks.json \
  --instruments /absolute/original-pusd-instruments.json \
  --start-seconds 1786263300 --end-seconds 1786263600 \
  --bar-seconds 60 --output /absolute/catalogs/v2-chain-selection
```

Both original CLOB exchange ABIs are supported. For v1, replace the addresses with `0x4bfb41d5b3570defd03c39a9a4d8de6bd8b8982e` and `0xc5d563a36ae78145c45a50134d48a1215220f80a`, the topic with `0xd0a08e8c493f9c94f29311604c9de1b4e8c8d4c06bd0c789af57f2d65bfec0f6`, and use original `USDC.e` definitions. The verified v1 example is block `35896869` from November 2022. v2 uses the addresses/topic above and `pUSD`. A snapshot query must use one ABI version. Fees remain in raw event evidence; they do not establish a historical fee formula. Summary events are excluded, identical order hashes remain distinct by block/log index, sizes remain exact, and native price quantization is counted. FPMM, depth and settlements are not synthesized from these fills.

To corroborate a TimeSeventeen v2 import, add `--chain-evidence /absolute/data/v2-eight-blocks.json` to the ordinary `archive --format time-seventeen-v2 ...` command. Both v2 exchanges must have been queried. Within those blocks, selected source rows and raw fills must account for each other; omissions and identity/amount conflicts fail. Outside them, the original strict normalized-amount check still applies.

An amount is replaced only after the canonical ID, order hash, maker/taker, token, side, block timestamp, builder and metadata match. The raw integer supplies the replacement. The original float must lie within one adjacent floating-point value of that independently obtained integer's normalization **and** less than half a base unit away; this detects the observed encoding damage without accepting a different integer amount. The vendor file is never rewritten. Quality reports count corroborated fills and recovered amount fields, and retain both sources with the actual later observation time. Block time remains an explicitly UNVERIFIED availability proxy.

## Captured historical availability

This path consumes original historical Gamma responses and both live CLOB connections. It does not need hand-authored instrument definitions. A bounded selection is about 31 MB:

```sh
python3 -B runtimes/data/snapshot.py download \
  --dataset lokima/polymarket-btc-eth-5m-updown-8h \
  --revision 97d05eac7324a410db4dabb4f06b25cc21aae23f \
  --include 'data/clob_ws/btc/1785357600/*.parquet' \
  --include 'data/reference/discovery.parquet' \
  --include 'metadata/clock.json' --include 'PROVENANCE.md' \
  --include 'metadata/window_meta.parquet' \
  --include 'metadata/quality_report.json' \
  --output /absolute/data/lokima-btc-1785357600

target/debug/polymarket-history capture \
  --snapshot /absolute/data/lokima-btc-1785357600/snapshot.json \
  --market-slug btc-updown-5m-1785357600 \
  --start-seconds 1785357600 --end-seconds 1785357900 \
  --bar-seconds 1 --output /absolute/catalogs/lokima-btc-1785357600
```

The adapter requires the recorded fee formula, initial tick, minimum size and parseable lifetime dates before the selected interval. It reuses the pinned native Gamma parser and fee rules. It rejects missing definitions, ambiguous duplicate trade identities, imprecise amounts, local sequence/clock gaps, interrupted connections through resolution, and mismatched dual-feed trades, tick controls or payouts. It only supports post-migration pUSD captures from 2026-04-29 onward. Only tick and its price bounds may change; currency, lifetime, precision, size and fee regimes must remain identical. Do not replace historical parameters with today's values.

Successful import does not guarantee that every target can execute: the existing Polymarket research adapter rejects each simulated trading fill below 1 unit of collateral to retain its conservative fee bound. Native BAR execution can split a larger order into such a small fill; the job then fails and retains that result. This is a research constraint, not an asserted exchange minimum.

Original `tick_size_change` messages rebuild the native definition at the primary connection's original event/receive times. Both connections must agree on each token's control chain. An identical consecutive retransmission keeps the first receipt and remains in detached evidence; a contradictory old tick or repeated-time version fails. Expired-contract tick messages remain evidence without authorizing a later trading regime. Trades and BARs are checked against the definition actually available at their receipt time. A BAR crossing an incompatible tick/bounds change fails; its OHLC is never rounded into compliance.

Captured trade IDs combine canonical instrument/side/price/size/event-time fields with the original transaction hash, using `ws_` and 128 bits of SHA-256 within the native ID limit. Hashes must be nonzero 32-byte hexadecimal values; casing and equivalent numeric spelling do not create distinct prints. Different transactions can have otherwise identical prints. Repeated prints within the same transaction, or repeats with a missing transaction hash, remain ambiguous and fail. A single print without a hash remains supported. Final native ID collisions also fail. Original payloads remain in the fixed source snapshot; receive time and local sequence are never invented trade identities. These IDs supersede the live parser's transaction-blind IDs in new capture imports; publish a fresh immutable catalog rather than combining both versions.

Captured trade IDs combine canonical instrument/side/price/size/event-time fields with the original transaction hash, using `ws_` and 128 bits of SHA-256 within the native ID limit. Hashes must be nonzero 32-byte hexadecimal values; casing and equivalent numeric spelling do not create distinct prints. Different transactions can have otherwise identical prints. Repeated prints within the same transaction, or repeats with a missing transaction hash, remain ambiguous and fail. A single print without a hash remains supported. Final native ID collisions also fail. Original payloads remain in the fixed source snapshot; receive time and local sequence are never invented trade identities. These IDs supersede the live parser's transaction-blind IDs in new capture imports; publish a fresh immutable catalog rather than combining both versions.

`poly_a` supplies original receive times; `poly_b` independently checks the selected trade-message multiset and resolution. Trades are sorted per instrument by event and receive time before native aggregation; equal timestamps retain original frame/array order. A bar becomes available at `max(interval_end, all contributing receive times)`; nonmonotonic bar availability is rejected, not shifted. Empty intervals remain absent. Native closes use the actual `market_resolved` event and receive times for both outcome tokens. REST final drains, recovered strike labels and price-based winner guesses are not used. Raw source evidence stays outside scientific catalog mounts.

For the verified first BTC window, 1,946 trade messages produce 483 observed one-second bars and two original payouts. The historical fee schedule is rate `0.07`, exponent `1`, taker-only. A separate bounded Polygon `ConditionResolution` lookup agreed with the observed `[0,1]` payout; its block time was earlier than the recorded WebSocket receipt, and was **not** substituted for receipt time. This is evidence for the selected capture interval, not the market's entire lifetime, exchange-wide completeness, economic profitability or a registered research qualification.

## Prepare isolated research partitions

The venue-independent `catalog-prepare` operator command uses the **same native measurement function as `DATA_VALIDATE`**. It selects only original bars, matching instrument definitions and explicitly declared complete settlements, writes a new physical catalog, reads it back, then publishes `catalog-metadata.json` last. It never overwrites an existing output or changes source files.

```sh
rustup run 1.98.1 cargo build --locked -p job \
  --features catalog-prepare --bin catalog-prepare

target/debug/catalog-prepare \
  --catalog /absolute/catalogs/lokima-btc-1785357600/catalog \
  --declaration /absolute/reviewed-declaration.json \
  --selection /absolute/native-selection.json \
  --output /absolute/partitions/discovery
```

`--declaration` uses [RuntimeCatalogMetadataV1](../../crates/contracts/src/catalogs.rs), omitting `row_count` and `quality`; `universe.instrument_definitions` can be omitted because it is read from the original catalog. If definitions are supplied they must match. Supply all other fields, including the actual origin, PIT/revision policy, source/availability explanation, original membership, calendar, partition and requested event window. `--selection` is [NativeDatasetSelectionV1](../../crates/contracts/src/execution.rs), with a local revision identity, complete native BarType strings, half-open event-label bounds, actual decision cutoff and any original settlement groups. Counter/nanosecond contract fields are **decimal JSON strings**. Native instrument/data timestamps remain their original integer nanoseconds. Registration-facing datetime bounds use microsecond precision; round a declared availability boundary later when necessary, never move the underlying observation earlier.

Repeat with new destinations for disjoint Discovery, Validation and Sealed intervals. Definitions for an untraded payout sibling are retained. Sealed metadata omits notionals and payouts; its physical catalog is for the independent evaluator only. Keep acquisition evidence, preparation declarations and selections containing sealed values outside researcher mounts. Dataset quality reports contain measured facts, not source certification: the tool preserves `UNVERIFIED` and cannot infer `VERIFIED` from successful decoding. Review the recorded clock, metadata history, feed coverage and original permission terms before making a narrower source declaration.

Instrument versions use the existing native definition array for every venue. Preparation keeps the latest original definition strictly before the interval start, plus all subsequent versions through the decision cutoff; when no earlier definition exists, the earliest original observation remains the baseline. An update received exactly at the interval start retains its update ordering checks. Every selected BAR must follow an available definition. Controls after event-end still matter to late-arriving BARs and pending orders; controls after cutoff are excluded from the physical partition. Declared availability must cover the original selected definition times. Fees and lifetime remain invariant; slippage uses the tick known at the actual decision, with availability covering both its original definition and the last BAR.

Simulation replays changes using the [pinned native bridge](../../vendor/nautilus-backtest/QZ_PATCH.md), retaining the same matching engine, compatible resting orders, cash and positions. It clears stale native market state when the tick changes and waits for subsequent original market data. The update itself supplies no prices or liquidity. Ordinary simulated fills must satisfy the definition known at their receive time, including its price grid and bounds; a violating native fill invalidates the result instead of being clamped. Original settlement payouts retain their separate validation. Ambiguous update/market-event timestamp ties and a preloaded definition unavailable at the first replay event are rejected. This covers tick-only BAR research, not continuous order-book reconstruction or arbitrary fee/contract mutations.

The bounded real capture was also split into 173 Discovery, 172 Validation and 138 Sealed bars; each isolated catalog passed the actual `job execute` `VALIDATE_DATA` operation. A separate native cash-account replay checked the source fee formula and redemption at the original resolution receipt time, with independent cash/commission reconciliation. These are data and execution integration checks, not fitted strategy results.

Formal service acceptance still uses the existing owner workflow: upload the original permission evidence as a REPORT artifact, create the source and `RESEARCH` grant, configure the Runtime's catalog root plus metadata file, register revisions, freeze an InputSet and run fresh validation. The [service Skill](../../skills/quazonai/SKILL.md) documents native authenticated commands. An offline report cannot create a DataGrant, authorize a Runtime mount, establish an untouched Sealed study or substitute for login. This task did not register or deploy data into the live service.

Remaining broad-history gaps include pre-CLOB FPMM normalization, lifecycle joins with historical block/receipt evidence, missing v2 dates, historical fee/tick changes and continuous depth/finality evidence. The pipeline now accepts an evidence-complete captured interval; it does not relabel incomplete older archives as qualified.

## Checks

```sh
python3 -B -m unittest discover -s runtimes/data -v
rustup run 1.98.1 cargo test --locked -p job --features polymarket-history,catalog-prepare \
  --bin polymarket-history --test catalog_prepare --test catalog --test managed \
  --test instrument_versions --test native_instrument_updates
rustup run 1.98.1 cargo clippy --locked -p job --features polymarket-history,catalog-prepare \
  --all-targets -- -D warnings
```

CI uses deterministic local Parquet fixtures and HTTP mocks. Live download/import observations are recorded in the [task verification](../../.opensdlc/tasks/polymarket-public-history/task.md#verification), separately from claims about full-source coverage.
