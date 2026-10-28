# Derived-cohort economic replay

<a id="task"></a>
## Task

Task ID: hyperliquid-tick-economic-replay. Deliver an explicitly modeled, retained economic replay for admitted tick-factor candidates, preserving the complete source universe and period and the existing shared trial budget. Endpoint: reviewed code, applicable CI and PR delivery; no real orders. Local checks use one owned debug target after disk capacity became available.

<a id="spec"></a>
## Design

One initial USDT wealth is explicitly mapped to USDT0 and converted only after observed bridge data and modeled latency. A fixed split funds native multi-currency Cash and Margin ledgers once. Original instrument types and IDs remain; derived VWAP simulation bars are execution carriers, not original ticks, books or BAR data. Fixed-grid targets use the original retained Wasm features; spot negatives retain cash, perp targets may be signed. No new covariance assumptions or optimizer.

Actual funding-rate clocks drive a public simulation module using preceding observed cohort marks. This is a funding estimate, not an oracle-settlement claim. Native USD PnL is swept in nominal units to USDC; the result records its minor-unit rounding bound. Separate perp and spot taker fees, additional spread/impact, latency, seeded jitter and participation are explicit scenario inputs. One outstanding delayed decision per asset survives intermediate grids until a later eligible execution attempt. Derived wire decimals round once to 18 fractional places, midpoint nearest even; internal balances retain native precision. Missing valuation/funding remains visible, and open terminal positions are not hindsight-liquidated.

The new explicit HTTP replay path binds an admitted retained COMPUTED single factor and its original price dataset/model/native request; a separate immutable registered funding catalog may supply rates with the same source revision and covering window; it uses existing durable execution and remote admission. It neither admits arbitrary formulas nor relaxes BAR portfolio contracts or consumes a new factor trial.

<a id="verification"></a>
## Verification

TypeScript worker/API, idempotency/recovery and funding binding checks passed, including 12 independent exact-source tests and service/research/worker/CLI typechecks. Local scoped high-precision tests passed: 15 economic/native replay tests and one shared-fuel test. They exercise actual official Cash/Margin engines, same-clock rates, fees, message-bus isolation, daily purge, per-pair cash inventory, sparse delayed execution, absent primary-view assets, non-parity FX and zero/negative-score exits. The official engine requires a time-based bar label; the model uses a nominal 1ms external label at unchanged irregular cohort clocks without resampling. Repository formatting and diff checks pass. Applicable final-head CI remains required; these software regressions are not historical performance results.

<a id="review"></a>
## Review and delivery

Draft PR #254 is open. Exact-head CI/review remain pending. Review fixes cover zero-score exits, UTC daily partitions, global VM fuel, empty primary-view assets, non-parity FX output precision, and per-pair cash inventories with separate wallet FX reserves. No factor trial, backtest performance result or deployment is claimed. Native input readers are bounded; official open-position audit history can grow and remains subject to the existing worker resource limit.

## Funding boundary correction

Missing-rate expectations now use the native positions held at the nominal hourly boundary before cohort execution. A position first opened at 03:15 does not create an obligation for an absent 03:00 rate. Actual funding records retain their original timestamps and continue booking through the existing exact-time path; no rates or prices are synthesized. Controlled native replay regression cases cover a later-listed entry with an observed 04:00:00.091 rate, a held 04:00 position with a missing rate, and a position closed before 04:00. This correction does not diagnose or fill separate NAV valuation gaps.
