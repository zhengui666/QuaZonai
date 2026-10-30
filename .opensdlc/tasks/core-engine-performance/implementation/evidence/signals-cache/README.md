# Task-local Wasmi compiled-module cache: implementation measurement

This standalone crate directly includes the current production `apps/job/src/signals.rs`. Its baseline compiles `SignalModule` at every model/cutoff invocation. Its treatment calls the implemented `SignalModuleCache::get_or_compile`; both instantiate fresh Stores and models for every asset/cutoff and carry each model's remaining fuel across the whole task. Both use the same current source and dependencies. This comparison isolates the cache's contribution after the dependency upgrade; it is not a whole-commit or end-to-end speedup.

The synthetic Wasm model has a mutable global counter and a model-specific constant, plus a floating-point division. Before timing each case, the harness checks every predicted floating-point bit against the uncached arm and an analytic model output, and checks remaining fuel after every prediction. Each measured pair must also have exactly equal output checksums and fuel consumption. No market, economic, or historical PIT claim follows from this synthetic workload.

| Case | Models | Cutoffs | Assets | Predictions per instance | Mode |
| --- | ---: | ---: | ---: | ---: | --- |
| short_metered | 8 | 16 | 2 | 16 | Explicit fuel |
| long_metered | 8 | 2 | 2 | 4,096 | Explicit fuel |
| saturated_metered | 64 | 4 | 2 | 16 | Explicit fuel; exceeds 32-entry admission limit |
| short_unmetered | 8 | 16 | 2 | 16 | Fuel metering disabled |

Each arm creates a new task-local cache; first compilations, exact-byte cache lookups, fresh instantiation, predictions, reuse accounting, and cache teardown are timed. WAT parsing and per-value equivalence validation are outside the timer. Every case has 5 warmup pairs and 30 measured pairs, with alternating arm order. Weak references identify actual reused compiled modules without keeping uncached modules alive. Expected treatment reuse is `min(models, 32) × (cutoffs − 1)`; excess models still compile and execute normally. The cache holds at most 32 admitted entries and 16 MiB of source bytes; this is not a bound on compiled IR heap or process RSS.

Build only after other compilation and measurements have stopped, using an independent target directory:

```sh
export CARGO=/workspace/.quazonai-setup/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin/cargo
export RUSTC=/workspace/.quazonai-setup/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin/rustc
export CARGO_HOME=/workspace/.quazonai-setup/cargo
export CARGO_TARGET_DIR=/tmp/qz-signals-cache-target
bash .opensdlc/tasks/core-engine-performance/implementation/evidence/signals-cache/build.sh
python3 .opensdlc/tasks/core-engine-performance/implementation/evidence/signals-cache/run.py \
  --binary "$CARGO_TARGET_DIR/release/qz-signals-cache-bench" \
  --rustc "$RUSTC" \
  --build-identity .opensdlc/tasks/core-engine-performance/implementation/evidence/signals-cache/build-identity.json
```

All harness dependencies are compiled in release mode and use versions present in the project lockfile. The run is pinned to one allowed CPU. `build-identity.json` records the exact source, harness, and lock hashes before compilation; the runner refuses inputs that changed before execution. Raw paired observations, summary statistics, resource usage, compiler/CPU metadata, and binary SHA-256 are saved alongside this file. Resource peak RSS covers the whole harness process, including both arms and untimed validation; it does not compare cache memory against baseline memory. The recorded kernel results exclude catalog loading, feature construction, forecast conversion, covariance, optimization, backtesting, and orchestration.

Results are pending execution. Historical measurements under `../../../evidence/signals/` remain separate and must not be presented as this implementation's results.
