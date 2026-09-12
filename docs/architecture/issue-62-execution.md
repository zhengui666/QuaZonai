# Issue62 implementation evidence

DESIGN.md is the normative contract. This file records implementation and
version-bound evidence, not a second design or a claim that Issue #62 is complete.

## Original Sealed opportunity reservation, 2026-09-12

Working source over `814409168b19563d5653217a43b8f481d07435dc` adds immutable
Sealed task associations and original Attempt-to-exposure records (migration 041).
The existing first native JobSpec transaction locks the root lineage and reserves
an opportunity before returning any capability. It requires the canonical formal
Validation projection, original model/calibration, exact policy/dataset and
inherited data origin. Prior disclosure blocks independent access. Root-wide
usage is not refunded after cancellation; replay uses the original reservation.
After a root-lock wait, capability/deadline and Worker lease are checked again;
failure rolls back both the new JobSpec and opportunity.

`verify-sx1Gdc` passed check/format/strict Clippy, 29 native validation tests,
126 Store tests and 20 HTTP/CLI tests, zero ignored, source unchanged and owned
PostgreSQL/PGMQ stopped. Three new real PostgreSQL tests cover concurrent
same-project max-one reservations, original Attempt replay/cancel-no-refund,
prior Operator summary disclosure and actual lease expiry while a root lock is
held. The tests use controlled scientific outputs and trusted test administration
that pays an ordinary trial; they do not prove production non-trial Sealed
admission, cross-project concurrency or scientific qualification. The first
passing run `verify-5JImI0` preceded reuse of the canonical source projection;
the final run above verifies that change. No protocol or native image changed.

Trusted Sealed task creation, formal publication and independent Reviewer flow
remain unfinished. This is not full Issue #62 acceptance or GitHub CI evidence.

## Managed held-out operation and real OCI, 2026-09-12

Working source over `941269e50ab448ed7ea86d52e79369ca1da89e1c` adds the internal
EVALUATE_SEALED_ALPHA operation and qz.alpha_sealed output. Its original JobSpec
binds a Sealed dataset, Wasm MODEL, optional original calibration MODEL and
PARAMETERS. SCORE requires that calibration; EXPECTED_RETURN forbids it.
The Job and Store adoption read the original model, not a second copy of fit
configuration inside PARAMETERS. Existing readers now allow the second immutable
object read. Generic output shape and exact input association remain distinct.
Runtime selection checks include this fifth data operation; the image marker
now requires alpha-sealed/1. No public researcher admission was added.

`verify-WCmzXU` passed check/format/strict Clippy, 150 contracts/domain/runtime,
8 managed, 30 native Codex and all 83 Job tests, zero ignored/source unchanged.
The earlier response-contract expected list omitted the new variant; it was
corrected and this suite rerun. `web-verify-mDEjVM` passed all six native double
generations with byte equality and handwriting unchanged, typecheck,
504 Vitest + 5 Node, wire/build/CLI checks, 36 Codex browser tests (33.4s) and
186 full browser tests (2.5m). Only Domain/Runtime schema bytes changed.

`owner-oci-LSjLSA` passed all 8 actual Docker tests (13.51s), zero ignored, using
image `sha256:409d22803b5eae314d9e8cda0657dfa58814a9090ebac5963329fb624057ec57`.
The new test uses actual Parquet/Wasm/frozen native OLS and registered FIXTURE
metadata, uploads exact objects through Runtime HTTP, executes the image,
downloads and binds its report, and compares it with the original native result.
Its first attempt failed before container execution because native catalog
loading requires multi-thread Tokio; only the test runtime flavor was fixed.
The final source was also checked by `verify-lX8JmJ`: check/format/strict Clippy,
29 native validation, 123 Store and 20 HTTP/CLI tests, zero ignored, source
unchanged and owned PostgreSQL/PGMQ stopped.

The Runtime test reuses existing Job fixture code and already pinned scientific
packages as dev-dependencies; Cargo.lock adds only those dependency edges.
This is synthetic OCI computation evidence, not trusted Sealed admission,
opportunity reservation, formal evaluation/qualification or full T42 delivery.

## Held-out metric projection and freeze compatibility, 2026-09-12

Working source over `e309821299ed7f33c9b2ce1f5ac6dd539cb6aca0` projects the original
Sealed report into existing MetricValue records and native capabilities. Asset
scopes remain `asset:N`; native values, missing reasons, paired counts, methods,
original artifact references and covered time intervals are retained, without
averaging Validation folds. Freeze/start checks require explicit supported Sealed
requirements and matching registered asset/bar order. Metadata reads declare
their allowed partition; ordinary DATA_VALIDATE still rejects Sealed before I/O.

`verify-X51wgQ` passed check/format/strict Clippy, 150 contracts/domain/runtime,
7 managed, 30 native Codex, 91 Store and 41 HTTP/Worker tests, zero ignored.
Source remained unchanged and the owned PostgreSQL/PGMQ cluster stopped.
The final direct Sealed suite passed all 5 actual Parquet/Wasm/CLI tests, including
numeric gate projection and absent-label INCONCLUSIVE. Earlier runs caught a
missing selected SQL column and a test Clippy violation; both were fixed before
this successful rerun. No protocol DTO changed, and no web/OCI rerun is claimed
for this slice. Managed Sealed admission, exposure, publication and qualification
remain unfinished; this is not full Issue #62 or current-head GitHub CI evidence.

## Native held-out computation primitive, 2026-09-12

Working source over `de5a23140146b1268a159d5f5cec073b080f3004` adds the restricted
`job evaluate-sealed-alpha` computation, not a managed Runtime operation or an
access grant. It calls the existing native catalog/causal EMA/Wasm forecast,
applies persisted OLS coefficients without fitting, and reuses the existing
ndarray-stats metrics. Original scores and per-row normalized returns coexist;
warmup and incomplete-label tails remain explicit. Research availability, exact
calibration asset order/bar/horizon, original calibration report/time, finite
values and complete result associations are checked. No-label metrics have
`INSUFFICIENT_DATA/NO_COMPLETE_LABELS`, not zero. No new dependency was added.

`verify-n1FyGw` passed check, format and strict Clippy; 150 contracts/domain/runtime,
7 managed, 30 native Codex and all 81 Job scientific tests, with zero ignored.
The four new Sealed tests use actual Parquet/Wasm, persisted native fits and the
real CLI. They check original-score preservation, independent RMSE reference,
stable past predictions when future data extends, time/asset/horizon and missing
fit rejection, result tampering, absent-label states and safe CLI failure.
All data are synthetic; no qualification or full Mission-to-market proof follows.

`web-verify-9EqbPA` passed all six native double generations with byte equality
and handwritten files unchanged, typecheck, 504 Vitest + 5 Node tests, decimal/
bigint/fraction wire checks, build/CLI help, 36 Codex browser tests (34.1s) and
186 full browser tests (2.6m). Only Domain schema bytes changed; API/Runtime and
generated frontend bytes are unchanged. Chromium viewports are not Safari/device
acceptance. The external OCI verifier now inventories untracked source contents
as well as tracked files, so new source is covered by the frozen-source check.

`owner-oci-WopDT1` built native image
`sha256:2a4745de5908e81086702739623af959d08e56aab9fd155fc5ce1673ae83b0f8`
and passed all 7 real OCI regression tests (11.26s), zero ignored, source unchanged.
These cover existing compile/identity/cancellation/restart/kernel boundaries, not
Sealed market execution. Exposure reservation, managed Sealed producer adoption,
formal evaluation, independent Reviewer and qualification remain unfinished.
No GitHub push, review request, merge or Issue closure is claimed.

## Separate frozen Sealed policy intent, 2026-09-12

New policy creation requires explicit `sealed_metric_requirements`, independently
validated and persisted alongside Validation requirements. Selection binds its
own evaluation kind. Migration040 leaves historical policies unchanged with a
NULL new column; no threshold copying, retrospective authorship or Sealed
qualification is inferred. This increment does not implement Sealed execution,
exposure reservation, independent Reviewer admission or qualification.

On working source over `2c47c7ede4a507b950ad918ad21e300216ad2904`, full native run
`verify-yMEX97` passed compilation, formatting, strict Clippy, 150 contracts/domain/
runtime tests, 7 managed tests and 30 native Codex tests. Store/Server completed
526 passed and 2 failed, none ignored: the old migration comparison omitted the
new NULL column, and a publication test manufactured a calibration without its
now-required native producer. This full run is **not** a green full-suite claim.
Only those test assumptions were corrected: explicitly assert historical NULL,
and use a valid DEMO Release consumer for the generic same-transaction seal test.
No production constraint was weakened. Targeted `verify-0tmoOd` then passed the
same compile/format/Clippy and 150/7/30 checks, plus 35 Store policy/publication/
upgrade tests and 6 policy HTTP tests. Both runs confirmed unchanged source and
stopped their own temporary PostgreSQL/PGMQ clusters.

`web-verify-phIEnJ` passed byte-identical double native generation of all six
outputs with handwritten files unchanged, typecheck, 504 Vitest and 5 Node tests,
decimal/bigint/fraction wire checks, production build and CLI help. Browser checks
passed 36 Codex settings tests (37.7s) and 186 full-site tests (2.8m), covering three
Chromium viewports, not Safari or physical devices. Semantic changes are confined
to policy DTOs and their response wrappers; routes and Runtime contract are
unchanged. No GitHub publication/review/merge or full Issue62 acceptance is claimed.

## Runtime admission and Mission owner-fence checkpoint, 2026-09-10

The current increment adds typed integration settings, write-only encrypted
credentials, revision-bound native TCP/TLS Runtime observations, formal Brief
execution-context freeze and atomic Cycle/initial-Run/PGMQ startup. Registration
is not a successful connection; queue admission is not proof of Worker execution.
The frontend uses native Rust-generated contracts, shared Ajv validators and
Rollup splitting without increasing the Workbox 2 MiB per-file precache limit.

Migration `202609100023_mission_owner_fence.sql` permanently binds each new
Mission credential to both the active Attempt and its current owner epoch.
Common machine authority checks that binding and the live database lease on reads
as well as writes. Historical Mission issuances with an unknown owner remain
unchanged audit records and require new issuance; valid CLI credentials survive
that Mission-only cutover. Existing applied migration files are not rewritten.

The regressions in `apps/server/tests/support/mission_attempt.rs` use native
claim/renew and historical SQLx upgrades. `mission_owner_wait.rs` additionally
proves actual PostgreSQL lock blocking before letting the native lease expire,
then requires HTTP 401 or issuance SQLSTATE 23514 after the lock is released.
It also rejects Attempt/owner injection into non-Mission credentials. These
fixtures do not constitute the full production Worker, Codex or T42 workflow.

Local `.ai-bridge/runtime-check-GZmgAU` recorded successful targeted HTTP/Store,
native TCP/TLS and strict Clippy checks with unchanged source. Formatting failed;
its proposed formatting was subsequently applied by the web author. That run is
not full-stack or publication approval. The final checkpoint requires a fresh
stable full Rust/Web/real-browser run and independent generated-output compares.
Private raw logs, test databases and test credentials must not be committed.
Latest committed-Head CI and an explicit clean Codex review remain separate gates;
PR #63 stays Draft until all Issue #62 work packages and acceptance are complete.

## Local review-patch verification, 2026-09-07

This is local evidence for the web-authored working-tree patch over
`4ce0fcbbaf530ba8a02124fa9ff84becf67bb156`, not a new GitHub CI result or complete
Issue62 acceptance. The execution adapter requested `gpt-5.6-luna`; the web author
wrote the source and scripts. Only native generated build artifacts were published
by the explicitly approved build step; the executor did not author source fixes.

- Rust **1.98.1** was installed in an isolated user-owned development prefix and
  explicitly selected for all current checks. Format check, contracts/domain/store/
  server compilation and Clippy with `--all-targets -- -D warnings` passed.
