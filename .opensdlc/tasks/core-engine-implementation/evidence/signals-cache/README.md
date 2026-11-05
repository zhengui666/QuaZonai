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
bash .opensdlc/tasks/core-engine-implementation/evidence/signals-cache/build.sh
python3 .opensdlc/tasks/core-engine-implementation/evidence/signals-cache/run.py \
  --binary "$CARGO_TARGET_DIR/release/qz-signals-cache-bench" \
  --rustc "$RUSTC" \
  --build-identity .opensdlc/tasks/core-engine-implementation/evidence/signals-cache/build-identity.json
```

All harness dependencies are compiled in release mode and use versions present in the project lockfile. The run is pinned to one allowed CPU. `build-identity.json` records the exact source, harness, and lock hashes before compilation; the runner refuses inputs that changed before execution. Raw paired observations, summary statistics, resource usage, compiler/CPU metadata, and binary SHA-256 are saved alongside this file. Resource peak RSS covers the whole harness process, including both arms and untimed validation; it does not compare cache memory against baseline memory. The recorded kernel results exclude catalog loading, feature construction, forecast conversion, covariance, optimization, backtesting, and orchestration.

## Recorded run

Run on 2026-10-30 with Rust 1.99.1, AMD EPYC 9V74, affinity CPU 0, and a 4-core cgroup quota. No Cargo/rustc process was present before or after this 1.50-second run. The included production source was unchanged from the recorded HEAD `25abfeb4e33c3cf3c7516c5c3f91c8caebb83839`; all exact hashes are in [metadata.json](metadata.json) and [build-identity.json](build-identity.json). The initial dependency compilation and final build receipts are retained separately.

| Case | Fresh median, µs | Cached median, µs | Ratio of medians | Median paired ratio | Compilations fresh → cached |
| --- | ---: | ---: | ---: | ---: | ---: |
| short_metered | 1,528.273 | 716.928 | 2.132 | 2.065 | 128 → 8 |
| long_metered | 15,402.045 | 15,163.570 | 1.016 | 0.996 | 16 → 8 |
| saturated_metered | 2,606.772 | 2,235.126 | 1.166 | 1.177 | 256 → 160 |
| short_unmetered | 1,323.348 | 685.657 | 1.930 | 2.016 | 128 → 8 |

All 120 measured pairs had equal output checksums and fuel consumption. Untimed validation passed for every prediction bit against both the uncached arm and the analytic stateful output, including every asset/cutoff state reset. Remaining fuel also matched after every call. Observed compiled-module reuses were respectively 120, 8, 96 and 120 per cached arm. Fresh instances and prediction counts were unchanged. Metered fuel consumption was respectively 53,248, 1,703,936 and 106,496; the unmetered case remained `None`, not a fabricated zero measurement.

The short synthetic workloads show a benefit from removing repeated compilation. The long workload is dominated by prediction work: its paired median ratio is 0.996, so this run does not establish a meaningful speedup there. Fixed admission still avoided 96 compilations in the 64-model scan without evicting the first 32 cached modules. Raw ranges show visible timing variability; for example, the fresh short-metered samples ranged from 1,182.516 to 5,698.645 µs. These observations do not establish production percentiles or end-to-end Study/forecast improvements.

See [results.csv](results.csv) for every measured pair, [summary.json](summary.json) for counts and full ranges, and [resource.json](resource.json) for the successful exit and whole-harness peak RSS of 14,080 KiB. That RSS covers both arms and validation and is not a comparative memory result. Strict job-level science and integration tests remain the responsibility of the coordinated repository test run; this isolated harness does not replace them.

Historical measurements under `../../../core-engine-performance/evidence/signals/` remain separate and must not be presented as this implementation's results.
