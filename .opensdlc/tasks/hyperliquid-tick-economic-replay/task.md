# Derived-cohort economic replay

<a id="task"></a>
## Task

Task ID: hyperliquid-tick-economic-replay. Deliver an explicitly modeled, retained economic replay for admitted tick-factor candidates, preserving the complete source universe and period and the existing shared trial budget. Endpoint: reviewed code, applicable CI and PR delivery; no real orders or local full native build.

<a id="spec"></a>
## Design

One initial USDT wealth is explicitly mapped to USDT0 and converted only after observed bridge data and modeled latency. A fixed split funds native multi-currency Cash and Margin ledgers once. Original instrument types and IDs remain; derived VWAP simulation bars are execution carriers, not original ticks, books or BAR data. Fixed-grid targets use the original retained Wasm features; spot negatives retain cash, perp targets may be signed. No new covariance assumptions or optimizer.

Actual funding-rate clocks drive a public simulation module using preceding observed cohort marks. This is a funding estimate, not an oracle-settlement claim. Native USD PnL is swept in nominal units to USDC; the result records its minor-unit rounding bound. Fees, additional spread/impact, latency, seeded jitter and participation are explicit scenario inputs. Missing valuation/funding remains visible, and open terminal positions are not hindsight-liquidated.

The new explicit HTTP replay path binds an admitted retained COMPUTED single factor and its original dataset/model/native request; it uses existing durable execution and remote admission. It neither admits arbitrary formulas nor relaxes BAR portfolio contracts or consumes a new factor trial.

<a id="verification"></a>
## Verification

TypeScript worker/API, idempotency and recovery checks and typechecking have run during implementation. Rust formatting and diff checks pass. Native tests are added for cash inventory, quantity/cost constraints, isolated ledgers, nominal settlement, once-only actual-rate adjustment and streaming timestamp cohorts; native compilation/execution awaits CI. Local Cargo builds are intentionally not run because the host is reserved for full-source preparation.

<a id="review"></a>
## Review and delivery

Draft delivery and exact-head CI/review are pending. No factor trial, backtest performance result or deployment is claimed. Native input readers are bounded; official open-position audit history can grow and remains subject to the existing worker resource limit.
