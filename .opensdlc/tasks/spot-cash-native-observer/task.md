# Spot cash native observation and runtime identity

Base candidate: spot-cash core v1, `libfile_71c813251ad48191af70eca9a3967fb4`, plus the fixed dev/A1 sources in that package. v1 is frozen for compilation; this work is isolated.

## Decision

The mathematical rule must work during target sizing, before output artifacts exist. Do not allocate fake artifact IDs or add a reservation system solely to satisfy the initial DTO. The next core contract should identify live facts with the real native engine instance UUID4, a distinct QZ observer session, the original snapshot event UUID4, observed sequence, and frozen dataset revision plus exact bar identity. The immutable settings value binds policy during execution. The actual native run UUID4 is read after the engine starts/finishes and retained in the completed run receipt. Persisted artifact IDs are attached by the existing adoption boundary after they really exist.

Keeping v1's required output artifact IDs and moving valuation entirely after adoption is smaller superficially, but cannot support in-run target sizing. It is not the selected integration architecture.

## Verified official public facts

Official Nautilus 0.63 source revision `a0400251110653b6d8ae6a9b5b89c4543fa85a2d`:

- `crates/common/src/msgbus/api.rs`: public typed `subscribe_bars`, `subscribe_portfolio_snapshot` and unsubscribe/publish functions. The router invokes higher priorities first; the portfolio bar handler has priority 10. A QZ price observer at priority 20 can record the exact bar callback before portfolio valuation callbacks caused by that publication.
- `crates/model/src/events/portfolio/snapshot.rs`: `PortfolioSnapshot` exposes original event UUID4, ts_event, ts_init, account kind, optional base currency and all AccountBalance rows. Cash total_equity includes position marks, so the new path must use raw balances only. Preserve the entire snapshot unchanged as evidence.
- `crates/portfolio/src/portfolio.rs`: snapshots publish synchronously on `events.portfolio.{account_id}` before entering the bounded snapshot ring. Capture the bus stream rather than assuming the final ring retained the full history.
- Native equity-curve snapshots occur at registration, each UTC midnight even while flat, and shutdown. Fine-grained snapshot_interval sampling only runs while positions are open. A constant intraday grid cannot be inferred from that setting. The next daily policy must explicitly choose true UTC boundary snapshots, or use an owned UTC timer calling official build_snapshot; it must not fill gaps.
- emit_snapshot may stamp a scheduled ts_event while ts_init remains construction time. A late snapshot must not pass as earlier balances: the first adapter rejects ts_event != ts_init or a mismatch with actual native callback clock.
- `BacktestEngine::instance_id()` and `run_id()` return UUID4, while QZ Id is UUID7. Keep both identities distinct. run_id is created within run before account initialization, not available when attaching before run. Do not fabricate a native UUID4 up front.

Sources:

- https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/common/src/msgbus/api.rs
- https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/portfolio/src/portfolio.rs
- https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/model/src/events/portfolio/snapshot.rs
- https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/backtest/src/engine.rs

## Minimal staged implementation

1. Read-only Job observer over official Bar and PortfolioSnapshot publications. Assign one local observer sequence across callbacks; copy exact native records and source identities. Match every selected callback bar to its frozen catalog row, reject rewrites/replays, bind native clocks and stop claiming completeness on any callback failure, dropped event or capacity overflow. No account/fee/trading logic belongs in this observer.
2. Revise the v2 core identity contract to accept these runtime facts directly. Keep the existing decimal arithmetic and evidence-consistency rule; do not duplicate it. The policy is the immutable actual settings used by the owner. Separate optional persisted artifact references from runtime identities.
3. At each target decision, obtain an official build_snapshot on the native event thread and value its raw balances against only already-observed, matched original bars. Capture this genuine snapshot and event ID before using the computed report total for sizing. This does not alter canonical_result or nativeSnapshot.
4. Introduce the multi-currency engine branch only after the fee adapter is integrated: Cash, no native base currency, no borrowing, leverage 1, one report-currency initial balance, only direct spot CurrencyPair instruments. Target quantity sizing and sell inventory come from actual balances, never cached net-position marks added to cash.
5. For no external flows, a generic observer must return unknown. Only the owner of a fresh, one-shot backtest can attest the closed execution path: no restored accounts, external clients, external account-state/transfer inputs, custom cash-flow venue modules or non-Bar data; known initial balance; only QZ-owned target/order operations and the selected official fee model; successful native lifecycle and reconciliation. Persist those concrete config/input/run facts. Do not turn a caller boolean or complete=true into proof.
6. Add explicit report-return/output/equity/summary branches and retain original per-currency native fees/PnL. Never relax old single-currency validation globally. Generate affected schemas with the native tools. Keep research admission blocked until real execution, fee correctness, provenance and consumers close together.

## Required Rust-focused verification

