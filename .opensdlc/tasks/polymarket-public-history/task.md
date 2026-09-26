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
## Delivery and remaining source coverage

The implementation and source/usage guide are in the isolated `codex/polymarket-public-history` worktree. No deployment, background collection or bulk historical download was requested or performed. Existing user data and the main checkout's unrelated files are preserved.

The [operator guide](../../../runtimes/data/README.md) distinguishes two implemented native formats from other archives that can only be acquired. Gap-free history through today, FPMM normalization, late-v1 timestamped lifecycle joins, licensed v2 continuity and historical parameter/PIT qualification are **not complete**. Existing research admission continues to require that original evidence. Both real outputs correctly retain `UNPROVEN` coverage, `UNVERIFIED` historical availability and `registered_in_quazonai=false`.
