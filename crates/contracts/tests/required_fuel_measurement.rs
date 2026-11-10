//! Wire-only fixtures: explicit unmetered evidence differs from an absent field.
use contracts::{science::*, strategy_portfolio::NativeStrategyCompositionResultV1, Id};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{from_value, json, to_value, Value};

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
    let input: Value = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
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
