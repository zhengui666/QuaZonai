# Native client bound account observations

## Scope and baseline

Slice A adds an opt-in client-bound observation format to the existing read-only
native observer and authenticated observation intake. The independent working
copy starts from the frozen V5 source archive. It does not include the separate
preflight or Forward-owner candidates, and never replaces their trees/schemas.

The added observer constructs or connects no exchange client. It adds no
credential handling, order submission, QZ risk engine, account ledger, valuation
model, account recovery algorithm or Live release. Native Account, Portfolio, Cache and ExecutionEngine remain the
authorities for native values and state. This change retains a provenance
association that the old QZ observation format omitted.

## Native producer

`NativeNodeObserver::attach_client_bound` selects an already registered official
execution client. It derives account identity from that actual client and
trader/session/environment from the actual node. The source version is the
compiled official `nautilus_core::consts::NAUTILUS_VERSION_CORE`; a caller does
not supply a replacement native version or account/session tuple.

The node must be idle and the selected client must exist before any segment is
created. The registered client and the actual client's identifier must agree.
The existing native account observer, native snapshot projection, sequence/drop
tracking, callback subscription and bounded retention queue are reused. V2 wraps
the unchanged V1 native observation with an exact schema version 2 and the
checked native client identity. Heartbeat connection evidence comes from the
same client/node; identity mismatch yields Unknown rather than connected.

The old `attach`, V1 stdin converter, CLI and Paper producer continue emitting
V1. There is no command that upgrades arbitrary binding JSON or old V1 frames
into client-bound evidence. A V2 producer may still produce Paper observations;
that does not make them Live-eligible or authenticate a venue account.

## Intake and immutable association

`POST /api/v2/forward/client-account-observations` accepts the strict V2 wrapper.
It reuses the existing authorized Downstream principal, ForwardSubmit scope,
project and environment checks. The service trusts that configured producer;
a body string does not attest remote process identity or prove a venue account.
The same current values and clocks still pass the original V1 structural rules.

The additive migration retains `native_client_id` on the original immutable
native-account source. Old sources stay NULL. The original source tuple cannot
switch from NULL to a client, from one client to another, or from V2 back to V1.
The client comparison happens before replay lookup. A new output file does not
alter a source identity; a genuinely new native session is a separate source.

Observation rows also retain the envelope protocol version. This is envelope
provenance, not a duplicated native account value. A database INSERT trigger
requires the protocol to agree with the source's original client binding. An
old server writer defaults to V1 and therefore cannot append unbound frames to a
V2 source after application rollback. The existing immutable-source trigger
prevents backfilling guessed client metadata into old rows.

Equal V2 replay returns the original receipt and native values. Changed client,
content or protocol conflicts. Existing V1 reads still project exactly the old
source, observation and valuation response shapes. The additive owner read
`GET /api/v2/projects/{project_id}/account-sources/{source_id}/client-binding`
returns the source/client association, or explicit unavailability for a legacy
source. It reuses existing owner/CLI ResearchRead authorization; no Downstream
read privilege, new scope or credential is granted.

The migration number 202610070001 was checked against V5 and the concurrent
Forward candidate, which contains no migrations. It is forward-only and does not
modify any already applied migration. Deploy migration before the new endpoints;
retain all records on rollback. V1 NULL protocol-1 rows remain compatible.

The existing Forward 103-to-104 upgrade regression still starts at exactly 103
and uses the application's native `Store::migrate`. With this additive migration,
its final-version expectation follows the embedded native migrator's latest
version and separately requires successful application of 104. All original
receipt, replay and protected-limit bypass assertions and CI execution remain.

## Tests and evidence limits

The focused checks cover:

- Domain V2 version/required-field/unknown-field checks, original native value
  preservation and unchanged account/time binding rules
- Official Sandbox node, no strategy or orders: actual client/account/trader/
  session/version produces V2; missing client and invalid queue reject before
  creating a segment; original snapshot money, event and clocks are unchanged
