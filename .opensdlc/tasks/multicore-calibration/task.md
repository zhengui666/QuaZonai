# Multicore ensemble calibration

## Goal and scope

Implement the owner's CPU multicore request as a separate PR against remote `dev`, starting at `8e4059b07b688ec8c42a6ed490da7b30e4ece5a4`. The independent official OLS calibrations in `fit-ensemble` are the parallel unit. Each fold and the final refit retain their original training-only panel, numerical operation order, output order and first failing calibration. Stateful replay, account decisions, original report evidence and trial counts are unchanged.

## Implementation

- Index borrowed training rows once by asset, retaining the original order inside each asset. This removes member-by-asset repeated scans without cloning the rows or caching every asset's labels.
- Workers borrow that index, reuse their score/label buffers and call the unchanged official `ScoreCalibration::fit`. No numerical solver is rewritten and no reductions are parallelized.
- Assign IDs in member-then-asset order. Workers stop taking later jobs after a known failure; already assigned earlier jobs finish. Results and errors are collected in original ID order.
- Use the calling thread plus scoped native threads. A thread-creation failure leaves the calling thread and already-started workers to consume remaining work. A computation panic remains a failure.
- Select concurrency from available CPU parallelism, optional `QZ_CALIBRATION_THREADS`, number of independent fits, minimum work and conservative scratch admission. The estimate is 256 bytes per observation in the largest group plus a 2 MiB thread stack, within a 256 MiB concurrent scratch estimate. It allows headroom for the official two-column OLS's input copies, design matrices, SVD and residual statistics. This is an estimate, not a total RSS guarantee, and does not reject a large fit or impose a research quota. Large individual fits can select one worker.
- Forward only the validated optional numeric thread setting from the Nautilus worker to its scientific subprocess. Existing environment isolation is preserved.

## Verification and acceptance

The new unit checks compare complete calibration coefficients/evidence against the pre-change traversal with 1, 2, 4 and 16 workers, and exercise competing failures whose completion order differs from original error order. Existing ensemble integration tests cover training boundaries, independent scalar regression/QP oracles, freezing and CLI output. The worker protocol check verifies actual child environment propagation and invalid limits.

Required commands:

```sh
cargo test --locked -p job --features hyperliquid-offline --lib ensemble_fit::calibration -- --test-threads=1
cargo test --locked -p job --features hyperliquid-offline --test ensemble_fit -- --test-threads=1
node --experimental-strip-types --test apps/nautilus/test/*.test.ts
```

Performance acceptance uses the same serialized `NativeEnsembleFitRequestV1` input on the original binary, candidate with `QZ_CALIBRATION_THREADS=1`, and candidate with multiple workers. Compare the complete output, elapsed time, total process CPU time and peak RSS in separate processes. Measure both multi-asset and large single-asset inputs. Controlled numerical fixtures establish software performance and equality, not real-market profitability.

Implementation and benchmark results are recorded below after execution. Hosted final-Head CI and independent review are required before merge; creating the requested PR does not deploy it.

## Execution evidence

- With the pinned Rust 1.99.1 toolchain, locked dependencies and `hyperliquid-offline`, all 3 new calibration unit tests and all 11 ensemble integration tests passed after rebuilding the changed job crate. This includes unchanged native-source/scalar-oracle, training-isolation, freezing and CLI tests. Debug builds use no debuginfo; these runs establish correctness, not optimized performance.
- `node --experimental-strip-types --test apps/nautilus/test/*.test.ts`: 14 passed, zero failed or skipped, on Node 24.19.0 after installing the locked research dependencies. `npm --prefix apps/nautilus run typecheck` also passed. These cover actual controlled child-process propagation and transport behavior, not native numerical computation.
- Changed Rust files pass pinned `rustfmt`; `git diff --check` passes. End-to-end optimized performance measurement and final independent review are pending.
- No production data or deployment changed.