- Contracts and domain suites passed **76 tests** with no failed/ignored tests.
  Both real HTTP OpenAPI reference tests passed. Both native generators ran;
  their reviewed outputs were regenerated byte-identically and published to
  `contracts/generated`. All three Node wire-corpus commands passed.
- Docker access was unavailable in the local executor. Rather than weaken socket
  permissions or touch existing services, the executor built official PostgreSQL
  **18.6** and exact upstream PGMQ **1.10.0** in the user-owned development prefix,
  then initialized an independent disposable cluster. The official PostgreSQL
  archive checksum and PGMQ Git release commit were checked. This is native
  PostgreSQL evidence, not evidence for the separately pinned OCI image.
- The focused atomic-Cycle/Run/role suites passed **45 tests**. The subsequent
  complete `cargo test --locked -p store -p server -- --test-threads=4` passed
  **196 Store + 40 server = 236 tests**, zero failed/ignored, including all eight
  transaction-composition regressions. The temporary server was stopped and the
  tracked-source diff was unchanged. Database execution ended at
  **2026-09-07T18:11:26Z**.

Local raw logs are `.ai-bridge/verify5`, `.ai-bridge/native-contract-output`,
`.ai-bridge/contract-publication` and `.ai-bridge/native-db-1`; these private build
logs, disposable database data and test credentials must not be committed. CI
must independently reproduce the committed Head and retain public artifacts.
Formal Brief freeze/Cycle-start services, independent scientific publication,
Agent/portfolio/delivery/UI and the remaining W0–W8/T01–T42 are still required;
these local results do not authorize merging the incomplete integration PR.

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

## Native service schema hardening (2026-09-06)

Based on remote `0f71046d4bb69b8d37b76429000e2d33daea57be`, the runtime
privilege query now covers app, tower_sessions and pgmq; missing service schemas,
object/schema ownership, CREATE, TRUNCATE, TRIGGER and reachable delegated
privileges fail closed. Ordinary native session and queue DML remains allowed.
The session catalog contract also verifies persistence, RLS, constraints, triggers,
rules, inheritance, column semantics and native index operator classes.

Three new PostgreSQL runtime-role tests and one real CLI migration test exercise
these boundaries. The migration test tries fourteen incompatible definitions and
checks that the epoch, exact migration history and existing session bytes stay
unchanged after each failed command; an ordinary expiry index and native session
CRUD remain positive controls. The two representative new tests were also run
against the unchanged baseline SQL and reproduced the original missing rejection,
not a connection/setup failure. No existing migration or Cargo.lock was edited.

The full locked local workspace, strict Clippy, formatting, build and native
contract/scalar comparisons passed for this candidate. Exact test totals and the
candidate tree are recorded in the PR evidence comment; this paragraph is not a
permanent claim that every future Head passed. Remote CI and independent review
must validate the published Head separately, and none of these checks replaces
complete product acceptance.

## Administrative delegation and exact evidence producers (2026-09-06)

This iteration starts from remote `6cada81a6f94bfa29831cf4ede220c8d3ca4e711`.
Native PostgreSQL membership traversal includes ADMIN OPTION even when INHERIT
and SET are false, follows delegated paths, and retains the original session
identity. It also rejects reachable native server-file/program roles. Four new
PostgreSQL tests include a real self-regrant counterexample, multi-hop and masked
session cases, and harmless-membership positive controls; no OS command is run.

Downstream principals require a project when created. The domain and Store
regressions prove rejection without a stranded principal or command receipt.
The new additive `202609060010_evidence_producers.sql` binds evaluation reports
and metric sources to the exact project/run and expected artifact roles.
Approval context must be frozen, belong to the release project, and contain its
exact evaluation reports. Four PostgreSQL tests cover both valid source layouts,
wrong projects/runs/kinds, draft or unrelated approval contexts, and rollback of
an upgrade containing incompatible historical evidence. Prior migrations, the
product Cargo.lock, original test assertions and historical evidence are retained.

Local validation of this code snapshot used Rust 1.98.0, the committed Cargo.lock,
and isolated native PostgreSQL 18.1 with PGMQ 1.10.0. The complete workspace test
command passed 206 tests, zero failures and zero ignored; strict Clippy, rustfmt,
the all-target build, generated API/domain diffs, and the 204 decimal plus 242
bigint shared Node cases passed. Exact source-tree identity and published-Head
CI are recorded in the PR, not retroactively assigned to these local logs.
These source/authorization regressions do not establish the still-missing
production research, runtime-isolation, portfolio or protected-account acceptance.

## Immutable research preparation (2026-09-06)

This implementation starts from remote `a41e8709576d344919ffbe121b2619932793fe0d`
and adds typed InputSet and EvaluationPolicy HTTP/Store commands, with DESIGN A4.3
written before code. Public input metadata never contains native storage references
or sealed bytes. Metadata registration is neither algorithm execution nor capability,
PIT, PASS, qualification or deliverability proof. Complete Brief/worker/native runtime
admission remains separate work, including rechecking current data authority.

The existing OperatorCommand/receipt path authorizes and atomically publishes a
frozen input aggregate or a policy/family bound to the project's immutable lineage.
Policy versions are allocated under the project lock. Dataset role/cutoff, source
and runtime availability, current immutable grant/revocation and exact artifact
ownership are rechecked with native row locks. Revocation inserts lock the same
grant; a separate read after lock acquisition uses a fresh READ COMMITTED snapshot.
Only exact scoped machine reads are enabled; no new MachineScope or SQL backdoor.

Twenty-two new tests cover three native wire/schema cases, two pure contract cases,
twelve real PostgreSQL cases and five real Axum/native-authentication cases. The
latter use actual Cookie/Bearer, TOTP, Argon2 and SecretVault with disposable migrated
databases; the new HTTP suite uses the actual router, not a mocked API. Real lock
waits, opposite-order revocation/consumption, concurrent idempotency, versions,
sealed/WF identity rules, complete CLI-grant request binding, safe field diagnostics,
large request limits and full rollback of failed receipts are exercised.

These tests exposed an existing zero-argument `guard_revision` defect: PostgreSQL
provides NULL TG_ARGV, and FOREACH failed with SQLSTATE 22004 for otherwise legitimate
Runtime/Downstream updates. The additive 0012 migration coalesces only that native
argument array; the regression executes the unchanged original function to reproduce
22004, then verifies updates, versioning and immutable-identity/source rejection
under the replacement. Applied migrations and Cargo.lock remain untouched.

Validation results, exact source tree and the independent published-Head CI/review
are recorded in the PR, not inferred from test definitions or a historical Head.
Parallel Run/SSE/CLI and Ant Design implementation is preserved and is not claimed
as part of this preparation slice.

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

## Run/Attempt 与持久 SSE 实现增量

新增 `crates/store/src/lifecycle.rs`、`crates/contracts/src/lifecycle.rs` 和
`apps/server/src/runs.rs`。复用 SQLx/PostgreSQL/PGMQ/Axum/Tokio，提供受信任域服务
准入、单 Attempt 接管/发送意图、取消/终态回执/归档，以及带认证作用域的 Run 查询
和 SSE；不是开放任意命令执行，也未把远端结果当成科学 qualification。

验证入口：

```sh
cargo test --locked -p store --test run_lifecycle
cargo test --locked -p server --test runs_http
cargo test --locked -p contracts --test lifecycle_wire
```

数据库套件验证同键并发、预算超发、PGMQ/事件故障整体回滚、lease/epoch、丢 ACK、
取消/完成竞态、锁等待后过期、回执关联与真实失败代码。HTTP套件以真实 TCP listener、
Axum middleware、原生加密/会话及 PostgreSQL 检查 cookie/Bearer、跨项目拒绝、
SSE 多批终态重放/断点续读/权限撤销/连接上限和断线不取消。共享 fixture 仅提供
领域记录，不模拟 HTTP 认证或将 FIXTURE 变为 REAL。

首次回归复现原有零参数 revision trigger 错误；新增迁移修复空 TG_ARGV。
其余已应用迁移原样保留，两个新增领域表的期望计数及生成合同同步更新。CI仍须在
推送后的精确 Head 上运行；测试定义、本地编译或本节文档均不能冒充远端 CI/Review
通过。没有在本节写入随后可能失效的当前测试总数或 release-ready 声明。

## 2026-09-06：交付边界与原生仓位审查增量

基于已入 PR 的 `0584a0c34ee931bdf7d31f64ee23c6e017ef17ed`，本轮新增
`202609060013_delivery_boundary.sql`，原有 SQLx 迁移与 Cargo.lock 不变。
覆盖 Package 精确项目/角色/版本、REAL 来源元数据、DEMO 不可审批或发出 Offer、
Research lineage 无环、Claim 行锁后 DB 实时过期检查、Forward 已转移状态与拒绝竞态。
升级检测到非法历史时原记录及全部迁移历史保持不变，不改写、删除或重新标注证据。

新增9个真实 PostgreSQL测试和1个真实Nautilus引擎测试。修复后的全workspace执行
得到238项通过，0失败、0忽略；严格Clippy与all-target构建通过。Nautilus Rust0.63.0
实际输出745 iterations、12 orders、24 events、**12 positions**；报告包含原生
position count并拒绝零仓位。它仍明确是 `FIXTURE`、`deliverable=false`、无Python。

旧审批/反馈关系测试的正向对照现在创建独立 PACKAGE 元数据和显式 REAL 关系样例，
DEMO helper始终保留FIXTURE且不能审批，反馈正例必须先真实执行Claim状态转换。
这些仅是隔离测试库里的身份/约束样例，不包含实际市场数据或声称科学资格，也没有
生产测试开关、重标现有fixture、下游真实交易或绕过产品Gate的代码。

此处结果不替代新增Head的GitHub CI和独立Review；完整产品仍按DESIGN验收。

## 2026-09-06：研究用途、Policy/Family 与字段诊断审查修复

基线为 PR Head `6191ceadb79ab2db03c2af1d0f214687200ef2d3`。
锁住的 DataUseGrant 事实现在包含封闭 allowed_uses；RESEARCH 不能登记
PORTFOLIO/FORWARD 用途，较高授权仅允许对应准备，不替代后续 Live 发布检查。
Policy 与 Family 通过原生生成的 UUIDv7 关系列、两个精确复合延迟外键一一绑定，
同事务可按任一顺序创建；不存在/错项目/错根/额外 Family 均不能永久提交。
新增 015 迁移审计历史而不改写它，保留 014 的 Brief 作者工作包序号。

指标代码、scope、方法列表及各项字符串的边界由原生 utoipa 发布到两份生成合同，
不手写第二套 schema。精确 Decimal 比较器保持不变，阈值错误现在指向真正的
缺失/多余端点，BETWEEN 倒序同时标记两端，不向客户端回显数值。

修复前对原 6191 源码实跑：6 项真实 PostgreSQL 回归、1 项原生 schema 回归、
1 项 Domain 诊断回归均复现失败。修复后新增总计 13 项回归，包括 10 项真实 PG、
1 项真实 HTTP、1 项生成 schema、1 项比较器诊断；还覆盖合法历史升级、坏历史
整体回滚、生成列不可覆盖和用途矩阵。原并发测试的 pg_stat_activity 精确查询
前缀同步新增 allowed_uses 字段，仍须观察真实 PostgreSQL 锁等待，未删除断言。

当前候选源使用原样 Cargo.lock、Rust 1.98.0、真实 PostgreSQL 18.1/PGMQ 1.10.0
执行全部 273 项 workspace/all-target 测试：0 失败、0 忽略。严格 Clippy、fmt、
全目标构建和两份原生生成合同已核对；204 Decimal、242 Bigint 共享语料通过。
本地结果不代表新提交 CI 或独立审查已通过；这些仍必须针对实际推送 Head 完成。
本增量不改变完整 W0–W8/T01–T42 验收、Draft 状态或 Codex review-only 边界。


## 2026-09-06：Run 恢复、当前许可与 Forward 来源审查修复

