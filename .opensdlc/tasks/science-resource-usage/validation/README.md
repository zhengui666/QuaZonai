# Reproduce factor diagnostic measurements

The fixtures are generated software observations, not archived market evidence.
The runner invokes the actual `job review-science` CLI with the same original JSON
for each binary. It compares complete stdout, exit code and failure stderr, and
records wall time, user plus system CPU and peak process RSS using Linux `wait4`.

Build and preserve a binary from the task's base revision and a binary from the
candidate using the same toolchain, lockfile, profile and features. Prefer
`cargo build --release --locked -p job --bin job` in separate worktrees with
separate `CARGO_TARGET_DIR` directories. A shared target reused between worktrees
can reuse a stale package artifact: use distinct targets or explicitly rebuild the
job package, and record each binary's checksum and source revision. Never label
unoptimized development measurements as production release performance.

Run from the repository root with Python 3 on Linux:

```sh
python3 .opensdlc/tasks/science-resource-usage/validation/generate-review-fixtures.py \
  --out /tmp/qz-review-fixtures --bench
python3 .opensdlc/tasks/science-resource-usage/validation/compare-review.py \
  --binary baseline=/absolute/path/to/base/job \
  --binary candidate=/absolute/path/to/candidate/job \
  --fixtures /tmp/qz-review-fixtures --out /tmp/qz-review-correctness
python3 .opensdlc/tasks/science-resource-usage/validation/compare-review.py \
  --binary baseline=/absolute/path/to/base/job \
  --binary candidate=/absolute/path/to/candidate/job \
  --fixtures /tmp/qz-review-fixtures --out /tmp/qz-review-measurements \
  --case bench-review --repeats 5
```

Order alternates each repetition. `report.json` records input/output and binary
checksums, every process measurement, medians and complete-result equality. A
mismatching result fails the run; failed requests are useful failure-regression
cases and never count as successful performance samples. For meaningful timing,
run after compilation and other CPU-intensive work have finished. Report actual
CPU/cgroup limits and RAM along with the results. Inputs with uneven group sizes
are intentional: they exercise growing reusable sample buffers. The skewed pooled
case has 96 groups, one with 6,000 assets and the others with three, allowing the
same large group to occur multiple times in one resampled sequence.

The Rust fixture helper in `apps/job/tests/support/review_diagnostics_fixture.rs`
uses the same small-case input formulas. Integer values divided by powers of two
keep Python/Rust fixture values exactly reproducible. The regression compares its
whole output with frozen base output rather than reimplementing the statistic.
