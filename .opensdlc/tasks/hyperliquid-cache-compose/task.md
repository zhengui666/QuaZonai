# Compose complete source-day cohort caches

<a id="task"></a>
## Task

Enable parallel production and reuse of completed immutable day units for all 443 source days, then native composition into one full-history per-instrument cache. Research still evaluates each complete asset stream once; unit boundaries do not reset EMA, model state, ordinal, fuel or labels. No archive, registration, deployment or research trial is changed by this implementation.

<a id="spec"></a>
## Design

`market-history compose-cohorts` accepts the original processing manifest, native instrument recipe, completed unit output directories, a new output directory and the existing byte budget. Units must cover the manifest's entire declared interval in contiguous complete UTC days. Their original source/instrument recipes must match; selected source files must be disjoint and collectively equal the original declared trade selection. Units with outside-window rows, missing/rejected inputs, duplicates, truncated views or mismatched frame counts are rejected.

The command aggregates the actual native import reports, preserving source diagnostics and exploratory availability qualifiers. The existing cache reader validates every frame; concatenation preserves original IEEE values and both existing 32/40-byte layouts. Only one unit report/manifest is retained at a time. One output file per asset supplies the unchanged preparation/evaluation path. No new representation, schema, worker path or backtest engine is introduced.

Completed unit directories can be reused by composition; interrupted producers need new output directories. Unit staging is disposable only after successful full composition and preparation; raw sources remain protected.

<a id="plan"></a>
## Ownership

Coordinator owns `bin/market/compose.rs`, documentation and integration. The CLI writer owns `bin/market-history.rs` wiring and actual two-unit-versus-monolithic tests. The cache writer owns `ticks/cohort_cache.rs` streaming concatenation and frame tests. Work is isolated in `/home/zzy/.codex/worktrees/hyperliquid-cache-compose/QuaZonai`, branch `feat/hyperliquid-cache-compose`, based on `0e508195`.

<a id="verification"></a>
## Verification

After the parent optimized build released the shared target, actual local checks passed: all 28 tick tests, all 25 market-history CLI tests and all 19 native contract/preflight tests. These include actual two-day CSV imports composed versus monolithic imports for both formats, overlap/partial-day rejection and exact frame preservation including signed zero. The composed cache also passes native view preparation and produces exactly the same complete evaluation result as the monolithic source cache. The upstream resource correction is merged through the actual PR #250 ancestor; composition itself keeps only asset metadata and opens one asset-unit append writer at a time. Formatting/diff checks passed; the prior immutable-head review was clean and a corrected-head review is requested. Current-head CI and GitHub Codex review remain required; no full source conversion or trial was submitted.

### Composition review corrections

Corrected the current-head GitHub review findings: bare relative outputs resolve their empty parent to the current directory in both composition layers, and full-selection missing partitions are checked through the same selector used by source imports before unit coverage can be declared complete. The existing 32/40-byte source-to-composition test now covers relative output and missing spot partitions excluded by otherwise complete narrow day units. All 25 CLI tests and both cache/resource tests passed after this correction. The actual complete archive declares zero missing partitions, so the source producer recipe is unchanged.
