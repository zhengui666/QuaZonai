# Issue62 implementation evidence

DESIGN.md is the normative contract. This file records implementation and
version-bound evidence, not a second design or a claim that Issue #62 is complete.

## Implemented source boundaries

The first-party tree uses `apps/job`, `apps/server`, `crates/contracts`,
`crates/domain`, `crates/store` and `crates/integrations`, without `qz-` directories.
Legacy backend/frontend/plugin/deployment implementations, compatibility services,
obsolete tests and archived designs have been removed. Git history, LICENSE,
NOTICE, third-party notices and user-data boundaries remain intact.

Scientific foundations reuse Rust-native Nautilus0.63.0, Clarabel0.11.1 and
Arrow56.2.0 on Rust1.98.0. There is no production Python bridge. Native feasibility
is not complete shared-capital research/portfolio acceptance. The earlier native
fixture produced weights 0.7999999999997491 / 0.20000000000025078, 745 iterations,
12 native orders and 24 native events with an Arrow round-trip. It is explicitly
FIXTURE, `deliverable=false`, `python_runtime=false`, not investment evidence.

The typed contracts and pure rules include decimal/bigint boundaries, metric
nullability, UUIDv7, ISO currencies, Codex default-setting omission, budget and
mission-turn limits, qualification, cancellation, version and lease fencing.
Both Rust parsing and generated schemas consume shared decimal/bigint corpora.
These rules do not replace persistent authorization or native component tests.

SQLx0.8.6 owns migrations, transactions and independently migrated test databases;
PostgreSQL/PGMQ owns durable queue delivery. The Store owns project-specific
relationships, immutable identities, budgets and reconciliation decisions. A
Mission has one immutable Session/Thread. Reservation plus PGMQ send is atomic;
one committed dispatch intent grants one send, while retries reconcile. Native
binding, terminal and usage receipt are separate immutable facts. Missing usage
retains reservations; pause, cancellation or lease loss never fabricates a refund.
Exact receipts precede queue acknowledgement.

The relational constraints bind artifacts, inputs, experiment ancestry, Alpha
qualification, frozen candidate contents, evaluations, Release and approval/offer
identities. They also bind machine principals/scopes, forward corrections,
Codex profile revisions, event cursors and run/attempt result manifests. Creating
these records does not itself expose or implement all corresponding services.

The HTTP authentication vertical in `apps/server` reuses Axum0.8.9,
tower-sessions0.14, PostgreSQL PostgresStore0.15, totp-rs5.7, Argon2id,
RustCrypto XChaCha20Poly1305 and cap-std. There is no memory-session fallback.
Clap exposes `init-state`, `migrate`, local one-use `bootstrap`, `serve` and native
OpenAPI. Login uses TOTP only, with a bounded bootstrap capability, native private
cookie, database replay prevention, expiry/epoch/revocation and atomic rate limits.
Device lists paginate and used trusted devices update activity. This vertical
does not provide research, Reviewer or approval authority to an Agent.

## Evaluation publication and revocation correction

The additive `202609060005_evaluation_publication.sql` seals a completed
Evaluation and its metric membership together. A deferred native constraint
trigger publishes the aggregate before commit; consuming references can seal it
earlier in that same transaction. Later metric inserts are rejected. Candidate
and allocation-Evaluation circular creation retains its existing deferred foreign
key and is covered by a positive regression. Upgrade backfills only the internal
publication markers and does not rewrite historical evidence.

Degradation observations must join the exact project, policy mandate, Release
candidate, FORWARD Evaluation and frozen InputSet, backed by the corresponding
Forward evidence window. Historical incompatible rows abort migration rather than
being deleted or silently relabeled. This relationship check does not replace
current policy authorization, freshness, degradation thresholds or Wake admission.

Browser session epochs cannot move backward. Equal epochs allow ordinary state
maintenance; newer epochs invalidate old authority permanently, with native bigint
overflow failure rather than wrapping. The regressions check actual browser/device
authority and a real concurrent row-lock wait, not just the stored integer.

The native Codex probe validates the exact observed `originator/version` product
token against pinned upstream source and publishes that observed value. It rejects
version prefixes, prerelease/build suffixes and a matching string elsewhere in the
user agent. These narrow format regressions do not count as real-account model
inference. The normal native stdio probe remains a separate required CI execution.

The source adds twelve PostgreSQL regression functions and two native-version
unit tests. Their definitions alone are not acceptance evidence: each execution
must identify its exact source/tree, committed lock, command and outcome. Temporary
public development-input exporters are removed after tools are retrieved; no tool
archive, user database, secret or standalone patch publisher is part of the product.

## Historical evidence: exact baseline, not the current Head

