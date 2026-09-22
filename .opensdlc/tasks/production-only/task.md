# Production-only repository

## Intent and scope

Owner request: maintain QuaZonai as a production system repository; delete docs/, remove executable Demo/Mock paths and development-status disclaimers. Preserve real research behavior, historical data, immutable migrations and licenses.

## Implementation

Delete the standalone in-memory UI backend and its dedicated fixtures/configuration/tests. Use a single real Rust/PostgreSQL/Worker/Caddy browser entry; cover viewport, accessibility and PWA behavior there. Remove the client’s injectable transport; retain strict response parsing as production functions with pure input tests. Remove the native demonstration command, EMA example module, demonstration Arrow writer and their command/documentation references. Keep fixed numerical references under test-only compilation and retain production Arrow history contracts. Keep historical non-real provenance rejection rather than relabeling old data; the current production Release writer already emits REAL only, so the redundant migration from the old mixed worktree is not transplanted. Consolidate necessary installation, recovery and data-preparation instructions into the existing root entrypoints and native Runtime README; remove obsolete historical ledgers.

## Verification and delivery

PR #106 records the six passing CI families for head 486101f3, including the hosted browser/PWA, Rust, PostgreSQL, native Runtime, Polymarket and Markdown checks. The supplementary native cleanup passed 139 job unit/integration cases with no failures or ignored cases, repository-wide Rust formatting, and Clippy for the production job library/binary with warnings denied. Tests use a private disposable temporary directory; no user database or running service is modified. Subsequent-Head CI and independent review are recorded in the PR, not inferred from an earlier commit. Old mixed bug/clean edits remain only in local recovery stashes and are excluded from this branch.
