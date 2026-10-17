# Recorded v2 quantity-bounded native fills

## Goal and observed defect

Continue the owner's USD3000 conditional native shared-account research, using only TimeSeventeen/Polymarket-v2 and openai:gpt-6.1-sol / max. PR201 is delivered and deployed at accepted Head40d7d1a3. The owner additionally authorized decimal half-up rounding of model OHLC to the unchanged0.0001tick and requested long training/validation windows. The current genuine input is training Apr3–Jul1 (89calendar/70observed days,6373BARs), validation Jul1–Sep1 (62/46days,1767BARs), independent Sep1–Oct1 (30/28days,2103BARs), without gap filling or October exposure reset.

Research139/submission147 attempted one complete ensemble155 and stopped on `RECORDED_FEE_QUANTITY_EXHAUSTED`: native fill4999.683675shares at1783876680000000000, unmatched4998.734700; the source minute covers0.948975shares. Every source BAR volume already equals its same-ID native trades and original fee records. This is not missing fee evidence: the native synthetic fill exceeds available original quantity. Do not guess a fee, reuse records, extrapolate quantity or weaken the strict fee allocator.

Preserve studies4/36/103/139 and all requests, trials, receipts, original data, databases, authentication and extensions. Cumulative attempts18 (5+9+3+1), next allowance299982 including failures. No independent September performance was evaluated; no portfolio or Sharpe result exists. No real orders, funds operations or account binding.

## Bounded design

- Add an explicit QZ-owned recorded-liquidity fill reference through official Nautilus0.63.0 `FillModel`. Keep legacy `NAUTILUS_DEFAULT_FILL` and original recorded-fee behavior unchanged, so prior request semantics are not silently rewritten.
- Pair the new mode only with `POLYMARKET_V2_RECORDED_FEES`, catalog backtests, pUSD/MARGIN1 and existing long-only/gross bounds. Keep native Paper unsupported and existing ordinary/authentic BinaryOption rules intact.
- Reuse the recorded-fee model's same shared replay clock and per-instrument/latest-completed-minute quantity pool. Build native fill liquidity from original remaining positive quantity, consumed only when the native fee callback books a genuine fill; no separate counter or duplicate source consumption. No source role inference. Missing/unmatched data still fails. Nautilus's public custom-book API falls back to default liquidity when a returned book produces no fills, so exhausted quantity MUST explicitly fail rather than return an empty book and accidentally permit unlimited fills.
- Use official book/price/quantity/order APIs. Preserve native price formation, declared seeded fill probabilities, positive latency and exactly one adverse native tick. MatchingEngine applies `is_slipped()` after custom-book fills too: delegate this once to the official DefaultFillModel, do not shift the custom book price as well. Do not copy/patch/vendor upstream implementations, alter observed prices beyond the owner's recorded tick rounding, fabricate fills, manually book cash or substitute account statistics.
- Native engine performs partial/no fills and retains order state. The original fee callback still charges transaction-linked fee_usdc by exact quantity fractions, once per fill, with unchanged half-even pUSD rounding and full lineage. No fee-rate fallback.
- Declare new fill policy explicitly in a subsequent linked request after delivery. Do not edit admitted139 input or rerun market strategies outside the durable research ledger.
- Public API inspection also shows L1 `apply_fills` extrapolates any non-IOC market remainder after the custom book (official lines4850–4935). The new explicit policy therefore uses native MARKET IOC orders. Native cancellations settle partial fills; abandon dependent deferred increases after a canceled reduction, and compute subsequent targets from actual holdings. Default-fill orders remain unchanged. No synthetic remainder, manual order fill or assumed released margin.

## Checks and delivery

Add closed-wire/schema and unsupported pairing/version tests; synthetic native regressions for oversized orders, partial fills, cross-side/repeated pool use, exhaustion/no fallback, actual clock boundaries, unchanged one-tick slippage, booked commission/ending balance and legacy strict over-quantity rejection. Reuse existing native integration helpers; no weakened assertions or generated-contract hand edits.

Run applicable local scientific/core/TypeScript checks, then exact final-Head CI and explicit clean GitHub Codex read-only review. Merge remote dev only after actual acceptance. Deploy compatible exact tested native artifact and matching QZ source into the original service; preserve private config/DB, backups, extensions and existing source overlay. Host-built native binary remains glibc-incompatible and is not deployable.

Then verify original4/36/103/139 immutable facts through installed quazonai CLI, register an explicitly updated fill-policy binding, and resume actual long-window shared-account research once with maxTrials299982 and stable original request ID. Continue validation search rather than treating source repair or factor IC as Sharpe success; independently evaluate only an original selected ensemble and obtain native single-account portfolio evidence through CLI.

