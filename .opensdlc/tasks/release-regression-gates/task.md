# Run nine explicit release regressions before merge

The full 3018f release exposed failures outside the selected pre-merge gates.
Preserve the existing selected PostgreSQL job, its twelve Dataset/Forward and eight
native-client cases, and the legacy portfolio/recorded-feature gates. Append nine
exact cases from the independently authored maintenance fixes: optional-wall
credentials (three), immutable legacy issuance (three), native Mission worker stack
(one) and systemd user stop/replace (two). This runner must be composed with those
business fixes; missing renamed cases fail exact listing rather than silently pass.

Each case requires one listed match, one executed pass, zero ignored cases and
successful Cargo/tee pipelines. All nine cases are attempted even after failure;
missing dependencies, compilation and ignored/list-only results are not acceptance.
The existing explicit localhost PostgreSQL container guard remains mandatory.

Reuse only necessary environment from rust-regression: original user resource
manager/dbus/cgroup context, official pinned Codex App Server installed from the
existing lock, and its path-specific userns profile with always cleanup. Preserve
native tests' existing timing and ordinary test thread stack; no RUST_MIN_STACK,
short added case timeout, real account/model or market endpoint is introduced.
The selected worker uses Cargo's server executable as MCP, so no portable CLI or
additional unshare fixture is built. This is native regression setup, not frontend,
UI or additional container acceptance. Full release regression already runs the
original targets and is not duplicated by this runner.

The gate is based on PR177's frozen 1600-file source, and is independent of the
3018f-based business fixes. Do not replace PR177's CI file from an older source.
Static YAML/shell/selector preservation is necessary but not a real-PG/native pass;
actual execution must occur in the supported isolated exact-head CI environment.
