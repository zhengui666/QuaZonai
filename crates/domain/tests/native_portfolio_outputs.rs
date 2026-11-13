//! Synthetic structural counterexamples, not native numerical or market evidence.
//! Genuine solver/account output is independently exercised by job/tests/managed.
use portfolio_config::execution_models;
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_config;
use chrono::{DateTime, Utc};
use contracts::{
    evidence::MetricStatus,
    execution::NativeTaskParametersV1,
    portfolio::*,
    runtime_jobs::{native_output_contract, RuntimeOutputV1},
    science::*,
    DbCounter, Id, Revision, SchemaV1,
};
use domain::execution::output_bindings;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn native_portfolio_refuses_unbound_execution_costs() {
    let input: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    let original = portfolio_config::request(&input);
    domain::execution::portfolio_build_request(&original).unwrap();
    for case in 0..4 {
        let mut request = original.clone();
        match case {
            0 => request.execution_settings.base_currency = "EUR".into(),
            1 => request.execution_settings.starting_capital = "1".parse().unwrap(),
            2 => request.execution_settings.fee_rates[0].taker = "0.25".parse().unwrap(),
            _ => request.execution_settings.fee_rates.clear(),
        }
        assert!(
            domain::execution::portfolio_build_request(&request).is_err(),
            "cost case {case}"
        );
    }
}

#[test]
fn native_one_tick_cost_planning_is_explicit_and_bound() {
    let input: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    let mut request = portfolio_config::request(&input);
    for asset in &mut request.assets {
        asset.transaction_cost_rate = "0.01".parse().unwrap();
    }
    for fee in &mut request.execution_settings.fee_rates {
        fee.taker = "0.01".parse().unwrap();
    }
    let NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
        &mut request.execution_settings.fill_model
    else {
        unreachable!()
    };
    parameters.prob_slippage = "0.5".parse().unwrap();
    let references = request
        .assets
        .iter()
        .map(|asset| NativePortfolioSlippageReferenceV1 {
            instrument_id: asset.instrument_id.clone(),
            currency: asset.currency.clone(),
            event_ns: input.forecasts.forecast_asof_ns,
            available_ns: input.forecasts.forecast_asof_ns,
            close_price: "10".parse().unwrap(),
            price_increment: "0.01".parse().unwrap(),
        })
        .collect::<Vec<_>>();
    let costs = domain::execution::portfolio_execution_costs(&request, &references).unwrap();
    assert_eq!(costs[0].transaction_cost_rate, "0.010505".parse().unwrap());
    let mut thirds = references.clone();
    thirds[0].close_price = "3".parse().unwrap();
    assert_eq!(
        domain::execution::portfolio_execution_costs(&request, &thirds).unwrap()[0]
            .transaction_cost_rate,
        "0.011683333333333334".parse().unwrap()
    );
    for case in 0..6 {
        let mut invalid = references.clone();
        match case {
            0 => invalid.clear(),
            1 => invalid[0].price_increment = "0".parse().unwrap(),
            2 => invalid[0].price_increment = "10".parse().unwrap(),
            3 => invalid[0].currency = "EUR".into(),
            4 => invalid[0].instrument_id = "foreign".into(),
            _ => {
                invalid[0].available_ns =
                    DbCounter::new(request.selection.decision_cutoff_ns.get() + 1).unwrap()
            }
        }
        assert!(
            domain::execution::portfolio_execution_costs(&request, &invalid).is_err(),
            "reference case {case}"
        );
    }
}

#[test]
fn native_portfolio_refuses_unbound_liquidity_numbers_or_references() {
    let input: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    let original = portfolio_config::request(&input);
    domain::execution::portfolio_build_request(&original).unwrap();
    for case in 0..3 {
        let mut request = original.clone();
        match case {
            0 => request.assets[0].available_notional = Some("100".parse().unwrap()),
            1 => request.mandate.constraints.liquidity_ref = Some(Id::new()),
            _ => request.mandate.constraints.max_participation = Some("0.1".parse().unwrap()),
        }
        assert!(domain::execution::portfolio_build_request(&request).is_err());
    }
}

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
fn accepts<T: serde::Serialize>(
    parameters: &NativeTaskParametersV1,
    name: &str,
    value: &T,
) -> bool {
    let contract = native_output_contract(name, "1").unwrap();
    let bytes = serde_json::to_vec(value).unwrap();
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
    let now = DateTime::<Utc>::from_timestamp(100, 0).unwrap();
    output_bindings(parameters, None, now, now, &[(descriptor, bytes)]).is_ok()
}

