# QuaZonai structural kernel measurements

These are **synthetic structural microbenchmarks**, not production Nautilus,
Wasmi, Clarabel, or full QuaZonai RT measurements. They isolate first-party loop
shapes at revision `d7da6bff197bcbf272ba998ae0a15b2c1b25071c`.

Sources inspected:

- `apps/job/src/simulation.rs:203`: clone the next target before cross-section
  completeness and target due-time checks; `208–225`: scan the latest bars.
- `apps/job/src/portfolio.rs:215`: for every asset, reverse-scan the full
  per-model forecast vector for that asset's last point.
- `apps/job/src/forecast.rs:78`: forecast vector is emitted in per-series
  contiguous blocks; `portfolio.rs` validates equal row counts and asset order.

The harness does not copy or patch third-party implementations. It only uses
the Rust standard library. Production `NativeTargetPointV1` DecimalValue and
Nautilus Bar are projected onto Strings and integers. Thus **clone timing
ratios must not be presented as measured production-type speedups**. The
relative cost of numeric payloads and compiler/crate boundaries can differ.

## Reproduce

```bash
rustc --edition=2021 -C opt-level=3 -C debuginfo=1 kernels.rs -o kernels
python3 run.py
```

`run.py` records the exact compiler used in this environment, CPU model, CPU
quota, memory limit, affinity, resource usage and raw samples. For another
machine replace its compiler path with the desired pinned compiler. CPU 0 must
be included in the current process's allowed affinity.

## Workloads and variants

Replay: 8 / 32 / 128 / 256 assets, 256 complete event-time cross-sections,
ordered instrument arrivals, four due targets at timestamps 64/128/192/256.
There are `assets × 256` BAR callbacks. All three variants keep a BTreeMap of
latest bars and compute the same target application times and target checksum.

1. Clone before completeness and due-time guards (current loop shape).
2. Borrow through guards; clone only once the target is actionable.
3. Additionally maintain an exact event-time frequency index, removing the
   previous latest bar's event count on replacement. This supports out-of-order
   event times and detects exact cross-section coherence; it does not assume
   monotone event timestamps. This optimization is measured separately because
   maintaining an index has a cost and can lose on small universes.

Forecast: 8×1,024, 32×1,024, 128×1,024, and 128×4,096 points, grouped by
instrument as in the producer.

1. Repeated reverse-find over all points (current loop shape).
2. Build a BTreeMap of last points in a single pass, then lookup all assets;
   construction cost is included, and arbitrary input ordering is supported.
3. Read known block-tail offsets and verify each instrument ID. This is an
   algorithmic ceiling applicable only when the equal-rows/ordered-block
   precondition is established. Production should capture each final point
   at its producer boundary rather than scatter assumptions about layout.

Exact `(forecast IEEE bits, event timestamp)` vectors are compared across
variants for every timed call. Replay compares the complete application-event
sequence, count and target checksum, plus a deterministic 4,096-event trace with
non-monotone event times and repeated/missing instruments. This is **not** full
fill/order/cash equivalence or numerical model validation.

Each scenario has five untimed warmup rounds and 30 measured rounds. Variant
order alternates forward/reverse. Inputs are made outside the timed region;
map construction is inside it. `black_box` is used to retain work. Counts are
collected in a separate const-generic instrumented pass and compile out of the
timed loops. Median and p95 use nearest-rank quantiles. Thirty samples provide
a local timing distribution, not a robust production p95/p99 claim.

`maxrss_kib` is process peak RSS including fixture creation and all kernels; it
is not per-variant peak or a production memory estimate. Cgroup CPU counters
are for the shared container and do not isolate this process. Shared hardware
noise can remain even with core affinity.

Files: `kernels.rs` (source), `run.py` (runner), `raw.csv` (all samples and
counters), `results.json` (metadata and summaries). No production source is
modified by this experiment.
