# Issue62 implementation evidence

DESIGN is the normative contract; this file records evidence, not a second design.

## Current changes

- Rename apps/job, crates/contracts, crates/domain and related package/build paths without qz- directories.
- Delete legacy backend/frontend/plugin/deployment implementations, obsolete tests and archived design; retain Git history, LICENSE/NOTICE and user-data boundaries. There is no legacy compatibility service.
- Replace PyO3 scientific probes with Rust-native Nautilus0.63.0, Clarabel0.11.1 and Arrow56.2.0. Upgrade Rust1.98.0 rather than using an old compiler as a Python exception.
- Fix mandatory nullable metric keys, INVALID_INPUT evidence classification, UUIDv7 schema, Fast=false omission, cancellation-after-real-failure and ISO currency membership. Preserve version/lease/terminal and precision tests.
- Complete previously missing design contracts for SelectionRule, immutable data authorization, native snapshot identity, logical feedback deduplication, operator rejection, candidate uniqueness, integration setup and machine credentials. These design additions do not claim their SQL/API implementation exists.

## Verification contexts (do not inherit results across Heads)

At `e8668ca850def834735414ed9ba94fed38d4aa7e`, the committed suites contain
8 contract, 28 domain and 8 native/report tests: **44 total**, not the earlier 35.
[CI 33962063262](https://github.com/zhengui666/QuaZonai/actions/runs/33962063262)
validated that exact dependency lock. The historical local 36 contract/domain
checks used a development lock and did not establish product-lock acceptance.

The subsequent mission-turn/bigint review-fix source has been tested locally on
Rust 1.98.0 **with the unchanged committed Cargo.lock**: 9 contract, 35 domain,
8 native/report tests, **52 total**, zero ignored/failed. Strict workspace Clippy
and `cargo fmt --all -- --check` pass. Both Rust parsing and generated schema
checks consume the shared corpora: 242 bigint and 204 decimal cases.

Mission turn reservations count used plus outstanding total and repair turns,
are bound to the exact Mission identity, and share the cycle token/cost guard.
A follow-up model turn does not reserve another experiment or job slot. Tests
cover disabled repairs, exact caps, overflow, missing/mismatched Mission usage,
used and in-flight turns, failed-reservation nonmutation and property cases.
These are pure rules, not yet a claim of persistent worker/lease admission.

The earlier native fixture run produced weights
0.7999999999997491 / 0.20000000000025078, 745 iterations, 12 native orders and
24 native events with Arrow round-trip. This is explicitly FIXTURE,
`deliverable=false`, `python_runtime=false`, not full business acceptance.
The upstream feasibility run 33952841460 is a development study, not a final
product check. Every new Head still needs independent read-only CI and review.

## Store integration rebased onto `b39771c`

This iteration preserves the exact numeric fixes already present at
`b39771c41f0300438023be5a2f9330c4d9db9d86` (native tree
`2683600c2cacc6c2353d7f61201f1c559232b62d`) and integrates the previously local
Store code. The source is not full product acceptance. Its eventual GitHub
commit and CI must be checked separately; local tests cannot certify remote runs.

SQLx0.8.6 migrations create 61 field-bearing domain tables and the per-turn
accounting view. The mature libraries own migrations, transactions, test databases,
queue delivery, numeric parsing and schema generation. First-party code owns QZ
relationships, permission boundaries, budgets and reconciliation decisions.

A Mission has one immutable Session/Thread binding. Each reservation freezes its
attempt, original epoch, settings revision, ordinal, inputs, tokens and estimated
cost. Reservation and native PGMQ send commit together. Dispatch intent grants
one send only; a replay reconciles without sending again. Native acknowledgement,
terminal observation and usage receipt are independent immutable facts, so a
completed turn with missing usage survives restart without fabricated zero spend.
Exact receipts precede queue acknowledgement. Taking over the same attempt checks
the new epoch; a new attempt cannot adopt an old reservation. Pause/cancellation
cannot erase actual usage; configured caps still block subsequent admissions.

The schema enforces exact native artifact identity, frozen input membership,
Qualification/Alpha/evaluation relationships, Release/candidate evaluations and
Offer/approval/release/downstream/environment tuples. A single-use human grant is
bound to one command target. These relational constraints do not replace the
remaining domain authorization or independent scientific qualification.

### Verification scope

The local build uses Rust1.98.0, PostgreSQL18.1 and PGMQ1.10.0. SQLx creates a
separate real database per test and applies the actual migrations. The disposable
local PostgreSQL configuration is reconstructed solely for these tests; it is
not a deployed production image or a claim of backup/restore acceptance.
The committed Cargo lock preserves all previous native dependency versions and
adds the Store dependency graph using Cargo. No manual checksum or resolver edits.

The checked suites define **87 tests**: 12 contracts, 40 domain, 8 native/report,
and 27 PostgreSQL tests (7 relational, 16 turn transactions, 4 terminal ledger).
The shared Node/Rust wire corpus contains 204 decimal and 242 bigint cases.
Each successful command must correspond to actual logs; a future Head must rerun.

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --workspace --all-targets
DATABASE_URL=postgres://TEST_USER@127.0.0.1:55432/postgres cargo test --locked --workspace
cargo run --locked -q -p contracts --example generate > /tmp/domain-v1.openapi.json
diff -u contracts/generated/domain-v1.openapi.json /tmp/domain-v1.openapi.json
node tests/contracts/decimal-wire.mjs
node tests/contracts/bigint-wire.mjs
```

The lost-memory test recreates a Store client after dispatch; it is not an OS-kill
or real model-inference test. The lease test performs a real database lock wait
and reads the clock afterward. The missing-queue test causes a native SQL error
and checks atomic rollback. Terminal tests preserve reservations across a client
restart, reject unproven refunds and unrelated/contradictory native identities.
Test-only SQL fixtures are not a fresh-instance Web/CLI product workflow.

A mandatory `store-postgres` CI job now covers these migrations and tests; the
foundation aggregate requires its success. This workflow must run against the
published Head. No green result from the earlier pre-Store commit applies to it.

## Browser authentication vertical (2026-09-06)

The first actual HTTP entrypoint now lives in `apps/server`. It reuses Axum0.8.9,
tower-sessions0.14 with **only PostgreSQL persistence**, PostgresStore0.15,
totp-rs5.7, Argon2id and RustCrypto XChaCha20Poly1305. A native `cap-std` directory
and private key protect enrolled TOTP material. There is no memory-session fallback.
`init-state`, native `migrate`, one-use local `bootstrap`, `serve` and native OpenAPI
are real Clap commands. None provides research/approval authority to an Agent.

Initialization requires a bounded local capability and the same private browser
session; the global database singleton makes concurrent binding mutually exclusive.
Normal login uses six-digit TOTP only. A fresh six-digit code is required for
sensitive device revocation. PostgreSQL commits replay steps, login authority,
fixed expiry, epoch and revocation independently of the cookie transport; an old
or concurrently saved native session cannot resurrect a revoked login. Rate limits
are atomic database counters, not process-local counts. Argon2 work is concurrency
bounded. Secret/verifier/credential values are absent from response errors and logs.

### New verification context

The selective locked local command below passed **97 tests**: 12 contracts,
40 domain, 3 native authentication/vault integrations, 35 actual PostgreSQL Store
tests and 7 server tests. Six of the server tests use real independent PostgreSQL
databases; one additionally sends real HTTP over a loopback TCP listener using
a distinct non-owner PostgreSQL role. That role can execute the authentication
flow but cannot alter or truncate domain tables. Native crypto is not mocked.
The HTTP tests also cover missing/wrong origin, host mismatch, malformed JSON,
cookie tampering, replay, rate limiting, logout and device revocation. Device
lists paginate rather than hiding revocable records after the first 100.

```sh
DATABASE_URL=postgres://TEST_USER@127.0.0.1:55432/postgres \
  cargo test --locked -p contracts -p domain -p integrations -p store -p server
cargo run --locked -q -p server -- openapi > /tmp/api-v2.openapi.json
diff -u contracts/generated/api-v2.openapi.json /tmp/api-v2.openapi.json
```

The existing eight scientific/native-report tests were **not rerun by that selective
command**; they retain their separate baseline/CI evidence. Do not add them to the
new local pass count or claim full-system acceptance. The committed workflow now
requires real Store **and HTTP** tests with PostgreSQL and compares both generated
contracts without rewriting tracked output. The exact new GitHub Head and run must
be checked after publication; preceding CI results do not validate these changes.

## Explicit gaps

The relational schema and per-turn Store are implemented, but complete Run
admission/takeover, research services and machine authorization, the remaining
API/Worker/CLI/MCP, native
same-Thread model/tool/job/evidence cycle, independent Reviewer, PIT/sealed
isolation, multi-Alpha shared-capital portfolio, target-only approval/feedback/
wake services, Ant Design UI/PWA, data migration, backup/restore and protected
real-account acceptance remain incomplete. Existing JSON checks verify structure
and versions, not a complete policy or authorization decision. Non-owner database
role authentication is exercised locally; production TLS, isolated deployment,
role-specific research permissions and deployed user-data migration still require validation.
Removing legacy tests is not acceptance. Do not merge until every W0–W8/T01–T42
requirement and the latest-Head CI/review conditions are met.
