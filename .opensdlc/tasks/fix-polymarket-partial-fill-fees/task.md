# Preserve fee bounds for Polymarket partial fills

<a id="task"></a>
## Task

Task ID: `fix-polymarket-partial-fill-fees`. Source: the authorized September 30,
2026 partial-fill audit and implementation request. Endpoint: all applicable CI
passes on the final head, GitHub Codex explicitly reports no remaining issues,
and that reviewed head is merged into remote `dev` with merge readback. The
implementation starts from `origin/dev` at `5e7f2959`.

<a id="intent"></a>
## Intent

A submitted order can exceed 1 pUSD while the native engine produces smaller
partial fills. The fee wrapper rejects each such fill before calling the official
fee model, even when its rounded commission fits the existing planning bound.
The observed public prints do not establish their original order types.

<a id="spec"></a>
## Requirements and design

Keep the whole-order amount floor, original collateral and native settlement
behavior. Ordinary fills require positive quantity and price. Retain the existing
native price bounds and replay failure path. Use the unchanged official
`PolymarketFeeModel` and compare its
actual commission with gross fill notional times the same source-derived planning
rate. Exact `BigDecimal` arithmetic preserves the declared cost bound; an actual
rounded fee above it remains unsupported. No contracts, dependencies or settings
change.

<a id="plan"></a>
## Implementation plan

The shared path is simulation venue setup → fee wrapper → official fee model.
`validate_market` already equates the frozen taker rate with `planning_fee`.
Update [the wrapper](../../../apps/job/src/prediction.rs), clarify
[the domain comments](../../../crates/domain/src/prediction.rs), and extend
[the existing regression module](../../../apps/job/tests/polymarket.rs).

<a id="verification"></a>
## Verification

Add one native regression covering a qualifying whole order with a sub-unit
partial fill inside the fee bound and a smaller fill whose real five-decimal
commission exceeds that bound. Existing normal-fill and settlement checks remain.
Local tests and builds are not run; final-head GitHub CI provides execution evidence.
`cargo fmt --all -- --check` and `git diff --check` pass. The new native regression
and existing tests have not yet run on the final head; CI and review acceptance
remain pending.

<a id="handoff"></a>
## Handoff

The patch awaits final-head CI, GitHub Codex review and remote `dev` merge.
No service write or dataset admission is part of this source change.
The change supports only fills whose actual native fee fits the frozen bound;
it does not promise arbitrary partial fills or research qualification.