At `e8668ca850def834735414ed9ba94fed38d4aa7e`, the suites contained 8 contract,
28 domain and 8 native/report tests: **44 total**.
[CI 33962063262](https://github.com/zhengui666/QuaZonai/actions/runs/33962063262)
validated that historical dependency lock. The upstream feasibility run
33952841460 is a development study, not a final product gate.

The latest baseline inspected for this correction is
`19496333808afc71d794af0871e5ef9704a3507a` with
[CI 34007972993](https://github.com/zhengui666/QuaZonai/actions/runs/34007972993):

| Job | Actual result at that exact baseline |
| --- | --- |
| `database-native` | Success: native PostgreSQL/PGMQ transaction contract |
| `store-postgres` | Success: **61 Store + 7 server = 68 tests**, zero failed/ignored |
| `rust-native-contracts` | Failure at `cargo fmt --all -- --check` on `evidence_bindings.rs` |
| `foundation-checks` | Failure, because not every required job succeeded |

The 61 Store tests in that job were auth 8, authority invariants 7, relational
constraints 7, device activity 3, evidence bindings 12, terminals 4, turn recovery
4 and turn transactions 16. The server suite had 7 tests; its database-backed
cases include an actual loopback HTTP listener using a distinct non-owner login.
The artifact `store-evidence-19496333808afc71d794af0871e5ef9704a3507a`
contains `tests.log`, `tested-commit.txt`, exact migrations, Cargo.lock and native
database/image versions. This count describes that artifact only.

Clippy, Rust tests, native scientific verification and Codex protocol probes were
**not completed by the failed native job at that baseline**. The separate passing
Store job cannot turn those skipped steps into success. Earlier unqualified
52/87/97-test snapshots are retired as current evidence: never combine different
commands, development locks or commits into a latest-Head pass count.

## Review correction in this source

`verify_runtime_role` inspects native PostgreSQL role membership, ACLs and ownership
across the entire `app` schema rather than sampling `operator_auth_state`. It
rejects destructive table privileges, schema CREATE, app object ownership and
elevated roles reachable by inheritance or SET ROLE, including an elevated
`session_user` hidden behind a restricted `current_user`. Ordinary non-owner DML
access remains supported. This is a startup guard, not a substitute for continuing
least-privilege administration or a complete machine-authorization service.

Before a new reservation or first dispatch, the Store holds the referenced Brief
lock and requires its frozen budget to equal the Cycle snapshot. The complete JSON
comparison includes tokens, costs/currency, turns, repairs and resource limits.
Existing receipt/terminal reads and actual-usage reconciliation remain separate
from permission to start new spending. Historical malformed snapshots cannot
expand new work or justify deleting actual consumption.

The additive `202609060004_doctor_boundary.sql` migration confines DOCTOR_READ to
a read-only CLI/AUTOMATION credential, never DOWNSTREAM/MISSION or a mixed research/
delivery scope. Existing issuer, epoch, project, Mission-lifetime and lock checks
remain. Previously issued invalid Doctor credentials require an explicit effective
revocation before migration can succeed; their immutable issuance stays in audit.
The upgrade regression uses native SQLx to apply the old migrations, verifies the
specific failing migration/SQLSTATE, then revokes and reapplies without rewriting
historical rows or changing applied migration checksums.

Terminal retries first verify the exact reservation/attempt binding, then compare
all immutable terminal fields before requiring a live fence. They recheck after
lock waits. Identical committed facts remain readable after lease expiry/takeover;
conflicting outcome, native identity, reason or observation time is a conflict.
Creating a missing terminal still requires current owner/epoch/lease. A terminal
never creates a usage receipt, releases a reservation or acknowledges the queue.

The correction adds 12 PostgreSQL regression test functions: 4 runtime-role,
2 budget-authority, 2 Doctor boundary/upgrade and 4 terminal retry/race tests.
The source also formats the existing evidence-binding suite without weakening CI.
**A test definition or successful `--no-run` compilation is not a passed database
test.** The modified source must obtain its own published-Head CI logs and review;
the historical 68-test result above must not be reused for it.

## Native upgrade cutover and control-plane vertical

The current source adds `Store::migrate` with the native SQLx migration lock on a
closed-on-exit dedicated connection and a transaction covering ordered PostgreSQL
application-table write barriers plus all pending native migrations. The existing
migration files retain their original bytes/checksums. Additive migration 0006
repairs a possible pre-guard cutover gap and invalidates all historical browser
epochs once, with an immutable audit receipt; it does not delete historical facts.
Upgrade regressions use real prior-schema writers, lock waits, failed batches,
connection cancellation and a bad observation committed while migration waits.

`contracts::control`, `domain::control`, `store::authority/commands/control` and
`server::access/control` implement a real Project/identity vertical. Native
Argon2id verifies bounded opaque Bearer capabilities from encrypted SecretVault
objects. Cookie and Bearer channels cannot be mixed or used as fallbacks. The
locked business transaction rechecks exact credential epoch, expiry, revocation,
project/Mission and scopes. Recent human authority, one-time CLI grants with full
request snapshots, CAS and immutable original response receipts are enforced in
the same transaction. No token/verifier enters public DTOs or receipts; issuance
retries never return the raw secret again.

The source tests cover real HTTP/private-cookie/TOTP/Argon2/SecretVault/database
requests, cross-project reads, forbidden automation management, grant substitution
and parent-resource binding, actual revocation lock waits, concurrent same-key
commands, stale CAS and rollback. These are bounded implementation proofs, not a
claim that the still-missing research/CLI/MCP/UI/worker/production acceptance is
complete. Current totals and outcomes must be taken from the exact tested commit's
CI log; historical totals above remain explicitly historical.

## Control review corrections: replay, verifier ownership and native rate limits

The grant HTTP path checks the authenticated CLI's exact existing receipt before
fresh TOTP/reauth quota, while still enforcing current authority and global epoch.
Credential issuance now holds the existing command transaction before materializing
a verifier. Reconciliation locks that same authority row and consults immutable
credential history on the primary before deleting only an authenticated, unpublished
MACHINE_VERIFIER object. A local maintenance command covers cancellation/crash orphans;
unknown database outcomes preserve files. No secret enters receipts or metadata.

PostgreSQL shared global/per-credential windows bound failed/in-flight native Argon2
checks. Successful verification refunds only its original windows, with independent
machine and human crypto slots. Native HTTP, SQL concurrency/rollback, filesystem
purpose/symlink and bounded-window tests exercise these paths. Counts and results
belong to the exact CI Head, not an unversioned claim in this document.

The database-native CI waits for final TCP readiness, not the official image's
initialization-only Unix socket server. The PGMQ contract itself remains unchanged.

## Verification commands and evidence rules

Run with the committed lock; do not format or regenerate tracked code inside a
read-only acceptance job. Generated outputs must compare equal to committed files.
Use an isolated native PostgreSQL/PGMQ database, never an existing user database.
The CI workflow pins and verifies the exact pull-request Head before execution.

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --workspace --all-targets
cargo test --locked --workspace --exclude store --exclude server
DATABASE_URL=postgres://TEST_USER@127.0.0.1:55432/postgres \
  cargo test --locked -p store -p server -- --test-threads=4
cargo run --locked -q -p contracts --example generate > /tmp/domain-v1.openapi.json
diff -u contracts/generated/domain-v1.openapi.json /tmp/domain-v1.openapi.json
cargo run --locked -q -p server -- openapi > /tmp/api-v2.openapi.json
diff -u contracts/generated/api-v2.openapi.json /tmp/api-v2.openapi.json
node tests/contracts/decimal-wire.mjs
node tests/contracts/bigint-wire.mjs
```

The workflow additionally executes the native solver/Nautilus/Arrow report,
locked official Codex stdio/schema probe, PostgreSQL/PGMQ transactions and legacy-
path rejection. The aggregate requires every foundation job. Record the actual
commit, command, zero/nonzero exit, passed/failed/ignored counts and artifact/run;
compile-only, missing credentials, skipped steps or cancelled runs are not passes.
No test fixture or source archive is evidence of a real subscribed model request.

Store restart tests recreate a client, not an OS/container crash. Real lock waits,
lease clock checks, missing-queue rollback and independent test databases cover
specific relational/concurrency contracts; they do not prove deployed isolation,
production TLS, backup restoration or end-to-end research. Crypto is native, but
a passing authentication test does not establish all role-specific research rights.

## Explicit gaps and completion boundary

Complete Run admission/takeover, research services and machine authorization,
remaining API/Worker/CLI/MCP, same-Thread model/tool/job/evidence integration,
independent Reviewer, PIT/sealed isolation, multi-Alpha shared-capital portfolio,
target-only approval/feedback/wake services, Ant Design UI/PWA, user-data migration,
backup/restore, protected real-account acceptance and production deployment remain
incomplete. JSON container/version checks are not complete policy validation.
Removing legacy tests is not acceptance, and foundation CI is not full W0–W8/T01–T42.

Keep PR #63 Draft until the entire Issue #62 contract is implemented and evidenced.
Completion requires the PR, all applicable CI on its latest Head passing, all
review findings resolved and an explicit no-findings Codex review of that Head;
only then merge. Verify main and required migration/isolation/recovery/end-to-end
evidence before closing #62. **On GitHub, Codex is review-only: never ask it to fix,
implement, commit or autonomously handle problems.**