- The existing V1 Sandbox/Paper and converter tests remain unchanged regressions
- Disposable PostgreSQL: exact/concurrent replay, client conflicts before replay,
  V1-to-V2 and V2-to-V1 rejection, no old-row metadata backfill, protocol races,
  heartbeat behavior and database rejection of old-writer downgrade attempts
- HTTP/SQL: unchanged native body, receipts and old owner reads; cross-project/
  environment restrictions; no new Downstream read rights; legacy-unavailable
  client metadata remains explicit
- An explicitly invoked real native-to-HTTP test runs the compiled
  `native_client_account_snapshots` example with the official Sandbox client,
  retains native V2 NDJSON, submits it through actual Axum/SQL and verifies exact
  readback/replay. The example contains no strategy, venue network or order.

The example is test-only and requires `native-sandbox-test`. It is not installed
as a production command or offered as a Live account connection. Hand-authored
SQL/HTTP fixtures are labeled controlled fixtures, not native venue evidence.
No passed test makes a source qualified for Live delivery or bypasses later
native pending/open-order funds validation.

The end-to-end fixture process runs with an empty environment in its disposable
output directory, so it cannot inherit user venue credentials or a default
working-directory dotenv file. Before Cargo or a database connection, the runner
requires the existing CI sentinel `QZ_TEST_PG_CONTAINER=store-database` and the
explicit disposable service URL shape
`postgres://postgres:*@127.0.0.1:55432/postgres`. Missing or different values fail
closed. CI supplies only its existing isolated PG18/PGMQ service. It adds eight
exact nonzero native/Store/HTTP executions to that
job without replacing V5's existing checks. The ignored pipeline is explicitly
selected with --ignored; listing, compilation and a default skipped case do not
count as acceptance.

## Actual independent-candidate validation (2026-10-07)

- `cargo check --locked --offline --workspace --all-targets --all-features`
  passed. Existing unrelated warnings remain; no new warning suppression was added
- 28 selected Rust tests passed with zero failures/ignored: one native OpenAPI
  closure/version/reference test, seven account domain tests, two official
  Sandbox observer tests, two original V1 converter tests, three original native
  projection tests and 13 existing official Polymarket format tests
- The no-order example was compiled and actually run with an empty environment
  in a disposable directory. It retained five V2 frames, including two original
  official Sandbox portfolio snapshots. This is native producer-process evidence,
  not venue account or PostgreSQL/HTTP pipeline acceptance
- Native domain and API schema exports match their retained generator stdout.
  Domain gained only four schemas; API gained those four and two routes. All old
  schema definitions and API paths remained byte-equivalent JSON values
- Ten JSON Schema positive/negative checks and ten generated response-validator
  checks passed, including exact version, closed fields, V1/V2 separation and no
  invented venue-authentication or authorization-lease fields
- The existing locked Web generators completed and produced the client/types and
  474 response-contract files. An interrupted first attempt is retained separately
  from the successful complete retry; generated files were not hand edited
- Store account-observation, Forward-upgrade and Server native-Codex HTTP targets
  compiled/linked with `--no-run`. Each of the six selected database cases was
  listed exactly once from the compiled executables, with zero actual executions
- Five runner-isolation rejection checks passed before Cargo or a connection.
  The eight CI source selectors exist exactly once. That is static runner
  validation, not execution of the eight-case database plan

PostgreSQL/PGMQ is unavailable in this cloud workspace. No SQLx test, HTTP/SQL
pipeline, migration execution, live account connection or real venue order was performed. The preserved V1
Sandbox regression does execute simulated orders and fills.
The eight-case runner still requires actual execution on the disposable CI
service, including its explicitly selected ignored native-to-HTTP case. Old V1
and new V2 database behavior is compiled, not runtime-accepted here.

The targeted generated TypeScript consistency check completed successfully after
explicit authorization; its earlier interrupted attempt remains separate from
the successful complete retry. No routine UI, browser, container or broad
frontend test suite is claimed. The final independent frozen-diff review passed
with no remaining P1/P2 findings; this is candidate review, not merge or runtime
account/SQL acceptance.
