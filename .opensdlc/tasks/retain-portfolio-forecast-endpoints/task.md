# Retain only consumed portfolio forecast points

## Scope

The owner requested evidenced Rust performance improvements within QuaZonai, without changing the business model, modifying upstream implementations or removing tests. This slice starts from dev after PR #169. Publication and final-commit CI remain separate delivery steps.

The changed production files are [forecast.rs](../../../apps/job/src/forecast.rs) and [portfolio.rs](../../../apps/job/src/portfolio.rs). No dependency, manifest, schema, execution budget, data-import adapter or CI selector changes.

## Redundant work and correction

Portfolio preparation previously materialized every historical `NativeForecastPointV1` for each Alpha, although it only consumed the latest point for each instrument. It allocated one instrument-name string per historical point, then searched the whole result backwards separately for each asset. Rolling Study repeats that preparation for every cutoff.

One shared forecast loop now serves both consumers. Public forecasts still return the complete report. Internal portfolio preparation retains only each instrument's last point and receives the consumed fuel separately; its partial result is never presented as a complete forecast report.

Every original model invocation, fresh per-asset instance, fuel deduction, causal feature update, label calculation, finite check and clock/ordinal conversion remains. Errors in discarded prefix rows still fail the task. Catalog selections, independent Study cutoffs, calibration, native solving and output validation do not change. No process-wide state or cache is introduced.

## Native verification

Rust 1.98.1, one Cargo job, dev/test debug info disabled, incremental disabled:

`cargo test --offline --locked -p job --features native-paper-test --lib --test forecast --test signals --test study --test instrument_versions --test allocation -- --test-threads=1`

Passed: 108 tests, zero failed or ignored. This includes three new tests comparing retained endpoints and exact fuel with the complete stateful forecast, cross-asset memory isolation, exhaustion of the whole-task fuel budget, and failures in discarded predictions, clocks and labels. Existing native catalog, Wasmi, Clarabel, instrument-version and rolling shared-account tests pass unchanged. Scoped rustfmt and diff whitespace checks pass.

This is focused local validation, not a full workspace/real-PG regression or final-commit remote CI result. No frontend or container routine was added or executed.

## Measurements and limits

A one-off native harness ran the actual Portfolio Build path: original synthetic Parquet catalog, Wasmi forecasts, Clarabel allocation and domain output validation. The workload contains two assets with 100,000 bars each. It is a bounded synthetic performance fixture, not market-quality evidence.

Each variant ran six times in interleaved A/B/B/A order, using the same persisted input objects and warm filesystem cache, without compilation during the timed pairs. Toolchain, dependency cache and unoptimized dev profile were identical. Every complete result compared byte-for-byte equal, including fuel. Fixture construction is excluded from timing. Process time includes result serialization and writing.

| Workload / measurement | Before median | After median |
| --- | ---: | ---: |
| 2 Alpha members, uninstrumented Build wall | 1.4199 s | 1.3573 s |
| 2 Alpha members, uninstrumented process wall | 1.6811 s | 1.6085 s |
| 8 Alpha members, uninstrumented Build wall | 3.6162 s | 3.5012 s |
| 8 Alpha members, uninstrumented process wall | 3.8577 s | 3.7585 s |
| 2 Alpha members, separately instrumented cumulative Rust allocation | 417.04 MB | 374.24 MB |
| 2 Alpha members, separately instrumented peak additional Rust heap | 92.78 MB | 92.78 MB |

The firm resource result is 42.80 MB less cumulative Rust allocation per two-member Build, about 10.3%. Retained forecast rows fall from 200,000 to 2 per member. Wall-time medians improve modestly, roughly 3–4%, with overlapping run ranges; this is not a promised production speedup. Overall peak heap does not improve because another phase dominates; observed process RSS ranges overlap and the candidate median is slightly higher. No peak-memory saving is claimed. These measurements do not establish optimized-release performance or faster Rust compilation.

## Compilation audit

The current [full native release regression](https://github.com/zhengui666/QuaZonai/actions/runs/37422825989/job/112138154921) recorded 6m12s for all-target/all-feature check, 13m21s for all-target build and 57.66s of test-profile compilation. Actual compiler entries and a local locked Cargo dependency tree confirm both Arrow 56 and 59 families. Nautilus persistence brings DataFusion 55 and Arrow 59; QuaZonai's IPC contracts use Arrow 56.

No dependency upgrade is bundled here: replacing the IPC family requires its own compatibility evidence, and deleting required native capabilities would be an invalid shortcut. The existing routine pipeline already avoids all-integration-target linking; the quoted release timings are not routine-CI timings. This slice removes measured runtime allocation rather than making an unsupported build-speed claim.
