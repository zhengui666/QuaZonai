# Cover the existing frozen spot fee scenario contract

## Failure and scope

PR 176 at bb9318ba58e025648e5a1e9280f3df9ed7a8c0be passed all 12 selected Dataset/Forward PostgreSQL cases in CI run 37579471524. The routine Rust job separately failed contracts/tests/native_models.rs because its expected list still contained seven adapters while NativeModelRefV1 already includes FrozenSpotFeeScenario.

The eighth variant is the existing QZ-owned thin adapter through Nautilus 0.63's public FeeModel trait. Its wire identity is FROZEN_SPOT_FEE_SCENARIO with native_version, not an upstream_class/upstream_version claim. Its explicit public-rate scenario acceptance remains unverified applicability; this correction does not create or qualify a fee model.

## Correction

Preserve every original seven-adapter assertion and the existing Polymarket unknown-field regression. Assert exactly one additional frozen-spot variant with its exact four required fields, fixed native version, closed typed acceptance/schedule parameters and no upstream identity properties. Add a controlled wire roundtrip and missing/unknown-field rejection checks, including nested schedule, source and rule objects.

Only tests and this task record change. Production Rust, generated contracts, native engines, fee admission rules, migrations and CI selectors remain unchanged.

## Verification

The original failure log and existing contract, domain admission and Job FeeModel adapter paths were inspected. Syntax and exact patch replay are checked locally without Cargo while the shared build window is assigned elsewhere. Corrected-head native_models execution and required CI remain necessary; static inspection is not a test pass.
