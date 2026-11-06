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
  The shared column adds one f64 buffer sized for the largest sampled column,
  including Vec capacity overhead, but restores compact sorting access;
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

Final production source: `d2a8c6610ac7a216c077d047c06816731533f11d`.

- Pinned Rust 1.99.1 and locked dependencies: `cargo test --locked -p job
  --test review_science` passes all 18 tests, zero skips/ignored tests. The job
  package was explicitly rebuilt to exclude stale worktree artifacts. The
  complete frozen-output regression covers 26 original-dev cases, including
  two expected missing-input errors and signed-zero/tied rankings.
- Independent final release CLI comparison: 29 original JSON requests on both
  binaries, 58 actual processes, with identical full stdout, exit codes and
  failure stderr. The two expanded inputs also match in every performance run.
- Final fixed-affinity measurements: five workloads, one warmup and five measured
  rounds per binary/input. Median peak RSS decreases 1.25–1.54%; CPU changes are
  workload-dependent and wall-time ranges overlap. No general CPU speedup or
  extreme memory reduction is claimed. See [all measurements and earlier
  unfavorable variants](validation/measurements.md).
- `cargo fmt --all -- --check`, diff whitespace and Python script syntax pass.
  The runner rejects zero repetitions and an empty case filter. Independent
  static review of the final production commit found no actionable defect.
- Applicable exact-Head hosted CI and GitHub Codex review remain pending. No
  merge, publication or deployment is claimed.

See [reproduction instructions](validation/README.md) for source/binary provenance,
fixed CPU affinity, warmup and the complete-result comparison commands.
