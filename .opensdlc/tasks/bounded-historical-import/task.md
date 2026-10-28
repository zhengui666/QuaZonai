# Prepare bounded historical source imports

<a id="task"></a>
## Task

**Task ID:** bounded-historical-import

**Source:** The owner's Hyperliquid research goal; local full-archive diagnosis of 886 files and 2,617,960,184 raw rows. The owner authorized 100 initial factor trials, failures included, and clarified that 0.1–1.5 ms describes network jitter to the trading node.

**Endpoint:** Deliver the minimum source-import changes needed to prepare explicit bounded units without modifying original files. This does not complete full-history factor research, portfolio execution or performance targets. PR #244 separately corrects trade identity and remains unchanged while its CI completes.

<a id="intent"></a>
## Intent

The existing importer filters files before decoding, but its prefix validation compares an expanded date against a literal `{date}` profile template. As a result, each bounded event-time import must select top-level stream prefixes and rescan the full history. Also, `crossed=true` includes source account-conversion rows that must be separated from ordinary market transactions.

Full-archive classification found 1,115,292 `Spot Dust Conversion` taker-marked rows, all with ID zero. A prior single-day check found different prices/quantities at the same time/market/ID. Original data must remain intact; these are not records to merge or assign invented IDs. Other special directions, including settlement, vault netting and deleveraging, require separate interpretation rather than automatic exclusion based on a label alone.

<a id="spec"></a>
## Requirements and design

- Permit concrete date prefixes that select existing manifest files within a declared stream directory and the original acquisition selection. Preserve existing top-level prefix selection, including a declared top-level stream with no file in a mixed selection; invalid, outside-scope or unmatched concrete date selections still fail.
- Add optional generic scalar `not_equals` row conditions, reusing existing equality semantics and filtered-row accounting. Existing profiles remain compatible. Arrays/objects are unsupported filter values.
- Keep complete processing declarations in the report's existing embedded source manifest. Never overwrite acquisition manifests or raw files, change clocks/precision, or invent source IDs.
- Do not silently change the shipped profile or apply a new filtering declaration to an old operator. Any later processing view must be explicitly separate from the original acquisition and executed by a binary supporting the conditions.
- Do not raise the million-row limit or claim that conversion-unit success establishes complete research coverage, historical contract validity or profitability.

<a id="plan"></a>
## Implementation plan

Reuse `selection_prefixes`, `selected_files`, `Stream::matches`, existing report accounting and short inline importer tests. No replacement downloader, scheduler, converter or backtest engine is needed. The next step after delivery is an explicitly limited resource-measurement unit, followed by full-range processing design; a measurement unit will not substitute for the requested full research.

<a id="verification"></a>
## Verification

Implemented generic `not_equals` conditions and concrete-file prefix selection. Added small regressions for selecting one date before decoding, declared exclusion/crossed handling and source preservation, invalid/out-of-acquisition prefixes, and nonscalar conditions including null source fields. Existing default-profile tests remain unchanged.

`cargo fmt --all -- --check` and `git diff --check` passed. Native execution remains pending CI; no recent reusable local Cargo target was found, so no duplicate full compilation was started on the archive disk. The full-archive classification scan completed with 1,309,537,738 taker-marked rows matching earlier coverage counts; this is input diagnosis, not a regression test or a trading result.

<a id="handoff"></a>
## Handoff

Worktree: `/home/zzy/.codex/worktrees/hyperliquid-import-units/QuaZonai`; branch: `fix/hyperliquid-import-units`, based on reviewed PR #244 Head `2a0509c`. Reconcile PR #244's merge before creating the final PR against `dev`. The implementation agent owns Rust changes/tests; the coordinator owns documentation. No factor trials have been submitted.
