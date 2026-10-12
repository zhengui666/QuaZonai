# Evidence and optional portfolios/backtests

Start with `quazonai research get ID`: preserve original data/policy, every failed/rejected/computed trial, the compared set, selection and final assessment or stop reason. No final evidence, null IC or failed evaluation is a limitation, not zero or PASS.

Only build a portfolio when requested and when the installed native adapter accepts the original evidence and strategy policy:

```sh
quazonai schema show PortfolioStart
quazonai portfolio build --file original-portfolio.json
quazonai portfolio list
quazonai portfolio get PORTFOLIO_ID
```

The file contains `requestId`, `name`, original numeric `researchIds`, `datasetId`, exact decimal-string `capital` and any original `weights` accepted by the adapter. Preserve the strategy assumptions admitted before final research evidence; do not retune them against the held-out sample. A list/get is a read, not another backtest.

A build receipt may still be queued/running/failed. Read the same portfolio again for its actual metrics, native shared-account equity curve and target weights. Unsupported ensemble/strategy adapters or absent independent evidence fail explicitly. Never sum factor curves, invent missing equity, or substitute a computed factor's IC for portfolio performance. `backtest build/list/get` is a spelling alias for this same consumer, not a separate simulator.

Preserve money, quantities and weights as decimal strings. Missing metrics or curve values remain null; Synthetic/native protocol tests do not prove market-data quality, profitability or real account access. A completed simulation is not permission to trade.

If target/account observation is requested, use the existing configured binding in [observations](account-transport.md). Target receipts/ACKs do not establish fills, balances or actual returns.
