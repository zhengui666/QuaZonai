# Multicore ensemble calibration

## Goal and scope

Implement the owner's CPU multicore request as a separate PR against remote `dev`, starting at `8e4059b07b688ec8c42a6ed490da7b30e4ece5a4`. The independent official OLS calibrations in `fit-ensemble` are the parallel unit. Each fold and the final refit retain their original training-only panel, numerical operation order, output order and first failing calibration. Stateful replay, account decisions, original report evidence and trial counts are unchanged.

## Implementation

- Index borrowed training rows once by asset, retaining the original order inside each asset. This removes member-by-asset repeated scans without cloning the rows or caching every asset's labels.
- Workers borrow that index, reuse their score/label buffers and call the unchanged official `ScoreCalibration::fit`. No numerical solver is rewritten and no reductions are parallelized.
- Assign IDs in member-then-asset order. Workers stop taking later jobs after a known failure; already assigned earlier jobs finish. Results and errors are collected in original ID order.
- Use the calling thread plus scoped native threads. A thread-creation failure leaves the calling thread and already-started workers to consume remaining work. A computation panic remains a failure.
- Default to at most two workers; `QZ_CALIBRATION_THREADS` can explicitly request a higher cap. Select actual concurrency from that cap, available CPU parallelism, number of independent fits, minimum work and conservative scratch admission. The estimate is 256 bytes per observation in the largest group plus a 2 MiB thread stack, within a 256 MiB concurrent scratch estimate. It allows headroom for the official two-column OLS's input copies, design matrices, SVD and residual statistics. This is an estimate, not a total RSS guarantee, and does not reject a large fit or impose a research quota. Large individual fits can select one worker.
- Forward only the validated optional numeric thread setting from the Nautilus worker to its scientific subprocess. Existing environment isolation is preserved.

## Verification and acceptance

The new unit checks compare complete calibration coefficients/evidence against the pre-change traversal with 1, 2, 4 and 16 workers, and exercise competing failures whose completion order differs from original error order. Existing ensemble integration tests cover training boundaries, independent scalar regression/QP oracles, freezing and CLI output. The worker protocol check verifies actual child environment propagation and invalid limits.

Required commands:

```sh
cargo test --locked -p job --lib ensemble_fit::calibration -- --test-threads=1
cargo test --locked -p job --test ensemble_fit -- --test-threads=1
node --experimental-strip-types --test apps/nautilus/test/*.test.ts
```

Performance acceptance uses the same serialized `NativeEnsembleFitRequestV1` input on the original binary, candidate with `QZ_CALIBRATION_THREADS=1`, and candidate with multiple workers. Compare the complete output, elapsed time, total process CPU time and peak RSS in separate processes. Measure both multi-asset and large single-asset inputs. Controlled numerical fixtures establish software performance and equality, not real-market profitability.

The [fixture generator](validation/generate_ensemble.py) and [process comparator](validation/run_compare.py) require only Python's standard library on Linux. Build the original and candidate binaries with the same pinned toolchain, lockfile, features and optimized profile, then preserve them under different absolute paths. The comparator records their hashes, input hashes, complete-output equality and `wait4` process measurements; it alternates binary order across repetitions. The constant-predictor case intentionally fails and verifies original error equivalence, so use only `bench-ensemble` for timing claims:

```sh
python3 .opensdlc/tasks/multicore-calibration/validation/generate_ensemble.py --bench --out /tmp/qz-ensemble-fixtures
python3 .opensdlc/tasks/multicore-calibration/validation/run_compare.py \
  --fixtures /tmp/qz-ensemble-fixtures --out /tmp/qz-ensemble-results \
  --binary baseline=/absolute/baseline/job \
  --binary serial=/absolute/candidate/job,threads=1 \
  --binary parallel=/absolute/candidate/job,threads=4 --repeats 5 --case bench-ensemble
```

Run the same comparator without `--case` to check both successful and intentionally failing inputs. Every benchmark row must have `all_success: true` before interpreting speed. [Thread observation](validation/observe_threads.py) can sample `/proc` in a separate run with `--binary`, `--input`, `--out` and optional `--threads` (omit it for the production default); requested threads alone do not prove actual concurrent execution. Do not mix that polling run into timing evidence.

Implementation and benchmark results are recorded below. Hosted final-Head CI and independent review are required before merge; creating the requested PR does not deploy it.

## Execution evidence

