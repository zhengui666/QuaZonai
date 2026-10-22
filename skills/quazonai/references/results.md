# Evidence and optional portfolios/backtests

Start with `quazonai research get ID`: preserve original data/policy, every failed/rejected/computed trial, the compared set, selection and final assessment or stop reason. No final evidence, null IC or failed evaluation is a limitation, not zero or PASS.

Saved literature is a separate source of research context: preserve paper/source links, returned metadata or abstracts, the Agent's supporting/contrary/background notes and their limitations. The research detail's literature tab also shows source errors and partial/unavailable searches. No owner-configured public topics means new retrieval was not enabled; `public_queries_not_configured` is not evidence of a provider outage or absence of papers. Existing receipts remain readable. A citation, excerpt or retrieved abstract does not establish full-text review or alter the original scientific assessment. Reading the literature again does not perform a new search.

## Interpret the original research before choosing a consumer

1. Recover `input.goal` and any structured `input.specification`: state the question, rationale, exclusions, original metric/target and search scope. Compare the result with that mandate, including negative trials and any `previousStops`, instead of selecting a favorable metric after the fact.
2. Match `final.trialId` to the selected `trialRecords` entry. Keep iterative Validation feedback separate from the final held-out assessment. Check each metric's `key`, `partition`, `component`, `status` and unit when supplied; absent metadata is unknown. Component IC is not a shared-account Sharpe ratio, and results from different datasets, horizons, costs or partitions are not directly interchangeable.
3. Read `criterion` when supplied. `computed` means a value was obtained without a declared target; `met`/`not_met` compare that value with the original target; `insufficient` and `failed` remain limitations. None of these statuses promises profitability or current execution support. For legacy research without a criterion, report the actual final metrics and original prose goal; do not invent a threshold retroactively.
4. For a failure, use the public `error`/`errorDetail` code, category, stage and suggested action when available. Distinguish model/data/request/resource/runtime failures from evidence against the hypothesis. Do not rerun science merely to inspect the same result.
5. Conclude whether the original question is supported, rejected or unresolved, citing the research, selected trial and factor IDs. State sample/assumption limitations and the remaining gap. A negative result can complete the requested research; optional portfolio or observation work starts only within the user's requested scope.

Use returned `factor list` / `factor get ID` identities and preserve `researchId`, `trialId` and `memberIndex`. Read `evidence`, `criterion`, `combinable` and `reason` together: `independent`, `historical`, `component_only`, `validation_only` and `unavailable` are different evidence classes. A listed or computed factor is not automatically independently assessed or usable in a portfolio. A component's performance in a selected ensemble does not establish its standalone performance; do not create or guess a replacement factor ID to bypass the reported limitation.

## Build and inspect only the requested portfolio

Only build a portfolio when requested and when the installed native adapter accepts the original evidence and strategy policy:

```sh
quazonai schema show PortfolioStart
quazonai portfolio build --file original-portfolio.json
quazonai portfolio list
quazonai portfolio get PORTFOLIO_ID
```

Prefer original atomic references from `factor list/get`: new files contain `requestId`, `name`, `factors:[{factorId,weight}]`, `datasetId` and exact decimal-string `capital`. Positive weights sum exactly to one. A factor ID binds original research/trial/member; combination-component evidence is not independently qualified single-factor performance. Legacy files with original numeric `researchIds` and explicit `weights` remain supported, mutually exclusive with `factors`. Optional `capacityPolicy` declares scenarios/participation/maximum age; otherwise compatible original research assumptions are inherited. Preserve the strategy assumptions admitted before final research evidence; do not retune them against the held-out sample. A list/get is a read, not another backtest.

Before submitting, explain why the selected members answer the requested portfolio objective and where the weights, capital, dataset and capacity assumptions came from. Reuse an existing matching portfolio for an inspection request. Retain the new request file and `requestId`; after admission, verify the returned portfolio's members, dataset and capital match it. Do not replace failed or unavailable members silently or present an automatically normalized weight vector as the user's original choice.

A build receipt may still be queued/running/failed. Read the same portfolio again for its actual metrics, native shared-account equity curve and target weights. Unsupported ensemble/strategy adapters or absent independent evidence fail explicitly. Never sum factor curves, invent missing equity, or substitute a computed factor's IC for portfolio performance. `backtest build/list/get` is a spelling alias for this same consumer, not a separate simulator.

`factor capacity ID` uses recorded assumptions; `factor capacity ID --file ASSUMPTIONS.json` supplies explicit `{scenarios:["1000"],participation:"0.01",maximumAgeSeconds:60}` after research, including eligible legacy studies. This read-only scientific assessment needs no request ID and never changes the original mandate/trials/evidence. A new portfolio can set its own `capacityPolicy` with the same fields instead of inheriting incompatible member assumptions. Capacity reports show historical available-at BAR feasibility, binding asset, minimum usable capital and scenarios under explicit conversion/execution assumptions. They do not establish future depth, current execution support or profitability. Do not add standalone capacities: combined overlapping demand shares one liquidity budget. `requestedAssumptions`, when present, records the original explicit policy; `scenarios` can also contain native-generated diagnostic scales and must not be copied wholesale into a new request. Older reports without request metadata do not establish the original scenario list.

For an existing computed portfolio, `portfolio capacity ID --file ASSUMPTIONS.json` uses only its saved original simulation request and catalog. It does not rebuild the portfolio, change saved capacity/results/targets, or send an account action. Missing original metadata/catalog yields an unavailable report, not reconstructed inputs. The response includes separate entry/rebalance and standalone-exit constraints, original limiting clocks, observation coverage and explicitly assumed half-volume sensitivity where available. Economic capacity remains unavailable without matched out-of-sample net returns, calibrated impact and execution/exit inputs; sensitivity is not a probability or confidence interval.

Preserve money, quantities and weights as decimal strings. Missing metrics or curve values remain null; Synthetic/native protocol tests do not prove market-data quality, profitability or real account access. A completed simulation is not permission to trade.

If target/account observation is requested, use the existing configured binding in [observations](account-transport.md). Target receipts/ACKs do not establish fills, balances or actual returns.

When reporting observed account results, retain the observation/portfolio/connection IDs, recorded `mode`, currency, source `provenance` and snapshot time. Keep target receipt status separate from account values and automatic-allocation status. A stale snapshot or null equity/cash remains unavailable; a `live` binding alone does not prove current fills or profit. Explain whether the evidence is research, native simulation or a downstream account snapshot before comparing performance.
