# Public Polymarket history

<a id="task"></a>
## Task

Task ID: `polymarket-public-history`. Requested by the repository owner in this task: support broad, high-quality, publicly available Polymarket history for personal strategy research without compromising generality. The owner selected source integration and bounded real-data verification now, with market/date downloads later. Scope is implementation, local verification and repository PR review/CI; no production deployment or unrequested full archive download. Merge remains subject to the repository's final-Head gates.

<a id="intent"></a>
## Intent

The existing operator-only native history command supports a bounded single-market API request and native JSON import. It cannot acquire public bulk snapshots or convert their real event records into native research bars. Preserve the offline scientific jobs, original data permissions, PIT admission, immutable snapshots and generic runtime contracts.

<a id="spec"></a>
## Requirements and design

Use a venue-independent operator acquisition helper for Hugging Face datasets: fixed revisions, explicit file selection, byte budget, original license/card evidence, SHA-256 verification, repeatable local downloads and a final snapshot manifest. Keep source schemas in the existing Polymarket operator adapter. Prefer on-chain match events with stable chain/block/log identities over ambiguous order-fill rows; retain raw archive files for audit. Produce only observed trades and bars aggregated from those trades. Do not synthesize missing bars, spreads, depth, settlements or earlier metadata availability. A public provider's completeness claim does not qualify a dataset.

Document source coverage by era and data type, licensing, known gaps and the path into existing catalog registration. Missing historical fees, resolution publication times or PIT metadata remain explicit research-admission gaps. No source-specific contract, database, API or frontend change is planned.

<a id="plan"></a>
## Implementation plan

1. Inspect native import, catalog loading and metadata admission; verify primary source cards, snapshots and physical schemas. A separate read-only research subagent checks independent public sources under OpenSDLC's parallel-work guidance.
2. Add the generic acquisition helper and offline regression checks under `runtimes/data/`; keep downloaded data outside Git.
3. Extend `apps/job/src/bin/polymarket-history.rs` through an operator-only module using already locked Parquet/SHA-256/native aggregation libraries. Validate source identity, file integrity, asset mapping, bounds and timestamps before publication; preserve conservative qualification status.
4. Add native round-trip and corruption/duplicate/selection regression coverage and update the existing Polymarket CI job. Write one operator guide and link it from architecture.
5. Run targeted Rust/Python/format/architecture/document checks and a pinned, bounded public snapshot import with recorded measured results.

<a id="verification"></a>
## Verification

Implementation is complete for public snapshot acquisition and the two documented native adapters. Base main: `217afbb643317960c839bdea5515e912bb24f3d7`. Existing main checkout's untracked `.agents/` and `skills-lock.json` are preserved; work is isolated in `codex/polymarket-public-history`.


The generic Python helper and the Polymarket operator adapter are separate; no wire schema, migration, API, frontend or scientific admission rule changed. The three optional Rust dependencies reuse versions already present in Cargo.lock; the default job does not activate archive preparation.

Verified commands and observations:

- `python3 -B -m unittest discover -s runtimes/data -v`: 10 checks passed, including immutable revision resolution, Git/LFS integrity, byte limits, interruption/reuse, path/symlink safety, conflicts and missing terms.
- `cargo test --locked -p job --features polymarket-history --bin polymarket-history`: 13 checks passed. Public-schema Parquet round-trips cover numeric chain ordering, fill deduplication, exact cash/share interpretation, v1 USDC.e collateral, empty bar intervals, invalid source rejection and minute-end book boundaries.
- `cargo clippy --locked -p job --features polymarket-history --bin polymarket-history -- -D warnings`: passed with Rust 1.98.1. `cargo fmt --all -- --check` and `git diff --check`: passed.
- `make check-architecture`: passed. No source-specific dependency entered contracts/domain/store.
- `make check-links` with the CI-pinned lychee 0.24.2: passed, zero broken local links. `node --test deploy/install.test.mjs`: 4 checks passed. `make check-docs`: all links and 14 native CLI/Skill/schema checks passed. `cargo test --locked -p job --test catalog --test polymarket`: all 19 adjacent checks passed.

