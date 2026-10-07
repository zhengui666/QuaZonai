# Spot cash report valuation core

Base: `ca3928bc26358131775ac9c9f92a61ed13b7aca1` (dev).

## Authorized scope

Ordinary Hyperliquid spot, official Nautilus 0.63 packages, multi-currency CASH and a separate report-currency valuation. No perpetuals, leverage, production orders, push, merge or deployment. No fork, patch or copied upstream implementation. This slice changes QZ contracts and pure Rust rules only; Cargo and public capture are untouched.

## Candidate behavior

- The existing `NativeSimulationSettingsV1` gains only `multi_currency_spot_cash: Option<NativeSpotCashPolicyV1>` with serde default/skip-none. Ten existing Rust fixture constructors explicitly retain `None`. No parallel settings/request container.
- The account plan requires CASH, leverage exactly 1, native base currency explicitly `None`, borrowing false and one fresh starting balance in the report currency. Report currency equals the original business `base_currency`.
- The only initial price method is `CLOSED_BAR_CLOSE`. The policy freezes allowed instrument IDs and positive maximum age. Instruments must be same-venue native `CurrencyPair`, direct quote/report currency, multiplier 1, non-inverse and non-expiring. No FX graph or stablecoin parity assumption.
- Native account snapshot totals are authoritative and valued once. Free plus locked must equal total. Position value, PnL and commissions cannot be added/subtracted in this module. An unavailable valuation has no total and no partial legs.
- Context and every snapshot retain session, policy, account, venue, original native artifact, row key, monotone snapshot sequence and as-of time. Price inputs retain their observation session and source artifact/row, allowed instrument, closed-bar open/end, availability clock and global observed sequence. Equal-nanosecond ordering must be proven by an earlier replay sequence. Both single valuations and daily series reject sequence collisions, backwards replay clocks and conflicting content for one frozen source row; unchanged prior marks can be reused within their frozen age limit. Only frozen allowed source artifacts are accepted.
- Daily returns are separately labeled `REPORT_CURRENCY_DAILY`, with explicit 365-day annualization and `FRESH_SIMULATION_NO_EXTERNAL_FLOWS`. Exact decimal arithmetic values balances; the final dimensionless statistical return converts to finite f64. Daily points require consecutive complete UTC boundaries and every snapshot on the declared grid. Partial days are excluded; gaps and unavailable marks remain unavailable. Unknown or nonzero external flows reject daily calculation. Frames cannot cross sessions or reuse snapshot keys.
- Exact decimal products outside NUMERIC(38,18) fail rather than silently rounding. No native snapshots or canonical result documents are rewritten; no call to Analyzer.add_return is added.

## Current integration boundary

`domain::portfolio::simulation_settings` explicitly rejects a present mode with `multi_currency_spot_cash_not_integrated`. This prevents accepted new configuration from silently using the old single-currency path. The pure planner and valuation API are callable for focused unit validation, but are not an executable Hyperliquid simulation or research admission.

Still required before enabling this mode:

1. A native snapshot/closed-bar adapter must supply genuine event-order sequence and full balances from the same engine session and frozen catalog. These typed projections do not by themselves authenticate a caller's assertions. Capture original snapshots as they occur; never backfill old frames from final cache state. Native input null must be explicit, not missing.
2. Engine account construction, target sizing, inventory sufficiency and cash-account reconciliation must consume the plan. AccountsManager may log/roll back errors without surfacing a direct failure; shutdown-on-error alone needs actual native validation.
3. Add explicit new branches in `domain/execution/output/simulation.rs`, equity projection and experiment/portfolio summaries. Do not weaken existing single-currency checks globally, call report valuations native account PnL, or select the first balance. Preserve exact per-currency native fees.
4. An official public `FeeModel`-based QZ adapter needs a frozen, supported signed schedule and correct charged currency/rounding. Native Hyperliquid zero maker/taker defaults are not evidence of free trading. Fee closure is deliberately separate and remains a research-admission blocker.
5. If report-return statistics use the official analyzer, populate its public portfolio-return series via a supported API; never `add_return`, which is a position-return alias. Existing snapshot-only logic stays unchanged.
6. Generate native schemas and affected clients from Rust after compilation. Do not hand-edit generated snapshots. No generated files are delivered as if current in this core candidate.

## Verification record

- Written: 22 domain unit tests plus two strict contract/schema tests, covering planning restrictions, old-wire omission, non-double-counted locked cash, missing/stale/future prices, source/order/session identity, decimal range, unknown currency, UTC complete days/gaps, external flows and explicit native null.
- Rust compilation, build, rustfmt and unit tests: NOT RUN. This cloud executor has no cargo, rustc or rustfmt. No Python/static surrogate counts as a Rust test pass.
- Native engine integration and fee acceptance: NOT IMPLEMENTED/NOT RUN.
- Independent static source review: reported blockers closed on this candidate. This is not compiler/test/native/schema evidence.
- No publication actions.

## Intended focused compiler/test commands

On the authorized existing full checkout with its pinned Rust toolchain, apply the candidate patch, then run:

- `cargo test --locked -p contracts --lib spot_cash`
- `cargo test --locked -p domain --lib spot_cash`
- `cargo check --locked --workspace --all-targets --all-features`
- Existing authorized Rust build/unit targets for the final integrated candidate

Record commands and actual exit results. The source package is a candidate, not evidence these commands passed.
