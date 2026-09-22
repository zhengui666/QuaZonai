# Personal simplification and native computation

## Intent
The sole owner requested removal/compression of unnecessary documentation, tests, CI, code and security gates, plus performance optimization through native reuse. Base: main 57878e5fe1cf012582c0f038466bc6fbe1ee1ca0. Scope is source maintenance, not deployment or closing Issue #62.

## Implementation
Task-local Wasmi Module reuse with fresh mutable stores; one selected catalog shared by portfolio members; borrowed ensemble members with unchanged ndarray::dot identity; consume owned result buffers. Remove optional extra-checks, not Wasmi validation or resource budgets.

Delete runtime role/ACL scanner and its dedicated tests. Allow the database owner in API/Worker; optional separate-role migration grants remain. Remove distributed machine-auth rate windows from request paths and their dedicated tests. Retain token verification, scopes/revocation, bounded Argon2, loopback/Origin, scientific isolation, idempotency and data recovery. Existing migrations and stored data are unchanged.

Remove CodeQL workflow, per-change dual SBOM generation and redundant CI aggregation; run the real PGMQ contract once in the Store job. Cancel superseded branch runs. Compress duplicated README/Agent/OpenSDLC guidance; use existing tempfile instead of a custom test-directory allocator. Keep license notices and useful native regressions.

## Verification
The shared-module regression covers globals, linear memory, failure isolation, exact predictions/fuel and invalid budgets. Existing native forecast/validation/portfolio, HTTP, database and recovery tests remain applicable. The owner-connection TCP regression and repeated invalid-token test replace removed policy assertions.

Run native cargo fmt, Clippy and all applicable workflows on the candidate Head. Benchmark: `cargo run --locked --release -p job --example benchmark_signals`; CI also records its debug-profile output without timing thresholds. Actual command outcomes, profile, CI and review references belong in the PR. Source edits and configured checks are not evidence of successful execution.

## Delivery
Web ChatGPT authors all changes; GitHub Actions executes native formatting/verification. No Codex implementation, user-host modification, production credentials, migration execution or deployment. Merge only after current-Head applicable CI and explicit clean read-only Codex review. Whole-product acceptance and whole-workload speedups are not claimed.
