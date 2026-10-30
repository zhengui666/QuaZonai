# Core engine implementation: independent verification preparation

Prepared on 2026-10-30 against remote `dev` commit `78a84bdd31f524b4965560fc4b6ae4ab53cafcef`, the merge of [PR 265](https://github.com/zhengui666/QuaZonai/pull/265). This is a review and acceptance checklist, not a record of executed tests or approval of the changing worktree. The implementation branch is `perf/nautilus-latest-core-engine-20261030`.

Sources: [AGENTS](../../../AGENTS.md), [review requirements](../../review.md), [project commands and evidence boundaries](../../project.md), [Makefile](../../../Makefile), [CI](../../../.github/workflows/ci.yml), [full retained science workflow](../../../.github/workflows/rust-regression.yml). Production source observations below refer to the immutable baseline, inspected with `git show 78a84bdd:<path>`; working files are being edited by other implementers.

## Old plan versus current dev

The [original performance report](../core-engine-performance/report.md) started from `d7da6bff`. Its timings and retired control-plane paths cannot be presented as current-dev end-to-end measurements. Keep the original evidence at its original scope.

| Proposal | Status at `78a84bdd` | Current action and verification boundary |
| --- | --- | --- |
| P01: buffered JSON output | Still relevant. `managed.rs:72-126,178-185` writes through an unbuffered `File`; `report.rs:52` directly serializes pretty JSON. Managed publication now stages objects and preserves an existing target. | Preserve byte limits, propagated serialization/write/flush errors, final hash/length, no-clobber publication and report sync/rename semantics. Verify bytes and publication failure paths as well as syscall reduction. |
| P02: delay target clone | Still relevant at `simulation.rs:668`. Replay has substantially more readiness and clock behavior than the old benchmark. | Exercise catalog cutoff, scoped rebalance, legacy synchronized rebalance, pending reductions, settlement and paper receipt clocks. A clone may move only after guards whose observable errors/state remain equivalent. |
| P03: cross-section readiness | Partially different architecture. `simulation.rs:674-702` validates an explicit `rebalance_instrument_ids` scope. The full `latest` scan is in the legacy branch at `704-719`. | Do not claim every current callback scans all assets. A future readiness cache needs timestamp replacement, stale/missing bar, scope, receipt-clock and cancellation tests. It is an experimental follow-up unless this change actually implements it. |
| P04: latest-only forecast output | Major part already implemented: `forecast_latest_market` in `forecast.rs:117`; prediction/finite checks still cover every bar, while only the final DTO per asset is retained. `portfolio.rs:229,255-259` still searches that shorter vector. | Remaining lookup is approximately O(A²), not the old O(A²T). Compare outputs and consumed fuel. Preserve every stateful model call and intermediate validation; do not claim the old DTO-elimination speedup again. |
| P05: reuse the same loaded selection | Same-cutoff duplicate catalog load is already removed: portfolio loads once (`45-56`), and Study shares `market` between preparation and liquidity (`111-142`). | Remaining cost is repeated full prefixes across cutoffs and raw object cloning. Do not reimplement the already-completed single-load change. Incremental prefixes require independently justified semantics and new measurements. |
| P06: compiled Wasmi reuse | Still relevant: `forecast.rs:154` creates a module for each forecast call. The current API takes `fuel_metering: bool`. | Cache immutable compiled code with a bounded policy and metering mode in identity. Every asset/cutoff retains a fresh Store/instance. Current Study carries remaining explicit fuel from one cutoff to the next (`study.rs:127-128`); `None` means unmetered. No restoration of retired implicit fuel/memory quotas. |
| P07: repeated covariance/result validation | Still relevant in optimization and domain result checks. | Only remove provably redundant internal work while preserving the external validator and infeasibility/error conversion. Incremental covariance or a different estimator is a separately registered numerical method, not an invisible optimization. |
| P08: Prepared/history copies | `portfolio.rs:357,369` clones forecasts and return history; Study retains per-cutoff frames. | Ownership transfer may be local and behavior preserving. Shared history representations need explicit compatibility, lifetime and peak-memory evidence. |
| P09: footer scans / string keys | Still relevant: catalog prefix reads inspect metadata; both price-only and native catalog paths construct string keys. | Preserve exact instrument/bar-type identity, timestamp selection, update evidence and price-only factor semantics. Measure typed-key changes separately from read pruning. |
| P10: Nautilus ingestion/analysis | Old dependency and API findings must be rechecked against the newly selected official version. Baseline still calls `add_data(events, None, true, true)` and keeps canonical JSON conversion paths. | Verify the new typed batch API's sort/validation contracts directly. Preserve deterministic equal-timestamp order, statistics, progress visibility and failure behavior. Disabling analysis is acceptable only where no downstream result consumes its statistics. |
| P11: formula-based calibration | Still a candidate; replacing formula regression is not necessary for dependency compatibility. | Keep as an experiment until measured material and shown to preserve rank, degeneracy, intercept, residual and finite-value behavior. |
| P12-P17: Rust worker/store/runtime SQL, locks, supervisor, HTTP slots and artifact BLOBs | Retired by the TypeScript service / official Pi Durable / separate remote Nautilus architecture. `apps/server`, `apps/runtime` and `crates/store` are absent from this baseline. | Do not recreate or optimize the deleted system. Any current control-plane investigation must measure the TypeScript service, durable host and `apps/nautilus` remote child protocol. Old SQLite microbenchmarks do not demonstrate current latency. |

Current dev also adds catalog ensemble, observed spot/cash, forward evaluation, native account observers, ticks/history and capital handling. The dependency migration must cover these actual callers. A newly observed profiling candidate is `catalog_prefix.rs:18-51`: the identity protocol loads and materializes canonical JSON for per-series and combined hashing. This is not a requirement to change that protocol; hashes and scientific identities cannot change silently.

## Dependency and numerical scope

The in-progress manifest snapshot inspected on 2026-10-30 has **17 unique direct Nautilus crate names**, all exactly `=0.67.0`: `analysis`, `backtest`, `binance`, `common`, `core`, `data`, `execution`, `hyperliquid`, `indicators`, `live`, `model`, `persistence`, `polymarket`, `portfolio`, `sandbox`, `serialization`, `trading` (each prefixed `nautilus-`). The lockfile snapshot has 22 Nautilus packages, all `0.67.0` from crates.io. These are observations of a changing worktree, not final acceptance; repeat on final Head. The old report's smaller dependency inventory is insufficient.

Review all production and dev dependency declarations, enabled feature combinations, lockfile sources, toolchain/MSRV and CI/image pins together. Official packages or a fixed official upstream revision are allowed; copied upstream implementations, forks and local dependency patches are not. Build both standard precision and `market-history`/high precision: one working default build does not establish the other representation's correctness.

Scientific acceptance retains frozen inputs, PIT cutoffs, Sealed isolation, independent evaluation, original account/inventory/receipt identity, scientific budgets and missing-value rejection. Specifically recheck Money/currency precision, fee conversion, settlement, order/fill event ordering, zero/near-zero numerical checks, scoped targets and cash inventory. Baseline native simulation explicitly rejects unsupported instrument-update replay; dependency support alone does not authorize relaxing that contract.

## Executable acceptance sequence

Run from the repository root with Node 22.19.0 or a supported later release and the **final checked-in** Rust toolchain. Commands here are prerequisites and targets, not assertions that they have run. Coordinate native builds with other workers rather than racing over the same target output.

### TypeScript, browser and packaging

```sh
npm ci --prefix apps/service --no-audit --no-fund
npm ci --prefix apps/research --ignore-scripts --no-audit --no-fund
npm ci --prefix apps/cli --ignore-scripts --no-audit --no-fund
npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
make check-ts check-deploy
make check-web
npm --prefix apps/web exec -- playwright install --with-deps chromium
npm --prefix apps/web run test:e2e
```

`check-ts` includes service, research, remote Nautilus protocol and CLI. `check-deploy` also executes public-source Python tests and package/native-image contract tests. Browser tests use actual HTTP and disposable Durable SQLite with controlled scientific subprocesses; they establish persistence and interaction behavior, not scientific correctness.

### Build and connect the actual native kernel

Use an absolute artifact directory unique to this checked revision; preserve its contents through verification. The build script first copies standard binaries and then builds/copies high-precision outputs. A subsequent build into the same Cargo target may overwrite `target/debug/job`, which is why `check-core` must consume the copied standard artifact.

```sh
QZ_VERIFY_ARTIFACTS="$(mktemp -d /tmp/qz-engine-artifacts.XXXXXX)"
bash deploy/docker/native-build.sh debug "$QZ_VERIFY_ARTIFACTS"
make check-science PROFILE=debug
make check-core PROFILE=debug QZ_JOB_BINARY="$QZ_VERIFY_ARTIFACTS/job"
```

`check-science` uses the full maintained Makefile list, including factors, catalog prefix, CLI, live progress, simulation, allocation, validation, Sealed, sandbox/paper observers, source/preparation, execution preflight, ensemble, capital, spot/observed and Study tests. Its generated native, observed and forward fixtures are prerequisites for `check-core`. Preserve the default fixture directory environment between both commands, or pass identical explicit `QZ_PM_NATIVE_FIXTURE_DIR`, `QZ_PM_OBSERVED_FIXTURE_DIR` and `QZ_PM_FORWARD_FIXTURE_DIR` to both.

`check-core` verifies fixture existence and executes the matching real kernel through service/native and remote HTTP consumers. `browser_catalog_fixture` and `catalog-prepare` must be beside `job`. A test that skips because its binary or fixture is absent is **not** a passed native check. For release performance measurements, use `native-build.sh release`, `PROFILE=release` and the matching release artifacts/tests; do not use a debug timing as production performance.

### History and high-precision tick path

CI runs these additional paths after `check-science` and `check-core`. Use the final pinned toolchain; the following is the debug form. Add `--release` to each Cargo test when matching release artifacts.

```sh
python3 -I -B -m unittest discover -s runtimes/data -p test_hub.py -v
cargo test --locked -p job --features market-history,catalog-prepare,hyperliquid-offline --bin market-history --test catalog --test catalog_prepare --test managed --test instrument_versions --test native_instrument_updates
QZ_VERIFY_TICK_FIXTURES="$(mktemp -d /tmp/qz-engine-ticks.XXXXXX)"
QZ_TICK_NATIVE_FIXTURE_DIR="$QZ_VERIFY_TICK_FIXTURES" cargo test --locked -p job --features market-history,catalog-prepare,hyperliquid-offline --lib ticks::
cargo test --locked -p job --features market-history,catalog-prepare,hyperliquid-offline --lib catalog::currencies
QZ_JOB_BINARY="$QZ_VERIFY_ARTIFACTS/job" QZ_TICK_JOB_BINARY="$QZ_VERIFY_ARTIFACTS/job-high-precision" QZ_TICK_NATIVE_FIXTURE_DIR="$QZ_VERIFY_TICK_FIXTURES" node --experimental-strip-types --test apps/service/test/tick-native.test.ts
```

For this broad native dependency migration, the retained all-feature suite is valuable additional coverage of optional native adapters:

```sh
make check-native-full
```

It runs workspace/all-target/all-feature Clippy with denied warnings and workspace/all-feature tests on the retained three crates. It is the existing manual/formal-release workflow, not the deleted Rust service regression suite and not an automatic requirement for every future TS-only change. Record actual execution, failures or resource blockers separately from routine CI.

### Affected-contract checks

When native schema changes are intentional, run `make generate-native-schema` and review the generated diff. Do not hand-edit generated contracts. Review existing fixture changes for preserved economic meaning instead of accepting a new output merely because the dependency changed. Run `make check-links` for current documentation; historical reports retain their documented baseline.

For behavior specifically changed by the patch, select or add meaningful cases: buffered serialization limit/failure cleanup; stale/same-timestamp/out-of-order bars and target scope; repeated model reuse with fresh mutable state, metered/unmetered configuration and fuel traps; correlated/degenerate numerical inputs; typed ingestion's order and progress visibility; account settlement and exact cost/quantity identity. Avoid tests that only assert the implementation's chosen data structure.

## Performance evidence required for performance claims

Use the actual baseline and candidate CLI on identical frozen fixtures, preserving output checksums or independently checked numerical/event equivalence. Record the commit and dirty-tree status, Rust/dependency versions, release profile, precision/features, hardware/cgroup limits, input assets/bars/cutoffs/models, cache state and command. If toolchain and dependency versions differ, attribute the aggregate result to the upgrade plus optimization; isolate a dependency-only candidate before attributing the difference to a particular code edit.

Measure wall time and peak RSS for real native forecast/Study/simulation, plus connected service completion when claiming user-visible RT. Include a small fixed-cost case and a material multi-asset/multi-cutoff case. Profile a representative slow workload to confirm where CPU/allocation/I/O time moved. Keep cold setup/compilation and warmed reuse separate. Use repeated runs and report variability; a handful of whole-job runs does not support p95/p99 claims. Native callback tails need enough directly collected callback samples.

Microbenchmarks support mechanism claims only: fewer writes, avoided DTOs/clones, module reuse or typed keys. Old `d7da6bff` numbers do not prove a speedup over `78a84bdd`. Do not add their speedups together, claim asymptotic wins already present in dev, or turn eliminated code paths into measured end-to-end savings. Existing tests prove their declared software behavior and do not establish market quality, profitability, deployed service performance or live-account authorization.

## GitHub CI and exact-final-Head review

The baseline CI explicitly checks out `github.event.pull_request.head.sha || github.sha`. The dependency/source/toolchain changes make scientific scope applicable. Final PR evidence must include successful `typescript`, `web` and `science-core` jobs on the delivered Head, including their real native consumers. Inspect job steps and scope output; a green aggregate with an incorrectly skipped science job does not satisfy acceptance. Push runs test release while PR runs test debug in the current workflow. Preserve tested revision metadata and exact standard/high-precision binaries for any later image production.

The [review policy](../../review.md#approval) requires separate independent review of the exact final Head, all actionable findings resolved, and GitHub Codex **read-only** review explicitly reporting no findings on that Head. Any subsequent code change invalidates the final-Head assertion until affected checks and review cover the new revision. Merge into remote `dev` only, then re-read the PR merge state and the remote commit. The parent implementation owner performs publishing and merge; this preparation performed no external writes.

Observed precedent from [PR 265](https://github.com/zhengui666/QuaZonai/pull/265), fetched through the installed GitHub connector on 2026-10-30:

- The [owner's manual trigger](https://github.com/zhengui666/QuaZonai/pull/265#issuecomment-6500455902) was a PR conversation comment containing `@codex review` followed by `Please review final head 081ce037 (read-only).` Opening a PR for review and marking a draft ready are also listed triggers. The authorized implementation owner can use the same read-only request with the **full final SHA**.
- The bot identity was `chatgpt-codex-connector[bot]`, GitHub user ID `199175422`.
- Its [explicit receipt](https://github.com/zhengui666/QuaZonai/pull/265#issuecomment-6500494424) states `Codex Review: Didn't find any major issues.` and identifies reviewed commit `081ce03753`.
- Its [summary comment](https://github.com/zhengui666/QuaZonai/pull/265#issuecomment-6500449590) carries machine-readable `codex-review-state:v1`: `code_review.commit_sha` is `081ce03753cbdf510f06e94c16d39aa9fb0c4561`, `status` is `completed`, `has_any_findings` is `false`, and `has_unresolved_findings` is `false`.
- The summary's **security** review refers to an earlier commit. Read code and security states separately rather than assuming every completed row covers the same Head. The formal submitted-review list for PR 265 is empty: ordinary conversation receipts contain the relevant evidence. Empty reviews or a thumbs-up reaction alone are insufficient.

Use PR conversation comments, submitted reviews and inline threads together; validate the bot identity, full commit association, completed state and actual no-findings result. A connector wrapper that only returns PR-triggered workflow runs or the first page cannot prove all checks are complete. Fetch applicable run/job details and follow pagination if needed. Commit-status aggregation alone can omit check-run results.

## GitHub access observations

The installed GitHub connector successfully identified `zhengui666`, searched remote branches and fetched PR 265 metadata/comments/reviews. It is the demonstrated read path for this session. These reads did not test write permissions.

Local `gh` is version 2.46.0. `gh auth status` reported an invalid `GH_TOKEN`; actual `gh api` requests failed with `Forbidden` under the session network proxy. That combination does not independently distinguish token validity from proxy denial. `git ls-remote origin refs/heads/dev` failed because no noninteractive HTTPS username could be obtained. Local `origin/main` still points at the older report baseline and there was no fetched `origin/dev` ref when inspected; do not infer remote dev from that stale local ref.

No credentials were exposed, login/configuration changed, push attempted, PR review requested, or external message sent by this verification worker. CLI access failure does not mean the GitHub connector is unavailable. The parent coordinator subsequently verified normal Git fetch and push, published the implementation branch, and created draft PR #267 through the installed connector. Earlier CLI failures above describe this reviewer process only; they are not a current publication blocker.