- With the pinned Rust 1.99.1 toolchain, locked dependencies and default features, all 3 new calibration unit tests and all 11 ensemble integration tests passed after rebuilding the changed job crate, including a fresh rebuild after selecting the final two-worker default. This includes unchanged native-source/scalar-oracle, training-isolation, freezing and CLI tests. Debug builds use no debuginfo; these runs establish correctness, not optimized performance.
- `node --experimental-strip-types --test apps/nautilus/test/*.test.ts`: 14 passed, zero failed or skipped, on Node 24.19.0 after installing the locked research dependencies. `npm --prefix apps/nautilus run typecheck` also passed. These cover actual controlled child-process propagation and transport behavior, not native numerical computation.
- Changed Rust files pass pinned `rustfmt`; `git diff --check` passes. Local independent source review found no actionable runtime issue; final-Head hosted CI and final review remain prerequisites for merge.
- No production data or deployment changed.

## Measured concurrency tradeoff

Both original and candidate binaries were built with Rust 1.99.1 using `cargo build --locked --release -p job --bin job`, default features and the default Cargo release profile, `CARGO_BUILD_JOBS=4` and `CARGO_INCREMENTAL=0`. The [recorded environment](validation/results/environment.json) is Linux x86_64 with a four-CPU cgroup quota, five allowed CPUs and 16 GiB memory; Rust reports four available CPU workers. Runs used the original affinity. Both generated panels have 192,000 report points: 6,000 timestamps × four assets × eight members, or 24,000 timestamps × one asset × eight members. The CLI measurements include JSON reading, panel validation, fold fitting, the final fit and serialization.

The [initial five-repeat comparison](validation/results/initial-1-and-4.json) showed complete output equality for every successful run. Compared with the original traversal, the explicit four-thread cap reduced median end-to-end wall time from 0.493 s to 0.469 s for the multi-asset panel and from 0.497 s to 0.468 s for the single-asset panel. Peak RSS increased from 62,684 to 65,048 KiB and from 64,860 to 73,268 KiB respectively. The candidate's serial path was already faster than the original, so these differences do not isolate the benefit of parallelism.

A separate [five-repeat 1/2/4-cap comparison](validation/results/followup-1-2-and-4.json) used the same candidate and identical inputs. Complete outputs matched, but four threads were not consistently faster than two:

| Panel | Thread cap | Median wall (s) | Median total CPU (s) | Median peak RSS (KiB) |
| --- | ---: | ---: | ---: | ---: |
| Four assets | 1 | 0.4690 | 0.4681 | 62,612 |
| Four assets | 2 | 0.4514 | 0.4594 | 63,556 |
| Four assets | 4 | 0.4473 | 0.4646 | 64,872 |
| One asset | 1 | 0.4681 | 0.4662 | 65,040 |
| One asset | 2 | 0.4623 | 0.4708 | 67,764 |
| One asset | 4 | 0.4796 | 0.4887 | 73,148 |

The default was therefore reduced to at most two workers: additional OLS scratch increases memory consistently, while the small wall-time differences vary between batches. A higher explicit cap remains available for measured workloads. A cap is not a constant worker count; fold size, available cores and scratch admission can select fewer workers. Separate `/proc` sampling observed two `qz-calibration` threads plus the caller in explicit-four-cap runs; this proves native concurrent execution but does not assert a constant four active workers. These measurements concern ensemble calibration and its enclosing CLI only, not every scientific operation or production workload. All original measurements remain linked above.

## Final default verification

The final runtime source is `2f6fa77a70fb7e3a5258706d4deb83b93ff7eed4`; later evidence-only edits do not change that code. The rebuilt default binary's SHA-256 is `f52bf3087f551af5918701cdab60f4c8ef07189644fe0b1c2ff7d61fa9319c1b`. With no thread environment setting, it produces [exactly the same complete outputs and errors as explicit `QZ_CALIBRATION_THREADS=2`](validation/results/final-default-equivalence.json) on all four controlled ensemble cases. A separate [thread observation](validation/results/final-default-threads.json) captured the caller plus one `qz-calibration` thread, establishing that the default actually executes independent fits on two native threads.

The [fresh five-repeat final-default comparison](validation/results/final-default-vs-baseline.json) alternated original and final binaries on identical successful inputs; all complete outputs matched:

| Panel | Binary | Median wall (s) | Median total CPU (s) | Median peak RSS (KiB) |
| --- | --- | ---: | ---: | ---: |
| Four assets | Original | 0.4860 | 0.4855 | 62,668 |
| Four assets | Final default | 0.4767 | 0.4836 | 63,680 |
| One asset | Original | 0.4840 | 0.4835 | 64,888 |
| One asset | Final default | 0.4770 | 0.4888 | 67,776 |

Median wall time is 1.9% / 1.5% lower, but the run ranges overlap; this does not establish a statistically significant speedup. Total CPU changes by −0.4% / +1.1%, while peak RSS increases by 1.6% / 4.5%. This PR delivers tested multicore capability with a measured, smaller default memory tradeoff. It does not claim memory compression or universal CPU savings. Use the serial setting when minimizing this additional scratch matters more than concurrency, and benchmark higher caps on the intended panel before selecting them.
