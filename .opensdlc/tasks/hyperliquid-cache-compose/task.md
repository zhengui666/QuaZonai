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

Implementation authored while the parent's optimized build owns the shared Cargo target. No compilation or tests have run yet. Planned existing tests compare actual two-day CSV imports against monolithic imports for both formats, including overlap and partial-day rejection; cache tests cover exact values and full-frame validation. Final CI and immutable-head review remain required.
