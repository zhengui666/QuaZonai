#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "../../../tests/support/strategy_composition.rs"]
mod fixture;
use contracts::strategy_portfolio::*;
#[test]
fn legacy_target_wire_preserves_original_scope() {
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
    assert_eq!(target.rebalance_instrument_ids.as_ref().unwrap().len(), 1);
}