- Native msgbus publication ordering and exact copied Bar/PortfolioSnapshot identity
- Bar mismatch/duplicate, same-clock ordering, future/late snapshot and callback failure/capacity paths
- Real native build_snapshot with two cash balances, proving raw total projection rather than native total_equity
- Observer detach/cleanup and separate-run identities
- In-run valuation before output artifact creation
- Actual multi-currency buy/sell accounting and signed fee reconciliation
- True UTC midnight snapshots while flat; duplicate boundary policy; partial day and missing boundary behavior
- Existing Polymarket/default None regressions unchanged

No v2 compilation or tests have run. The parent-provisioned .rust-tools toolchain is now available; v2 is waiting for the single Job build slot while A1 compiles. v1 remains the separate current compiler input.

## Current isolated checkpoint

Implemented in this candidate:

- The core contract now uses a strict native UUID4 parsing-only type, true instance/event IDs, QZ observer session, frozen dataset revision and source row key. Context retains the immutable actual settings, and reports retain its policy. Output artifact IDs are not required for runtime math.
- NativeUtcMidnightBoundaries is explicit. Daily source checks remain across all supplied frames, while the daily arithmetic selects only actual adjacent midnight endpoints. Unknown/nonzero flows remain rejected; the completed-run owner must provide actual run ID plus exactly matching first/last snapshots and count. The observer cannot produce this evidence.
- `job::spot_cash_capture::record_native_run` wraps one fresh official engine run. It observes actual typed callbacks, matches complete Bars, including OHLC/volume precision, against explicit source boundary rows, retains every original PortfolioSnapshot unchanged, and computes a separate report valuation in the snapshot callback before artifact persistence exists.
- The generic observer verifies callback clocks, rejects backdated/late snapshots, tracks one sequence over bars and snapshots, rejects replay/source changes, bounds records, remembers callback failures, binds the exact native message-bus instance before/during/after run, and detaches from the original bus while restoring a different current bus. The run wrapper verifies actual initial fresh cash balances and returns the real native run identity. Flow evidence remains None.

Still not implemented: trusted Hyperliquid closed-bar source mapping (current A1 has quotes/trades), QuoteMid path, a capture handle used by target sizing, native multi-currency trading branch, owned closed-run flow attestation, fee integration/reconciliation, output/curve/summary consumers, schema regeneration and research admission. Existing `multi_currency_spot_cash_not_integrated` remains unchanged. This is an executable observer/core candidate, not a production closure.

Written tests: 30 domain, 4 contract/schema and 15 Job component/native-run tests. None compiled or executed. The source plan is not evidence that the pinned official APIs compile or the real native callback timing passes. Independent static source review closed all reported blockers. Compilation and test execution remain pending.

## Next concrete engine edits

The next separately reviewed slice should edit `apps/job/src/simulation.rs` at these existing seams, with explicit Some/None branches:

- `run_with_strategy`: construct one native Cash venue with base_currency absent, borrowing false, leverage 1 and the actual fresh report balance; install the independently validated fee model. Attach the observer before run/account registration and give TargetReplay its same read-only capture handle. Do not alter the None path or Polymarket handling.
- `TargetReplay::apply_bar` near the existing portfolio.equity lookup: call official Portfolio::build_snapshot on the native event thread and record that actual snapshot before valuing it. Use only a complete report total from the same current callback ordering. Unavailable valuation stops that target decision; it is never replaced with the old native equity entry.
- Replace the three new-mode uses of net_position in study current weights, strategy constraint current weights and desired-minus-current quantity with actual base-currency account inventory. native position quantity can differ from spendable inventory after a base-denominated buy fee. Use free balance for sell affordability and total inventory for value/current allocation. Do not globally change net_position behavior.
- Preserve existing sell-before-buy/native-order-settlement sequencing, but derive cash affordability and fee reservation from the actual native balances and frozen fee model. Do not infer execution success from nonnegative balances, absent open orders or a flat native position alone. Base-fee residual native position quantity is not extra account inventory.
- The current observer bounds its context by first/last native bar receipt. A production runner must bind the actual native start/end, including any latency-deferred final settlement. Keep runtime upper-bound checking separate from the exact completed-run window; never backdate a late final snapshot or silently drop its economic changes.
- Native registration/shutdown can collide with a midnight snapshot timestamp. Preserve all original events in the tape and define/test which genuine boundary observation the daily projection selects. Until that rule is explicit, core continues rejecting duplicate same-nanosecond snapshots instead of choosing one arbitrarily.
- `crates/domain/src/execution/output/simulation.rs`: a separate strict new-mode branch validates native Cash/base_currency explicit null, all per-currency balances and source-bound report valuations. The legacy branch keeps its one-currency/native-equity checks. The report's native run identity must match canonical_result, and its events/rows must match the retained original tape.
- `crates/domain/src/execution/output/equity_curve.rs`, `experiment_summary.rs` and portfolio summary: explicitly select report valuation/REPORT_CURRENCY_DAILY for this mode. Keep original native per-currency fees/PnL labeled as such. No first-equity-currency selection, stablecoin aliasing or fabricated native portfolio series.

Removing `multi_currency_spot_cash_not_integrated` requires these pieces to close together, plus the trusted source row mapping and actual native fee/accounting tests. The current observer candidate intentionally does not remove it.
