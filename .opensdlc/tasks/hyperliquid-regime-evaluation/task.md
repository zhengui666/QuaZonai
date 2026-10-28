# Native tick evaluation by causal BTC daily regime

<a id="intent"></a>
## Intent

The authorized full-history Hyperliquid study needs prediction statistics in bull, bear, range and unknown states before its first four factor trials. Preserve all 523 identities, the 443-day interval, fixed 60-second labels, chronological independent partitions and the shared 100-trial budget. This change neither submits trials nor claims strategy/account performance.

<a id="spec"></a>
## Design

An optional tick-only `evaluation_regimes` object carries completed BTC endpoint marks for each UTC day. The only method uses the D-1 and D-31 last observed cohort VWAP; exact positive decimal comparisons against strict +5%/-5% boundaries derive BULL/BEAR/RANGE. A missing endpoint derives UNKNOWN. The original event clock remains an unverified availability proxy. Endpoint values are declared research inputs, not independent authenticity proof.

Native validation requires cache representation, consecutive complete UTC days covering the selection and whole-day partition cuts. Each endpoint belongs to its specified past day. Group membership is determined by the decision event, never by future label time; labels crossing a regime boundary remain eligible when they stay within the original partition. All original exclusion reasons and aggregate selection metrics remain unchanged. The state does not enter the Wasm feature ABI or reset EMA, model, ordinal or fuel.

Reuse native online Pearson/count accumulators per partition/state/asset, retaining empty assets and UNKNOWN. Report included-pair actual label-span min/max and fixed bucket counts, without retaining points. Primary requests clip day inputs at the validation boundary. Model briefs omit daily endpoint payloads; iteration feedback exposes TRAIN/VALIDATION groups only. Sealed inputs and final evaluation remain owner-only.

`prepare_request.py REQUEST DAILY_SUMMARY DAILY_LEDGER NEW_OUTPUT` binds the already computed daily ledger to an existing full-scope request. It preserves all factor settings and verifies its exact endpoint classification agrees with the original ledger; it does not scan source trades or submit a trial.

<a id="plan"></a>
## Implementation

Rust contracts/domain own the optional request, endpoint validation and response. `ticks/stream.rs` reuses the existing metric path; `prepare_views` clips evaluation days with the primary range. TypeScript contracts, research request clipping, model projection and dataset export follow the same rule. Old requests retain their serialized result shape. The existing official contracts generator produces the source-derived OpenAPI document.

Small software cases compare grouped native metrics with explicitly grouped original points for both label modes, multiple assets, missing states, cross-day labels, partition exclusions, counts and unchanged fuel. TS cases exercise real dataset publication and primary/sealed clipping. The prior C01/C03/F01/F02 formulas fit the existing eight-feature ABI; no formula DSL, account replay or new worker path is added.

<a id="verification"></a>
## Verification

Local Rust job/domain builds are intentionally not run: disk is reserved for full archive conversion. The owner allowed only the official contracts example generator in an isolated small target. Native scientific tests run in CI. Targeted research/service TypeScript tests and typechecks are recorded after execution; queued or unrun checks are not passes.

Executed: 12 research tests, 9 real cache-export tests and both TypeScript typechecks passed. The official `cargo run --locked -p contracts --example generate` completed in the isolated 280 MiB target, which was then removed; no job/domain build ran. `prepare_request.py --self-check` passed, and binding the existing real 443-day ledger reproduced all 31 UNKNOWN, 130 BEAR, 165 RANGE and 117 BULL assignments. The resulting 523-instrument, 60-second request passed the TypeScript validator without submitting a trial. Native tests remain pending CI.

<a id="review"></a>
## Review and delivery

Final-commit CI, independent review and GitHub Codex review are pending. No deployment, registration, factor submission or account result is part of this change.
