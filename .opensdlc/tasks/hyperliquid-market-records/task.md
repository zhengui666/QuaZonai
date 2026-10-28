# Preserve historical market trade identity

<a id="task"></a>
## Task

**Task ID:** hyperliquid-market-records

**Original request:** Continue the owner's Hyperliquid factor, backtest and portfolio research for a few thousand USDT, targeting Sharpe >5 and annualized returns >150% in bull, bear and ranging markets. The owner clarified network jitter of 0.1–1.5 ms between the local machine and trading node, and authorized 100 initial factor trials, including failures.

**Endpoint:** Repair the source importer defect discovered during full-archive input diagnosis. Deliver through a PR to `dev` with the applicable current-Head CI and reviews. This repair does not complete the investment research or establish profitability.

<a id="intent"></a>
## Intent

The importer currently keys every historical trade by `(instrument_id, trade_id)`. Hyperliquid's [official trade type](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions) specifies `(block_time, coin, tid)` as the global identity because `tid` is an order-pair hash. Reusing an ID at another timestamp must not delete a legitimate record or report a conflict.

The original archive and raw native trade IDs remain unchanged. Same-time duplicate/conflict behavior remains intact. This is a necessary input correction, not permission to mark unprepared datasets ready or bypass unsupported tick portfolios.

<a id="spec"></a>
## Requirements and design

- Include the original trade event timestamp in the importer and shared tick catalog reader identity keys, alongside its native instrument and source trade ID.
- Preserve records sharing instrument/ID at different event times. Continue detecting repeated or conflicting records at the same event time.
- Keep existing BAR aggregation, native precision, source provenance, counts and failure behavior.
- Do not invent IDs for special source rows. Separate diagnostics found `Spot Dust Conversion` rows with ID zero at one timestamp; event-time identity alone does not resolve those rows. Their market-versus-account classification remains separate work.

<a id="plan"></a>
## Implementation plan

Inspect the shared import loop and its existing duplicate/resource tests. Change the key once, add a small regression covering cross-time reuse and same-time conflict behavior, and run the relevant importer tests using available native build artifacts. Update source documentation only where its identity description changes.

<a id="verification"></a>
## Verification

The importer and shared tick reader keys now include `trade.ts_event`; the original native ID is unchanged. Importer and reader regressions cover a reused ID at a later time, same-time duplicates and same-time conflicting values. Existing exact-duplicate coverage remains. Catalog preparation and tick evaluation reuse the shared reader, so imported records remain consumable downstream.

`cargo fmt --all -- --check` and `git diff --check` passed. Native tests have not run locally: no matching recent Cargo target cache was found, and a duplicate full build would consume the limited disk reserved for the source archive. The existing CI mapped-public-history step executes the changed binary tests with `market-history,catalog-prepare,hyperliquid-offline`; its actual result remains pending.

The investment-research diagnostic inspected 886 original files and 2,617,960,184 raw rows; these data statistics are not tests of this patch. The original worktree remains untouched by product edits.

<a id="review"></a>
## Review

Independent review of the initial importer-only commit found that the shared reader still used the old identity. The reader and its regressions were added before delivery. Other identity maps in the independent Polymarket importer and simulated fill accounting have different source meanings and remain unchanged. Review of the final corrected Head and its CI is pending in PR #244.

<a id="handoff"></a>
## Handoff

Worktree: `/home/zzy/.codex/worktrees/hyperliquid-market-records/QuaZonai`; branch: `fix/hyperliquid-market-records`, based on remote `dev` at `a0682f8c`. The importer writer owns the Rust implementation/tests; the coordinating writer owns this task record and documentation. No submission has consumed the 100-trial research budget.