本地基线是远端 a52fa0fd88f44aeeee59a6d7f1bccfadf7b11fe0 的原生 tree
469e918b0eacaf193416e8d2cf133c38e1e57bdd；原样 Cargo.lock/Rust1.98。
新增016/017迁移，001–015原字节保留，014仍为Brief作者流程的预留序号。

修复前新增回归实跑复现六项故障：10ms请求超时杀掉25ms迁移DDL、过期NOT_SENT
仍被续租、终态Attempt仍可改、成功Attempt含错误码、未知兼容事件拒绝、旧浏览器
可取消。另用未安装017的真实数据库复现了跨项目Forward报告被接受。负向结果是
旧缺陷证明，不作为通过结果。原有终态重传测试改为真正等待活动期设置的短租约
过期，不再通过修改已终结Attempt安排测试；PGMQ批量读取的测试一次保留两个消息，
不在其visibility窗口内错误地二次读取。并发测试仍要求观察原生锁等待。

实现复用SQLx原生事务/锁、PGMQ、BigDecimal、utoipa和Axum SSE：新消费重新授权，
确切回执与未知结果对账不受事后撤销抹账；无Cycle只允许有界管理任务；未发送过期
保存NOT_DISPATCHED事实而非伪造远端失败；已终结Run的任何Attempt写入均拒绝。
部署连接独立取消statement_timeout而不污染请求池。政策fraction与capabilities
由原生schema生成并用214条共享精确语料检验，不重建Decimal实现。

Forward保存一次性领取元组和显式legacy来源，拒绝领取前的历史消息、凭拒绝状态
伪造领取，以及不符合精确项目/角色/schema/origin/access的报告。合法既有反馈在
随后拒绝时保留；坏历史令升级整体回滚。测试中的REAL元数据仅为可丢弃库内的
关系正例，没有市场数据/报告字节，不宣称产品验收。

验证入口：全workspace/all-target locked测试、严格Clippy、all-target构建，
contracts/example generate与server openapi原生生成后比对，两份schema的204
Decimal、242 Bigint、214 Fraction共享语料；真实PostgreSQL18.1/PGMQ1.10与HTTP。
实际通过结果绑定发布Head的CI/PR证据，不在本节冒充远端Review已通过。
完整W0–W8/T01–T42仍须继续交付，Codex仅独立review。


公有开发依赖/工具已取回到隔离开发目录，删除完成的client-development-inputs、
prepare-web-dependencies、web-development-inputs工作流；它们不是验收，不保留
随每次PR提交重新取依赖的永久开发任务。正式CI仍只读、使用已提交锁文件。

## 2026-09-06：Brief 草稿作者流程与 SSE payload 合同

本地基线为远端205bc4c0fd16b593263dd744895826a7ee5456cf的完整tree
5456cc177ceb45d0b59e1e46fed89cb3b22dd2f6。该基线独立CI34049936238成功，
但其Codex审查指出payload生成schema缺少对象/版本约束，不能将Completed当无问题。

新增真实Brief create/read/list/PATCH：完整非秘密规范意图绑定路径project_id与
schema_version，既有Operator单次CLI grant、幂等原始响应、项目锁/CAS和绑定
替换同事务。014使用预留迁移号，不改任何已提交迁移或Cargo.lock。父Brief锁将
DRAFT编辑与FROZEN成员封口串行化。运行身份只新增brief_data_bindings一张表的
DELETE权限，旧不可变历史守卫与其他表无DELETE/TRUNCATE/TRIGGER边界保持。
保存草稿不证明当前许可、PIT或原生能力，不提供假成功的freeze入口。

205的payload审查通过原生utoipa发布JSON object、必需schema_version=1及可扩展
公开属性修复；已有事件类型运行时仍严格校验，未知兼容事件不改状态投影。

新增18项Brief测试（9真实PostgreSQL、4真实HTTP/私有Cookie/TOTP/Argon2、3领域、
2线协议）和1项SSE schema测试。非owner真实部署身份可创建/替换草稿，不能删除
其他表或改冻结成员；原生PG锁等待覆盖编辑/冻结，故障注入证明绑定与CAS回滚。
旧浏览器测试通过原生SessionStore关联合法历史BrowserLogin，不倒退认证时间，
不关闭触发器或改变生产鉴权。

本地对本增量完整源码运行Rust1.98与原样Cargo.lock：313项workspace/all-target
测试通过，0失败、0忽略；fmt、严格Clippy、all-target build通过。两份OpenAPI
由实际Rust生成。发布必须再跑对应新Head独立只读CI和review；本地结果不等于
完整W0–W8/T01–T42、原生账号验收或可合并状态，Codex仅review。

## 2026-09-09：可执行原生预测、组合求解与共享资金模拟

在 `ace5c6502dbf1988a4891aa572da6ddbe735842b` 上的网页作者工作树，执行器实际运行
`cargo test --locked -p job --tests`（62通过）、`cargo test --locked -p contracts -p domain`
（25+55通过）、workspace/all-targets Clippy 和格式检查，均为exit 0。原生任务覆盖
目录Parquet读写、严格资产/时间/步长检查、受fuel限制的Wasm预测、冻结标签、Clarabel
约束、原生Walk-forward/CPCV/协方差/OLS、真实CLI和独立并发模拟进程。

该回执绑定计划 `3ab3630ca819cd57b9f65627da5be7615ce61ba2e8cdaab833e785ee0b57e10b`，
结束于2026-09-09T09:15:24.624Z。原始命令输出位于本地忽略目录
`.ai-bridge/owner-science-9/`；本记录不是远端CI成功声明，也不包含本轮Store/Server
数据库回归。之后网页作者按同次原生输出补全domain OpenAPI；其一致性需再次执行比对，
HTTP OpenAPI在该次原生生成中无差异。不要把handoff进程exit 0替代每个命令的结果。

发现并修复的实际问题：Wasmi默认分派在未优化无限循环回归中溢出宿主栈，使用上游
portable-dispatch后保留预算和回归；Nautilus logger使用具体LoggerConfig；原生收益的
position fallback不能当组合资本收益，适配改为唯一账户原生equity snapshots的日收益；
跨日全现金0收益与日内样本不足分开。同进程内核复用导致相互干扰的测试改为实际生产
一任务一进程边界，并用四个同时运行的原生job保留账户、权益及0收益断言，不加串行锁。

这些计算入口仍不拥有HTTP/MCP调用方的任意路径、REAL来源、PIT、资格或交付授权。
正式数据登记、Brief冻结/Cycle启动、Worker/Codex、独立评估发布、两Alpha组合和
交付/Forward/Wake以及完整UI验收仍按DESIGN完成；本增量不得被当作整个Issue62关闭依据。

## 2026-09-11：原生账号及 Mission 接线

本机接管后的账号/配置增量 `d849dc35eb056393600d1f13ecdcb37701c39810` 已通过
原生账户策略/注销、PostgreSQL/HTTP、10项CLI传输、503项Web单元、5项PWA文件与
150项三视口浏览器检查。原生发行版不支持debug-only登录issuer覆盖；没有将模拟
OAuth成功当成真实账号验收，没有读取生产profile或完成受保护账号推理。

后续Mission适配使用同一官方0.144.4：native tool_search → 原生MCP namespace调用
→ 实际stdio服务 → HTTP/认证/PostgreSQL → 原生shell工作区写入 → 进程退出/重启
→ 原Thread恢复。一个Turn中4次受控模型请求累计48个原生报告token，下一Turn
再增加12；不是只取最后一次请求。随机工作区外文件与服务环境canary未进入模型请求。
个人全局提示文件/配置覆盖在Thread发送前拒绝，原生数据和用户文件不被复制或修改。
锁定版本的default_permissions、精确二进制只读范围和原生工具发现差异见reuse记录。

本地focused验证目录 `.ai-bridge/verify-DwWYyZ` 的编译、fmt、严格Clippy、领域、
managed、native Codex、Store及HTTP/MCP检查均exit0；源码快照未变，独立测试PG已停止。
模型上游和父研究数据仍是明确fixture；尚不是Cycle自动Mission启动、真实科学Job/
Evaluation返回同Thread、完整T07/T42或可合并证据。CI已登记上述native测试，远端实际
结果必须绑定推送后的Head；本记录不代替GitHub review或完整Issue62交付。

## 2026-09-11：Cycle 原生配置选择与续轮版本

CycleStart 必须明确 researcher_profile/reviewer_profile 的 profile_id 与十进制
expected_revision；migration028 将两个选择随 cycle_startups 封口，历史空选择保持未知。
启动复用原生配置快照锁和现有事务，配置过期、缺失或账号操作进行中不产生 Run/PGMQ
半状态。修改 Profile 不改写旧 Cycle 或原始响应，准确重放仍读取原回执。
新模型轮预约和首次发送重新检查 Session 的 Profile 与锁等待后的 Attempt lease；
已发送调用仍可绑定原生 Turn、对账和结算，不因配置变化退款或重发。

本地 `.ai-bridge/verify-VBYni7` 全量验证 exit0：编译/fmt/严格Clippy、领域及
managed/native Codex 检查通过，Store+Server 共451测试通过、0失败、0忽略；
源码快照未变，独立测试 PostgreSQL 已确认停止。之前 `.ai-bridge/verify-vUFFS9`
只有固定79张表的旧测试断言失败，已改为重复迁移前后的完整表清单一致及必需表检查。
OpenAPI/TypeScript/Ajv 在 `.ai-bridge/web-verify-zO8r57` 两次生成逐字节一致，
手写源未变，构建、wire、150浏览器用例通过；新增合同测试的TypeScript缺值检查
随后修正，原生命令复核 typecheck、504 Vitest 与5个PWA文件测试全部通过。
这些是本增量实际代码/原生事务和受控协议证据，不是完整Cycle→Mission→科学Job→
Evaluation、真实账号T07、完整T42或远端最新Head的CI/review/可合并结论。

## 2026-09-12：原生准备到 Mission 准入与 Thread 绑定

029迁移增加run_missions的不可变驱动/角色/Profile快照；科学Worker在两条终态
ack路径前统一推进正式Cycle。成功必须具有精确的原生DATA_QUALITY生产者证据；
暂停和账号操作保留通知，配置/输入错误与预算耗尽如实收束。同Cycle同角色只入队一次。
Mission与科学消费者共用PGMQ原生条件读取，各自不隐藏或claim另一类任务。

Thread回执仅保存原生身份、版本、公开有效设置和实际非秘密覆盖项，默认设置保持省略，
不复制原生聊天。当前fence绑定唯一Session，重复回执逐项一致；配置修改、取消或接管
不会替换Thread。新turn可在已绑定的DISPATCHING/RECONCILING Mission中预约，
但要求已提交的首次发送意图、当前Profile/lease、有效预算和前轮已真实结算；不假造RUNNING。

历史失败：`.ai-bridge/verify-NkKl06`暴露外键误指admission主键而不是run_id；
`.ai-bridge/verify-nxpReW`全量456通过/5失败/0忽略，失败均是新Profile快照缺少
schema_version封套；`.ai-bridge/verify-Sr5WGg`仅剩首轮不能在DISPATCHING预约。
以上均已修正。当前`.ai-bridge/verify-CYPMGw`定向检查exit0：check/fmt/严格Clippy
及真实PostgreSQL的Mission12、turns18、恢复8、Run生命周期30、约束7，共75通过，
0失败/0忽略；源码快照未变，独立PG已停止。数据库测试中的原生返回仍是明确fixture，
不是实际自动Codex Worker、独立Reviewer、T07/T42或最新GitHub Head的全绿验收。

可信Mission签发沿用现有随机能力、Argon2与SecretVault；固定角色范围、当前Attempt/
owner、每owner一次签发与接管失效已通过真实PostgreSQL及原生验证器测试，包含撤销、
禁用、取消、重复请求和错误科学Run。测试秘密只存在独立临时目录。
后续全量 `.ai-bridge/verify-4YvHCB` exit0：check/fmt/严格Clippy，领域149、managed6、
native Codex25、Store+Server465通过，0失败/0忽略；源码未变，独立PG停止。
这些是本地增量回归，仍不代表Mission Worker已接通或完整Issue62可合并。

