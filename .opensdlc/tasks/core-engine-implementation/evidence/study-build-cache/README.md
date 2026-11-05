# Same-version Study / Build cache attribution

This probe measures the actual `job study-portfolio` CLI and the existing public Rust `job::portfolio::build` business path. The Build wrapper and fixture exporter are a temporary example in isolated worktrees, not a production command or public benchmark API. It excludes HTTP/service orchestration and makes no production percentile claim.

Both arms use the same frozen tracked candidate source, Nautilus 0.67.0 dependencies, lockfile, Rust 1.99.1, release profile and all features, including high precision. The baseline changes only the two `SignalModuleCache::default` admission budgets to zero in a temporary worktree. Compilation/validation still occurs at the original call site; fresh Store/instance state, original fuel accounting, request/trap ordering, feature construction, risk statistics, Clarabel and native replay remain unchanged. This is attribution for the implemented cache within the upgraded engine, not a 0.63-to-0.67 comparison or the sum of all optimizations.

## Frozen workload and successful-work requirements

| Case | BARs per asset | Assets | Models / distinct source byte strings | Study cutoff BARs | Build forecasts per asset/model |
| --- | ---: | ---: | ---: | --- | ---: |
| short | 64 | 2 | 16 / 16 | 17, 20, …, 62 (16 cutoffs) | 62 |
| long | 4,096 | 2 | 16 / 16 | 1,000, 2,000, 3,000, 4,000 | 4,094 |

The two original micro-lot synthetic instrument definitions, BAR clocks, volumes and independently declared zero venue-fee source come from the existing controlled test helper. Prices are an explicitly synthetic integer-grid path: `base + 7*n + 150*((n*step)%period)` ticks, 100,000 ticks per unit, with `(base,step,period)` of `(100000,7,17)` and `(200000,11,19)`. This keeps both return series variable under the unchanged solver tolerance. Each of the sixteen legal constant-return Wasm programs has a distinct source SHA-256; their weights are exactly 0.0625. Each model receives an explicit 100,000,000 fuel task budget. Capital is 1,000,000 USD with a declared 1% cash reserve. These are runtime workloads, not research-quality or capacity evidence.

An initial smoke run using the helper's linear price path stopped after 11 of 16 frames with `INACCURATE_RESULT_NOT_AUTHORIZED` and `simulation: null`; the final dual residual was approximately `1.7136e-10` against the unchanged `1e-10` tolerance. It was rejected before measurement. The revised fixture uses the above integer-grid price path; no production behavior, tolerance, inaccurate-result authorization or allocation validation was changed to admit it.

The runner rejects early termination, non-optimal allocations, absent allocation targets, missing native replay, zero native orders/positions, mismatched consumed target counts, an all-cash second frame, or missing/zero metered fuel. It derives prediction counts from the frozen BAR prefixes and two-observation warmup, then checks total fuel against those counts times the per-prediction cost observed in the untimed configuration probe. It requires every full stdout byte—including complete frame inputs, allocations, fuel and native financial/canonical results—to equal the reference, across both arms, warmups and measured pairs. No field is removed or numerically rounded by the comparator. The production business entry points also execute their original request/result validators.

Study's expected module compilation count is 256→16 for short and 64→16 for long. Those are structural counts from this known unique working set, not an added production compilation counter. The separate signals-cache harness measures actual reuse. Build has one cutoff and sixteen distinct modules, so it has **no expected reuse**; it is an explicit control and may show retention/lookup overhead.

## Reproduction and provenance

`prepare.py` freezes `HEAD` plus the exact tracked binary patch, hashes every tracked file, checks both reconstructed worktrees against those bytes, copies the temporary example, and writes the baseline-only cache-budget patch. It refuses untracked production files. The example has no production API changes. A coordinator-supplied, already verified candidate CLI is copied before any baseline build can overwrite shared Cargo output. The exact example source is hashed separately because it is injected only into the temporary worktrees.

```sh
python3 prepare.py --repo /workspace/QuaZonai \
  --work-root /tmp/qz-study-build-cache-20261030 \
  --candidate-job /tmp/qz-core-acceptance-20261030/bin/job
```

Compile only in a coordinator-authorized window. Both worktrees use:

```sh
export CARGO_HOME=/workspace/.quazonai-setup/cargo
export RUSTUP_HOME=/workspace/.quazonai-setup/rustup
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/workspace/QuaZonai/target
# In candidate/:
touch apps/job/src/signals.rs
cargo build --locked -p job --release --all-features --example qz_core_pair
# Copy target/release/examples/qz_core_pair to bin/candidate-build.
# Export once, outside every timing interval:
bin/candidate-build fixtures /tmp/qz-study-build-cache-20261030/fixtures
# In baseline/:
touch apps/job/src/signals.rs
cargo build --locked -p job --release --all-features --bin job --example qz_core_pair
# Copy target/release/job and the example to bin/baseline-job and bin/baseline-build.
```

The commands above illustrate the separate working directories; use absolute paths for copies/export. A shared Cargo target needs exclusive coordination and forced source freshness: Cargo can reuse a previous worktree's library when the changed source in another worktree has an earlier mtime. This occurred in preparation: the initial apparent baseline binaries were identical to the candidate and were rejected before measurement. Touching the baseline's `signals.rs` then forced its library recompilation; the successful build receipt and distinct binary hashes are retained. A different binary hash alone is insufficient proof, so `qz_cache_proof.rs` directly includes each arm's exact production file in an untimed temporary example, checking actual compiled-module identities, equal predictions/fuel, source hash, and release at task end. Across two scans of the sixteen frozen modules it must report baseline 32 compilations / 0 reuses versus candidate 16 / 16. This is a configuration probe, not instrumentation of the timed business path.