Bounded real-source verification used the actual downloader and native CLI, not fixture records. Local evidence is under `/home/zzy/projects/QuaZonai/.ai-bridge/polymarket-history/`, outside tracked source. All source files were size/hash checked against fixed Hub revisions:

| Source selection | Measured conversion | Evidence |
| --- | --- | --- |
| Moose `7eeb860dea5b79d5c74f3182b70bd08c85c8f833`, November 2022 `order_filled` (13,147 bytes; SHA-256 `c593bac9d72e1f4d9ca54855d4f33418abc564cfb729de9622f384e575ee71b7`) | 138 source rows scanned; selected token: 58 fills, 55 excluded exchange summaries, 46 self-trades retained, 7 rounded native prices, 43 one-minute bars | `moose-2022-11/snapshot.json`, `native-moose-2022-11-usdc-e/import-report.json`, detached source evidence and native catalog |
| Joseph `efe472f1f00a62f2cab1fed2851da439d9c1fe11`, 2026-06-06 minute books (98,063,548 bytes; SHA-256 `1075549c2b5c4a046060d439db1c0f29f7b17664c6ec960f89ae6d6d0fb6c160`) | 2,823,041 source rows scanned; selected token: 22 snapshots, 22 native quotes, 875 native snapshot deltas | `joseph-2026-06-06/snapshot.json`, `native-joseph-2026-06-06/import-report.json`, detached source evidence and native catalog |

Native Parquet row counts were independently read back with the existing Parquet library. Temporary inspection code was removed after verification. The source-backed token/condition identities used preparation-only native definitions, explicitly marked historically unverified and timestamped when observed. These tests establish real acquisition/conversion, **not** admissible historical research. An initial manual book definition used incorrect uppercase `PUSD`; correcting it to native `pUSD` resolved the expected deserialization rejection. Final v1 verification uses actual bridged `USDC.e` and rejects `USDC`/`pUSD` aliases.

<a id="review"></a>
## Review

An independent read-only subagent checked source semantics and the implementation. It found native Clear records with zero precision could not share a Parquet partition with instrument-precision Add records, and that book selection used minute-start labels while emitting minute-end events. Both were repaired at their proper shared boundaries and covered by full Parquet read/write and half-open window regressions. Final independent source/implementation/documentation review reported no remaining concrete findings.

Local independent review is complete. The GitHub PR owns final-Head hosted CI and Codex review results; pending checks or feedback are not approval to merge.

<a id="delivery"></a>
## First delivery and remaining source coverage

The implementation and source/usage guide are in the isolated `codex/polymarket-public-history` worktree. No deployment, background collection or bulk historical download was requested or performed. Existing user data and the main checkout's unrelated files are preserved.

The [operator guide](../../../runtimes/data/README.md) distinguishes two implemented native formats from other archives that can only be acquired. Gap-free history through today, FPMM normalization, late-v1 timestamped lifecycle joins, licensed v2 continuity and historical parameter/PIT qualification are **not complete**. Existing research admission continues to require that original evidence. Both real outputs correctly retain `UNPROVEN` coverage, `UNVERIFIED` historical availability and `registered_in_quazonai=false`.

<a id="qualification-followup"></a>
## Complete and qualified history follow-up