## 2026-09-12：可信 Mission bootstrap 与原生接管

`MissionLauncher`复用原生Deployment配置/账号/catalog解析，创建独立空Git工作树，
签发固定MCP范围，凭Run首次发送许可创建持久Thread并保存Session。探测不付费，
默认模型覆盖仍省略。恢复使用原Thread/工作树并核对原始公开设置；未知start和未产生
原生rollout的空Thread不创建替代Session。Git原生执行版本2.55.0；不克隆产品仓库、
复制认证或读取原生聊天文件。

`.ai-bridge/verify-WRDho0`暴露测试辅助函数参数数目及原生恢复失败；静态原因投影
随后在`.ai-bridge/verify-KepXdL`确认RESUME_THREAD/-32603。根因是底层机器授权
允许RECONCILING而MCP身份入口和实验提案仍拒绝，已统一两入口，未伪造RUNNING。
当前`.ai-bridge/verify-7FUpAq` exit0：check/fmt/严格Clippy，Profile HTTP4、MCP5、
Mission bootstrap3，共12通过/0失败/0忽略；源码未变，独立PG停止。实际官方进程
重启、同Thread上下文、同账本两轮累计24个原生报告token及旧owner不能重发均通过。
上游模型回答和市场准备仍是fixture；测试手动调用可信逐轮Store，生产Worker消费/
资源约束/请求产物/科学结果回送尚未完成，不能当作完整T07/T42或GitHub当前Head验收。

后续全量`.ai-bridge/verify-DHzFHw` exit0：check/fmt/严格Clippy、领域149、managed6、
native Codex25、Store+Server469通过，0失败/0忽略；源码快照未变，独立PG确认停止。

## 2026-09-12：逐轮公开请求与恢复投影

`prepare_mission_turn`复用原预约事务逻辑，将原始公开prompt的固定schema产物、
预算预约和PGMQ消息原子发布；文件I/O之后复核租约/期限，字节计入输出预算。
重放逐字节比较，不接受改prompt/改key；失败或过期回滚全部数据库半状态，未知文件
提交沿用现有Run锁定的未引用对象对账。恢复只读取该Run/Session/Attempt/command的
原请求，checkpoint区分已发送但无ACK、原生Turn绑定、终态与已结算用量。
配置变化不擦除旧请求或虚构退款。没有读取或复制原生聊天/隐藏推理。

`.ai-bridge/verify-FBFVNA` focused检查exit0：check/fmt/严格Clippy、领域、managed、
原生Codex及相关Store/HTTP/MCP通过，源码未变、独立PG停止。新增事务并发/失败/
租约测试与原生同Thread两轮测试均通过；后者现在使用实际保存和重新读取的公开请求，
不再借准备任务参数代替prompt。模型回答/市场准备/费用仍是标明的fixture；真实
Worker逐轮驱动、资源约束、定价未知处理和科学结果回送尚未完成，不作为全量或CI证据。

## 2026-09-12：原生单轮驱动与未知结果保留

`MissionConnection::drive_turn`现在直接消费真实请求产物及唯一发送许可，核对原生
Thread/Turn，按同Session累计token差结算。首次原生终态使用数据库接收时间，重复
查询保留原时间；无用量时只保存终态，不补零或释放预约。未知发送ACK不按列表顺序
猜身份，不重发。Run或未结算Turn到期由共享数据库入口先提交取消，再调用原生
interrupt；interrupt ACK不等于用量结算或整个Run已取消。没有可信费用报价时，
新费用上限调用在发送前明确拒绝；原生费用不可用模式不伪造金额。

第一轮`.ai-bridge/verify-kCWWbm`为14通过/1失败，费用分支测试错误地使用默认
UNAVAILABLE fixture。已在Draft阶段显式建立USD/ESTIMATED测试预算后冻结；未修改
冻结预算或放宽断言。原生两轮测试改为调用生产驱动；新增丢ACK、丢用量、费用拒绝及
真实原生interrupt故障验收，慢模型响应仅是明确的本地HTTP fixture。

随后全量`.ai-bridge/verify-TczpoZ` exit0：check/fmt/严格Clippy、领域149、managed6、
native Codex25、Store+Server475，共655通过/0失败/0忽略。源码快照未变，独立PG
已确认停止。这是628b18fe基础上的本地工作增量证据，不是新GitHub Head的CI结果。
Worker队列/CLI接入、整个进程树资源限制、自动科学结果回送与Reviewer仍未完成，
不能把单轮驱动通过当作T07/T42、Issue62完成或允许合并。

## 2026-09-12：原生进程树资源与取消竞态

Mission复用Linux systemd user scope与prlimit，对整个原生进程树施加冻结CPU速率、
内存、进程数和剩余墙钟。连接持有经自身PID成员校验的原cgroup.kill描述符，关闭和
异常Drop清理原组，不按可复用名称误杀后来的组。原生内部文件单独限64MiB，不把
QZ较小的研究产物预算误用作Codex SQLite迁移上限；两者不是工作区总磁盘配额。
整个bootstrap统一限110秒，低CPU下单RPC60秒/MCP启动45秒仍在此界限及Run期限内。

真实故障定位包括：外部验证器缺XDG_RUNTIME_DIR、systemd只接受百分数两位小数、
1MiB文件限制使原生SQLite WAL触发SIGXFSZ、主进程退出后遗留bwrap阻塞同Run重连。
固定0.144.4源码及运行还证明turn/start先确认入队，不能假造RUNNING或立即中断
未开始的Turn；驱动现在等待真实开始，完成竞态的拒绝只按精确原Turn查询终态。
deadline测试保留DB取消意图先于真实原生终态、未知用量不补零的断言；启动阶段
可能已被中断，因此不要求一定发生Provider请求，但禁止重复请求。

`.ai-bridge/verify-wOL0cl`局部16项通过；最终全量`.ai-bridge/verify-OWRz0r` exit0：
check/fmt/严格Clippy、领域149、managed6、独立native Codex28与Store/Server478
均通过，0失败/0忽略；native组有重复执行的Server用例，不是另外28项独立覆盖。
内核实测包含内存耗尽SIGKILL、剩余墙钟到期SIGTERM、实际cgroup/rlimit读回及
主PID退出后的后代清理。源码未变，独立PG已确认停止。模型回复和市场准备仍是
显式fixture，不含真实账户或生产数据。这是eb115840基础上的本地增量证据，不是
GitHub CI。Worker/CLI自动Mission、科学结果闭环、Reviewer、工作区总磁盘配额及
完整恢复/用户流程仍未完成，不能据此合并或关闭Issue62。

## 2026-09-12：逐轮超预约后的新发送阻断

新预约和首次派发现在共用同Cycle不可变reservation/receipt的原生SQL比较；token或
精确小数费用超过原预约，即使Cycle总额未超限也阻断新发送。原真实结算、原回执重放
和确认未发送的退款不受此新准入检查影响，不增加另一套可重置计数或标记。
`.ai-bridge/verify-Vtz2Vq`在ed8f9632基础上运行check/fmt/严格Clippy及真实PGMQ/PG
Mission、turn、recovery、Run生命周期、约束回归，80项通过/0失败/0忽略，源码未变、
独立PG确认停止。新反例保持used+reserved+requested低于Cycle总额，分别验证token
超预约和仅小数尾部费用超预约，并验证其他Mission事先预约的首次发送也被阻断。
这是局部业务账本证据，不是Provider实际计费、全量CI或Issue62完成证明。

## 2026-09-12：浏览器冻结、明确启用与双角色启动

复用既有严格HTTP DTO、ResourceSelect、ProjectEditor和RunDetail，新增Brief执行确认与
持久Cycle列表，不新增依赖或平行业务状态机。非归档项目可冻结，成功后还须显式启用
才能启动；Profile分别明确选择，Project/Brief/Runtime/Profile修订保持原字符串。
未知回执保留原内容和幂等键，后台失败或配置刷新不能改写原意图；首次明确409要求
重载，未知请求后再收到409仍不能声称之前未执行。排队只显示Cycle/准备Run事实。

浏览器回归定位到共用QueryPanel的真实卸载问题：AntD Space会展开Fragment并以位置
给无key子项编号，插入两条读取警告使既有表单重新挂载。现在共用内容容器有稳定key，
原表单/待确认请求跨警告插入和移除保留；三尺寸丢ACK与后台失败回归覆盖这条路径。
局部TypeScript/Vite及33项浏览器回归通过（58945）；合成响应只证明页面合同，不是
真实数据冻结、模型研究、资格或完整Issue62证据。全量生成与浏览器验证另行记录。

最终`.ai-bridge/web-verify-eDjimU` exit0：Rust原生合同双次导出、TypeScript/Ajv双次生成
逐字节一致，Runtime合同不变；TypeScript、504项Vitest、5项PWA文件测试、三组数值
wire检查、生产构建、Codex设置专项及全部168项三尺寸浏览器测试、CLI帮助均通过。
手写源码前后不变，生成文件无差异。这是ca22a9f4基础上的本地增量验证，不是GitHub
当前Head CI；未运行真实账户付费研究，也不表示全量W0–W8/T01–T42或交付链路完成。

## 2026-09-12：工具续轮失败不能结算中途用量

锁定原生0.144.4的真实反例：第一条合成模型响应调用原生shell并报告12个token，
实际工具完成后第二条模型请求收到不含用量的断流。原驱动错误地返回
FAILED/actual_tokens=12完整回执。`.ai-bridge/verify-3rRkL9`因此16通过/1失败，
check/fmt/Clippy通过且源码未变、独立PG已停止；不是推测或仅mock客户端。
修复只在共用原生驱动中限制自动结算为COMPLETED，失败/中断保留终态与未知预约。
不新增表或计费器；独立可信完整用量仍可由既有Store账本结算。

最终`.ai-bridge/verify-Ftn1NN` exit0：全工作区check/fmt/严格Clippy，以及原生
HTTP Profile4、MCP5、Mission8共17项通过/0失败/0忽略，源码未变、独立PG确认停止。
故障回归还核实第二次原生请求包含实际shell结果，不把仅生成工具调用当执行成功。
这是88990e23基础上的局部增量证据，未证明Provider真实计费、运行中预算中断、
生产Worker自动Mission、完整科学/Reviewer链路或GitHub最新Head完成。

## 2026-09-12：原生超额停止与终态来源核验

