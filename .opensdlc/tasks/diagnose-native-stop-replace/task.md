# Diagnose native stop/replace failure without changing cleanup authority

## Exact failure and baseline

This isolated candidate starts from the 3018f10b189fb912fea93da20840bc7eada0cdc1
dev merge source, not PR 177 or another feature slice. Release run 37582752653,
job 112672750127 failed two original server library cases:

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

Cloud read-only manager inspection returned `Operation not permitted`; that
access was stopped without an alternate bus or escalation. No native systemd
test or Cargo command has run for this candidate yet. Standalone rustfmt parsed
the changed Rust source. This patch is diagnostic preparation, not a claimed
fix or a native test pass. The real failing stage must be established before
changing acceptance behavior.
