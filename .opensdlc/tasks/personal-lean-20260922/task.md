# Personal simplification and native computation

## Intent
The sole owner requested removal/compression of unnecessary documentation, tests, CI, code and security gates, plus performance optimization through native reuse. Base: main 57878e5fe1cf012582c0f038466bc6fbe1ee1ca0. Scope is source maintenance, not deployment or closing Issue #62.

## Implementation
Task-local Wasmi Module reuse with fresh mutable stores; one selected catalog shared by portfolio members; borrowed ensemble members with unchanged ndarray::dot identity; consume owned result buffers. Remove optional extra-checks, not Wasmi validation or resource budgets.

Delete runtime role/ACL scanner and its dedicated tests. Allow the database owner in API/Worker; optional separate-role migration grants remain. Remove distributed machine-auth rate windows from request paths and their dedicated tests. Retain token verification, scopes/revocation, bounded Argon2, loopback/Origin, scientific isolation, idempotency and data recovery. Existing migrations and stored data are unchanged.

Remove CodeQL workflow, per-change dual SBOM generation and redundant CI aggregation; run the real PGMQ contract once in the Store job. Cancel superseded branch runs. Compress duplicated README/Agent/OpenSDLC guidance; use existing tempfile instead of a custom test-directory allocator. Keep license notices and useful native regressions.

## Verification
The shared-module regression covers globals, linear memory, failure isolation, exact predictions/fuel and invalid budgets. Existing native forecast/validation/portfolio, HTTP, database and recovery tests remain applicable. The owner-connection TCP regression and repeated invalid-token test replace removed policy assertions.

Run native cargo fmt, Clippy and all applicable workflows on the candidate Head. Benchmark: `cargo run --locked --release -p job --example benchmark_signals`; CI also records its debug-profile output without timing thresholds. Actual command outcomes, profile, CI and review references belong in [PR #104](https://github.com/zhengui666/QuaZonai/pull/104); the field-level plan is [Issue #105](https://github.com/zhengui666/QuaZonai/issues/105).

Native CI [35699565235](https://github.com/zhengui666/QuaZonai/actions/runs/35699565235) on 0512b2c emitted `Error: reuse changed results or fuel` despite the job's successful conclusion: tee masked the exit code. That Head is not accepted. Wasmi defaults to lazy translation, charging compilation to the first instance only. The correction selects native Eager compilation before instantiation, adds cold/warm instance budget/exhaustion regression and propagates the benchmark exit with pipefail. Execution fuel must match between independently instantiated copies under this policy; historical lazy-translation counters remain untouched and are not claimed equivalent. Compilation remains subject to the native job's wall-clock and process limits.

## Delivery
Web ChatGPT authors all changes. Under the current execution-only authorization, local Codex requests gpt-5.6-luna and executes only web-authored native test/build/format/Git/GitHub commands; GitHub Actions verifies the integrated candidate. Failures return to the web author, never to Codex for implementation. Preserve the unrelated dirty checkout, user data, services and credentials; no production migration or deployment. Merge only after current-Head applicable CI and explicit clean read-only Codex review. Whole-product acceptance and whole-workload speedups are not claimed.
