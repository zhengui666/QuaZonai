# Confirm a collected transient through the native manager without weakening its fence

## Exact failure and baseline

This correction starts from PR 177 maintenance head
`4e746d8c471997528b4cc3bb5821698e15ceb307`. Its historical diagnostic patch began
at dev merge `3018f10b189fb912fea93da20840bc7eada0cdc1`. The original release run
`37582752653`, job `112672750127` failed two server library cases:

- `codex_native::service::native_tests::native_stop_replace_cancels_an_existing_start_and_leaves_no_populated_group`
- `codex_native::service::native_tests::native_stop_replace_cancels_a_deterministically_queued_start`

Both reported `Unavailable`. The first reached its final result assertion; the
second panicked while stopping the fixture's oneshot blocker, before reporting
the original target result. Adjacent native persistent-mask and cgroup/kernel
tests passed, so this is not evidence that the CI user manager was unavailable.

The existing adapter folds manager mask/reload, mask observation, stop,
observation parsing, cgroup reads and final fencing into the same failure enum.
The retained CI log does not identify which condition failed. A cgroup
exists/read race is a candidate only, not an established root cause.

## Bounded diagnostic change

Keep every production success/failure predicate, native manager operation,
timeout, stop/replace command, PID/job check, populated check and persistent
mask check unchanged. No unknown or missing observation becomes successful
cleanup. No resource-accounting or database terminal state is changed.

Only test builds with `native-codex` can emit new diagnostics. A registry contains
exact fresh names allocated by this module's native fixtures; other units are
not logged. Diagnostics contain the failing stage, exit/OS/error code, the
seven already-requested unit properties or the existing populated-zero check.
They never print native stderr, service command lines, environment, credentials
or unrelated user services. Traces are emitted on failure, not during ordinary
successful native transitions.

The queued fixture retains the primary target outcome and all cleanup outcomes
independently. A blocker/unmask failure can no longer replace the primary error;
any failed cleanup still fails the test. A pure regression covers owned-unit
selection, property projection and preservation of the primary error.

## Required validation and limits

Run the unchanged native cases in the existing CI user systemd environment with
`cargo test --locked -p server --lib --features native-codex <exact-case> -- --exact --nocapture`.
The user manager, `XDG_RUNTIME_DIR`, native `systemctl`, `busctl`, `systemd-run`
and cgroup v2 are required. These cases do not require a real account, Codex
credential or new container. Reuse the existing rust-regression CI setup.

## Actual diagnostic result and bounded correction

The diagnostic patch reached head `4e746d8c471997528b4cc3bb5821698e15ceb307`.
CI run `37596191488`, job `112709308070` executed both original tests and both
failed. The actual user manager reports `Version=255.4-1ubuntu8.17`. In both
target cases and the queued case's blocker cleanup, stop returned zero and the
complete original mask/terminal/cgroup checks passed; the final fence rejected
`LoadState=not-found` with an empty fragment. The previous cgroup-read race
hypothesis is not the observed cause. The diagnostic-only pure fixture passed
separately, which did not establish native stop correctness.

Official upstream v255.4 provides a concrete cache/collection explanation:

- [`unit_remove_transient` and `unit_free`](https://github.com/systemd/systemd-stable/blob/v255.4/src/core/unit.c)
  remove a collected transient's fragment outside manager reload.
- [`lookup_paths_mtime_exclude` and `unit_file_build_name_map`](https://github.com/systemd/systemd-stable/blob/v255.4/src/basic/unit-file.c)
  exclude the manager-owned transient directory from automatic cache invalidation
  and can retain the previously built name map.
- [`unit_load_fragment`](https://github.com/systemd/systemd-stable/blob/v255.4/src/core/load-fragment.c)
  opens the selected cached fragment; a removed transient path can fail before
  the lower-priority persistent mask is selected.
- [`manager_reload`](https://github.com/systemd/systemd-stable/blob/v255.4/src/core/manager.c)
  clears the name maps and reconstructs units from the serialized manager state.

These sources support the observed sequence; the Ubuntu-patched manager must
still execute the corrected original tests. This affects the production native
adapter as well as fixtures: `resources.rs` uses `systemd-run --collect` and
calls the same `stop_and_confirm` before confirming resource closure/recovery.
It is not corrected by changing a fixture to keep the transient alive.

Starting from head `4e746d8`, the correction performs at most one native `Reload`
only after stop returned success, the first complete original confirmation
passed, and the observed load state was exactly `not-found`. It then repeats
`observe`, `masked`, job/PID/active checks, exact cgroup-path correlation and the
existing populated-zero check. The final accepted states remain exactly the
original `masked`, or successful-stop `loaded` with a nonempty fragment.
`not-found` is never a success condition. Reload or either complete confirmation
failure remains a failure. No unmask, second stop, sleep/poll loop, altered
command timeout, process-accounting write or database terminal-state change is
introduced. A small private closure-based helper makes this one action's control
flow testable; it is not a new native backend or protocol.

Four new pure/component regressions cover one refresh followed by the original
fence, persistent not-found, failed-stop/no-refresh, reload/observation failure,
revived PID/job/active/group observations, exact cgroup identity and populated
values. They use controlled returned observations, not a real manager, and do
not substitute for the original native tests.

Cloud manager access remains stopped after the earlier `Operation not permitted`.
For this correction, the assigned exclusive Cargo window rebuilt Contracts,
Domain, Store, Integrations and Server from this isolated source. Five service
rule/component tests (four new and the original observation parser) and the
existing diagnostic pure fixture actually passed: six passed, zero ignored.
The `evidence/01-service-list.log` verbose build records the exact source paths;
`02-service-five.log` and `04-diagnostic-one.log` record execution. The diagnostic
fixture intentionally catches a panic to verify preservation of the primary
failure; its final test result is one pass. Standalone rustfmt also passed.

Real native systemd and PostgreSQL tests for this correction remain NOT RUN.
The cloud manager is not accessed again. A production-fix acceptance claim
requires the corrected exact CI head to pass both original native stop/replace
cases and preserve the persistent-mask late-launcher boundary. The six pure
passes establish control-flow/rule behavior, not an actual manager repair.
