#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "../../../tests/support/strategy_composition.rs"]
mod fixture;
use contracts::strategy_portfolio::*;
use domain::execution::strategy;

#[test]
fn declared_constraints_use_actual_current_weights_instead_of_previous_target() {
    let report = fixture::current_report();
    let mut mandate = report.request.mandate;
    let StrategyCompositionOutcomeV1::CurrentDecision { target, .. } = report.outcome else {
        unreachable!()
    };
    mandate.constraints.max_turnover_per_rebalance = "0.1".parse().unwrap();
    assert!(strategy::constraints(&mandate, &target, &["0".parse().unwrap()]).is_err());
    strategy::constraints(&mandate, &target, &["0.2".parse().unwrap()]).unwrap();
    mandate.constraints.max_asset_weight = "0.2".parse().unwrap();
    assert!(strategy::constraints(&mandate, &target, &["0.2".parse().unwrap()]).is_err());
    mandate.constraints.max_asset_weight = "1".parse().unwrap();
    mandate.constraints.max_ex_ante_risk = Some("0.5".parse().unwrap());
    assert!(strategy::mandate(&mandate).is_err());
    mandate.constraints.max_ex_ante_risk = None;
    mandate.constraints.cash_funding =
        Some(contracts::portfolio::NativeCashFundingPolicyV1::PreserveMinCashAfterLinearTurnoverV1);
    // Fixed target composition has neither the cost vector nor a funded QP.
    // It must not silently accept the new observed-CASH allocation policy.
    assert!(strategy::mandate(&mandate).is_err());
    assert!(strategy::target_bounds(
        &mandate.constraints,
        &mandate.base_currency,
        &mandate.exposure_tolerance,
        &target,
        &["0.2".parse().unwrap()],
    )
    .is_err());
}

#[test]
fn output_cannot_change_account_purpose_or_original_request() {
    let original = fixture::current_report();
    for change in 0..6 {
        let mut altered = original.clone();
        let StrategyCompositionOutcomeV1::CurrentDecision {
            account_start,
            target,
            predictions_per_member,
        } = &mut altered.outcome
        else {
            unreachable!()
        };
        match change {
            0 => account_start.account_id = "DIFFERENT".into(),
            1 => target.asof_ns = fixture::count(45_000_000_000),
            2 => target.valid_until_ns = fixture::count(54_000_000_001),
            3 => predictions_per_member.clear(),
            4 => {
                altered.request.members[0]
                    .policy
                    .initialization
                    .first_ordinal = 0
            }
            _ => altered.consumed_fuel = Some(contracts::DbCounter::ZERO),
        }
        assert!(strategy::result(&original.request, &altered).is_err());
    }
    let mut missing = original.request.clone();
    missing.input_provenance.feature_artifact_origins.clear();
    assert!(strategy::request(&missing).is_err());
    let mut mixed = original.request.clone();
    mixed.purpose = StrategyPortfolioPurposeV1::HistoricalReplay {};
    assert!(strategy::result(&mixed, &original).is_err());
}

#[test]
fn current_strategy_unmetered_output_cannot_disguise_explicit_fuel_exhaustion() {
    let mut report = fixture::current_report();
    report.consumed_fuel = None;
    assert!(strategy::result(&report.request, &report).is_err());
    report.request.total_fuel = None;
    strategy::result(&report.request, &report).unwrap();
    report.consumed_fuel = Some(contracts::DbCounter::new(1).unwrap());
    assert!(strategy::result(&report.request, &report).is_err());
}

#[test]
fn legacy_target_wire_omits_scope_and_current_strategy_refuses_opt_in() {
    let mut report = fixture::current_report();
    let StrategyCompositionOutcomeV1::CurrentDecision { target, .. } = &mut report.outcome else {
        unreachable!()
    };
    let mut value = serde_json::to_value(&target).unwrap();
    assert!(value.get("rebalance_instrument_ids").is_none());
    let roundtrip: contracts::science::NativeTargetPointV1 =
        serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(roundtrip).unwrap(), value);
    value["rebalance_instrument_ids"] = serde_json::json!([target.targets[0].instrument_id]);
    *target = serde_json::from_value(value).unwrap();
    assert!(strategy::result(&report.request, &report).is_err());
}
