# Factor diagnostic resource measurements

## Final source and method

Baseline source: `8e4059b07b688ec8c42a6ed490da7b30e4ece5a4`.
Final production source: `d2a8c6610ac7a216c077d047c06816731533f11d`.
Later evidence-only commits do not change the measured Rust code.

Both actual CLI binaries were built with pinned Rust 1.99.1 and
`cargo build --locked --release -p job --bin job`, default features and release
profile, `CARGO_BUILD_JOBS=4`, `CARGO_INCREMENTAL=0`, and no optimization override.
The baseline SHA-256 is
`8c1dd553cd01546d7922072844ed3359fba80d7be32d5bb554a8a60ff8f7c2ce`;
the final candidate SHA-256 is
`316071d3d4df5c8f31df4cc4bc2da28c8cee624de04c07d49c17587c680905c0`.

The Linux x86_64 environment has a four-CPU cgroup quota (`400000 100000`),
16 GiB memory limit and permitted CPUs 0–4. Final measurements pin each process
to CPUs 0–3, warm each input/binary once, then run five measured rounds with
alternating binary order. Compilation has finished before measurement. The
runner uses blocking `wait4`, includes complete JSON parsing/calculation/output,
and checks complete stdout bytes, exit codes and failure stderr. Warmup records
are retained but excluded from summary medians. See [environment](results/environment.json).

These are controlled software observations, not market data or profitability
claims. Irregular inputs contain 3–40 assets per group. The skewed case contains
96 groups, one with 6,000 assets and the others with three; resampling can repeat
the large group. No input-size cap, resample count or statistic was reduced.

## Final five-case results

Values are baseline → candidate medians. RSS is the process peak, in KiB.

| Workload | Wall ms | User + system CPU ms | Peak RSS KiB | RSS change |
| --- | ---: | ---: | ---: | ---: |
| Pooled, 240 irregular groups | 107.128 → 107.184 | 106.314 → 106.130 | 27100 → 26760 | -1.25% |
| Cross-sectional, 240 irregular groups | 30.293 → 30.326 | 29.593 → 29.829 | 26736 → 26388 | -1.30% |
| Pooled, one 6,000-asset group | 136.893 → 127.473 | 136.444 → 126.832 | 31436 → 31044 | -1.25% |
| Pooled, 2,400 irregular groups | 1141.946 → 1128.861 | 1140.858 → 1126.190 | 124660 → 122744 | -1.54% |
| Cross-sectional, 2,400 irregular groups | 273.288 → 273.750 | 272.631 → 273.084 | 120868 → 119160 | -1.41% |

All five measured median RSS values decrease by 1.25–1.54%. CPU changes range
from a 7.04% decrease on the skewed input to a 0.80% increase on the small
cross-sectional input. All wall-time ranges overlap. This does **not** establish
a general CPU speedup, a minimum saving on other inputs, or an extreme memory
reduction. The pooled regression detected in an earlier variant was
removed; deployment-scale results still require the user's actual workload.

The [final report](results/resource-contiguous-final-benchmark.json) retains all
60 processes (10 warmups and 50 measured runs), ranges, checksums and equality
checks. The separate [release correctness report](results/resource-contiguous-correctness.json)
contains 29 same-input baseline/candidate comparisons (58 processes), including
two expected missing-input failure cases (four processes). Every complete result agrees. The two
additional 2,400-group inputs also agree in every final performance run.

## Earlier diagnostics retained

No unfavorable run is replaced or omitted. Reports identify binaries by checksum.
The direct-pair unstable-sort variant is `bb71c1a9`; the direct-pair stable-sort
variant is `f8603b50`. Both were replaced by the final contiguous-column cache.

- [Initial release correctness](results/resource-release-correctness.json):
  29 inputs for the unstable-sort variant.
- [Initial five-round benchmark](results/resource-release-benchmark.json):
  unpinned, no warmup; the small cross-sectional CPU median increased about 7%.
- [Initial 10× size diagnostic](results/resource-release-scale10.json): unpinned,
  no warmup, strongly nonstationary times for both binaries. These data cannot
  support an attributed CPU improvement and are retained unchanged.
- [Pinned, warmed 10× diagnostic](results/resource-release-scale10-pinned.json):
  the unstable-sort pooled CPU median increased 5.50%, while RSS decreased 2.36%.
- [Three-way sort diagnostic](results/resource-release-sort-diagnostic.json):
  pinned/warmed pooled CPU medians were 1.104833 s (baseline), 1.138834 s
  (unstable direct-pair sort), and 1.267530 s (stable direct-pair sort). Restoring
  stable ordering alone did not fix the regression. The final code uses one
  shared contiguous f64 column so sorting regains the baseline's access pattern.

The extra column retains one f64 buffer sized for the largest sampled column,
including Vec capacity overhead; this measured CPU/memory tradeoff is intentional. It still avoids
retaining the baseline's two copied input columns and retains the other buffer
reuse, scope-specific computation and early-release changes.
