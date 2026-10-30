# Polymarket execution format contract

## Scope

Maintain the offline Job integration-test target against the current official
`nautilus-polymarket = 0.67.0` dependency. The tests construct native in-memory
orders and call `PolymarketOrderBuilder::validate_market_order` and
`validate_limit_order` directly. No signer, execution client, credential,
transport, submission, production model or Paper execution path is added or
changed. No dependency or feature change is needed.

The 15 tests cover explicit market BUY quote amounts, market SELL shares,
unsupported denominations, MARKET GTC rejection, native construction's MARKET
GTD rejection, reduce-only rejection, LIMIT share/quote semantics and post-only
time-in-force. Every adapter validation checks that the entire serialized order
remains unchanged. The Paper case reproduces only the current OrderFactory
argument shape; it does not execute TargetReplay or claim end-to-end coverage.

In 0.67, LIMIT BUY accepts either pUSD amounts (`quote_quantity=true`) or
outcome shares; LIMIT SELL still requires shares. The supported LIMIT time-in-force
shapes are GTC, GTD, IOC and FOK. Post-only remains restricted to GTC/GTD.
MARKET BUY still requires pUSD and MARKET SELL requires shares, with IOC/FOK.
Unsupported SELL quote quantities and immediate post-only orders now return the
official structured `UnsupportedQuoteQuantity` and `UnsupportedPostOnly` reasons,
rendered as `UNSUPPORTED_QUOTE_QUANTITY` and `UNSUPPORTED_POST_ONLY`. Tests compare
these official reason values instead of obsolete diagnostic wording.

The format preflight target also covers the same supported LIMIT denomination
matrix while retaining the original quantity, price, exact nanosecond expiry,
time-in-force and flags. Native quantities that would truncate to zero in the
official signing path are rejected at the real adapter layer, including an
immediate BUY whose nonzero share amount produces a zero signed pUSD amount.
The production preflight continues to delegate to the two public official
validators; it adds no conversion, old-version rejection or replacement error.

Format acceptance does not establish live market/instrument validity, tick or
expiry compatibility, balances, inventory, credentials, signing, submission,
fills or reconciliation. Account-state ownership and target-to-order policy
remain separate architecture decisions. Existing Paper-only admission stays in
place; quantities, time-in-force and reduce-only are never silently converted.

## Validation

The 0.67 migration passed both targets locally on 2026-10-30 UTC in standard
and high precision release builds. Each build passed all 15 direct contract
tests and all 20 format preflight tests, with zero failed, ignored or filtered
cases. These results cover the two migration targets, not remote CI or live
execution acceptance.

High precision:

```sh
cargo test --locked --release -p job --all-features --no-fail-fast --test polymarket_execution_preflight --test polymarket_execution_contract -- --test-threads=1
```

Standard precision:

```sh
cargo test --locked --release -p job --features job/native-paper-test,job/native-sandbox-test,job/catalog-prepare,job/polymarket-history,job/hyperliquid-offline --no-fail-fast --test polymarket_execution_preflight --test polymarket_execution_contract -- --test-threads=1
```

Local run logs: `/tmp/qz-final-preflight-high.log` and
`/tmp/qz-final-preflight-standard.log`.

Official 0.67 source inspected at release revision
`d53849b216e20f2c122ca01b4dd0da377638b186`:

- [Public LIMIT/MARKET validators](https://github.com/nautechsystems/nautilus_trader/blob/d53849b216e20f2c122ca01b4dd0da377638b186/crates/adapters/polymarket/src/execution/order_builder.rs#L230).
- [Official collateral BUY submission branch](https://github.com/nautechsystems/nautilus_trader/blob/d53849b216e20f2c122ca01b4dd0da377638b186/crates/adapters/polymarket/src/execution/submitter.rs#L505).
- [Structured denial constructors](https://github.com/nautechsystems/nautilus_trader/blob/d53849b216e20f2c122ca01b4dd0da377638b186/crates/model/src/events/order/denied_reason.rs#L444).

## Historical validation on 0.63

The integrated V5 source passed all 13 tests on 2026-10-07 UTC, with zero
failures or ignored cases, using:

`cargo test --locked --offline -p job --test polymarket_execution_contract -- --test-threads=1`

This local result predates the 0.67 upgrade and does not establish current
remote CI or production acceptance. The current routine science target runs
`polymarket_execution_preflight`; the direct official contract target is run
separately during migration validation.

The then-pinned official API was inspected at revision
`a0400251110653b6d8ae6a9b5b89c4543fa85a2d`. No official source implementation is
copied, patched or forked. This test-only change is not a Live host or production
trading acceptance.