沿用现有Run事件和Turn账本：首次达到预约的原生部分用量形成`mission.token_limit`
及同事务取消意图，随后才原生中断；未结算期间同Cycle的新预约/首次派发被拒绝。
最终回执不能低于已记录观察，足额真实回执和旧请求对账仍可进入，观察历史不改写。
没有新增计费器、队列、表、依赖或业务hash；原生rollout budget的跟踪/提醒没有当成
硬性额度保证（[官方配置说明](https://learn.chatgpt.com/docs/config-file/config-reference)）。

真实原生测试发现两条边界，而非只放宽断言：一是工具续轮请求可能早于用量通知，
因此中断不能承诺撤回已经在途的第二次模型请求；二是0.144.4的真实断流通知为
FAILED，而`thread/turns/list(itemsView=notLoaded)`重建列表显示COMPLETED。
`.ai-bridge/verify-VKCPI9`的单例诊断实际观察2次请求及InProgress/Completed，
60秒内没有列表Failed。此前`zgJQv9`遇到只读列表-32603，`gjlqCg`只读重试仍超时；
均真实失败、源码未变且各自独立PG已停止，未把这些结果记为通过。

修复后只以真实`turn/completed`通知或其既有持久记录确认终态，通知立即入账。
ACK/列表只恢复身份；丢失真实终态时保留UNKNOWN，-32600也不能补造取消。
迟到部分用量不能把已失败Turn变为成功/取消，仍记录超额且保留全部未结算预约。

验证边界：首轮完整`.ai-bridge/verify-HnefIj`的check/fmt/严格Clippy、domain149、
managed6、native28通过，Store/Server482通过/1失败（错误地要求续轮请求必为1次）；
这不是最终全绿证据。最终Store代码在`.ai-bridge/verify-kuNCRF`通过82项真实PG回归，
含并发停止幂等、旧fence拒绝、跨Mission未结算门禁和39不能结算已观察40的反例。
最终原生状态修复在`.ai-bridge/verify-PQwg2g`通过精确反例，再以相同源码在
`.ai-bridge/verify-i4VkRN`通过check/fmt/严格Clippy及Profile4/MCP5/Mission10，
共19项通过/0失败/0忽略。两种原生顺序分别断言真实Cancelled和真实Failed、
120/100用量事件、原预约未结算；缺通知的列表完成不能生成终态，已有真实终态则
保持首次数据库观察时间。全部验证器确认源码未变和各自独立PG停止。

这是6d5a4ae6基础上的本地增量；合成模型响应/市场准备不冒充真实付费账户或科学
结果，未证明严格token/美元上限、生产Mission自动领取、科学/Reviewer完整链路、
GitHub最新Head CI或全量W0–W8/T01–T42完成。

## 2026-09-12：幂等首轮准备

首轮准备复用既有公开请求产物、逐轮预约和PGMQ事务；冻结Brief与剩余token账本
决定请求，已有任意预约则保留原请求，包括人工接续、未知发送与Profile后续变化。
费用受限但缺原生计费时拒绝准备，不插入产物、预约或队列消息。余额读取提交后
才进入原事务再次检查，避免嵌套持锁或把旧余额当发送许可。

`.ai-bridge/verify-phLsaW` exit0：全工作区check/fmt/严格Clippy与84项真实PG回归
通过（constraints7、missions19、run_lifecycle30、turn_recovery8、turns20），
0失败/0忽略；源码前后不变且独立PG确认停止。新增检查验证产物发表失败不留下
预约、首次额度10000、未知发送不换请求、已有人工请求不被首轮覆盖，以及无价格
不产生可发送工作。此为587803bd基础上的本地准备增量；尚未接入常驻Worker，
不代表实际模型发送、科学结果、Mission/Cycle完成或GitHub当前Head验收。

## 2026-09-12：常驻 Mission 首轮消费

复用Worker、原PGMQ、Run租约、MissionLauncher和逐轮账本，科学/Mission分别有界。
显式部署三项配置才领取Mission；未配置不隐藏其消息。整个驱动维护10秒续约60秒，
关闭/失去续约回收本机原生子进程，不造取消或退款。已结算轮接管不再开连接/重发，
Mission消息保留给尚需实现的科学、结论及完整收束阶段。

真实守护进程回归通过：未配置队列read_count不变；自动准备、发送并以12token结算
唯一原生Turn，重启不重复发送/预约/ack；等待实际模型响应时，单Mission槽不妨碍
科学终态消息被处理并archive，数据库租约真实延长；关闭后原预约、消息和未知用量
保留，Run不假称终态。CLI三种不完整配置均在访问数据库之前以exit2拒绝。
市场准备和模型响应仍是受控fixture，不是付费模型研究或真实科学结果。

首轮完整`.ai-bridge/verify-Fw5gJ0` exit1：check/fmt/严格Clippy、domain149、managed6、
native28通过，Store/Server488通过/1失败/0忽略。唯一失败是旧迟到用量回归将列表
Completed当成必然；本次实际返回Failed。实时快照与历史重建允许这两种投影，
测试现在仍拒绝其他状态，且驱动前必须没有持久终态；最终真实Failed、120/100事件、
不结算部分用量和时间先后断言全部保留，不修改业务驱动或放宽最终结果。

相同业务代码在`.ai-bridge/verify-aVDYqb` exit0：check/fmt/严格Clippy、Profile4、
MCP5、Mission13共22项通过/0失败/0忽略。两次源码前后不变，各自独立PG确认停止。
前一完整结果仍不是全绿。同步明确Issue62第6.3–6.4节的原生Turn语义：内部工具
续请求共享本Turn预约并累计用量，新turn/start单独预约；不宣称限制Provider HTTP
请求次数，不新增模型代理或接管原生工具循环。这是50c0d024基础上的本地增量，
不是GitHub当前Head CI或全量W0–W8/T01–T42完成证据。

## 2026-09-12：提案与编译任务的原子关联

新增030的不可变experiment_compilations边，复用已有Run/Attempt/原生定义/PGMQ。
可信当前Researcher Mission可为同Cycle的正式提案准备一次CompileModel；原代码、
参数指针在关联建立后冻结，旧来源不回填。任务只携CODE和服务生成的参数，无任何
Dataset挂载；准备的experiments=0，其他资源仍由原准入事务预约，期限不越过Mission。
重放取原Run，不重新发表或收费；没有新引擎、队列、公共执行DTO或应用hash。

`.ai-bridge/verify-6zRwrd` exit0通过check/fmt/严格Clippy及104项真实PG回归。
之后只补最终关联写入失败的故障断言：原生文件已发表，Run/任务定义/PGMQ已在
同一事务准备，再由独立测试数据库触发器拒绝最后插入；五项计数确认Run、产物
记录、队列、原生定义、CPU预算全部回滚，已发表对象保留，移除测试触发器后可
正常重试。没有通过删除文件或修改正式迁移来让检查通过。

最终`.ai-bridge/verify-2LK0GF` exit0再次通过check/fmt/严格Clippy与104项PG回归
（constraints7、data_validation7、experiment_compilations2、experiments11、missions19、
run_lifecycle30、turn_recovery8、turns20），0失败/0忽略。两次源码前后不变、独立PG
确认停止。正例检查并发只建一个Run/一份预算、原生参数精确引用原CODE且无市场
输入；反例检查陈旧fence、发表失败、非零试验计数、执行后改代码及删关联均拒绝。
这是79bb243c基础上的任务准备证据，不代表真实编译/预测执行、同Thread结果回送、
Alpha资格或完整交付；Worker后续阶段尚须接通。

## 2026-09-12：编译生产者到Discovery预测的原子关联

031迁移保存不可变experiment_forecasts，原提案的科学run_id与关联同事务提交且
不能换Run。可信准备入口要求原编译真实SUCCEEDED及其原生采纳的MODEL，明确
选择冻结Discovery版本，并以同一元数据适配生成有限选择。固定bars horizon与
Brief一致，预测参数复用原生任务校验；只挂载一个Dataset、原MODEL和服务参数，
不挂载代码或Sealed。existing Run准入一次预约experiments=1，并发重放返回原Run。

`.ai-bridge/verify-JzgA0E` exit0：check/fmt/严格Clippy及106项PG回归。随后只加强
参数负例并同步CLI/用户文档/薄Skill：保持文件字节长度，使Sealed数据选择、错误
horizon、超额fuel、错误EMA周期和自报MODEL分别触发真正语义校验，不只靠大小
不符失败。`cargo test --locked -p domain` exit0，96项通过/0失败/0忽略。
最终`.ai-bridge/verify-P9fRv6` exit0再次通过check/fmt/严格Clippy和106项PG检查
（constraints7、data_validation7、experiment_compilations4、experiments11、missions19、
run_lifecycle30、turn_recovery8、turns20）。两轮源码前后不变，独立PG均确认停止。
新正例验证未完成编译拒绝、原MODEL绑定、并发一个Run/一份试验与CPU预约、原生
Job输入及冻结run_id；负例验证错误参数/陈旧fence、最后关系插入故障回滚全部
Run/产物记录/PGMQ/预算，正常重试可继续。

这里的编译输出为明确标注的受控协议fixture，证明真实数据库/文件/队列事务，
不冒充执行rustc、Wasmi或真实市场预测；本地b1da5b7b上的此增量尚未接入Worker
自动科学推进/同Thread反馈，更不代表分折、Reviewer、资格或完整PR验收。

## 2026-09-12：常驻Mission自动准备科学步骤与过期探测恢复

Worker在最新原生Turn已确认结算后，按ordinal从现有正式提案中选取一个就绪的
编译或预测步骤，复用原Mission资源分配和原有Run准入；排队/运行中的身份不重建。
原生Client先关闭，科学Worker独立执行；准备不是Mission完成，消息不提前ack。
首轮公开请求新增确切Cycle及冻结政策family ID、现有Wasm ABI/参数合同，避免
Agent在仅有Brief/Mission工具的情况下猜测必填提案身份。复用共享fixture，API与
Worker使用同一真实ArtifactStore目录，不重复构造另一套原生编译回执。

完整`.ai-bridge/verify-AydzuH` exit1：前置check/fmt/domain/managed/native通过，
Clippy发现共享测试的一条多余import；Store/Server为493通过、1失败、0忽略，失败
是新增自动编译用例。删除import后，单例`.ai-bridge/verify-l1WeIe`仍exit1，安全
诊断明确是runtime_probe_stale：模型首轮约77秒，原Runtime探测有效期只有60秒。
没有通过忽略错误、改变预期、延长探测TTL或删除断言获得通过。

修复复用原有fenced Runtime探测：当前活跃Researcher Mission可刷新其冻结Runtime，
准备/发表重验状态、租约、版本及期限，真实HTTP仍在事务外；不要求已派发的模型
Mission回到NOT_SENT。临时诊断已移除。新增用例等待真实60秒探测过期，官方App
Server完成真实Turn后通过带原Vault凭据的原生HTTP刷新，随后关联原编译/受控采纳
MODEL/预测Run，多次接管仍只有一个模型请求和一份科学身份。未结算慢Turn即使
已有提案也不准备编译、不探测、不伪造用量或Mission终态。

`.ai-bridge/verify-osf4B5` exit0通过check/fmt/严格Clippy及该原生单例（126.55秒）。
最终完整`.ai-bridge/verify-QL1AOB` exit0：check/fmt/严格Clippy、149项领域/合同/
Runtime测试、6项真实原生Job子进程、28项原生Codex检查和494项Store/Server回归，
全部0失败/0忽略。检查期间源码未变，独立PG确认停止。上述成功仍只是5c7d78c2上
的本地增量证据；编译输出fixture不冒充真实rustc/Wasmi科学执行，完整同Thread
结果反馈/结论、独立Reviewer、评估/资格和剩余产品链路尚未完成，不是PR合并证据。

## 2026-09-12：原Thread科学反馈与一次性修复预约

在8c57d16d上增加已采纳编译失败/Discovery预测终态的公开反馈。复用已有原Session
Turn命令唯一键和同事务请求产物/预算/PGMQ发表，未增加反馈队列或状态表。最新
模型Turn没有完整usage结算时不继续；失败回送计REPAIR，预测成功计RESEARCH。
预测只读取原生产者的RESEARCH报告，保留origin/原生版本/fuel及全体观察计数，
明确标注首尾各16条抽样，形式评估为NOT_PERFORMED；不制造指标或资格。
失败只陈述公开Run原因，不捏造尚未接入的详细编译器诊断。修复保留原实验父血缘。

`.ai-bridge/verify-WJqhOH` exit0：check/fmt/严格Clippy及107项真实PG回归，0失败/
0忽略。新例证明失败编译不读取预测或诊断正文、未知模型用量不接续、产物发表
失败回滚预算/消息、并发结果选择只有一份修复预约、结算后重放不重复扣费及旧
owner拒绝。抽取的原请求事务仍由原有Turn/恢复用例覆盖。

`.ai-bridge/verify-SLsSW3` exit0：check/fmt/严格Clippy及真实原生科学/Mission单例
通过（212.20秒）。它等待原探测真实过期，首轮完成后准备原编译/预测，采纳受控
40条预测输出，验证反馈中的36条预测/31条完整标签/32条首尾抽样及原产物身份。
然后启动新App Server进程恢复同一Thread，实际收到首轮上下文和科学反馈；总共
两次受控Provider请求、两份真实原生Turn结算，重投无第三次调用且Mission未提前
终结。原生进程/Thread恢复是真实执行；编译/预测/Provider内容是明确fixture，
不是付费模型推理、rustc/Wasmi数值执行或市场/Alpha资格证据。
两次验证均源码不变、独立PG确认停止；之后只追加本证据。研究结论、完整科学
评估、Reviewer、资格及其余产品流程仍未完成，尚未push/review/merge或关闭Issue。

## 2026-09-12：原生公开回答与中断后的摘要恢复

在b3f07d81上接入锁定原生协议的summary视图，只读取精确Turn最后的公开回答，
不读取full/items、rollout或隐藏推理。公开text/phase/原生item身份在完整成功终态
及usage结算后作为不可变qz.mission_summary保存，沿用原Mission输出预算；重放
比较原字节，发布失败保留已结算用量。真实原生测试验证重启及第二轮后的分页
仍返回原回答，且没有额外模型请求；PG用例验证来源、并发重放、回滚和旧owner。

首轮完整验证`.ai-bridge/verify-RLFnHY` exit1：Store/Server496通过、1失败。
失败发生在daemon立即重启原Mission时，PROFILE_CONNECTION/Unavailable；原
cgroup.kill已发送，但systemd尚未回收同名scope。修复位于共享Mission启动入口：
只查询精确scope的原生LoadState，最多等3秒且不超过剩余墙钟，不杀活跃owner、
不换名字重开任务。`.ai-bridge/verify-Ny0zbH` exit0验证实际scope活跃时拒绝、
退出后复用，以及强制摘要发布失败后的真实Worker恢复：仍只有1次Provider请求。

最终`.ai-bridge/verify-187ozo` exit0：check/fmt/严格Clippy，149项领域/合同/
Runtime、6项真实Job子进程、30项原生Codex和498项Store/Server检查，全部0失败/
0忽略。验证期间源码未变，独立PG确认停止；随后仅追加本证据。受控Provider/
科学fixture不是付费账号、科学数值或完整T08/T42证明；Mission收束、正式评估、
独立Reviewer及后续产品仍未完成，不能据此push review、合并或关闭#62。

## 2026-09-12：有界Mission执行收束与终态后ACK恢复

在23ae6736上复用原Run终态事务/Attempt结果引用/PGMQ完成会话收束。只有最新
成功公开回答、全部Turn终态和完整用量，以及已完成并回送原Thread的全部关联
科学任务齐全时才提交；未处理提案、未知任务、缺用量/回答均不能凭空结束。
最后的原摘要是结果引用，不另建完成表/包装报告。原取消CAS保持优先；会话
SUCCEEDED不修改Cycle、Experiment outcome、Evaluation或Qualification。

`.ai-bridge/verify-ZBGCKi` exit0：check/fmt/严格Clippy及109项真实PG回归，0失败/
0忽略。新增/扩展用例覆盖缺失终态/用量/摘要、旧owner、终态写入失败整事务
回滚、并发一次采纳、原摘要关联、先终态后归档、重复ACK，以及先取消、缺用量
仍未决、完整成功回答到达后保留CANCELLED且不生成资格。

`.ai-bridge/verify-Kt3j7G` exit0：check/fmt/严格Clippy、真实scope复用检查及实际
App Server/Worker恢复用例。注入摘要发布失败后重启，再注入PGMQ归档失败；
终态已经提交、消息仍待ACK，后续Worker只归档，总Provider请求仍为1。
`.ai-bridge/verify-Wlzimi` exit0：check/fmt/严格Clippy及实际两轮研究用例。原
编译/预测在途时不收束，原预测报告回到同Thread且保存第二个回答后，唯一
Mission终态与PGMQ归档成立，2次Provider调用不变。

三次验证期间源码不变、独立PG均确认停止。它们是本地定向增量证据，不是新Head
全量CI或完整T08/T42。受控科学输出/Provider不冒充真实数值或付费账号；没有
完整用量的失败/取消、预检/详细编译诊断、正式评估、Reviewer、Cycle结论及
其余产品工作仍须继续。尚未push/review/merge或关闭Issue。

## 2026-09-12：原预测的未授资格Alpha版本及初次实验裁决

在b83fe34e上复用已有Alpha/Version表和command_receipts，从精确成功Discovery
预测创建一次RESEARCH版本。版本保存原MODEL/CODE/根血缘/镜像及冻结Brief的
单位和horizon；不填校准、不改变PENDING、不授资格。Worker自动登记后才能继续
回送与收束；原生两轮回归确认此元数据步骤不增加Provider请求。

迁移033修正先建立评估主体便永远冻结PENDING的顺序冲突：被消费实验的输入仍
不可变；初次结果仅能与精确Alpha/冻结政策的新Evaluation在同事务发布。旧评估
已封口不能补写裁决；结果必须匹配真实评估状态，已发布裁决不能再改。关系测试
中的受控PASS只验证约束，不是数值评估或资格证明。

首次`.ai-bridge/verify-02OdRu`编译因Option::map调用笔误失败，修复后全量
`.ai-bridge/verify-DHZfNf`功能测试全部通过：149领域/合同/Runtime、6真实Job、
30原生Codex、501 Store/Server，0失败/0忽略；但Clippy拒绝新增测试的复杂元组，
因此该轮整体exit1。随后仅将该断言改为sqlx::Row命名列读取，不改变产品源码；
`.ai-bridge/verify-BcfPh7` exit0通过check/fmt/严格Clippy及110项相关真实PG回归。
两轮验证期间源码未变、独立PG均确认停止；新初次裁决关系用例包含于前一轮全量。
以上不是远端最新Head CI，也不代替正式评估、独立Reviewer、全部W0–W8/T01–T42。
未push、请求review、合并或关闭Issue，继续完成剩余开发。

## 2026-09-12：独立原生Alpha分折、训练校准及逐折指标

在8281f731上实现本地`job validate-alpha`，不接管PG或资格权限。复用原目录读取
与同一EMA特征迭代器、solow-cv0.7.3、Wasmi2.0.0、linregress0.5.4及已安装的
ndarray-stats0.7.0；没有新增依赖或自写统计估计器。逐资产/折/训练/测试/不连续
块重建模型，累计fuel不重置；只将训练标签给OLS，全部测试标签在模型执行后
独立形成。保留全部折、原ordinal、预测、校准状态、原生IC/RMSE及缺失原因。
跨折重复观察仅去重计数，不冒充统计独立样本；无全局平均、PBO或自动PASS。

真实合成Parquet+Wasmi+原生统计回归覆盖原训练范围/OLS系数、测试收益、逐折
IC及独立RMSE参考、CPCV全部组合与不连续块的状态重置、全任务fuel耗尽、不一致
horizon、样本不足、常数校准、原生溢出及实际子进程严格JSON/安全失败输出。
原forecast特征重构的因果预测/未来扩展不变及跨资产状态回归保持通过。

原生Rust合同以相同命令两次独立导出：`.ai-bridge/web-verify-4lk6qB` domain-only
exit0，输出逐字节一致，手写源码不变；仅更新domain-v1.openapi.json，不宣称
已接通HTTP/Gateway合同。最终`.ai-bridge/verify-jgaEza` science exit0：全工作区
check/fmt/严格Clippy、149领域/合同/Runtime、30原生Codex及72项Job测试全通过，
0失败/0忽略（72包含另行通过的6项managed测试）。验证期间源码未变。
这是本地原生计算与合同证据，不是新的远端CI、真实市场有效性或完整T08。
受管ValidateAlpha协议、原Run/政策采纳、正式Evaluation、sealed独立评估、Reviewer
及其余全部开发仍须继续；没有push、请求review、合并或关闭Issue。

## 2026-09-12：受管Alpha分折与原请求逐折采纳

在bad625f2上将同一原生计算接入`VALIDATE_ALPHA`/ALPHA_EVALUATE，输入仅原
VALIDATION目录、MODEL和PARAMETERS，输出封口为qz.alpha_validation.v1。
分折适配移至domain::execution::validation，Job与结果采纳复用同一锁定solow-cv；
没有复制切分算法、新增队列或依赖包。逐资产source_row_count与原warmup/horizon
用于重建全部原生train/test索引；漏折、替换索引、顺序/原来源/重复标签冲突、
超fuel、越过cutoff、校准/指标状态不一致均拒绝。保持原生缺失/非有限原因，不填0。
Runtime能力从既有输出登记表派生；镜像标签增加实际数值版本与alpha-validation/1，
旧镜像须重建登记新OCI ID，不能借旧native-stack声明新能力。

真实Parquet/Wasmi/CV/OLS回归覆盖WalkForward、全部CPCV组合、重复源标签一致性、
常数/溢出及17种报告篡改；真实managed子进程拒绝DISCOVERY/SEALED训练，沿用原
输出限额和封口。SQLite目录范围回归覆盖四种数据操作的注册类型、资产、事件及
可见截止；这些不是新镜像的实际OCI执行证据。

`.ai-bridge/web-verify-EzTVis` exit0：三份Rust合同各两次导出、客户端两次生成
逐字节一致，手写源码不变；504前端单元、168浏览器回归、wire、类型及构建通过。
`.ai-bridge/verify-3ljWRQ`全量轮的501 Store/Server、7managed、30原生Codex通过，
但Runtime旧测试仍断言5种JSON报告，实际新增为6种，因此整体exit1。随后仅修改
该测试为精确类型清单及镜像标签一致性检查，不改产品/生成物。
最终`.ai-bridge/verify-gPLRYG` science exit0：check/fmt/严格Clippy、149领域/合同/
Runtime、30原生Codex和74项Job测试通过（含另行运行的7managed），0失败/0忽略。
两轮源码均冻结；全量轮独立PG已确认停止。未重跑新镜像OCI，未称为完整T08或
远端CI。正式评估准入/发布、政策/资格、Reviewer及其余工作仍继续；未push/review/merge。

## 2026-09-12：原生逐折指标到既有政策检查

在af6acfbc上新增可信`alpha_validation_metrics`转换：先重验原请求全部分折，
再原样保留每个原生值/状态/原因，关联原evaluation/report身份、资产/折scope、
原生方法/版本、单位、完整bar规格与固定horizon。纳秒原报告不变，MetricValue的
微秒时间边界只向外取整；IC和RMSE分别计实际配对数，缺校准不是零收益或有效
预测。相同可信适配提供能力记录给既有精确Decimal阈值函数，没有另写估计器、
全局平均或资格机制；尚未写入数据库Evaluation。

真实Parquet/Wasmi/CV/OLS用例逐项核对全部转换记录、原数值、上下时间边界、
方法/单位/周期、样本数及引用；错误单位为UNSUPPORTED，缺校准为INCONCLUSIVE，
部分报告拒绝。纯数值阈值的PASS断言不是FIXTURE市场资格或完整评估。
`.ai-bridge/verify-XRwigA`的149领域/合同/Runtime、30原生Codex和75Job全部通过
（75含单独通过的7managed），但Clippy拒绝测试的两处chunks_exact(2)，整体exit1。
仅改为原生as_chunks::<2>()后，`.ai-bridge/verify-HTTaGY` compile exit0通过全工作区
check/fmt/严格Clippy；最终原生Alpha验证7项再次通过，0失败/0忽略。验证期间源码
均冻结，无DTO/生成器变更。继续修正首阶段试验计数并接正式评估，未push/review/merge。

## 2026-09-12：首阶段只计一次试验

按B1将原编译阶段改为预留/结算一次试验，预测延续阶段为零；同步纠正A3.5–A3.7
之前相反的描述。复用同一事务准入与结算，不增加公共免计费参数；普通AlphaEvaluate
零试验仍拒绝。预测必须关联原已计一次的编译准入。迁移034只约束新增阶段关联，
不重写、退款或清零历史试验；原命令重放只读回同一Run。

真实PG回归确认并发编译只预留一次、失败编译仍计一次、重放不重复计费、发布回滚
不漏预留，后续预测不再计试验但继续计CPU。`.ai-bridge/verify-hKBrqV` mission-store
exit0：118项相关PG回归、check/fmt/严格Clippy通过；`.ai-bridge/verify-Avh8yE`
mission-science exit0：真实原生Mission两轮会话及编译/预测/Alpha登记链路通过，同样
通过check/fmt/严格Clippy。两轮源码冻结、独立PG均确认停止；未重跑全量501后台
测试，未宣称远端CI或完整T08。正式验证与发布等全部剩余开发继续，未push/review/merge。

## 2026-09-12：冻结与启动核验原生验证方法

复用原生指标方法登记和分折参数边界，在Brief冻结、Cycle启动时读取原已登记
Validation元数据，检查Selection的准确方法/版本/单位/周期及资产/折scope，逐项
核对required指标。原生CPCV组合数超过256时直接拒绝，不生成部分折。当前受管
合同仅一个Validation目录版本（可多资产），不默选多个版本中的首项、不将Sealed
选择意图改为普通验证。登记政策不是可执行证明；历史意图和证据不被重写。

真实PG测试核对只读原Validation元数据、缺对象或损坏字节不留下冻结上下文/回执，
原成功回执重放不再读对象。真实原生Alpha用例覆盖12种错方法/版本/单位/周期/scope/
选择类别、required未实现指标与CPCV组合上限；optional缺失不冒充required通过。
`.ai-bridge/verify-IGh6He` full exit0：502 Store/Server、149领域/合同/Runtime、
30原生Codex、7managed通过，零失败/零忽略，独立PG确认停止；随后
`.ai-bridge/verify-BNLrpn` science exit0再次通过149领域/合同/Runtime、30原生Codex
和75Job（含另行通过的7managed）。两轮check/fmt/严格Clippy均通过，源码冻结。
无新依赖、DTO或生成物变化；没有构建/运行新OCI镜像，正式Evaluation与后续交付仍
须继续实现，以上不是最新远端Head CI或完整T08/T42。未push/review/merge或关闭Issue。

## 2026-09-12：原试验的正式Validation准入

内部Store新增正式验证准入，与Discovery预测共享原MODEL/提案参数、预算和原生
发表事务；只换为冻结Validation输入、split/target/政策。迁移035原子绑定原试验/
Alpha/政策/目录/Run，参数与报告为EVALUATOR_ONLY；不改变原Discovery Run、不计
第二次试验，不授予Evaluation或资格。两种Alpha阶段均拒绝冻结执行镜像漂移，验证
同时重查真实方法能力。编译及Alpha阶段按缩短后的剩余墙钟重算CPU，容量不足拒绝。

真实PG回归验证缺原Alpha、镜像/方法变化、发表失败回滚、并发唯一Run/CPU预留、
原试验计数、准确政策/模型/目录/参数、原生JobSpec及不可变关联。CPU最窄单元测试
通过。`.ai-bridge/verify-ssV6Mq` mission-store exit0：119项相关PG回归零失败/忽略；
`.ai-bridge/verify-NY1BQ2` mission-science exit0：原生App Server会话与原提案/科学
阶段准入及反馈回归通过。两轮check/fmt/严格Clippy通过，源码冻结，独立PG确认停止。

证据范围澄清：此处及前述mission-science用例真实运行Codex App Server，但其
Responses Provider与编译/预测终态是受控fixture；它不实际执行科学Job。真实Job/
分折计算证据来自独立75项Job及managed测试，不能拼称为完整端到端链路。新原生
镜像/OCI链路尚未执行。本次也未将Validation接入自动Mission调度、Evaluation发布
或受控正式反馈；这些与其余全部合同继续实现，无新增公开DTO或依赖，未push/review/merge。

## 2026-09-12：真实OCI镜像装配与编译验收

首次真实构建在`job --version`失败：usr-merged宿主的ldd把ELF所需`/lib64`
加载器解析到`/usr/lib64`，旧装配漏掉绝对请求路径。复用原文件复制逻辑保留两者。
第二轮镜像可启动，但7项OCI仅4通过；原宿主私有工具链的700/600权限被原样带入，
固定容器用户不能执行rustc或读取库。仅在新镜像根中规范选定公开文件的读/执行
权限，不改变宿主文件、Docker权限或生产状态；构建新增真实rustc版本启动检查。

最终外部回执`owner-oci-40q4sS`（会话25182）exit0：基于989401ea及上述冻结补丁，
真实构建job/runtime、装配、Docker构建、job/rustc启动和全部7项native-oci通过，
零失败/零忽略，验证前后源码快照一致。Docker原生测试镜像ID为
`sha256:43c8a3369210baaf74006b6eee03caab9e6feedec2fdbb4791d4df051d739b7e`。
检查覆盖实际Rust→Wasm、并发重放唯一容器、完成任务崩溃恢复、Gateway退出后的
原生墙钟截止、取消晚到屏障及现有内核边界回归；5项原生文件复制测试也通过。
此前两轮失败均保留外部回执，不能归为成功。没有以root运行Cargo、挂载生产数据
或扩大用户Docker权限。此项仍非正式Validation目录OCI链路、完整Mission科学链路
或T01–T42/远端CI通过；所有剩余开发继续，未push/review/merge。

## 2026-09-12：正式Validation评估与原队列收尾

原生Worker统一在首次终态和终态重投的ACK前发表评估；复用原Run锁、冻结政策和
已采纳参数/manifest/全部分折报告。Evaluation、所有逐折指标、受限完成报告和实验
首次裁决同事务封口，不重跑模型、不依赖Mission在线、不新增队列。有效期来自原生
完成时间；来源/PIT、实际测试样本、登记行缺失和精确阈值仍独立判定。失败/取消
保留INCONCLUSIVE及空指标，不制造原生方法记录；无资格或Reviewer批准。

基于92861a及冻结补丁，`verify-mVUVWY`完整回归508 Store/Server通过、1失败：
新唯一索引错误限制了非正式验证的既有评估场景。修正为只锁定原
experiment_validations关联的数据库防重复检查，保留原失败测试预期；增加正式Run
直接SQL重复发表拒绝。随后`verify-R6K6Y6`相关137项PG回归全部通过、零忽略，
check/fmt/严格Clippy通过，源码冻结，独立PG确认停止。新增五项PG用例覆盖原子回滚、
文件/指标失败、并发一次发布、生产者/全部指标、ACK、重放、精确5%缺失边界、
失败阈值、未派发取消和拒绝manifest。科学字节是显式受控fixture，不是实际Job执行。
此前完整回归的149领域/合同/Runtime、7managed、30原生Codex均通过，但该完整
回执整体仍为失败；局部复测不改称全绿。没有DTO/依赖/生成物变更。
自动Mission验证、正式反馈与全部后续合同继续开发；未push/review/merge或关闭Issue。

## 2026-09-12：自动正式验证及原Thread评估反馈

Mission在原Discovery成功后依次登记RESEARCH Alpha、准备正式Validation；复用原
阶段准入与PGMQ，不为中间观察抽样额外创建模型Turn。完整评估发表后，仅从数据库
投影原Evaluation和按冻结口径选定的MetricValue；不读取EVALUATOR_ONLY报告字节。
反馈保留来源、期限、原生产者及缺值，不是跨试验排名、校准、资格或Reviewer批准。
Mission正常收束新增正式验证/评估/反馈回答屏障；公开实验元数据只披露精确原验证
关联的首次裁决，原报告GET仍受限。没有新队列、依赖或公开DTO/生成物变化。

基于5525ec72及冻结补丁，`verify-QEYJux`相关137项真实PG回归通过、零忽略。
`verify-TdMh6y`原生扩展用例在发布故障恢复与ACK之后失败：测试错误地重新领取已
归档消息；原生PGMQ不再返回它，幂等入口是acknowledge_run。仅修正测试，未添加
消费兼容路径。最终`verify-F4Oqew`原生用例通过（218.27秒），check/fmt/严格Clippy
通过，源码冻结，独立PG确认停止。实际App Server验证过期探测刷新、原编译/预测/
验证准入、Worker发布故障/终态恢复/ACK、无报告字节读取的反馈、原Thread保留先前
上下文的第二轮、两个完整用量/公开回答及最终Mission归档；一项试验、一个正式
评估、零资格。Responses Provider与科学报告仍为受控fixture，不是实际Job/OCI
全链路或真实账号T42；不能与独立数值测试拼称端到端完成。
取消/期限/未知用量恢复、独立Reviewer、完整试验选择、Sealed/校准/资格、Alpha与
组合交付操作面、部署/迁移/恢复及其余验收仍待完成；未push/review/merge或关闭Issue。

## 2026-09-12：取消与到期后的已对账 Mission 收束

可信Worker在新连接、科学步骤或结果反馈之前复用原Run收束入口。取消/真实到期
不再要求尚未准入的后续科学阶段或新模型总结；已准入编译/预测/验证仍须有原
Attempt（含未派发null）的真实终态。已开始Validation的评估发布消息独立保留，
Mission取消不自动取消别的Run，不抹除原试验，也不冒充评估已发表。
无发送意图的原预约通过既有用量结算，在同一个Run事务记录NOT_SENT和零用量；
任何发送意图或缺失最终用量仍保留预约。取消可没有Thread/摘要，没有产物就不写
accepted_at；正常成功仍要求公开回答与全部科学/反馈证据。不增加取消队列或账本。

在34a961e9及冻结补丁上，`verify-gfQI0z`首轮137项通过、6个新增场景失败，均因
无摘要却写accepted_at，被accepted_manifest_present约束拒绝。沿用原生终态写法
修正该一处写入，未修改数据库约束。`verify-ya6yVQ`全部143项真实PG回归通过、
零忽略，覆盖原事务回滚/并发重放、零轮次、未知发送/晚到用量、真实期限、编译
结束但不继续预测、已排队验证及后续评估发布。check/fmt/严格Clippy均通过。
`verify-ni8gQZ`四个原生App Server场景通过：两个新增Worker取消场景（63.62秒），
原未知发送ACK场景（52.32秒）和原生期限中断/不得补造usage场景（74.16秒）。
两次最终验证源码不变，独立PostgreSQL均确认停止。原生模型响应为受控Provider；
科学任务内容为有明确标识的fixture，不声称实际Job/OCI完整链路、真实账号T42或
GitHub当前Head CI。未知原生用量仍可能保持待对账；完整选择/Sealed/校准/Reviewer/
资格、Alpha与组合交付操作面、部署/迁移/恢复及其余验收仍须继续，未请求review或合并。

## 2026-09-12：原 Alpha 与正式 Validation 只读操作面

按 DESIGN A3.12 复用原表、授权、UUID游标和MetricValue合同，实现6个HTTP GET、
对应原生CLI命令与Ant Design Alpha页面。仅Operator或精确项目RESEARCH_READ的
CLI可读取完整操作视图；Mission保留原冻结选择口径反馈，不扩大权限。版本按原
Alpha和正十进制版本号读取，不替换活动版本；来源取原Discovery数据任务，不把
生成CODE的SYNTHETIC当市场来源。只有原正式Validation/政策/输入/终态Attempt/
生产者/发布标记全部匹配的评估可见；Sealed和非正式旧记录不可见。指标分页不补
零、不重算，保留方法/单位/期间/计数/来源；原有效期与数据库读取时间分开显示。
这些读取不产生计算、暴露预约、资格或交付，不读取受限报告字节。

基于b85e06d9与冻结补丁，前三次编译检查发现新增CLI名称冲突和测试的原生String/
UTC类型构造错误，修正调用而未加兼容层。最终`verify-oLlzjL`check/fmt/严格Clippy
均为0；40项Store及15项HTTP/CLI回归全通过，0忽略，独立PG确认停止，源码不变。
其中实际原生CLI经过TCP/Axum/Bearer/PG，HTTP核对原版本/分页与大计数；科学结果
仍是明确的受控fixture，不能替代完整Job/OCI或真实研究链路。

`web-verify-QI0NUw`的171项浏览器检查通过、3项因测试使用不存在的Drawer CSS类
失败；改用实际dialog语义。`web-verify-rWkyH6`的3项失败随后定位到指标横向表格
没有键盘焦点；核查所有Table，同类撤销历史表也修复。复用原生onHeaderRow和
tabIndex，使空表也可聚焦，不加滚动脚本、不关闭axe规则、不强制点击。
最终`web-verify-eSjgEy`通过：三份Rust导出与TypeScript/Ajv生成重复字节一致，
手写源不变；typecheck、504项Vitest、5项Node检查、decimal/bigint/fraction线协议、
build和CLI help通过；36项Codex设置浏览器检查及全站177项三视口检查通过。
新回归实际验证Tab/方向键滚动、空指标、0与null区别、精确计数、原来源、分页、
切换项目清理旧详情及读取故障不当空结果。旧HTTP路径和已有schema语义未改变。

此阶段没有新依赖、迁移或资格入口。完整选择/校准/Sealed/独立Reviewer/资格、
双Alpha组合交付、部署/迁移/恢复及T01–T42剩余证据仍须继续；未push、请求review、
合并或关闭Issue。GitHub只读复核PR63仍为Draft、远端37e5713e，Issue62仍开放。

## 2026-09-12：原 Mission 确认前的冻结试验选择

按DESIGN A4.9复用原acknowledge_run事务、project→cycle→run锁与PGMQ archive，
只为原RESEARCHER Mission形成一次选择。取消亦等待已准入原生任务终态及正式
Validation发表。cycle_selections以原Cycle为身份及成员封口；成员先写、deferred FK
指向最后写入的头，头和成员不可改删，形成后不能追加试验；原已授权命令可重放。
未新增Run、队列、模型轮次、依赖或应用级hash身份。失败保留原队列，原终态不撤销。

原生SQL包含同项目/Family/根血缘全部登记历史，每个实验一次；冻结原Run/Alpha/
Evaluation/Metric引用、当时执行状态和排除理由。历史未绑定正式流水线的Run仍保留
原引用，不冒充未执行或正式证据；其他Cycle后来完成或追加试验不回写旧快照。
按原冻结比较输入/执行假设和完整正式发表关联筛选，不能误用含独立MODEL的派生
InputSet UUID。原required指标完整、VALID且精确方法/单位/频率的finite值由PG排序；
MAX/MIN及正负零平手均按原UUID次序。科学REJECT可参与同口径比较，不变成批准。
候选不足/未完成可比试验为INCONCLUSIVE，COMPLETE只表示比较集合数量满足。

两个只读GET、原生cycle selection/trials CLI和Cycle的Ant Design Drawer展示原
规则/计数/成员/排名/缺值/来源，分页按原实验UUID；不存在形成前的空成功快照、
客户端选赢家或重新排名入口。复用原Operator/精确项目CLI权限，Mission不得借用
完整操作视图。UI保留0/null、大计数和原Run，原生表头焦点/方向键支持窄屏横向查看。

以0eeef659及冻结补丁验证。首轮verify-iAlLuL的Rust check/fmt/严格Clippy通过，
新迁移的PL/pgSQL IF内CASE缺括号导致依赖迁移的测试失败，未当作业务通过；修正后
verify-K6ilpf全部通过：119项真实Store测试、19项HTTP/原生CLI测试，0忽略，原
PGMQ回滚/并发/重放、正负零/方向、历史原Run和原生取消评估均核验。源码不变、
临时PG停止。HTTP验证原Mission空取消快照、形成前404、严格分页及原回执。

web-verify-GRep8i完成六个原生生成物的重复字节核验，但新Drawer一处JSX闭合错误
使typecheck/build失败，浏览器未启动；修复后web-verify-It81tU全部通过：六个
原生生成物再次重复一致、手写源不变；typecheck、504项Vitest、5项Node、decimal/
bigint/fraction线协议、build及CLI help通过，36项Codex设置与全站183项三视口
浏览器检查通过（2.6分钟）。API语义只增加两个GET和五个schema，唯一已有schema
变化为CycleReadAction新增VIEW_SELECTION；无已有HTTP路径改动或删除。

随后仅增加常驻Worker断言，verify-sJFjr0捕获测试的UUID/领域Id类型混用，修正后
verify-uM2UGy check/fmt/严格Clippy及三项原生用例通过：真实Codex 0.144.4/App
Server原Thread反馈用例258.88秒，两个取消用例66.28秒。快照在测试手工ACK重放前
已由Worker自动形成，精确引用原试验/评估/指标，仍只有两次原Provider模型请求；
无Turn/未发送Turn取消保持零Provider请求，未为快照补开Thread。原消息已归档，
临时PG确认停止，源码不变。受控Provider/科学报告仍不是完整真实数值Job/OCI或真实账号T42。

此阶段未push、请求GitHub review、合并或关闭Issue。最终校准/Sealed/独立Reviewer/
资格、双Alpha共享资金组合交付、迁移/部署/恢复及T01–T42其余证据必须继续完成。

## 2026-09-12：冻结原生 SCORE 校准与原 Evaluation 原子发布

按DESIGN A4.4复用已经实际拟合的linregress0.5.4模型，不增加科学Run、队列、
模型轮次或依赖。原生每折输出训练标签的真实available纳秒；固定选择各资产最后
原生折，不以指标选赢家、不混入测试/Sealed标签再拟合。全部最后折均有可用SCORE
拟合才冻结；一项失败不回退早折，EXPECTED_RETURN不伪造校准。JSON模型只保留
原生系数、精确训练子集、资产/bar规格/horizon和原报告引用，不复制训练标签。
持久模型使用已锁定ndarray乘加应用原生仿射参数，并与真实RegressionModel.predict
实际比对；新预测须晚于整个模型训练截止。DB微秒向上取整，JSON保留原纳秒。

原Validation的SUCCEEDED + VALID发布事务先写全部指标及实验首次结论，再创建
EVALUATOR_ONLY校准MODEL和calibrations引用，由已有引用触发器封口Evaluation。
038迁移检查原Run/Attempt、数据来源、训练InputSet/cutoff、horizon及原生估计版本，
不回填历史已封口评估。失败回滚两份新产物的元数据和实验结论；Worker复用原Run锁
确认无引用后分别回收对象。重放不读取或再写模型，不加trial。科学REJECT保持REJECT，
原AlphaVersion仍不可变且未附加校准，本阶段不生成资格或Sealed读取能力。

验证使用e7699eca及冻结补丁，编辑/验证串行：

- verify-iWmise编译失败：ndarray0.17只读视图需要借用后做乘法；未启动PG。
- verify-tjWS5l原生28项通过，但Store六项捕获校准引用过早封口Evaluation，
  违反原实验首次结论守卫。只调整同事务写入顺序，不放宽不可变约束。
- verify-Wx6Ch2全通过；复核后将训练时间的DB边界由执行墙钟改为原冻结输入的
  decision_cutoff，并增加超界一微秒的触发器注入/完整回滚检查。
- 最终verify-LO0XsC：check、fmt、严格Clippy通过；原生28项
  （alpha_validation 9、validation 9、managed 7、science_cli 3）、Store119项、
  HTTP/CLI19项通过，0失败/忽略。包括第二文件失败、校准行失败、输入超界、并发
  唯一发布/重放、真实纳秒保留、不可改删及缺证据不产生模型。源码不变，PG确认停止。
- web-verify-WTmW94：六个允许的原生生成输出两次逐字节一致，手写源不变；
  typecheck、504项Vitest、5项Node、decimal/bigint/fraction、build及CLI help通过；
  Codex设置36项和全站183项三视口浏览器测试通过（2.7分钟）。语义差异仅domain
  新增两个冻结校准schema，domain/runtime的NativeValidationFoldV1新增训练截止；
  所有HTTP路径和server API schema不变，六个输出中仅两份JSON实际改变。
- 随后只改原Worker恢复测试，verify-cmw5AQ check/fmt/严格Clippy通过；真实Codex
  0.144.4/App Server原Thread用例221.34秒通过，两个取消用例66.92秒通过。校准行
  失败后对象目录精确恢复原集合，成功恢复在手工ACK前已有唯一校准，仍只有两次
  Provider请求；受限模型未进入反馈正文。源码不变，PG确认停止。
- owner-oci-QnSnJk重建原生Job/Runtime和镜像
  `sha256:60489424d37c17de87ddbaf2720037331cff2f099e53dc325c589f7e2d04277e`，
  实际job2.0.0-dev.1/rustc1.98.1，七项真实OCI编译/原任务恢复/取消/既有边界测试
  全通过（11.92秒），源码不变。没有替换生产服务或修改Docker socket权限。

原生OLS/Parquet/Wasmi数值测试使用明确合成行情；Store与App Server用例使用受控
科学报告/Provider，OCI用例不是完整市场Validation或Sealed测试，不能拼成真实账号
T42已过的声明。本阶段未push、请求review、合并或关闭Issue；须继续不可变校准
版本附加、Sealed预约/执行/披露、独立Reviewer/资格、组合交付及全部剩余合同。

## 2026-09-12：不可变校准版本与只读来源

按DESIGN A4.4在原Validation发布事务创建同Alpha的下一版本，附加原冻结校准；
039迁移约束精确源版本/原实验/血缘/CODE/Wasm MODEL/单位/horizon/镜像，并阻止
重复附加。原版本与评估不改写；新版本没有借来的Validation或Qualification。
活动指针只在RESEARCH且仍指原版本时推进，保留清空指针、SUSPENDED和RETIRED。
未产生可用校准不创建派生版本，科学REJECT仍为REJECT；原候选快照继续引用源版本。
复用原事务和队列，不增加拟合、Run、依赖或候选映射表。

一个只读GET及alpha calibration CLI、Ant Design来源Drawer返回校准元数据和源
版本原Validation，不读取模型/标签/索引字节。沿用Operator/精确项目CLI权限；
Mission不能借用。保留原决定/有效期、精确大整数和向上取整微秒时间；读取失败
不是空成功，源评估不是新版本评估。原生生成仅增加CalibrationView和一个GET，
已有API/schema和Runtime合同语义均不变。

以c69a085e及冻结补丁验证，编辑/验证串行：

- 首轮verify-vNqpB6有一项新测试失败：通用fixture签发一小时Mission凭据，超过
  真实Run期限。改用现有正式Mission签发入口，未放宽生产约束。
- 最终verify-4mGTyJ的check/fmt/严格Clippy及28项原生科学、122项真实Store、
  20项HTTP/原生CLI测试全部通过，0忽略，源码不变，临时PG确认停止。覆盖版本
  写入故障与两对象/评估/结论回滚、并发唯一发布、重放、原版本归属、选择不随
  活动指针变化，以及生命周期/人工指针保留。
- web-verify-QTvol7：六个允许原生输出重复字节一致，手写源不变；typecheck、
  504项Vitest、5项Node、204个decimal/1694个bigint/214个fraction用例、build与
  CLI help通过；36项Codex设置和全站186项三视口浏览器测试通过（2.7分钟）。
  实际变化为domain/API JSON、TypeScript及Ajv JavaScript四个生成文件。
- verify-0m2jrz的check/fmt/严格Clippy与真实Codex0.144.4/App Server原Thread
  用例通过（218.81秒）；两个取消用例通过（70.64秒）。故障改注入alpha_versions
  写入，原两对象精确回收；Worker恢复后已产生附加校准版本，反馈仍引用原评估，
  不披露校准模型，仍只有两次Provider请求。源码不变，临时PG确认停止。

原生科学数据与Provider仍为显式受控输入；这些测试不是完整真实账号、市场Job/OCI
或Sealed/T42证据。本阶段未push、请求review、合并或关闭Issue；继续Sealed预约/
实际执行/披露、独立Reviewer/资格、组合交付、迁移/恢复与全部剩余合同。
