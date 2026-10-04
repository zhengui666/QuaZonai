# Native account observations

## Scope

Read-only Paper/Live account observations from an already authorized downstream identity. Nautilus 0.63.0 remains the sole accounting/valuation engine. Account totals have no release, handoff, evaluation or Alpha ownership. Preserve currencies separately, precise decimal amounts and nanosecond strings, native stale/unpriced flags, source/node session, received time and durable sequence/drop gaps.

The source creates its binding from trusted configuration. Existing Forward downstream credentials, project authorization and allowed environments govern intake. No exchange credential, order, execution connection or live node is created by this change. A Paper fixture is not production account evidence.

## Implementation

- `crates/contracts/src/account_observation.rs`: strict field projection and source/current/history contracts
- `crates/domain/src/account_observation.rs`: structural validation and separate connection/valuation status
- `crates/store/src/account_observation.rs`: immutable source binding and append records; durable cursor/latest pointer in the same transaction; unchanged envelope replay returns the original receipt
- `apps/job/src/account_observer.rs`: public native `Portfolio::build_snapshot`, `PortfolioSnapshot`, `Money::as_decimal`, `TypedHandler<PortfolioSnapshot>` adapter, bounded nonblocking queue and drop counter
- `apps/job/src/account_observation_cli.rs`: `job native-account-observation` converts bounded official native snapshot JSON, once or into a retained NDJSON segment; no extra installed executable, network or credentials; connection remains unknown
- `apps/server/src/account_observation.rs`: authenticated intake, project source list, current snapshot and paginated immutable history
- CLI: `quazonai client forward accounts submit|sources|current|history`

`POST /api/v2/forward/account-observations` receives one unchanged envelope. Reads use `/api/v2/projects/{project_id}/account-sources`, then `/{source_id}/current` or `/{source_id}/observations` with existing UUID cursor/limit pagination. Foreign/expired history cursors fail explicitly. History includes heartbeats, which have no valuation; `latest_snapshot_id` remains unchanged by a heartbeat or replay. Older native valuations remain in history but cannot replace a newer current snapshot.

Connection freshness is derived from the source heartbeat clock with a documented 120-second boundary, not from market snapshot arrival. Native valuation flags and their own timestamps remain separate. Empty native PnL arrays remain empty. Unpriced snapshots preserve native partial values for audit and return `UNPRICED`; callers must not display these as complete headline equity. The source's `has_gap` stays true for the observed session once a sequence/drop gap is detected. No historical interpolation or backfill is invented.

## Validation and remaining integration

Earlier branch-local work ran four domain tests. That evidence does not validate this dev integration. The current slice is based on the PR135 descendant of dev1a7, keeps the shared source-tools executable, adds no relay/producer/owner-Agent/UI paths, and has not yet compiled or executed. Native export, regenerated contracts and exact-head evaluation are pending.

Required next on the integration machine (existing unique Cargo scheduler):

1. Generate native domain/API schemas and the frontend client together with other contract changes; no generated files are edited by hand.
2. Run `cargo test --locked -p job --test account_observer`. The first test calls official `Portfolio::build_snapshot` on a native multi-currency cash account with USD and USDT. A separate controlled native snapshot tests stale/unpriced transport flags. These are fixtures, not an exchange connection or filled-position PnL acceptance.
3. Run `cargo test --locked -p store --test account_observations` against disposable PostgreSQL 18/PGMQ 1.10.0. Cover concurrent replay, immutable conflict, atomic cursor, heartbeat-only/older valuation/session restart, foreign cursor, wrong project, forbidden identity/environment and rollback.
4. Build server, the portable CLI, the existing `job` executable and `native_account_snapshots` example; explicitly run the ignored `native_portfolio_submit_preserves_values_and_original_receipts` HTTP test with `QUAZONAI_NATIVE_CLI_BIN`, `QUAZONAI_NATIVE_JOB_BIN` and `QUAZONAI_NATIVE_ACCOUNT_FIXTURE_BIN` set to their actual Cargo outputs. Send the native-generated envelope through authenticated HTTP intake, read it back from PostgreSQL via current/history GET and CLI, and compare native money/timestamps/flags. Repeat the exact envelope and verify unchanged receipt/current freshness; restart the reader and resume the original durable cursor.
5. Run applicable architecture, native/store/http/CLI checks and independent exact-head review. This candidate does not include a completed HTTP/CLI execution result.

The observer queue is deliberately not a durable producer spool. The downstream transport consumer must persist/retry the exact envelope; if its cursor is lost, begin a new source session and disclose the break. The server is the durable read side. A native history ring or Redis current stream cannot recover missing account history automatically. No SSE, source settings UI, native open-position read model, LiveNode/Sandbox host, exchange login or live connection is claimed in this slice.

This scope uses the existing typed JSON and transaction equality; it adds no SHA verification or package/tree/binary digest gate. Temporary test resources and original-source clock semantics remain explicit. Complete all generated domain/API/client changes from the native generators before publication.

The dev integration removes the historical per-vector 256/4096 ceilings and exact native-version rejection. The normal transport body size and typed monetary/time semantics remain; version text is recorded as declared provenance. A regression preserves larger compatible snapshots without adding a replacement count gate.

Source observed_at wall time is retained as evidence, not used as the delivery cursor. Future source time is marked stale by the current read model; advancing sequence/drop cursors accept wall-clock rollback and preserve unchanged replay receipts. Native valuation time ordering remains independent.
