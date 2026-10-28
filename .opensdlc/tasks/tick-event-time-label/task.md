# Explicit event-time tick labels

<a id="task"></a>
## Task

Task ID: `tick-event-time-label`. Requested as the prerequisite for comparable 10/60-second Hyperliquid research horizons. Implementation starts from cohort base `c1293e60153f9871ff4c3abc30ea21e41bf94d38`; the base PR is coordinated separately.

<a id="intent"></a>
## Intent

Add a common event-time label duration without redefining the existing H-cohort label. This is exploratory factor research, not fixed-frequency execution, network-delay simulation, historical availability qualification or a real-order path.

<a id="spec"></a>
## Requirements and design

`FIXED_EVENT_TIME` carries exact positive `horizon_ns` as a counter string. The label uses the first complete same-instrument cohort at or after `t+horizon_ns`; its actual timestamp controls partition exclusion. Missing future cohorts remain incomplete. Old `FIXED_TRADE_COHORTS` JSON and semantics remain unchanged. Mixed fields and time overflow are rejected. Main features remain causal within the selected view. The accepted read-only plan was explicitly authorized for implementation.

<a id="plan"></a>
## Implementation plan

Rust contracts/domain add the tagged target and exact range validation. The native evaluator uses a monotonic future index; cache evaluation opens a second monotonic reader with O(1) label memory, charging only the main reader to the source budget. Existing H pending queue remains unchanged. TS validation, dataset facts and UI expose the actual unit; existing preparation/export callers preserve the target. Native scientific contract exports are regenerated through `crates/contracts/examples/generate.rs`; historical HTTP/Web snapshots are not hand-edited. Relevant tests cover irregular event spacing, first completion, boundaries/tails, old format, no future features, source budget, fuel and bounded prepared views. One parallel agent owns TS/UI; the feature owner owns Rust/docs.

<a id="verification"></a>
## Verification

Passed in the isolated feature worktree:

- `CARGO_TARGET_DIR=/home/zzy/.cache/quazonai-cohort-check cargo test --locked -p domain ticks::tests -- --test-threads=1`: 2 passed.
- `CARGO_TARGET_DIR=/home/zzy/.cache/quazonai-cohort-check CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -p job --features market-history,catalog-prepare,hyperliquid-offline,high-precision --lib ticks:: -- --test-threads=1`: 23 passed, 166 filtered out. An earlier invocation with different feature/debug flags was interrupted before the full build; the successful run uses the existing high-precision feature set.
- `node --experimental-strip-types --test apps/research/test/tick-input.test.ts apps/service/test/tick-cache-export.test.ts apps/cli/test/tick-schema.test.ts`: 13 passed.
- `npm --prefix apps/research run typecheck`, `npm --prefix apps/service run typecheck`, `npm --prefix apps/web run typecheck`: passed.
- `npm --prefix apps/web test -- src/native-input.test.ts`: 3 passed.
- `npm --prefix apps/web run build`, `cargo fmt --all -- --check`, `git diff --check`: passed.

The new worktree needed its own locked Web dependency install because the reused primary dependency directory lacked a declared icon package; no lockfiles changed. Independent read-only review of contracts, evaluation, view bounds, TS/UI and docs, including the final reader-parameter and error-propagation test adjustments, found no actionable issues. The initial claim that no current generator was required was incorrect: `contracts/generated/domain-v1.openapi.json` is the retained native scientific export and must match `contracts::openapi_json()` exactly. Inherited the base generator/documentation correction, then ran `make generate-native-schema` in this feature worktree. The generated schema is never hand-edited. The initial exact export comparison passed after generation; extending the existing tagged-union regression exposed Utoipa's internally tagged derive leaving variant objects open (18 passed, 1 failed). Source-defined `PartialSchema` now closes both target variants, matching Serde. After regenerating again, the full `polymarket_execution_preflight` suite passed 19/19, including `generated_schema_contains_the_exact_native_contract_export` and the strict target variant regression. This uses the same high-precision feature/cache/debug settings as the tick command above. Package source roots were refreshed sequentially when switching worktrees to prevent stale shared-target artifacts. Independent read-only incremental review found no issue. The 23 tick tests were rerun from these refreshed packages and all passed. Browser interaction, full retained native tests and final-Head hosted CI have not run here. These checks do not establish market performance, trading-node latency or executable holding periods.

<a id="delivery"></a>
## Delivery

Implementation is prepared for a stacked PR against `feat/hyperliquid-cohort-research`; retarget to `dev` after the base merges. Hosted final-Head CI and GitHub Codex read-only review remain required before merge. No deployment or real model trial has been performed.

<a id="handoff"></a>
## Handoff

Worktree `/home/zzy/.codex/worktrees/tick-event-time-label/QuaZonai`, branch `feat/tick-event-time-label`. No deployment, service configuration, raw data changes or real model trials are part of this feature.
