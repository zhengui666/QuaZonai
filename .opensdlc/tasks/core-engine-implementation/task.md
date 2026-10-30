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

## Implementation

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

Implementation is complete on the feature branch. Final-Head CI, independent/Codex
review and remote merge are tracked in [PR #267](https://github.com/zhengui666/QuaZonai/pull/267);
no deployment or live-account result is claimed. The original study artifacts retain
all original versions and measurements.

Local evidence recorded before final publication:

- Rust 1.99.1 release library suite: 325 passed, zero failed/ignored.
- Default and high-precision catalog regressions: 17 passed each, including the
  oversized same-timestamp cohort stopping before a later corrupt row group.
- Native fee-source CLI: 4 passed; catalog preparation: 11 passed; typed/native
  replay, Paper/Sandbox, forecasting, allocation, observed cash and Study paths
  were executed. The new precision simulation and observed-spot suites passed
  20 and 21 tests; instrument-transition scope was repaired and reran 3/3.
- EMA migration and independent OLS/QP: 10 passed with unchanged tolerances.
- All workspace/all-feature/all-target `cargo check` and ordinary Clippy completed.
  Clippy retains lint warnings; this is not a denied-warning full-release claim.
- Source helper/deployment checks passed using official Node 22.19.0.
- Web types/unit/build and 71 browser tests passed. JSON/evidence/worker tests
  passed 38/38, with all three affected TypeScript typechecks passing.
- The first connected-core run passed 86/92. All six fee-source fixture failures
  were corrected without relaxing the business criteria and reran 6/6. The
  final complete CI remains mandatory, including the optimized HTTP paths.
- Changed/new documentation links passed (19 Markdown files, 271 occurrences);
  whole-repository link checking also exposes pre-existing retired-task links.

Measurements and limits:

- [Current Wasmi cache](evidence/signals-cache/README.md): complete bit/fuel
  equality in 120 pairs; short kernels benefit, long inference has no stable win.
- [Actual Study/Build CLI](evidence/study-build-cache/README.md): frozen inputs,
  full-output/fuel/native-account equality in 120 pairs. Short Study medians were
  78.238→73.509 ms; long 298.998→297.123 ms. Negative controls/order effects
  prevent attributing the entire short difference to cache reuse.
- [Real CPU profile and JSON probes](evidence/pm-json-hotpaths/README.md): a
  controlled HTTP test exposed existing recursive token emission and per-token
  SHA updates. Bounded writes/explicit traversal preserve evidence identities.
  The five-pair function measurements predate the rare accessor correction and
  are not a final HTTP or production p95 claim.
- [Fee input preparation](fee-source.md), [EMA seed migration](evidence/ema-seed-migration.md)
  and [native BAR liquidity](evidence/cohort-liquidity-migration.md) document the
  changed upstream semantics and independent numerical checks.

Retired Rust control-plane proposals are inapplicable to current dev. Speculative
BLAS/faer, solver replacement, incremental estimators and cross-cutoff global
caches are not enabled without their required measurements and scientific
qualification. No old engine, reader, seed algorithm, dual deployment or version
rollback is introduced.
Live project-board synchronization is pending because supported board tools are not
connected and the executor policy does not allow its private origin.