The owner subsequently requested complete and qualified data, rather than preparation alone. PR [#125](https://github.com/zhengui666/QuaZonai/pull/125) was merged at `af585486869b6143e54417e32cb51f803202b135`; its eight final-Head checks and explicit clean Codex review do not establish historical data qualification. Follow-up work is isolated in `codex/polymarket-qualified-history`. The owner's instruction to decide routine questions autonomously remains in effect. The original bounded-acquisition choice remains applicable; downloading every archive is not a substitute for qualifying it.

Acceptance requires original provenance and permissions, auditable coverage for an explicit market/time/type scope, historically available instrument/fee/universe/calendar evidence, coherent timestamped settlements when needed, original native files, fresh DATA_VALIDATE and real scientific execution. A full-history claim must include all requested eras and explain unrecorded off-chain fields; one passing sample cannot establish every market's completeness. Missing public evidence, unsupported fee regimes and unavailable service identity remain failures, not values to fill or flags to promote.

Implementation plan:

1. Independently investigate historical source evidence, the v1/v2 transition, creation/settlement events and anonymous chain corroboration. Freeze actual schemas, revisions, licenses and measured coverage.
2. Add an offline, venue-independent catalog preparation entrypoint, reusing the exact native quality measurement used by managed DATA_VALIDATE. Preserve declared source provenance and original instrument/timestamp facts; write isolated partition files and publish metadata last. Keep Store/Runtime qualification rules unchanged.
3. Close source-format and lifecycle gaps only from supported original records. Keep AMM economics, CLOB fee versions, event time and observed availability distinct. Do not reinterpret mid/depth records as fills or infer payouts from final prices.
4. Verify real source acquisition/conversion, original catalog readback, qualification rejection cases and scientific execution when source evidence permits. Preserve unresolved evidence explicitly; do not report the user's stronger objective achieved until it is established.

Independent work covers public-source research, the generic catalog preparation/shared-measurement implementation and the v2 parser in separate files. The primary agent owns captured-source adaptation, integration and this task record.

### Follow-up evidence

- Added optional `catalog-prepare`, shared native measurement with DATA_VALIDATE, physical partition isolation, original settlement siblings, readback and atomic final metadata publication. No schema, migration or admission relaxation. Fixture/UNVERIFIED declarations remain so; missing source fees and future definitions fail.
- Added `capture` for original Gamma plus two contemporaneous CLOB connections. It checks matching trade messages and resolutions, original timestamps, sequences and interruptions, exact quantities, historical parameters and fixed tick regime. Native aggregation now preserves late receive times. It does not claim that collector sequences prove exchange completeness.
- Added `time-seventeen-v2`: canonical event identity, both original v2 summary addresses, pUSD and exact unique six-decimal amount recovery. Normalized float amounts do not become original raw logs. The fixed inventory reaches 2026-09-25 but lacks 2026-09-10; early April files are preproduction.
- Real v2 revision `f14b2977b23852380f828e0a03ec0ad31af710c2`, 2026-08-09: the complete 206,660,190-byte file passed published SHA-256 `fd76acd9ec8e39cc989162210bf6683ee57b703f0dbe3c4940b0fe3afc965fa4`. Actual CLI scanned 4,015,344 rows; one selected token produced 5 trades / 3 bars, one excluded summary and no rounding or duplicates, with exact native readback. Its Gamma definitions retain today's observation time, so historical availability remains UNVERIFIED. A different BTC selection correctly failed without publication: 8 non-summary records had unrecoverable amounts. Across the day, 28,494 of 2,529,399 non-summary fills failed the strict amount check; two observed values are regression cases. This source cannot be accepted wholesale as lossless history.
- The actual lokima acquisition is pinned to `97d05eac7324a410db4dabb4f06b25cc21aae23f`, ODC-BY, with 30,900,196 selected bytes. Two interrupted downloads failed integrity checks without publication; a later complete transfer and CLI re-verification passed. No digest was bypassed.
- The first BTC window `[1785357600,1785357900)` has 1,946 matching, unique prints (Up 833 / Down 1,113), historical rate `.07` / exponent `1`, and two observed payouts. Prices and six-decimal sizes are lossless. Native output has 483 observed one-second bars and two closes. Empty seconds are not filled. The tick change is after this selected window. Both connection interruptions occur after the actual resolution receipt.
- Independent Polygon corroboration found `ConditionResolution` in block `91101943`, log `1088`, transaction `0x1ac67e0e0106e39e01e57cc0b3123f8b8ae21999faa906f07af174f9cf6d96c2`, payouts `[0,1]`. Block time `1785357935` is not the source's receipt time: the primary WebSocket received its resolution at `1785358010621464655` ns. Native settlement preserves the latter availability and the original WebSocket event time.
- Actual `catalog-prepare` plus `job execute` DATA_VALIDATE passed for Discovery 173, Validation 172 and Sealed 138 rows, accounting for all 483 bars. Source hashes remained unchanged and measured results matched the readback. Sealed quality omitted notionals and payouts. These local integration copies do not constitute an untouched scientific study or production registration.
- A separate actual native cash-account replay consumed one explicit integration-test target and all 483 bars plus two closes: 4 closed orders, 2 closed positions, no open orders/positions. Independent per-fill fee/cash reconciliation matched `0.693120 pUSD` in commissions; redemptions occurred exactly at the original resolution receipt and incurred no trading fee. The target is a deterministic execution check, not an Alpha/performance claim.

Evidence remains outside Git under `/home/zzy/projects/QuaZonai/.ai-bridge/polymarket-history/`: `lokima-btc-1785357600/snapshot.json`, `native-lokima-btc-1785357600/`, `qualified-btc-1785357600/run-20260927T021941Z-8a622b/acceptance-result.json` with its 84-file inventory, `simulation-btc-1785357600/independent-check.json`, and `v2-production-validation/verification.json` with the pinned daily snapshot, failed selection and successful native readback. Original licensing, clocks, raw provenance, independent audit and chain responses are retained with the acceptance run. Both reproducible local scripts are retained beside these outputs.

Independent review found that lexicographic payload storage could reverse equal-time prints and alter OHLC, and that the native live Gamma parser defaults malformed start dates to epoch zero. Capture now retains original frame/array order for timestamp ties and explicitly parses both original dates against native lifetime fields. Native Parquet regressions cover both cases; the reviewer confirmed both repairs. The reviewed actual capture was imported again into a new directory and its source/native records matched the earlier accepted capture exactly.

Local integrated tests: 18 history/capture/v2, 7 catalog preparation, 8 catalog, 21 managed and 11 adjacent Polymarket tests passed; optional-feature all-target Clippy passed with warnings denied. The 10 Python checks and architecture check passed. `make check-docs` passed all local links and 14 native CLI/Skill/schema checks using CI-pinned lychee 0.24.2. Hosted final-Head gates are recorded by the PR at delivery.

The live installed CLI lacks saved-device support and the current built CLI returns `CLI_LOGIN_REQUIRED`. No credentials were read, issued or fabricated; no production database or Runtime configuration was changed. Official source grant, Runtime registration, frozen InputSet and live admission remain unperformed. The tested capture window supplies actual missing historical evidence, but the user's strongest all-era/all-market completeness objective is **not established** by one window; the broad archive gaps above remain explicit.

### Original chain evidence follow-up

PR [#126](https://github.com/zhengui666/QuaZonai/pull/126) merged at `c3d0ebabd1154abe6e7da08708ea74301f257512`; its eight final-Head and post-merge checks passed, with explicit clean Codex review. The stronger objective remains active. Work continues in `codex/polymarket-chain-history` without altering the main checkout's unrelated files.

The eight rejected BTC records can be recovered from original Polygon ABI integers: two anonymous endpoints agree on all eight logs and block headers. Preserve those originals and the damaged vendor values. Add a bounded venue-independent EVM acquisition helper, then decode the official v1/v2 fill ABI in the existing optional operator boundary using already locked Alloy. Support independent raw-chain import and exact corroboration/replacement of matching v2 archive rows. Validate the chain/block/log/transaction/contract identity and original order parties, token, side and timestamp; never round a vendor value into guessed evidence. Provider agreement does not establish historical reception, finality timing, complete markets, fee schedules or permissions. Native scientific admission remains unchanged.

Parallel work is limited to the generic acquisition helper and its offline checks; the primary agent owns native decoding, integration, real-source verification and this task record. Broader source research remains read-only. All changes require real data validation and final-Head hosted review/CI before merge.

The implementation adds no new library version: optional Alloy core 1.7.3 and URL 2.5.8 already exist in the lockfile. Original JSON-RPC responses are frozen atomically by the generic Python helper. The native adapter rechecks identities, endpoint agreement, block/transaction ordering and ABI bytes before selecting assets. Normalized vendor damage is accepted only against the matching original ABI integer, within one adjacent float value and strictly less than half a base unit. The original archive is not rewritten; missing raw evidence leaves the previous strict rejection in force.

Real verification evidence is retained under `.ai-bridge/polymarket-history/native-chain-validation/`:

- `v2-eight-blocks.json`: a fresh actual helper acquisition, 1,833,215 bytes, SHA-256 `d596ba337e2fb5a8af5070842459e29f4fdc152a4689a474b0243dcd2ee357d7`. Both RPC endpoints agree on 777 logs across the eight explicit blocks. Native direct import selects 262 fills, excludes 120 summaries and produces eight observed minute bars, with 34 price quantizations.
- The formerly rejected full-day BTC selection now scans the original 4,015,344 vendor rows and imports all 3,809 selected fills / 29 observed minute bars, excluding 1,755 summaries. It corroborates 262 fills and recovers all eight damaged amount fields from original ABI integers. It retains 655 counted native price quantizations, with no duplicate or self-trade removal.
- `v1-2022-block.json`: a fresh actual helper acquisition of block 35,896,869, 34,044 bytes, SHA-256 `9d2e121cc42a39e55f103a1fe19bfddba7984e5935d6a57b1a7fc1a00e8caf7f`. The original v1 fill exactly matches the previously imported Moose record; the accompanying exchange summary is excluded.
- Independent native Parquet readback matches all original prepared records. A separate Python integer/Decimal calculation checks all 262 raw fills, all 3,809 vendor-selection records and every resulting OHLCV bar. The full original vendor snapshot hashes and original instrument observation times remain unchanged. `verify_native.py` and `independent-verification.json` preserve this check; generated data remains outside Git.
- A first import with unrelated market definitions correctly rejected the empty selection before publication. A public RPC rejected the default Python user agent; the collector now identifies itself explicitly and the real two-endpoint acquisition succeeded. No credential or paid fallback was used.

Independent read-only review found mismatched collector/reader endpoint-count and parent/time checks, plus incorrect manual IPv6 host parsing. The collector now rejects those inputs before publication and native host parsing uses the existing URL library. The reviewer reproduced rejection of the old cases and independently matched the 262 decoded fills. Regression coverage includes original BUY/SELL floating-point damage, same-order summary exclusion, RPC disagreement and the half-base-unit bound near `2^52`.

Local validation passed 23 history/capture/chain checks, 7 catalog preparation, 8 catalog, 21 managed and 11 adjacent Polymarket checks (70 total), plus 23 Python acquisition checks. Optional-feature all-target Clippy, formatting, diff whitespace, architecture and `make check-docs` (links plus 14 native CLI/Skill/schema checks) passed. A longer continuous acquisition exposed a connection failure after 119 complete blocks; no partial archive was published. The helper now retries only pre-body connection failures at most twice, with unchanged byte budgets; HTTP errors, partial bodies and inconsistent evidence remain final failures. Source grants and runtime admission were not implied by those results; a fresh native `client identity` still returned `CLI_LOGIN_REQUIRED` without reading any credential.

Broader read-only source work measured OpenMarket's actual sparse quote partitions and overwritten metadata, PMXT's database-insertion timestamp and absent metadata/resolution archive, and a krish301 repository containing only its card. The guide records the limitations. None supplies a newly qualified longer capture window. Chain timestamps remain UNVERIFIED historical availability, today's Gamma definitions retain today's timestamps, and no source grant, live Runtime registration or frozen InputSet is fabricated. This delivery improves precise historical-event support; it does not close the stronger all-era/all-market qualification gaps.
