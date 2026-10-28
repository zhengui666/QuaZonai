# Full-history cohort-cache factor research

<a id="task"></a>
## Task

**Task ID:** hyperliquid-cohort-research

**Original request:** Research Hyperliquid factors, backtests and portfolios for a few thousand USDT, with network jitter 0.1–1.5 ms and targets Sharpe >5 and annualized returns >150% in bull, bear and ranging markets. The owner authorized 100 initial factor trials, including failures.

**Endpoint:** Deliver the complete source-to-derived-cache-to-streaming-summary research path through the existing service and native worker. Existing native tick and BAR requests remain compatible. This step does not implement tick portfolios, claim the economic targets, or replace network measurements with event timestamps.

<a id="intent"></a>
## Intent

The complete selected source contains 1,308,222,597 candidate market rows and 614,393,279 exact instrument/event cohorts. Its 32-byte derived cohort representation is estimated at 18.310 GiB before actual output is measured. Native tick research currently materializes all trades, cohorts and result points and limits total source rows to one million. Merely increasing that constant would exhaust memory.

Current seven-feature Wasm ABI remains unchanged. Signed order flow, TWAP tags and liquidation factors need separate explicit feature contracts and are deferred; their absence must remain visible in the study rationale.

<a id="spec"></a>
## Requirements and design

1. Add explicit `input.representation: EVENT_TIME_COHORT_CACHE_V1`. Legacy absence means original native TradeTick input, retaining `maximum_rows` semantics and its existing cap. Cache input instead requires exact-string `maximum_source_rows` and omits `maximum_rows`; never reinterpret the old field as a working-memory limit.
2. Reuse mapped source filtering, native decimal conversion and the existing BigDecimal cohort reduction. Each cache frame is little-endian `(event_ns:u64, vwap_bits:u64, quantity_bits:u64, trade_count:u64)`. The statistical f64 marks are stored losslessly as their original IEEE values; raw source files stay unchanged. These are derived cohorts, not native TradeTick records or executable prices.
3. Each instrument has one immutable ordered file. Preserve unfinished cohorts across source files, reject time reversal and same-cohort duplicate/conflicting source identities, and enforce the existing complete-cohort row limit. Working memory is bounded separately from total source rows. Completed output includes actual coverage, precision, file sizes, source revision, filters and limitations.
4. Streaming evaluation opens instruments in the existing dictionary order, preserves one Wasm instance/EMA/ordinal stream per instrument across every source unit and partition, and carries the remaining global fuel to the next instrument. Features, warmup, labels and partition exclusions retain the original semantics. Labels use a bounded H-cohort queue; summary evaluation does not accumulate points or score vectors.
5. Pooled and per-asset partition metrics use stable online covariance, not an average of local ICs. Compare numerical results with the existing ndarray implementation using explicit tolerance and disclose the changed numerical method. Preserve unavailable statuses, exclusion counts and fuel behavior.
6. Prepared primary and independent datasets are explicit immutable range views over newly created derived cache files. Primary view byte ranges physically bound reads before validation end; independent view covers the original full request. The native reader validates the view against the immutable request, seeks only declared byte ranges and cannot widen a primary view to held-out data. No raw archive hardlinks or source mutation.
7. Reuse the local-config dataset provider, original research ledger, exposure logic, worker catalog root transport and dataset publication conventions. The cache representation remains explicit in prepared metadata, dataset, request and results. Preserve UNQUALIFIED_EXPLORATORY and EVENT_TIME_PROXY_UNVERIFIED.

### Shared implementation interface

Coordinator owns `apps/job/src/ticks/cohort_cache.rs`, the `market-history` cache operator integration, `operator/preparation/ticks.rs` cache preparation, documentation and this task. The cache module exports internally:

```text
pub(super) fn load(root: &Path, request: &NativeTradeTickFactorEvaluationRequestV1) -> Result<CacheInput>
CacheInput { pub coverage: NativeTradeTickCoverageV1, pub assets: Vec<CacheAsset>, ... }
CacheAsset { pub instrument_id: String, pub cohort_count: u64, ... }
CacheInput::cohorts(&self, asset: &CacheAsset) -> Result<CacheReader>
CacheReader: Iterator<Item=Result<super::Cohort>>
```

The Rust evaluation writer owns `ticks.rs`, `ticks/stream.rs`, Rust tick contracts/domain and their tests; the TypeScript writer owns `packages/contracts/src/tick-input.ts`, service export/revision integration and necessary TS callers/tests. Shared-file changes are serialized by the coordinator. No additional native command or worker path flag is needed: the existing `--catalog` points to the explicit cache/view root and `evaluate-tick-factor` dispatches by request representation.

<a id="plan"></a>
## Implementation plan

Source builder and bounded cache reader first; parallel Rust streaming evaluation/contracts and TypeScript registration/export callers against the shared interface; then preparation integration and complete source-to-service checks. Reuse official Nautilus conversions and EMA, existing Wasmi FeatureModel and remote execution. Verify old/native versus cache feature and metric parity, cross-file cohorts, source identity failures, bounded primary view, global fuel, and existing requests.

<a id="verification"></a>
## Verification

Implemented the mapped-source cache producer, exact frame readback, explicit range-view preparation, streaming seven-feature evaluation, TypeScript request/export/revision callers and existing native-worker test integration. Source conversion retains unfinished cohorts and identity checks across files; controlled CSV regressions cover the actual importer branch. The existing native HTTP/Durable test now exercises both representations through real worker commands when CI supplies the generated inputs.

Actual local checks:

- `cargo check --locked --offline -p domain` and the targeted domain tick-budget test passed.
- `CARGO_TARGET_DIR=/home/zzy/.cache/quazonai-cohort-check cargo check --locked --offline -p job --features market-history,catalog-prepare,hyperliquid-offline --lib --bin market-history --bin catalog-prepare --tests` passed, including all test metadata. It reported eight existing unused-code warnings outside the changed paths; no unrelated warning cleanup was attempted.
- `npm --prefix apps/service run typecheck` and `npm --prefix apps/research run typecheck` passed.
- `node --experimental-strip-types --test apps/service/test/tick-cache-export.test.ts apps/service/test/dataset-export.test.ts apps/service/test/dataset-revision-publish.test.ts apps/service/test/tick-input.test.ts apps/research/test/tick-input.test.ts` passed all 69 tests, without skips. The coordinator reran this exact command after integration. Independent review reproduced and confirmed fixes for mutable producer paths and incomplete primary byte ranges.

The metadata check does not execute the native tests. Native numerical, Wasmi/fuel, source-to-cache-to-preparation and actual worker runtime results remain pending CI. No full cache, research trial or portfolio has been generated by these software checks. The source census and ordering diagnostics are resource/coverage measurements, not qualification or tests of this change. A private target was moved off the small `/tmp` tmpfs after its space became exhausted; only this task's compiler cache moved, with no raw-data changes.

The first CI run reached native execution but failed the existing exact native-schema export comparison: the input contract changed while its checked source-derived snapshot had not been regenerated. The original native generator is now exposed as `make generate-native-schema`, and its output is regenerated rather than hand-edited. This correction changes no evaluator behavior; the corrected Head still requires its actual CI results.

<a id="handoff"></a>
## Handoff

Managed worktree `/home/zzy/.codex/worktrees/hyperliquid-cohort-research/QuaZonai`, branch `feat/hyperliquid-cohort-research`, initially stacked on PR #246 Head `e96517fac9cc5f7fbce5c5aad73f822b96a516cc`. The original dirty worktree and the entire raw archive remain unchanged. No factor trial budget has been consumed by this implementation work.
