# Rust-only routine validation and explicit regression

## Request and boundary

The owner requested routine Rust compilation/build and unit tests without frontend or container acceptance. Preserve the unchanged failing portfolio-release case and real PostgreSQL/PGMQ dependencies in both original Store/Server targets. Do not delete/ignore it, enlarge its stack, or substitute compilation for execution.

Integration: [PR #166](https://github.com/zhengui666/QuaZonai/pull/166), targeting `dev`. This proposal is separate from the ongoing stack diagnosis/fix. It must be coordinated with the diagnostic result before changing the PR; no current diagnostic run is restarted or cancelled by this proposal.

## Implementation

- Keep routine [CI](../../../.github/workflows/ci.yml) and job IDs `rust-native-contracts` / `store-postgres`
- Keep the workspace all-target/all-feature `cargo check`; link only lib/bin production targets with existing optional Job/Server features
- Run [classified rules/local components](../../../tests/native/rust_unit.sh), including Contracts/Domain rules in Cargo integration targets, Store non-PG rules and default-feature Server rules
- Run [the exact original real-PG case](../../../tests/native/portfolio_release_regression.sh) independently in Store `experiment_compilations` and Server `portfolio_study_http` with `native-codex`; verify one list match and one actual passed non-ignored test in each, retaining separate statuses and running Server after Store failure
- Keep real PG18/PGMQ1.10.0 extension checks/migrations; omit npm/Codex/portable-CLI/systemd preparation only from this narrowly selected PG job
- Preserve the former complete default Rust CI jobs in [Rust regression](../../../.github/workflows/rust-regression.yml), with no routine skips; manual dispatch accepts an optional revision (empty uses the selected workflow branch version), and workflow_call requires a revision
- Call regression directly from [version publishing](../../../.github/workflows/release-version.yml); image `build` depends on it, for main and dev versions
- Keep `release.py` CI_PATHS limited to automatically triggered `ci.yml`, avoiding a wait on the non-automatic regression workflow
- Preserve existing separate manual frontend/OCI/browser/container acceptance and version CLI/image/install production

## Coverage and risk

`--lib --bins` alone is not a unit-test classification: Store lib embeds six real-PG cases, Server's `native-codex` lib feature embeds system probes, and CLI/Job lib/bin code embeds HTTP/process tests. Routine exclusions are scoped to those verified modules/cases. Local file components are labeled honestly.

The former non-DB CI run reported 575 tests. The initial classification estimated about 375 retained from that group, plus 38 Server and eight Store local cases. Independent review narrowed the client skips to two origin socket/HTTP cases and five account HTTP/FIFO/process cases, restoring seven original rule/file-component cases in each shared CLI/Server copy. The refined static estimate is about 382 from the old non-DB group, 45 Server and eight Store local cases; none is a new observed pass count, and final totals need actual target listing/execution. Contracts/Domain's 198 rule cases in integration targets are retained. Roughly 193 former non-DB integration cases and broad PG business chains move out of every PR; they remain in manual and mandatory version regression. Integration/example linking and doctests move there as well. Compile coverage remains all-target/all-feature, but it is not linking/runtime evidence.

Observed earlier run timing: non-DB check 6m29s, build 13m50s, test compile 1m02s/run 2m29s; PG test compile 7m25s/run 22m42s. These workloads are not a paired benchmark. No percentage or sub-five-minute claim is made. The native job now uses a pinned mainstream dependency-only Rust cache with compiler/host/environment/manifests/lock and workflow/shared-command-plan inputs. It excludes workspace crates, installed binaries and incremental state. Saving on an ordinary failed run is enabled, but canceled/timed-out runs may not save. Actual cache hit/restore/save costs and paired same-plan cold/hot timings remain unmeasured.

## Verification and delivery

Static YAML/Bash/Make/command-plan and controlled shell-contract checks can run in the proposal workspace. They cannot establish actual Rust compilation or transaction success: this workspace has no Rust compiler/Cargo. Independent native review and actual final-Head CI remain required. Proposal preparation is not remote publication, merge, release or a passed stack regression.

## Temporary unresolved-failure observation

Until the stack repair is verified, the required PG job obtains the unique Store test executable from its own normal `cargo test --no-run --message-format=json` output and performs bounded read-only inspection of seven related x86-64 function families. Only static prologue reservations are reported; this does not establish total runtime stack usage or a repaired chain. Static inspection failures do not suppress either original case, and any failure keeps the job nonzero. Remove this observation and the environment-gated fixture markers after the verified repair.
