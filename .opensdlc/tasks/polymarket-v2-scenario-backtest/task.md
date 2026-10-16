# Polymarket v2-only conditional portfolio research

## Goal and authority

Continue the owner's USD3000 research to a strategy combination with conditional shared-account Sharpe >5, using only TimeSeventeen/Polymarket-v2 and openai:gpt-6.1-sol / max. Owner explicitly accepted disclosed execution-rule assumptions; this is a research scenario, not verified historical execution. Do not introduce other source data, substitute IC for Sharpe, fabricate fees or zero-fill missing values. The owner clarified that market/token identity, direction, price, quantity and actual fees are sufficient for this research model; adapt the implementation rather than demand unrelated venue metadata.

Original study 4 / submission 12 and its five failures remain intact. Recovery study 36 / submission 44 completed nine genuine trials and independent evaluation. Current cumulative use 14, remaining 299986; next study must use this remainder, not reset. Preserve original requests and unknown-start identity. No orders, funds operations or account bindings.

## Design

- Reuse official native forecasting, blending and Nautilus shared-account simulation. Add an explicit price-BAR forecast command using the existing factor-only loader; ordinary execution-data forecast remains unchanged.
- Configure one explicit threshold mapping and disclosed model units, rounding, latency/slippage and USD/pUSD conversion assumptions. Fees must be derived from v2's real recorded fees and amounts with the derivation disclosed; never infer absent fees as zero. Keep modeled technical definitions separate in provenance from historical venue claims.
- Admit ensemble proposals, all member Wasm models and exact weights as one disclosed portfolio candidate in the existing durable trial ledger. Compute validation-only shared-account metrics for iterative search, retaining failures and cumulative budget. Do not sum factor or position curves.
- Preserve complete causal prefix for forecast warmup, but trade only validation for iterative feedback. Final assessment trades only the selected unseen holdout. Keep sealed data and final metrics away from the researching model. No tuning against final assessment, path renaming or new-study exposure reset.
- Repair the optional consumer to replay the original sealed binding for prefix-only datasets, and consume the selected ensemble with native exact blending. Preserve legacy single-model behavior and original policies.

## Verification and delivery

Run focused native CLI/forecast regressions, TypeScript checks and real native core flow. Final applicable CI and explicitly clean GitHub Codex read-only review are required before merging remote dev. Deploy exact tested compatible native binaries and corresponding QZ application changes without replacing credentials, original SQLite, user data or unrelated checkout changes. Normal research and portfolio operations use installed quazonai CLI only.

## Current state

Implementation started on isolated branch feat/polymarket-v2-scenario-backtest at remote-dev base 4ad99d70. No new study or portfolio has been submitted. Main user checkout remains untouched except task-owned goal/research records. Known same-source evidence: three v2 layers inspected; raw OrderFilled preserves fee_usdc and exact source amounts, enriched layers provide condition IDs and lifecycle fields. No future winner or payout labels will become signals.

## Latest owner clarification — recorded fees only

The owner explicitly forbids guessing fees and points to existing v2 fee fields. The unregistered training-average fee proxy preparation is withdrawn and MUST NOT be used for research or portfolio metrics. Use original `fee_usdc` linked to source transaction ID, quantity and time; if simulated quantities differ, retain an explicit, auditable proportional allocation of the matching original fee records. No fitted fee rate, external formula or missing-to-zero fallback. Raw source zeros remain recorded zeros. Execution-rule assumptions do not authorize synthetic fees.

The accounting representation is a native CurrencyPair share-price proxy (one modeled share unit per original v2 token share), not a historical BinaryOption definition. Official Nautilus rejects CurrencyPair in a single-currency CASH account, so the implemented conditional model uses MARGIN at leverage 1, nonnegative targets/cash and gross exposure at most 1. It does not assert physical cash-token inventory. Strict BinaryOption venue fee/lifetime behavior is unchanged.

## Implementation and checks in progress

- Implemented `forecast-factor` by reusing price-BAR science; ordinary `forecast` retains execution-definition requirements. New durable ensemble proposals compile/forecast/blend natively, then simulate ONE account on validation only; selected original ensembles replay the original Sealed binding independently. Component IC is labeled as diagnostics, not account Sharpe. The final outcome remains owner-only.
- `POLYMARKET_V2_RECORDED_FEES` reads transaction-linked `recorded-fees.json` from the original prefix/Sealed catalog. Actual shared native fill clock binds to its latest completed minute boundary. That BAR's records are consumed once FIFO by `(timestamp,id)`; original fee amounts are proportionally allocated by same-ID original native trade quantity and rounded once per fill at pUSD precision. Actual fill time, source BAR time and original records are retained. Missing/null/exhausted records fail, never zero or average fallback. Provider directions are retained, not asserted liquidity roles.
- New real input has 21315 prefix and 29551 full Sealed fee records, joined to existing native TradeTicks by source ID and timestamp. Raw `token_amount` remains `source_quantity`; no newly guessed quantity normalization. All 4138 full BAR quantity sums match and JSON/native Parquet readbacks are equal. Original average-rate files are now visibly `*.withdrawn.json` and absent from active preparation paths. No study or portfolio has used that proxy.
- Source-observed maximum training fee/notional, rounded upward only for sizing cash reserve, is explicitly not the commission policy. Native fees come solely from the record allocator. Disclose modeled ticks/size, positive latency/OHLC/slippage, USD/pUSD 1:1 assumption and native 252-day daily-return convention. Preparation is still unregistered.
- Passed local recorded-fee tests: 7, including exact nonterminating-ratio allocation, full native account commission and ending-balance equality. Source decimal fields for all 29551 full records are exactly representable without rounding. Passed full `make check-science`: 482 tests / 40 suites. Passed final `make check-ts`: service 58, research 27, CLI 3, plus `make check-core`: 2 genuine native flows (legacy and ensemble validation/independent/Sealed replay). The core fix corrected only controlled acquisition pagination/receive-clock facts and provided later receipt waves to settle native fee-driven rebalancing; original native validation, BAR values, fees, thresholds, partitions and assertions stayed intact. `make check-deploy` passes with `TMPDIR` in session scratch (the host `/tmp` tmpfs lacked space; no user files/caches were deleted). Final-Head CI/review/deployment remain pending; prior PR200 results are not relabeled as this feature's checks.
- Cumulative admitted trials remain 14; next request allowance remains 299986. No new research/portfolio or Sharpe result has been admitted or claimed.
- Recorded fees are catalog-backtest-only. Native Paper/streaming Paper reject this mode without falling back to the instrument's sizing-reserve fee fields; no account connection is introduced by this task. The native account regression covers this unsupported-mode rejection when Paper is compiled.
