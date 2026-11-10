//! Structural virtual-capital wire tests; no account, funding or market evidence.
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_fixture;
use contracts::{
    delivery::PackageOriginV1,
    forward::DownstreamWeightsSubmitV1,
    portfolio::AllocationInputV1,
    science::{PaperInitializationRefV1, PortfolioCurrentWeightsV1, PortfolioWeightsSourceV1},
    strategy_portfolio::FreshPaperCashV1,
    Id,
};
use serde_json::{json, Value};

fn historical_weights() -> PortfolioCurrentWeightsV1 {
    let input: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    portfolio_fixture::request(&input).current_weights
}

fn root() -> PaperInitializationRefV1 {
    PaperInitializationRefV1 {
        artifact_id: Id::new(),
        downstream_id: Id::new(),
        trader_id: "TRADER-001".into(),
        account_id: "SIM-001".into(),
    }
}

#[test]
fn old_weights_keep_original_bytes_shape_and_no_inferred_initialization() {
    let old = serde_json::to_value(historical_weights()).unwrap();
    assert!(old.get("paper_initialization").is_none());
    let restored: PortfolioCurrentWeightsV1 = serde_json::from_value(old.clone()).unwrap();
    assert!(restored.paper_initialization.is_none());
    assert_eq!(serde_json::to_value(restored).unwrap(), old);
}

#[test]
fn declared_model_condition_and_root_are_explicit_and_exact() {
    let mut weights = historical_weights();
    let root = root();
    weights.paper_initialization = Some(root.clone());
    weights.source = PortfolioWeightsSourceV1::PaperInitialCapital {
        account_start: FreshPaperCashV1 {
            downstream_id: root.downstream_id,
            trader_id: root.trader_id.clone(),
            account_id: root.account_id.clone(),
            base_currency: "USD".into(),
            starting_capital: "1000".parse().unwrap(),
            execution_assumptions_id: Id::new(),
        },
    };
    weights.asof_ns = contracts::DbCounter::new(9_007_199_254_740_993).unwrap();
    let wire = serde_json::to_value(&weights).unwrap();
    let restored: PortfolioCurrentWeightsV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(restored, weights);
    assert_eq!(wire["asof_ns"], json!("9007199254740993"));
    for field in ["paper_initialization", "source"] {
        let mut changed = wire.clone();
        changed[field]["unrecognized"] = json!(true);
        assert!(serde_json::from_value::<PortfolioCurrentWeightsV1>(changed).is_err());
    }
    for field in ["artifact_id", "downstream_id", "trader_id", "account_id"] {
        let mut changed = wire.clone();
        changed["paper_initialization"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<PortfolioCurrentWeightsV1>(changed).is_err());
    }
    assert_eq!(
        serde_json::to_value(PackageOriginV1::Synthetic).unwrap(),
        json!("SYNTHETIC")
    );
    assert_ne!(
        serde_json::to_value(PackageOriginV1::Demo).unwrap(),
        json!("SYNTHETIC")
    );
}

#[test]
fn snapshot_lineage_is_only_a_reference_and_historical_omission_is_preserved() {
    let weights = historical_weights();
    let wire = json!({"schema_version":1,"project_id":Id::new(),"environment":"PAPER",
        "external_message_id":"native-observation","asof_ns":weights.asof_ns,
        "available_ns":weights.available_ns,"valid_until_ns":weights.valid_until_ns,
        "base_currency":weights.base_currency,"cash_weight":weights.cash_weight,"weights":weights.weights});
    let old: DownstreamWeightsSubmitV1 = serde_json::from_value(wire.clone()).unwrap();
    assert!(old.paper_initialization.is_none());
    assert_eq!(serde_json::to_value(old).unwrap(), wire);
    let mut linked = wire;
    linked["paper_initialization"] = serde_json::to_value(root()).unwrap();
    let parsed: DownstreamWeightsSubmitV1 = serde_json::from_value(linked.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), linked);
    linked["paper_initialization"]["starting_capital"] = Value::String("invented".into());
    assert!(serde_json::from_value::<DownstreamWeightsSubmitV1>(linked).is_err());
}