#[test]
fn retired_allocation_and_study_outputs_are_not_accepted() {
    for name in ["qz.native_portfolio", "qz.portfolio_study"] {
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

fn simulation() -> (NativeTaskParametersV1, NativeSimulationResultV1) {
    let day = 86_400_000_000_000;
    let start = day + 1;
    let end = 2 * day;
    let request = NativeSimulationRequestV1 {
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec!["A.SIM-1-SECOND-LAST-EXTERNAL".into()],
            event_start_ns: count(start),
            event_end_ns: count(end + 1),
            decision_cutoff_ns: count(end + 2),
            maximum_rows: 100,
        },
        settings: NativeSimulationSettingsV1 {
            price_benchmark: None,
            multi_currency_spot_cash: None,
            schema_version: SchemaV1,
            base_currency: "USD".into(),
            starting_capital: "1000".parse().unwrap(),
            account_kind: NativeAccountKind::Cash,
            leverage: "1".parse().unwrap(),
            fill_model: execution_models::fill(),
            fee_model: execution_models::fee(),
            latency_model: execution_models::latency(1),
            snapshot_interval_ms: 1000,
            exposure_tolerance: "0.000001".parse().unwrap(),
            fee_rates: vec![NativeFeeRateV1 {
                instrument_id: "A.SIM".into(),
                maker: "0".parse().unwrap(),
                taker: "0".parse().unwrap(),
            }],
        },
        target_points: vec![NativeTargetPointV1 {
            schema_version: SchemaV1,
            rebalance_instrument_ids: None,
            asof_ns: count(start),
            valid_until_ns: count(end),
            targets: vec![AllocationTargetV1 {
                instrument_id: "A.SIM".into(),
                currency: "USD".into(),
                weight: "0".parse().unwrap(),
            }],
            cash_weight: "1".parse().unwrap(),
        }],
    };
    let summary = BTreeMap::from([
        ("venues.total".into(), "1".into()),
        ("orders.open".into(), "0".into()),
        ("orders.inflight".into(), "0".into()),
    ]);
    let result = NativeSimulationResultV1 {
        settled_replay_ns: None,
        spot_cash_report: None,
        schema_version: SchemaV1,
        native_version: "0.69.0".into(),
        iterations: count(100),
        events: count(0),
        orders: count(0),
        positions: count(0),
        consumed_target_points: count(1),
        summary: summary.clone(),
        statistics: Vec::new(),
        returns_kind: NativeReturnsKind::PortfolioDaily,
        returns_status: MetricStatus::Ok,
        returns_reason: None,
        returns: vec![NativeReturnV1 {
            timestamp_ns: count(end),
            value: Some(0.0),
            reason_code: None,
        }],
        canonical_result: json!({
            "schema":"nautilus-backtest-result/v1",
            "run":{"outcome":"completed","iterations":"100","total_events":"0",
                "total_orders":"0","total_positions":"0",
                "backtest_start_ns":start.to_string(),"backtest_end_ns":end.to_string()},
            "summary":summary,
            "accounts":[{"Cash":{"base":{"id":"SIM-001","account_type":"CASH",
                "base_currency":"USD","balances_starting":{"USD":"1000.00 USD"}}}}],
            "portfolio_snapshots":[{"account_id":"SIM-001","account_type":"CASH",
                "base_currency":"USD","total_equity":["1000.00 USD"],"ts_event":end.to_string()}]
        }),
    };
    (
        NativeTaskParametersV1::SimulatePortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: Id::new(),
            request: Box::new(request),
        },
        result,
    )
}

fn fill_parameters_mut(model: &mut NativeModelRefV1) -> &mut NautilusFillParametersV1 {
    match model {
        NativeModelRefV1::NautilusDefaultFill { parameters, .. } => parameters,
        _ => unreachable!(),
    }
}

#[test]
fn default_fill_validates_probabilities_seed_and_latency() {
    use domain::portfolio::simulation_models;
    let (NativeTaskParametersV1::SimulatePortfolio { request, .. }, _) = simulation() else {
        unreachable!()
    };
    let mut settings = request.settings;
    for seed in [0, 7, i64::MAX as u64] {
        for (limit, slippage) in [("0", "1"), ("1", "0"), ("0.75", "0.25")] {
            let parameters = NautilusFillParametersV1 {
                prob_fill_on_limit: limit.parse().unwrap(),
                prob_slippage: slippage.parse().unwrap(),
                random_seed: count(seed),
            };
            *fill_parameters_mut(&mut settings.fill_model) = parameters.clone();
            assert_eq!(simulation_models(&settings).unwrap().0, &parameters);
        }
    }
    for (limit, slippage) in [
        ("-0.000000000000000001", "0"),
        ("1.000000000000000001", "0"),
        ("1", "-0.000000000000000001"),
        ("1", "1.000000000000000001"),
    ] {
        let parameters = fill_parameters_mut(&mut settings.fill_model);
        parameters.prob_fill_on_limit = limit.parse().unwrap();
        parameters.prob_slippage = slippage.parse().unwrap();
        assert_eq!(
            simulation_models(&settings),
            Err(domain::DomainError::Invalid("native_simulation_parameters"))
        );
    }
    let parameters = fill_parameters_mut(&mut settings.fill_model);
    parameters.prob_fill_on_limit = "1".parse().unwrap();
    parameters.prob_slippage = "1".parse().unwrap();
    settings.latency_model = execution_models::latency(0);
    assert_eq!(
        simulation_models(&settings),
        Err(domain::DomainError::Invalid("native_simulation_parameters"))
    );
}

