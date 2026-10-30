# Performance evidence and reproduction

Baseline: `d7da6bff197bcbf272ba998ae0a15b2c1b25071c`, observed 2026-10-29.
These are local component/structural probes, **not production end-to-end RT**.
No production source or upstream implementation was modified.

## Contents

| Evidence | Source | Results |
| --- | --- | --- |
| Replay/forecast loop structures | [kernels.rs](kernels/kernels.rs), [runner](kernels/run.py), [method](kernels/README.md) | [raw samples](kernels/raw.csv), [metadata and summaries](kernels/results.json) |
| Actual production Wasmi module | [standalone manifest](signals/Cargo.toml), [harness](signals/src/main.rs), [lockfile](signals/Cargo.lock) | [raw samples](signals/results.csv), [summary](signals/summary.json), [environment](signals/metadata.json), [process resources](signals/resource.json) |
| JSON write granularity | [Rust probe](json_write.rs) | [five paired observations](json_write_results.json), [unbuffered syscall trace](json_unbuffered_syscalls.txt), [buffered syscall trace](json_buffered_syscalls.txt) |
| Exact Runtime quota SQL | [Python probe](quota_sql.py) | [query, plans and observations](quota_sql_results.json) |
| Latest Nautilus upgrade target | Official Rust sparse registry, PyPI and exact release packages | [versions, MSRV, checksums, upstream SHA and Python release metadata](nautilus-latest.json) |

Raw timing files are the original observations. Source packaging only changed absolute
paths to repository-relative paths, made the structural runner's compiler configurable,
and declared the signal measurement crate as its own workspace. Numerical loops and
fixtures were not changed. Do not overwrite these observations when reproducing;
use a temporary copy/output directory or retain the original files first.

The JSON and SQLite exploratory timings may overlap unrelated compilation on the
shared host; use their write counts, byte equality and query plans as the primary
evidence. The structural and Wasmi measurements were coordinated to avoid heavy
compilation. Affinity reduces interference but does not make a cloud host exclusive.

## Toolchain

The original timing probes below used the pre-upgrade repository. Reproduce those
historical observations with Rust 1.98.1; do not relabel them as measurements of
Nautilus 0.67.0 / Rust 1.99.1. The revised implementation plan requires a separate
baseline on that new stack, with both optimization comparison arms using it.

In this execution environment the original
binary was `/workspace/.quazonai-setup/rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin/rustc`;
Cargo and registry cache were under `/workspace/.quazonai-setup/cargo`.
On another machine use your normal pinned toolchain; do not depend on these paths.
Check `taskset -pc $$` before choosing a CPU. Observations used CPU 0 and a four-core
cgroup quota on an AMD EPYC 9V74 host; they are not a hardware capacity commitment.

## Structural kernels

In a temporary copy of the `kernels` directory:

```sh
rustc --edition=2021 -C opt-level=3 -C debuginfo=1 kernels.rs -o kernels
python3 run.py
```

The runner uses `RUSTC` if set, otherwise `rustc`. Its CPU-0 affinity is intentional;
adjust both execution and recorded metadata if CPU 0 is unavailable. See the detailed
[method](kernels/README.md) for fixtures, quantile convention and simplified types.
Logical clone counts are not allocation-profiler measurements. Neither the projection
of DecimalValue into String nor `black_box` establishes production optimizer behavior.

## Production Wasmi module

From the repository root, use a temporary build directory:

```sh
cargo build --release --locked \
  --manifest-path .opensdlc/tasks/core-engine-performance/evidence/signals/Cargo.toml \
  --target-dir /tmp/qz-signals-reproduction
taskset -c 0 /tmp/qz-signals-reproduction/release/qz-signals-bench \
  > /tmp/qz-signals-reproduction.csv
```

Use `--offline` only when all pinned dependencies are cached. The harness directly
includes repository `apps/job/src/signals.rs`; it does not copy that implementation.
Its recorded SHA-256 is
`b7cd88d2b7b1da3f27399f4b4aee01c7423e4c2e81706bbe4e63615326e2f334`.
If the production file changes, record the new revision and hash and treat the new
measurement as a different baseline. All dependencies in the standalone lockfile
used versions present in the original project lockfile.

The two fixtures use a simple `close / previous_close - 1` model. They establish
compilation reuse behavior for that module only. Every measured pair checks equal
checksum and total fuel; they do not validate all possible stateful Wasm programs.
Resource JSON describes the whole combined process, not separate per-arm peaks.

## JSON writer

The original observation compiled `json_write.rs` with `rustc -C opt-level=3`, using
`-L dependency=target/debug/deps --extern serde_json=<matching rlib>` from the project
cache. Thus its dependencies were **debug**, even though the harness was optimized.
To reproduce that setup, choose the serde_json 1.0.151 rlib built by the same compiler.
Alternatively build a temporary Cargo binary with exact serde_json 1.0.151 and this
source, but record the profile change and do not compare its times as identical runs.

The binary takes `buffered|unbuffered` and an output path. Run both with disposable
paths, compare their bytes with `cmp`, and retain SHA-256 plus stderr counters.
The observed matching SHA-256 was
`b8632cee7ee3c99e41e6c448a8edbe757cb42e11a6fcffaac4bc27a38fc745da`.
For syscall verification run `strace -c -e write,fsync`; the saved traces include
11 extra stderr writes in each process, so their total write count differs from
the probe's file-write counter by 11. Never compare strace times to untraced times.
The probe creates synthetic arrays and does not validate the production manifest,
file-permission or cancellation boundaries.

## Quota SQL

`quota_sql.py` resolves the repository from its packaged location, loads the actual
Runtime migrations, extracts the SUM query from current source, and uses a disposable
in-memory SQLite connection. It writes `quota_sql_results.json` beside itself; preserve
the original before rerunning. It asserts the exact occupied-byte total at every size.
Python SQLite 3.53.1 was used; an older SQLite may not support the original migrations.
The output retains the exact SQL and query plan, but only aggregates of the 31 timings;
those exploratory timing samples were not retained individually.

## Scope of verification

- Structural fixtures check final-point values/times and target-trigger sequences.
- Wasmi pairs check exact checksum/fuel against current production module code.
- JSON pairs check identical bytes and reduced write calls.
- Quota SQL checks occupied bytes and scan plans.

These do not establish real catalog coverage, scientific evidence quality, solver
equivalence, full native fills/cash, OCI cancellation/recovery, production memory,
or production p95/p99. Those are implementation acceptance tasks in the main design.

## Latest-release research

`nautilus-latest.json` records the UTC lookup time, the highest non-yanked Rust
release without a prerelease suffix for each of the 12 directly used crate kinds,
and the release checksums and required Rust version. Python's default non-prerelease
selection and the separately published 2.0.0rc8 release are distinguished. The
project's execution path is Rust; this plan adds no Python Nautilus dependency.

API conclusions in report section 5 were checked against exact official 0.67.0
packages, rather than inferred from nightly documentation. Upstream implementation
sources were inspected only in temporary directories and are not copied here.
The receipt establishes the researched target, not a successful upgrade build.
Recheck the registry before implementation and release; pin any resulting target
exactly and update its acceptance record instead of resolving a floating `latest`.
