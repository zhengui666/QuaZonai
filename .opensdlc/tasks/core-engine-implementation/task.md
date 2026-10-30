# Latest Nautilus and core-engine optimization implementation

## Goal

Implement the reviewed [performance design](../core-engine-performance/report.md)
against the current remote `dev`, then merge the reviewed and checked branch back
into remote `dev`. The owner requires the latest official Nautilus without old
version compatibility or rollback paths. Preserve original user data and receipts.

## Baseline and branch

- Base: `dev` at `78a84bdd31f524b4965560fc4b6ae4ab53cafcef`.
- Working branch: `perf/nautilus-latest-core-engine-20261030`.
- Latest-release lookup: 2026-10-30; all 17 directly used Nautilus crates 0.67.0,
  Rust 1.99.1. See [upstream receipt](evidence/upstream-versions.json).
- The old design's source baseline predates the TypeScript migration. Its retired
  Rust control-plane proposals are not restored. Existing terminal forecasts and
  same-cutoff catalog reuse are retained rather than reimplemented.

## Implementation in progress

Upgrade all native dependencies/toolchains and runtime identities; migrate catalog
reading to native typed sessions with original PIT/limits; replace removed instrument
fee fields with independently frozen venue fee sources; retain actual installed Paper
fee policies; add typed backtest inputs and remove duplicate result conversions.

Reduce JSON writes, task-local Wasm recompilation, intermediate matrix/forecast
copies and canonical hashing allocations. Experimental solver/back-end/incremental
statistics directions require their own measured benefit and numerical qualification.

## Acceptance and evidence

[Verification plan](verification.md) records the current architecture, required
checks, native core flow and final-Head GitHub CI/Codex review criteria. Raw
component measurements are in [evidence](evidence/); they do not establish production
p95/p99 or market validity. Implementation, local tests, CI, merge and deployment
are distinct states; this task does not authorize live account operations.

Current status: implementation and integration validation in progress. Nothing has
been merged or deployed. The original study artifacts retain their original versions.
Live project-board synchronization is pending because supported board tools are not
connected and the executor policy does not allow its private origin.
