# Nautilus 0.67 EMA initialization: numerical migration evidence

This is a scientific behavior change in the official `nautilus-indicators` crate, not an optimization equivalence claim. Production QuaZonai keeps the upstream implementation and reports `nautilus-indicators: 0.67.0`; it does not emulate the 0.63 seed or change the regression estimator to preserve an obsolete fixture.

## Upstream source

The downloaded crates identify these official revisions in `.cargo_vcs_info.json`:

| Crate | Official source | SHA-256 of `src/average/ema.rs` |
| --- | --- | --- |
| 0.63.0 | [a040025, update_raw L118–136](https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/indicators/src/average/ema.rs#L118) | `bb23c6c675132b277ecfcb4e6d4f309fd876b89cd4f81e8802f671074fcfae80` |
| 0.67.0 | [d53849b, update_raw L139–158](https://github.com/nautechsystems/nautilus_trader/blob/d53849b216e20f2c122ca01b4dd0da377638b186/crates/indicators/src/average/ema.rs#L139) | `b64004d7339525a3a47ed1c90cc615ca58d7f4715c2b737e70d0cf19cff7cd21` |

0.63 seeds `value` from the first observation, recursively updates immediately, and marks initialized after `period` observations. 0.67 accumulates `seed_sum`, sets the first initialized value to the arithmetic mean of the first `period` observations, then uses the same recursive FMA. Both first become initialized at observation `period`. QuaZonai additionally requires a previous price and both EMA windows, so with periods 2/3 its first prediction remains observation 3 (ordinal 2).

For the exact decimal path `p_n = base + n/1000`, the new EMA(2) at observation 3 is `base + .0025`; EMA(3) is `base + .002`. The old first-value seed gives `base + 23/9000` and `base + 9/4000`, respectively. The differences are approximately `5.556e-5` and `2.5e-4`, far above binary rounding. With the new seed, the two EMA lags on this linear path remain `.0005` and `.001`: their spread is mathematically constant `.0005`.

## Observed failure and its cause

The original ensemble fixture used linear prices and the two Wasm predictors `close` and `fast_ema - slow_ema`. On the first EUR/USD training fold, the latter now has only binary rounding variation. The actual unchanged production `linregress` SVD estimator and independent centered scalar OLS produced:

```text
training cutoff ns = 1500000000000; n = 20
score_min = 4.99999999999944933e-4
score_max = 5.00000000000166978e-4
sum((x - mean(x))^2) = 2.24332329326180039e-31
scalar intercept = 1.50534015669926833e7
native intercept = 1.97537221917919517e-3
scalar slope = -3.01068031300292473e10
native slope = 9.87686109589774211e-7
```

These were printed by a release run of the failing `original_panels_fit_fold_local_ols_and_match_independent_scalar_qp` test (`/tmp/qz-ensemble-ols-diagnostic.log`). The scalar formula divides by essentially rounding noise, so its coefficient assertion has no identifiable reference value. This is not evidence for relaxing a tolerance or changing production OLS. The diagnostic run used Rust 1.99.1 and `--locked --release --all-features`; unlike the subsequent verification, it omitted the coordinator's explicit `CARGO_PROFILE_RELEASE_DEBUG=0`/`CARGO_INCREMENTAL=0` environment and is not a performance comparison.

## Regression coverage and correction

[`apps/job/tests/ensemble_fit.rs`](../../../../apps/job/tests/ensemble_fit.rs) now writes a separate declared synthetic catalog for its fitting fixture. Its exact five-decimal prices are `(base_ticks + 100*n + n*n)/100000`, with the same instruments, event/availability clocks, volumes, selection, fee-source bytes, Wasm programs and causal folds. No production data or scientific report is rewritten. A fixture assertion requires training feature centered sum of squares above `1e-9` before using the scalar coefficient oracle.

The original coefficient limits remain **intercept `< 1e-10`, slope `< 1e-8`**. The independent constrained two-member QP loss limit remains `< 1e-11`; emitted loss, exact decimal sum of weights, held-out isolation and native apply/replay checks also remain.

The added `native_sma_seed_preserves_warmup_and_exposes_the_linear_spread_counterexample` test retains the original linear price path and runs the actual Parquet/forecast/Wasm pipeline. It checks both instruments, the two missing warmup predictions, first prediction ordinal/event/availability/label clocks, independently derived fast/slow values at every available prediction, the theoretical constant spread, and the materially different old-seed first values. An exactly constant predictor still triggers the existing `CALIBRATION_CONSTANT_SCORE` rejection. No new production near-constant threshold is introduced.

Existing model artifacts can consume different startup features after this upstream upgrade even when their Wasm bytes are unchanged. Old forecast/calibration evidence cannot be represented as numerical identity with a 0.67 rerun. Under this migration's explicit incompatibility policy, models and calibrations must be regenerated/refitted and reevaluated under the new feature semantics; no old first-observation-seed implementation is retained. The new evidence must retain its native versions and model/selection/fold identities. This fixture validates the declared migration; it does not assert unchanged performance for real models or fix every possible ill-conditioned predictor. No production schema or model ABI was changed for this fixture correction.

## Verification

Passed with Rust 1.99.1 and the coordinator's stable build environment:

```sh
env CARGO_HOME=/workspace/.quazonai-setup/cargo \
  RUSTUP_HOME=/workspace/.quazonai-setup/rustup \
  PATH=/workspace/.quazonai-setup/cargo/bin:$PATH \
  CARGO_BUILD_JOBS=3 CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p job --release --all-features --test ensemble_fit -- --nocapture
```

Result: **10 passed, 0 failed**, test execution 0.25 seconds (compilation 6m12s, excluded from any performance measurement). Log: `/tmp/qz-ensemble-ols-fixed.log`. This includes the unchanged strict OLS/QP tolerances, new SMA-seed regression, actual CLI fit/apply and native rolling-account tests. The multi-day export test returns early unless its explicit export environment is set; its presence in this ordinary pass count does not prove a fresh export. The separate connected-core/export validation remains the coordinator's responsibility.
