//! Synthetic structural counterexamples, not native numerical or market evidence.
//! Genuine solver/account output is independently exercised by job/tests/managed.
use contracts::{
    portfolio::*,
    runtime_jobs::{native_output_contract, RuntimeOutputV1},
    DbCounter, Id, Revision, SchemaV1,
};
use serde_json::json;

#[test]
fn mandate_constraints_use_the_same_checks_as_native_allocation() {
    let mut request: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    domain::portfolio::portfolio_constraints(&request.constraints).unwrap();
    request.constraints.asset_overrides = vec![
        AssetBoundV1 {
            instrument_id: request.assets[0].instrument_id.clone(),
            min: "0".parse().unwrap(),
            max: "1".parse().unwrap(),
        };
        2
    ];
    assert!(domain::portfolio::portfolio_constraints(&request.constraints).is_err());
    assert!(domain::portfolio::allocation_input(&request).is_err());
    request.constraints.asset_overrides.clear();
    request.constraints.min_cash_weight = "2".parse().unwrap();
    request.constraints.max_cash_weight = "1".parse().unwrap();
    assert!(domain::portfolio::portfolio_constraints(&request.constraints).is_err());
    assert!(domain::portfolio::allocation_input(&request).is_err());
}

#[test]
fn rebalance_intent_requires_exact_kind_fields_and_native_iana_timezone() {
    let mut schedule = RebalanceScheduleV1 {
        schema_version: SchemaV1,
        kind: RebalanceKind::Manual,
        interval_seconds: None,
        calendar_ref: None,
        timezone: "America/New_York".into(),
        session_offset_seconds: None,
        max_input_age_seconds: 60,
        target_ttl_seconds: 60,
    };
    domain::portfolio::rebalance_schedule(&schedule).unwrap();
    schedule.interval_seconds = Some(60);
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.kind = RebalanceKind::FixedInterval;
    domain::portfolio::rebalance_schedule(&schedule).unwrap();
    schedule.interval_seconds = Some(0);
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.kind = RebalanceKind::CalendarSession;
    schedule.interval_seconds = None;
    schedule.calendar_ref = Some("XNYS-2026".into());
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.session_offset_seconds = Some(-300);
    domain::portfolio::rebalance_schedule(&schedule).unwrap();
    schedule.timezone = "Not/A_Zone".into();
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.timezone = "Asia/Shanghai".into();
    schedule.target_ttl_seconds = 0;
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.target_ttl_seconds = 60;
    schedule.max_input_age_seconds = 0;
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    let mut wire = serde_json::to_value(&schedule).unwrap();
    wire["fallback_timezone"] = json!("UTC");
    assert!(serde_json::from_value::<RebalanceScheduleV1>(wire).is_err());
}

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
#[test]
fn retired_allocation_and_study_outputs_are_not_accepted() {
    for name in [
        "qz.native_portfolio",
        "qz.portfolio_study",
        "qz.native_simulation",
        "qz.strategy_portfolio",
    ] {
        let contract = native_output_contract(name, "1").unwrap();
        let bytes = b"{}";
        let descriptor = RuntimeOutputV1 {
            kind: contract.kind,
            schema: contracts::runtime::RuntimeArtifactSchemaV1 {
                name: name.into(),
                version: "1".into(),
            },
            storage_ref: Id::new(),
            storage_version: Revision::INITIAL,
            byte_count: count(bytes.len() as u64),
            media_type: contract.media_type.into(),
        };
        assert_eq!(
            domain::execution::output_shape(&descriptor, bytes),
            Err(domain::research::invalid(
                "native_output.schema",
                "NATIVE_OUTPUT_CONTRACT_INVALID"
            ))
        );
    }
}
