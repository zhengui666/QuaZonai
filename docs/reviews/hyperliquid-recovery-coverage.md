# Hyperliquid recovery coverage: fixed-version evidence and CI test design

Status: source inspection and official documentation checked; no runtime test performed. No user account, credential, native process or upstream source was touched.

Baseline QuaZonai: 18ed0462b1ad64f5a2525de0252bb84514657896.
Nautilus revision: 634e09456c949f488b2bfcf6b6897b33cbf218d7.
Official reference: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint

## Evidence, not a claim of an observed trading incident

The venue documents userFills as the most recent 2,000 fills; userFillsByTime returns up to 2,000 per response and only the latest 10,000 remain available. historicalOrders returns the most recent 2,000 orders. Queries require the actual master/subaccount address, not an API signer address.

The fixed adapter already handles saturation during initial mass status: crates/adapters/hyperliquid/src/execution.rs:2331-2451 fetches orders before fills, tests lengths against HYPERLIQUID_RECENT_HISTORY_LIMIT (2,000), and marks reports_complete false. live/src/execution/manager.rs:1236 requires complete reports before recovery of closing fills. It is incorrect to report that all capped responses are blindly treated as complete.

The WebSocket reconnect path differs: execution.rs:3256 fetch_reconnect_reports calls read_history (3323), then reconnect_reports_from_history (3339). Those functions use one recent-orders response and one recent-fills response, filter by recovery start time, and return Vec<ExecutionReport> rather than a coverage flag. The follow-up branch at 3212-3224 clears its matching recovery window when report conversion succeeds. No cap check or userFillsByTime pagination was found in this path. A sufficiently active outage can therefore exceed the fetched history; complete gap recovery is not established by this source path. Runtime consequences and other native reconciliation mitigations have not been tested.

Queries do use the configured account override: QZ apps/job/src/account_runtime.rs passes config.account_address to the native execution client; native http/client.rs:1374 prioritizes this override over vault/signer fallback. This does not prove that a user's supplied address is correct. QZ currently rejects configured vault operation.

## QZ meaning of recovery

QZ RECOVERY_STATUS ready checks native-cache failure, not venue-history coverage. The UI already explicitly says a completed RECOVER receipt does not certify order reconciliation or permission to trade. RECOVER completes only its own command receipt, and remote successor bindings do not rewrite older account/Pi requests. No code was found that clears all UNKNOWN merely because a new session was accepted.

A possible minimal disclosure enhancement is an explicit historyCoverage: UNPROVEN observation field. It must not be inferred from ready/connected, used to synthesize balances, or presented as repair of the native reconnect path. No such change is included here.

## Implemented now (not yet executed)

`apps/job/tests/hyperliquid_recovery_coverage.rs` calls the public adapter method with 0, 1,999 and 2,000 historical-order rows and empty fills/positions/open orders. At this fixed version, empty fills bypass historical order conversion, isolating the order-history saturation flag without requiring instrument fixtures. The test asserts complete below the cap and incomplete at the cap, and verifies the explicit fictitious account address on account-scoped requests.

The subprocess uses `env_clear` before constructing the adapter, so upstream environment fallback cannot read user credentials. All three endpoint overrides point to the loopback fixture. The server accepts only POST /info and five read methods; an unexpected path/method panics and fails the test. This is not a separate exchange-request counter. No private helper is copied, no live node is started, and no WebSocket connection is made.

This implementation does NOT cover the fill-history cap, instrument parsing completeness, actual reconnect lifecycle, or arbitrary-history recovery. The larger cases below remain a design, not test results. Only source formatting and static inspection have been performed locally; GitHub CI must compile and execute the test.

## CI-only public API integration design

Use a separate integration test in the QZ job crate, with account-runtime enabled, importing the fixed official HyperliquidExecutionClient, ExecutionClientCore and ExecutionClient trait. Construct an in-memory Cache and explicitly supply a deterministic, test-only private key and fictitious account address. Never consult the process environment for keys. The constructor requires credentials even though the test makes no exchange action; the test key must be described as a public fixture, not a user credential.

Set all three public endpoint overrides: base_url_http, base_url_ws and base_url_exchange. Bind fixtures only to 127.0.0.1 ephemeral ports. Exchange endpoint must fail and increment a counter on every request; assert zero exchange requests. No node.run, strategy or order submission is needed for the first stage. Instrument definitions enter through public on_instrument, not private caches. Use the real public generate_mass_status method and assert its actual ExecutionMassStatus result.

Required first-stage cases:
1. Fully resolved, uncapped native fixtures: reports_complete true.
2. Exactly 2,000 historical orders with otherwise complete fixtures: reports_complete false.
3. Exactly 2,000 fills: reports_complete false. Do not imply fewer fills always prove completeness if parsing/DEX coverage is incomplete.
4. Query-address fixture: every account-scoped Info request contains the explicitly supplied account address, distinct from the test signer. No private address is queried.
5. Exchange request counter remains zero; unexpected Info methods fail the test instead of forwarding upstream.

Fixture preparation must supply all required native instrument and state schemas. Empty/unknown instrument fixtures cannot test the saturation distinction, because independent parsing incompleteness would mask the result. A test that merely reimplements the length comparison in QZ is not valid evidence.

Second-stage reconnect requires an actual loopback WebSocket server that implements subscriptions, confirmations, disconnect/resume and event ordering, plus the native event sender. Public connect/start lifecycle may perform bootstrap account queries; every endpoint must remain overridden and reject outbound actions. Simulate a gap exceeding 2,000 distinct events and record native public execution events. Assert what was actually emitted and whether public completeness evidence exists; do not assert nonexistent coverage APIs. Also test repeated timestamps and duplicate fill IDs, without implementing a replacement fill engine.

The private reconnect_reports_from_history helper is not callable from an external QZ integration test. Copying it or modifying upstream visibility would violate the test's purpose. The public WebSocket lifecycle fixture is therefore materially larger than the startup mass-status test and remains unimplemented. Existing upstream small-fixture reconnect ordering tests are not a long-gap coverage test.

Run compilation and tests only in GitHub CI. Do not run this design against real mainnet/testnet endpoints, restart a user's process, or replay any original request.