#[test]
fn simulation_cannot_borrow_another_account_capital_currency_or_return_window() {
    let (parameters, result) = simulation();
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    for dimension in 0..14 {
        let mut invalid = result.clone();
        match dimension {
            0 => {
                invalid.canonical_result["accounts"][0]["Cash"]["base"]["balances_starting"]
                    ["USD"] = json!("1001.00 USD")
            }
            1 => {
                invalid.canonical_result["accounts"][0]["Cash"]["base"]["base_currency"] =
                    json!("EUR")
            }
            2 => {
                invalid.canonical_result["accounts"][0]["Cash"]["base"]["account_type"] =
                    json!("MARGIN")
            }
            3 => {
                invalid.canonical_result["portfolio_snapshots"][0]["account_id"] =
                    json!("OTHER-001")
            }
            4 => {
                invalid.canonical_result["portfolio_snapshots"][0]["total_equity"] =
                    json!(["1000.00 EUR"])
            }
            5 => invalid.canonical_result["run"]["total_orders"] = json!("1"),
            6 => invalid.canonical_result["run"]["backtest_start_ns"] = json!("0"),
            7 => invalid.canonical_result["portfolio_snapshots"][0]["ts_event"] = json!("0"),
            8 => invalid.consumed_target_points = count(2),
            9 => invalid.returns[0].timestamp_ns = count(1),
            10 => invalid.returns_reason = Some("MISSING".into()),
            11 => invalid.returns[0].value = None,
            12 => invalid
                .summary
                .insert("orders.open".into(), "1".into())
                .map(|_| ())
                .unwrap(),
            _ => invalid.canonical_result["accounts"] = json!([]),
        }
        assert!(
            !accepts(&parameters, "qz.native_simulation", &invalid),
            "simulation dimension {dimension}"
        );
    }
    let mut changed = parameters;
    if let NativeTaskParametersV1::SimulatePortfolio { request, .. } = &mut changed {
        request.settings.starting_capital = "999".parse().unwrap();
    }
    assert!(!accepts(&changed, "qz.native_simulation", &result));
}

#[test]
fn native_simulation_binding_validates_scoped_full_account_targets() {
    let (mut parameters, result) = simulation();
    let NativeTaskParametersV1::SimulatePortfolio { request, .. } = &mut parameters else {
        unreachable!()
    };
    request.target_points[0].rebalance_instrument_ids = Some(vec!["A.SIM".into()]);
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    for dimension in 0..6 {
        let mut invalid = parameters.clone();
        let NativeTaskParametersV1::SimulatePortfolio { request, .. } = &mut invalid else {
            unreachable!()
        };
        let point = &mut request.target_points[0];
        match dimension {
            0 => point.rebalance_instrument_ids = Some(vec![]),
            1 => point.rebalance_instrument_ids = Some(vec!["A.SIM".into(), "A.SIM".into()]),
            2 => point.rebalance_instrument_ids = Some(vec!["OTHER.SIM".into()]),
            3 => point.cash_weight = "0.9".parse().unwrap(),
            4 => point.targets[0].weight = "-0.1".parse().unwrap(),
            _ => point.cash_weight = "1.1".parse().unwrap(),
        }
        assert!(
            !accepts(&invalid, "qz.native_simulation", &result),
            "scope binding dimension {dimension}"
        );
    }
}

#[test]
fn complete_native_snapshot_collection_crosses_the_old_million_point_gate() {
    let (parameters, mut result) = simulation();
    let first = result.canonical_result["portfolio_snapshots"][0].clone();
    let start = result.canonical_result["run"]["backtest_start_ns"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let snapshots: Vec<_> = (0..1_000_003)
        .map(|index| {
            let mut snapshot = first.clone();
            snapshot["ts_event"] = json!((start + index).to_string());
            snapshot
        })
        .collect();
    result.canonical_result["portfolio_snapshots"] = snapshots.into();
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    result.canonical_result["portfolio_snapshots"][1_000_002]["account_id"] = json!("FOREIGN");
    assert!(!accepts(&parameters, "qz.native_simulation", &result));
}

#[test]
fn full_native_statistic_and_summary_collections_keep_binding_and_duplicate_checks() {
    let (parameters, mut result) = simulation();
    result.statistics = (0..4097)
        .map(|index| NativeStatisticV1 {
            group: NativeStatisticGroup::General,
            native_key: format!("native-statistic-{index}"),
            currency: None,
            value: Some(1.0),
            reason_code: None,
        })
        .collect();
    for index in 0..8190 {
        result
            .summary
            .insert(format!("native-summary-{index}"), "recorded".into());
    }
    result.canonical_result["summary"] = serde_json::to_value(&result.summary).unwrap();
    assert_eq!(result.summary.len(), 8193);
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    result.statistics.push(result.statistics[0].clone());
    assert!(!accepts(&parameters, "qz.native_simulation", &result));
    result.statistics.pop();
    result.canonical_result["summary"]["native-summary-0"] = json!("foreign");
    assert!(!accepts(&parameters, "qz.native_simulation", &result));
}
