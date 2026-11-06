# Reduce factor diagnostic CPU and peak memory

## Goal

Reduce temporary memory and repeated work in `factor_diagnostics` while preserving
complete scientific output, input validation, fixed-seed resampling and partition
boundaries. This is the resource-usage PR requested separately from multicore
calibration. Base: remote `dev` at `8e4059b07b688ec8c42a6ed490da7b30e4ece5a4`.

## Change

- Count and iterate selected observations without retaining two reference arrays.
  Release the duplicate-detection tree and overlap timestamps before resampling.
- Compute only the requested pooled or cross-sectional statistic. Cache each
  cross-sectional group's optional value at its original position so time slices
  can reuse it without re-ranking or moving missing groups between slices.
- Reuse one contiguous score/label column cache, the ordering and two rank
  buffers, the sampled group indices and pooled pairs across the 512 replicates.
  The shared column costs up to one additional f64 per sampled row compared with
  direct pair indexing, but preserves the baseline's compact sorting access;
  unlike the baseline, two input columns are not retained simultaneously.
  Keep total ordering, tie ranks, random draws and floating-point accumulation
  order unchanged. No sample quota, estimator, confidence method or wire changes.

Buffers retain their high-water capacities for this invocation. Irregular group
sizes can grow a bootstrap sample beyond the original point count; measurements
must include that case. This work does not promise constant memory or eliminate
the original input, grouped observations or standard stable-sort workspace.

## Acceptance

1. Complete baseline/candidate results and failure behavior agree for both scopes,
   tied and signed-zero scores, constant/short groups, missing excluded values,
   other partitions, overlapping labels, duplicate and invalid included inputs.
2. Existing review-science regressions pass on the candidate, including complete
   evidence beyond former point quotas. Formatting and independent review pass.
3. Same-input runs measure wall time, user plus system CPU and peak RSS. Report
   fixture dimensions, build profile, source revisions, repeat count and resource
   limits. Synthetic workloads prove software behavior and workload-specific
   resource changes, not real-market performance or profitability.
4. Open a separate PR targeting `dev`; final-Head hosted CI/review and merge remain
   distinct from local validation and from deployment.

## Validation

Fixed-affinity, warmed release measurements exposed a pooled CPU regression in
both direct-pair ranking variants. The three-way diagnostic measured median CPU
1.10483 s for the baseline, 1.13883 s for unstable sorting (`bb71c1a9`) and
1.26753 s for stable sorting (`f8603b50`). Neither variant is accepted as a CPU
improvement. The implementation now restores contiguous values and the original
stable ordering while reusing one column instead of allocating two. Its exact
commit, full-output regression and resource measurements remain pending.
All earlier favorable, unfavorable and nonstationary runs are retained. The
checks below describe the earlier buffer implementation, not final validation.

- Pinned Rust 1.99.1, locked dependencies, actual candidate native library and CLI:
  `cargo test --locked -p job --test review_science`
  passes 18 tests, with no ignored or skipped tests. The complete frozen-output
  regression covers 26 baseline cases, including two expected input errors.
- Independent actual-CLI comparison runs 28 identical requests on each binary
  (56 processes): all stdout bytes, exit codes and error outputs agree. This
  includes the 26 edge cases plus both irregular-group benchmark inputs.
- Baseline unoptimized binary SHA-256:
  `46c6ebfca23cc28f584580992ca59e7e75dc614b9bf237c2d8e36de10214f141`.
  Candidate unoptimized binary SHA-256:
  `e2236a368dca0449e21bcac7cec42d240d259a049ecf4c028dd6a531e2a05418`.
  These establish behavior only; concurrent compilation and an unoptimized
  profile exclude these runs from release-performance claims.
- `cargo fmt --all -- --check`, diff whitespace and Python script syntax pass.
  Independent static inspection found no actionable production defect; review of
  the final commit remains required. Release resource measurement and applicable
  exact-Head hosted CI remain pending.

See [reproduction instructions](validation/README.md) for the benchmark generator
and complete-result runner. No merge, publication or deployment is claimed.
