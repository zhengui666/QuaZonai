# Polymarket execution format contract

## Scope

Add one offline Job integration-test target against the existing official
`nautilus-polymarket = 0.63.0` dependency. The tests construct native in-memory
orders and call `PolymarketOrderBuilder::validate_market_order` and
`validate_limit_order` directly. No signer, execution client, credential,
transport, submission, production model or Paper execution path is added or
changed. No dependency or feature change is needed.

The 13 tests cover explicit market BUY quote amounts, market SELL shares,
unsupported denominations, MARKET GTC rejection, native construction's MARKET
GTD rejection, reduce-only rejection, LIMIT share/quote semantics and post-only
time-in-force. Every adapter validation checks that the entire serialized order
remains unchanged. The Paper case reproduces only the current OrderFactory
argument shape; it does not execute TargetReplay or claim end-to-end coverage.

Format acceptance does not establish live market/instrument validity, tick or
expiry compatibility, balances, inventory, credentials, signing, submission,
fills or reconciliation. Account-state ownership and target-to-order policy
remain separate architecture decisions. Existing Paper-only admission stays in
place; quantities, time-in-force and reduce-only are never silently converted.

## Validation

The integrated V5 source passed all 13 tests on 2026-10-07 UTC, with zero
failures or ignored cases, using:

`cargo test --locked --offline -p job --test polymarket_execution_contract -- --test-threads=1`

The target is included in the existing routine Rust rule-test script. This
local result does not establish remote CI or production acceptance.

The pinned official API was inspected at revision
`a0400251110653b6d8ae6a9b5b89c4543fa85a2d`. No official source implementation is
copied, patched or forked. This test-only change is not a Live host or production
trading acceptance.
