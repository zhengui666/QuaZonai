# Explicit signed-flow cohort cache

<a id="task"></a>
## Task

**Task ID:** hyperliquid-flow-cache

The owner's Hyperliquid research requires order-flow mechanisms as well as price controls. Add a complete explicit flow-cache path before generating the full source cache, so the 169 GB archive need not be reread merely to recover the omitted direction. This task is based on fixed-event-time labels at PR #248 Head `333ee22e`; it does not submit trials, deploy or generate a full cache.

<a id="spec"></a>
## Requirements and design

- Preserve native seven-feature requests and `EVENT_TIME_COHORT_CACHE_V1` 32-byte frames unchanged.
- Add `EVENT_TIME_COHORT_FLOW_CACHE_V1`: the same first 32 bytes plus one signed-quantity f64, calculated by exact BigDecimal BUY quantity minus SELL quantity in the existing reduction before conversion to f64. Unknown aggressors are rejected only in flow mode; balanced known flow is valid zero.
- The explicit `qz.tick-cohort-flow-features.v1` schema appends `signed_quantity` as the eighth feature under the existing function ABI v2. No TWAP/future labels enter features. Preserve both fixed-cohort and fixed-event-time targets, partition isolation, global fuel and source budgets.
- Complete producer, frame-width-aware range views, streaming evaluation, service export/revision binding, model briefing and tests. Existing worker `--catalog` transport remains unchanged.
- A 40-byte layout estimates 22.888 GiB per physical copy for the measured 614,393,279 cohorts. Actual output and available space must be measured before full preparation; compiler artifacts may be reclaimed only after their processes finish and tested binaries are available. Raw/user data remain untouched.

<a id="plan"></a>
## Implementation plan

Coordinator owns cohort-cache layout/producer/reader/preparation, market-history flags, native connected-test production and docs. The Rust writer owns tick contracts/domain, shared exact reduction, optional signed mark, streaming eighth feature and its tests. The TypeScript writer owns request validators/export/projection, model brief/tool text and tests. All changes share the managed worktree and one branch; shared files have one writer.

The existing full-source coverage result reports zero unmapped taker sides for both source streams. The final market-candidate filter is a subset; no additional full scan is required to motivate flow support. This observation is not independent source qualification.

<a id="verification"></a>
## Verification

Actual local checks passed: 26 native tick tests (including both label modes and the eighth feature), two domain tick tests, job library/binary/test metadata checking, service/research/web typechecks, 74 selected TypeScript tests, four web input tests and the production web build. Source-import CLI runtime and connected native HTTP checks remain pending, as do the final source-derived schema regeneration, immutable-head review and CI. No full archive conversion, model trial or profitability result is claimed.

<a id="handoff"></a>
## Handoff

Managed worktree `/home/zzy/.codex/worktrees/hyperliquid-flow-cache/QuaZonai`, branch `feat/hyperliquid-flow-cache`, based on `333ee22e9d000da63b4f1b30e8605f3817f77a0d`. Other worktrees and their CI continue independently. This task does not change the owner's 100-trial budget.
