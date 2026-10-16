# Polymarket V2 native evidence binding

## Goal and scope

Allow the existing single-instrument target-weight experiment/composition pipeline to use Dataset-owned BinaryOption evidence and the existing Nautilus 0.63 replay. This is a separate candidate based on the frozen V5 source. It does not implement a venue account, order transport, a fee formula, matching engine or settlement engine. Existing Paper delivery and ordinary asset behavior retain their contracts. No production data, PIT, Sealed evidence or financial assumptions are changed.

## Necessary QZ changes and official reuse

- contracts/settlement, science/experiment and strategy_portfolio: optional source-owned context containing original externally tagged native definitions and complete original two-outcome close groups. Caller experiment parameters cannot inject this field. Legacy absent fields serialize unchanged.
- domain/prediction and execution input/output checks: exact source/cutoff/fee/account/result correlation. Reuse catalogs::instrument_versions, prediction::target_window, scoped/visible_settlements and native fee-source validation. These checks do not compute fills, payout prices or cash.
- Store external_experiment: derive only from authorized frozen DatasetBinding, re-read and compare after compilation, require specific capability before trial admission. Existing accepted report/native task comparison covers the new field.
- Store strategy/strategy_publication: bind each accepted report's original context to the selected replay and current Dataset context; reject old None sources reinterpreted as BinaryOption. Preserve immutable receipts and current Forward/freshness rules.
- Job experiment/strategy: pass original visible close groups into NativeSimulationRequestV1; correlate native catalog versions using existing select_instrument_versions; check current target TTL through the existing lifetime rule. Actual execution remains simulation::simulate/simulate_strategy, SimulatedExchange, SettlementAwarePolymarketFee -> official PolymarketFeeModel and original InstrumentClose.
- Runtime/OCI label: polymarket-target-policy/1 is advertised only by the matching new native-stack label; Store refuses missing capability rather than downgrading.

## Semantics

Only one traded outcome is supported. Complete sibling settlement evidence remains required whenever that condition's closes are available. Pre-expiry windows may have no known settlement; no future close is fabricated. Existing close_events explicitly rejects pending resolution when replay crosses expiry. N-observed-BAR labels, fixed TTLs, warmup/fold boundaries and production sample thresholds are unchanged. Missing context, wrong native fee/account, expiry violation, altered source definition/close or older runtime capability fails closed.

## Compatibility and verification

Missing/None contexts keep legacy serialized request/report bytes unchanged. New BinaryOption records require the matching updated Contracts/Server/Runtime/Job and typed clients; old deny-unknown-field consumers are not claimed to read the new context. Existing TargetPackageV2/Paper output is unchanged. The OCI native-stack label includes the new capability so an older Job cannot advertise support.

- Locked offline Contracts/Domain/Store/Job/Runtime checks including tests passed. Existing spot_fees/dead_code warnings remain unrelated.
- New native target-policy suite: 7 passed, including Cash experiment, historical composition, current continuation, exact old-wire bytes, source/capability/TTL/close rejection and future-close isolation.
- Existing native experiment: 7 passed; Polymarket V1: 12 passed; strategy composition: 1 passed.
- Initial synthetic minute-BAR positives correctly failed the original expired-fill guard because the seconds-based fixture TTL was 10 seconds. Only the new synthetic fixture now explicitly declares 120 seconds for its minute bars; production fill guards and all input financial assumptions are unchanged. The initial failing log is retained.
- Original Rust generators produced all three schema snapshots. Structural comparison changes only the new context and request schemas; other document fields match the frozen V5 baseline. Same-lock Web generation and tsc passed; generated response checks passed (472 files, exact regeneration match).
- Selected Domain/Runtime tests passed: 5 experiment summaries, 8 native outputs, 3 strategy rules and 2 runtime compatibility contracts. Total applicable final tests: 45 passed. The review corrections cover unabridged member TTLs and already-available native closes for current targets; final independent review is separate from these test results.
- No disposable-PostgreSQL, OCI, UI container, full workspace/all-feature CI, production dataset/PIT qualification or live financial execution is claimed. All data in the new tests is explicitly synthetic.
