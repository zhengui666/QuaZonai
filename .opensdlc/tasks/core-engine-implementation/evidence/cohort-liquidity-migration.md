# Native cohort liquidity across Nautilus 0.63 → 0.67

The coordinated release library run initially reported 310 passing and 13 failing tests. Two failures were in the derived-cohort economic adapter: expected BTC quantities 0.5 and 0.2 became 0.125 and 0.05. These are actual native underfills, not quantity-unit conversion errors. The original log is `/tmp/qz-release-lib-tests2.log`.

## Upstream cause and execution order

The official `nautilus-execution` source was read from the installed, unmodified crates.io packages:

| Package | Upstream VCS revision | `src/matching_engine/mod.rs` SHA-256 |
| --- | --- | --- |
| 0.63.0 | `a0400251110653b6d8ae6a9b5b89c4543fa85a2d` | `255ef5dff96d7bd4f390e8d9c1cdbfda0daec0630981bcf2d66f5981578e030a` |
| 0.67.0 | `d53849b216e20f2c122ca01b4dd0da377638b186` | `8d4aa603d25381c2e584ba0c6006e34fd18ba9da4eb43bff0eb48802e6a09541` |

- [0.63.0 matching engine](https://docs.rs/crate/nautilus-execution/0.63.0/source/src/matching_engine/mod.rs), `process_trade_ticks_from_bar` around lines 1785–1873: equal-price legs were skipped. In these tests, an earlier BAR with volume 10 had established more synthetic book liquidity; an unchanged later mark did not refresh that book to the smaller current volume.
- [0.67.0 matching engine](https://docs.rs/crate/nautilus-execution/0.67.0/source/src/matching_engine/mod.rs), `process_trade_ticks_from_bar` around lines 2072–2138: a positive-sized leg is processed even when its price is unchanged. When nothing is resting, the implementation may collapse replacements, but it still installs the last positive-sized synthetic tick.
- The same 0.67.0 file, `BarTickSizes::from_volume` around lines 8487–8540, distributes the rounded native size increments over OHLC. Volumes divisible into four equal increment counts leave one quarter at the close. Small or indivisible volumes have explicit remainder handling; **one quarter is not a universal formula**.
- [0.67.0 backtest engine](https://docs.rs/crate/nautilus-backtest/0.67.0/source/src/engine.rs), lines 923–940: exchange routing processes the BAR before `DataEngine::process_data_ref` publishes it to the strategy. The cohort adapter submits IOC orders in `on_bar`, after those synthetic legs.

Both failing fixtures use OHLC = 100 and a size increment of 0.0001. Current volumes 0.5 and 0.2 divide exactly, so the new native closing liquidity is respectively 0.125 and 0.05. A participation cap is an upper bound on the submitted order; it does not guarantee that quantity fills. The migration changes actual modeled exposure and therefore may change modeled returns, fees, and residual holdings. Existing results with old native-version receipts retain their old interpretation.

## Retained behavior and stronger checks

Production capacities, prices, order sizes, IOC handling, liquidity consumption, and native matching are unchanged. No volume multiplier, substitute fill, balance repair, or old-engine fallback is introduced. The economic result's execution-method text now discloses native OHLC size allocation and same-price synthetic-book refresh without claiming observed market depth.

The native cash regression retains its original 0.5 capacity and independently calculates:

```text
submitted quantity = min(target difference 1, participation capacity 0.5) = 0.5 BTC
native closing quantity = 0.5 / 4 = 0.125 BTC
gross cash spent = 0.125 × 100 = 12.5 USDC
commission = 12.5 × 100 / 10,000 = 0.125 USDC
remaining cash = 1,000 − 12.5 − 0.125 = 987.375 USDC
```

It checks the original native order's requested quantity, filled quantity and canceled IOC remainder, the actual `OrderFilled` quantity/price/commission, inventory, cash, and summed commission. The test-only accessor returns owned snapshots from the official native cache; it does not synthesize events.

A separate small-capital regression preserves the **original 0.2 capacity** and verifies that a 0.3 target becomes a 0.2 submitted order and a 0.05 actual fill. The resulting 5-USDC inventory is below the fixture's 10-USDC minimum order amount. A later exit attempt leaves that real holding visible, with no fabricated closure or top-up.

The existing exit-retry scenario supplies explicit fixture capacities 0.8 and 0.4, so native closing liquidity is 0.2 for entry and 0.1 for the first exit. It checks real IOC underfills (0.3 submitted → 0.2 filled, then 0.2 submitted → 0.1 filled), and retains the original assertions for no entry top-up, stale-NAV exit scheduling, 0.1 dust at price 99, subsequent exit at price 101, and the original exit-delay evidence. This preserves that scenario's purpose while the separate test prevents the original lower-liquidity behavior from disappearing.

## Verification status

Formatting and `git diff --check` passed. The coordinated release run in `/tmp/qz-final-native-tests2.log` reports **325 job library tests passed, 0 failed**, including `ticks::economic::native::tests::native_cash_buy_moves_inventory_caps_participation_and_deducts_fee_once` and both `ticks::economic::replay_tests::small_policy_*` regressions. No extra Cargo build was started by this implementer for these cases. That invocation also contained separate integration-test failures; the library result is not a claim that the entire command or final-Head acceptance passed. The coordinator tracks their resolution and full delivery validation separately. These controlled software checks are not historical liquidity or profitable-capacity evidence.

## Polymarket resting remainder and original close

Expanded integration validation also found a separate 0.67 improvement in the
same official matching engine: `slipped_price` only returns a remaining L1 fill
price when `is_price_tradable` accepts its positive value and original instrument
bounds. See the matching engine around lines 5777–5814 and 5871–5879, plus
[the official fill model](https://docs.rs/crate/nautilus-execution/0.67.0/source/src/models/fill.rs)
around lines 274–278.

The retained GTC regression has an original maximum of 0.99, a 0.01 tick and an
eight-share BAR. The first ordinary fill remains two shares at 0.99. The next
slipped price of 1.00 is outside the original bound, so the remaining order does
not receive that fill and is canceled at the configured expiration. The original
settlement receipt later redeems only actual holdings at 1/0, with zero settlement
commission. This is a price-bound protection, not a guarantee that every resting
L1 remainder is capped by original BAR volume. Separate positive, aligned BAR
prices below/above the original bounds retain the exact catalog rejection.

Explicit early closes also mark native expiration processing complete. In 0.67,
that check precedes the market-status check: a later order is rejected as
`Contract {id} has expired`, even before the unchanged configured expiry. The
regression retains the original close, receipt clock and instrument expiry;
the native reason does not invent a new expiry or authorize another order.
