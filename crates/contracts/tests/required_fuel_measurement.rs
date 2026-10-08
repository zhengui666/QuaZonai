//! Wire-only fixtures: explicit unmetered evidence differs from an absent field.
use contracts::{Id, science::*, strategy_portfolio::NativeStrategyCompositionResultV1};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, from_value, json, to_value};

#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "../../../tests/support/strategy_composition.rs"]
mod strategy_support;

fn assert_required_measurement<T: DeserializeOwned + Serialize>(mut wire: Value) {
    for value in [Value::Null, json!("0"), json!("9007199254740993")] {
        wire["consumed_fuel"] = value.clone();
        let parsed: T = from_value(wire.clone()).unwrap();
        assert_eq!(to_value(parsed).unwrap()["consumed_fuel"], value);
    }
    wire.as_object_mut().unwrap().remove("consumed_fuel");
    let error = match from_value::<T>(wire.clone()) {
        Ok(_) => panic!("missing consumed_fuel was accepted as unmetered"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("missing field `consumed_fuel`"),
        "{error}"
    );
    for value in [
        json!(0),
        json!(9007199254740993_u64),
        json!("-1"),
        json!("01"),
    ] {
        wire["consumed_fuel"] = value;
        assert!(from_value::<T>(wire.clone()).is_err());
    }
}

#[test]
fn forecast_measurement_is_required_nullable_and_keeps_decimal_strings() {
    assert_required_measurement::<NativeForecastResultV1>(json!({
        "schema_version":1,"native_versions":{},"points":[]
    }));
}

#[test]
fn validation_measurement_is_required_nullable_and_keeps_decimal_strings() {
    assert_required_measurement::<NativeAlphaValidationResultV1>(json!({
        "schema_version":1,"native_versions":{},"unique_test_observations":"0","folds":[]
    }));
}

#[test]
fn experiment_measurement_is_required_nullable_and_keeps_decimal_strings() {
    let current = strategy_support::current_report();
    assert_required_measurement::<NativeExperimentEvaluationResultV1>(json!({
        "schema_version":1,"native_versions":{},"dataset_revision_id":Id::new(),
        "model_artifact_id":Id::new(),"request":{
            "schema_version":1,"selection":current.request.selection,"instrument_id":"TARGET.SIM",
            "feature_schema":[],"split_policy":{
                "schema_version":1,"kind":"WALK_FORWARD","train_size":"1","test_size":"1",
                "purge_observations":"0","embargo_observations":"0",
                "interval_validation_required":true,"sealed_revision_id":Id::new()
            },"label_horizon_observations":1,"target_ttl_ns":"1",
            "decision_output":"TARGET_WEIGHT","settings":current.request.settings
        },"feature_artifact_ids":[],"instrument_id":"TARGET.SIM","source_row_count":"0",
        "feature_count":1,"folds":[]
    }));
}

#[test]
fn strategy_measurement_is_required_nullable_and_keeps_decimal_strings() {
    assert_required_measurement::<NativeStrategyCompositionResultV1>(
        to_value(strategy_support::current_report()).unwrap(),
    );
}

#[test]
fn portfolio_build_measurement_is_required_nullable_and_keeps_decimal_strings() {
    let input: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    assert_required_measurement::<NativePortfolioBuildResultV1>(json!({
        "schema_version":1,"bar_notionals":[],"slippage_references":[],"input":input,
        "allocation":{"schema_version":1,"solver_status":"FAILED","iterations":0,
            "objective_value":null,"primal_residual":null,"dual_residual":null}
    }));
}

#[test]
fn portfolio_study_measurement_is_required_nullable_and_keeps_decimal_strings() {
    assert_required_measurement::<NativePortfolioStudyResultV1>(json!({
        "schema_version":1,"frames":[]
    }));
}