## State

Implemented on `fix/polymarket-v2-recorded-liquidity`, based on remote dev1d98fcd5. Contract/domain checks and 14 focused native fee/liquidity tests pass. The one independent review's concrete cancellation-resumption finding has been reproduced and corrected. No new research after139 admitted; cumulative use remains18. Maintenance tests use synthetic data, never ledger-external market strategy simulations.

## Public API findings for implementation

- Official nautilus-execution0.63.0 `matching_engine/mod.rs`4217–4265 calls FillModel.get_orderbook_for_fill_simulation; a book with no fills falls back to default liquidity. Require positive remaining quantity and supported MARKET orders before returning a nonempty source-bounded book, otherwise explicitly fail. Do not use an empty-book/zero-size trick or pretend unsupported no-fill is supported.
- The engine catches fill-model callback errors and only logs them (fill_market_order around4323–4337). Logging is bypassed in our JSON transport. Capture the first callback failure in a shared in-memory observer and check it after engine.run, like the existing FillFeeObserver; otherwise an exhausted pool could produce false computed success. No extra persistent store.
- Synthetic fill-model books skip the engine's usual liquidity-consumption counter (around4369); use the SAME per-minute shared recorded-fee quantity pool, not an independent book/reset counter. FeeModel updates that pool only for native charged fills. Read-only fill previews cannot consume it.
- Engine applies FillModel.is_slipped after normalizing fills (around4751). Keep the book at original native bid/ask and delegate the seeded DefaultFillModel probability methods; do not pre-shift prices or double-charge slippage.
- Existing native fee tests live apps/job/src/recorded_fees/tests.rs (7tests, helper market/source/order and actual account test222–354). Existing strict over-allocation test165 must remain. Native DefaultFill + old fee model MUST retain139's strict rejection behavior.
- Suggested explicit wire: POLYMARKET_V2_RECORDED_LIQUIDITY, native_version0.63.0, closed existing NautilusFillParametersV1. New domain branch only when paired with original POLYMARKET_V2_RECORDED_FEES; use normal pUSD/MARGIN1 validation. Add QZ variant to source schema (10variants total), not generated historical contracts.
- The new adapter lives under `apps/job/src/recorded_fees/liquidity.rs`; it shares the existing allocator and uses official public APIs only. Original PR201 remains delivered; its acceptance does not accept this repair.

<a id="verification"></a>
## Verification

- Focused native: `cargo test --locked -p job --features native-paper-test,native-sandbox-test,catalog-prepare,polymarket-history --lib recorded_fees:: -- --test-threads=1`: 14 passed after the cancellation correction, including consecutive-target resumption, partial IOC/source caps, shared read-only previews, actual clock boundaries, first callback error observation after upstream swallow, original strict failure, compiled Paper refusal, exact fees/native balance and one adverse tick.
- Contracts: 6 native-model tests passed. Domain: 13 native-portfolio output and 4 prediction-source tests passed. Targeted Clippy with warnings denied, scoped Rustfmt and diff checks passed.
- Full `make check-science` completed 493 tests/40 suites before the final cancellation-resumption correction; affected native tests were then rerun (14 passed). Matching final executable build and `make check-core` passed (2/2 real native flows). TypeScript service/research/CLI checks passed (58/27/3). Web and deploy checks passed after restoring locked Web dependencies (initial `tsc` was absent); no source or assertion changes were needed. Final-Head GitHub CI remains mandatory.
- The failed first synthetic callback test reached target-expiry handling rather than the intended callback error. Replaced it with two genuine native IOC orders on the same replay clock: the first consumes original liquidity, the second callback is swallowed by the upstream run but explicitly observed as exhausted, with no fallback fill. A separate regression verifies partially canceled reductions retain actual holdings.

<a id="review"></a>
## Review and delivery

One independent local read-only source review found P2: a capped partial IOC cancellation did not resume a target already waiting for settlement. A new consecutive-BAR regression failed before correction (second fill240000000000 vs expected180000000000), then passed after calling the existing `resume_after_settlement` after cancellation bookkeeping. The reviewer reported no other concrete findings. This is not a claim of final-Head CI or hosted review acceptance.

New exact-Head CI, explicit GitHub Codex read-only review, remote-dev merge and compatible tested deployment are not yet completed. The PR owns subsequent delivery status and native reports. Do not substitute PR201 evidence or host-built binaries. Deployment must preserve all four CLI study snapshots and the original service's private configuration/DB/source-overlay settings.