Build `qz_cache_proof` separately in each worktree, forcing the temporary example source's mtime after switching arms, and save its output as `<arm>-cache-proof.json`. Preserve the already copied business binaries. The shared target's top-level executable may hold the baseline after the build. All measurements and connected-core checks use separately copied binaries. Before delivery from that target, touch the original candidate `signals.rs` (without changing its bytes) and rebuild/verify the candidate. Initial candidate compilation used one build job; final example and baseline compilation use two. Build concurrency is recorded and excluded from timing; both arms use the same optimized profile and compiler.

Run a validation pair first, then the actual measurement only after other builds and core tests stop:

```sh
python3 run.py --work-root /tmp/qz-study-build-cache-20261030 \
  --out /tmp/qz-study-build-cache-20261030/validation --validate-only
python3 run.py --work-root /tmp/qz-study-build-cache-20261030 \
  --out /tmp/qz-study-build-cache-20261030/measured --warmup-pairs 5 --pairs 30
```

Every invocation is a new child process pinned to one allowed CPU, with an empty environment. Wall time includes process startup, request reading, catalog/artifact I/O, the entire business path, full stdout serialization and teardown. Fixture generation, compilation and result comparison are outside the timer. Five warmup pairs precede thirty measured pairs per case; arm order alternates. The OS page cache is warm. Linux `wait4` records each child process's own peak RSS and CPU usage; RSS is neither a compiled-code memory bound nor an isolated cache heap measurement.

`results.csv`/`observations.jsonl` retain every pair; summary medians and paired ratios are descriptive for this workload and host. Full reference outputs, input hashes, unique-model hashes, binary hashes, source identity and compiler/build receipts are retained with the run. Input and binary hashes are checked again at the end. Compiler processes present at either end reject the measurement window; broader host noise still cannot be excluded entirely. The two-asset cases do not establish large-universe covariance/solver improvements, production tail latency, or cold-disk behavior.

## Results

Recorded 2026-10-30 04:02:21–04:02:56 UTC, AMD EPYC 9V74, CPU 0 affinity, four-core cgroup quota. Other coordinated compilation/core/Node tests had stopped; no Cargo/rustc process was present before or after. Each case has five warmup pairs and thirty measured pairs, with fifteen measured pairs in each order.

| Case | Baseline median wall, ms | Candidate median wall, ms | Candidate median change | Median paired baseline/candidate | Peak RSS median, baseline→candidate KiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| short Study | 78.238 | 73.509 | −6.05% | 1.066 | 60,320 → 60,328 |
| long Study | 298.998 | 297.123 | −0.63% | 1.004 | 109,190 → 108,922 |
| short Build, no reuse | 14.023 | 13.234 | −5.62% | 1.074 | 36,638 → 36,792 |
| long Build, no reuse | 40.837 | 40.074 | −1.87% | 1.014 | 39,750 → 40,026 |

All **120 measured pairs**, plus warmup and preliminary validation pairs, had identical full stdout. Every Study completed all 16/4 frames, with 32/8 native orders, two native positions and 16/4 consumed targets. Every allocation was `OPTIMAL`; the second frame held actual positions. Derived prediction calls were 19,200/319,744 and consumed fuel 38,400/639,488. Build's 1,984/131,008 predictions consumed 3,968/262,016 fuel. The configuration probe observed two fuel per prediction for these exact constant Wasm modules. Native final USD account totals were respectively `999042.2437159725690713` and `1000331.153441821017097`, identical in both arms; these synthetic account outcomes are not investment evidence.

The short Study candidate was faster in 28 of 30 pairs. Its descriptive 5,000-resample interval for the paired median ratio was [1.0412, 1.0906]. The long Study interval [0.9850, 1.0308] spans parity, so this run establishes no meaningful benefit there. Build performs sixteen distinct compilations in both arms and has no reuse; its two descriptive intervals also span parity, and its samples show order sensitivity. The observed Build shifts must not be reported as compile-reuse benefits. They also limit attribution of the whole short Study difference: this comparison supports an observed improvement for the specified workload, not a claim that every millisecond came from cache hits. These resampling intervals do not remove systematic host, layout, retention or order effects and are not production percentile confidence bounds.

Per-process peak RSS medians changed by less than 1% in all four cases. That is an observation for these small modules and two assets, not a memory ceiling or evidence about a large compiled-module working set. The stronger direct evidence for eliminating compilation is the source-bound identity probe and the separate [signals-cache kernel measurement](../signals-cache/README.md); the end-to-end results explicitly retain catalog, feature, optimizer, replay and output costs.

Raw pairs and metadata are in [results.csv](results.csv), [summary.json](summary.json), [metadata.json](metadata.json) and [paired-diagnostics.json](paired-diagnostics.json). [Frozen inputs](frozen-inputs.tar.gz) contain the original Parquet, separate fee source, models and requests; [input-sha256.json](input-sha256.json) binds every byte. `full-output/*.json.gz` retains each complete reference stdout losslessly; decompression must match its SHA-256 in the summary. Source patch/hash inventory, the baseline-only delta, exact build receipts, rejected preparation attempts and the two configuration proofs are under `provenance/`. No timing sample from either rejected preparation case is included.
