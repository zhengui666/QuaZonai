//! Controlled model-capital conditions only; no initialization receipt or account execution.
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_fixture;
use contracts::{
    forward::{DownstreamWeightsSubmitV1, ForwardEnvironmentV1},
    portfolio::*,
    science::*,
    strategy_portfolio::FreshPaperCashV1,
    DbCounter, Id,
};
use serde_json::json;

fn example() -> NativePortfolioBuildRequestV1 {
    let input: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    let mut request = portfolio_fixture::request(&input);
    let root = PaperInitializationRefV1 {
        artifact_id: request.current_weights_artifact_id,
        downstream_id: Id::new(),
        trader_id: "TRADER-001".into(),
        account_id: "SIM-001".into(),
    };
    request.current_weights.paper_initialization = Some(root.clone());
    request.current_weights.source = PortfolioWeightsSourceV1::PaperInitialCapital {
        account_start: FreshPaperCashV1 {
            downstream_id: root.downstream_id,
            trader_id: root.trader_id,
            account_id: root.account_id,
            base_currency: request.execution_settings.base_currency.clone(),
            starting_capital: request.execution_settings.starting_capital.clone(),
            execution_assumptions_id: request.mandate.execution_assumptions_id,
        },
    };
    request.current_weights.asof_ns = request.selection.decision_cutoff_ns;
    request.current_weights.available_ns = request.selection.decision_cutoff_ns;
    request.current_weights.valid_until_ns = DbCounter::new(
        request.selection.decision_cutoff_ns.get()
            + u64::from(request.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000,
    )
    .unwrap();
    request.current_weights.cash_weight = "1".parse().unwrap();
    for weight in &mut request.current_weights.weights {
        weight.weight = "0".parse().unwrap();
    }
    for asset in &mut request.assets {
        asset.current_weight = "0".parse().unwrap();
    }
    request
}

#[test]
fn initial_model_condition_is_self_identified_and_preserves_real_source_rules() {
    let request = example();
    domain::portfolio::paper_weights_source(&request).unwrap();
    domain::execution::portfolio_build_request(&request).unwrap();
    assert_eq!(
        request.current_weights.asof_ns,
        request.selection.decision_cutoff_ns
    );
    assert_eq!(
        request.current_weights.available_ns,
        request.selection.decision_cutoff_ns
    );
    // There is no backdated receipt field: actual creation time stays in the immutable Store record.
    assert!(serde_json::to_value(&request.current_weights)
        .unwrap()
        .get("created_at")
        .is_none());
}

#[test]
fn missing_root_wrong_scope_capital_nonzero_holdings_and_extended_clocks_fail() {
    let original = example();
    for case in 0..18 {
        let mut changed = original.clone();
        match case {
            0 => changed.current_weights.paper_initialization = None,
            1 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .artifact_id = Id::new()
            }
            2 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .downstream_id = Id::new()
            }
            3 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .trader_id = "OTHER-001".into()
            }
            4 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .account_id = "OTHER-001".into()
            }
            5 => changed.execution_settings.starting_capital = "1".parse().unwrap(),
            6 => changed.mandate.capital_assumption = "1".parse().unwrap(),
            7 => changed.mandate.execution_assumptions_id = Id::new(),
            8 => changed.current_weights.base_currency = "USDT".into(),
            9 => changed.current_weights.cash_weight = "0.9".parse().unwrap(),
            10 => changed.current_weights.weights[0].weight = "0.1".parse().unwrap(),
            11 => changed.current_weights.asof_ns = DbCounter::ZERO,
            12 => changed.current_weights.available_ns = DbCounter::ZERO,
            13 => {
                changed.current_weights.valid_until_ns =
                    DbCounter::new(changed.current_weights.valid_until_ns.get() + 1).unwrap()
            }
            14 => {
                changed.current_weights.valid_until_ns =
                    DbCounter::new(changed.current_weights.valid_until_ns.get() - 1).unwrap()
            }
            15 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .trader_id = " TRADER-001".into()
            }
            16 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .account_id = "SIM-001 ".into()
            }
            _ => {
                let PortfolioWeightsSourceV1::PaperInitialCapital { account_start } =
                    &mut changed.current_weights.source
                else {
                    unreachable!()
                };
                account_start.starting_capital = "0".parse().unwrap();
            }
        }
        assert!(
            domain::portfolio::paper_weights_source(&changed).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn lineage_compares_original_root_and_downstream_without_mandate_or_session_reset_keys() {
    let request = example();
    let original = request
        .current_weights
        .paper_initialization
        .as_ref()
        .unwrap();
    domain::portfolio::paper_initialization_binding(original, original, original.downstream_id)
        .unwrap();
    for case in 0..4 {
        let mut changed = original.clone();
        match case {
            0 => changed.artifact_id = Id::new(),
            1 => changed.downstream_id = Id::new(),
            2 => changed.trader_id = "OTHER-001".into(),
            _ => changed.account_id = "OTHER-001".into(),
        }
        assert!(domain::portfolio::paper_initialization_binding(
            &changed,
            original,
            original.downstream_id
        )
        .is_err());
    }
    assert!(
        domain::portfolio::paper_initialization_binding(original, original, Id::new()).is_err()
    );
    let mut later = request.clone();
    later.current_weights.source = PortfolioWeightsSourceV1::LastTarget {
        candidate_id: Id::new(),
    };
    later.current_weights_artifact_id = Id::new();
    domain::portfolio::paper_weights_source(&later).unwrap();
    assert_eq!(
        later.current_weights.paper_initialization.as_ref(),
        Some(original)
    );
    later.current_weights.source = PortfolioWeightsSourceV1::ForwardSnapshot {
        downstream_id: Id::new(),
        external_message_id: "original".into(),
    };
    assert!(domain::portfolio::paper_weights_source(&later).is_err());
}

#[test]
fn live_initialization_and_live_rooted_snapshots_are_rejected_without_changing_snapshot_clocks() {
    let request = example();
    let mut snapshot:DownstreamWeightsSubmitV1=serde_json::from_value(json!({"schema_version":1,"project_id":Id::new(),"environment":"PAPER","external_message_id":"original",
        "paper_initialization":request.current_weights.paper_initialization,"asof_ns":"10","available_ns":"11","valid_until_ns":"12","base_currency":request.current_weights.base_currency,
        "cash_weight":"1","weights":request.current_weights.weights})).unwrap();
    domain::forward::weights(&snapshot).unwrap();
    assert_eq!(snapshot.asof_ns.get(), 10);
    assert_eq!(snapshot.available_ns.get(), 11);
    snapshot.environment = ForwardEnvironmentV1::Live;
    assert!(domain::forward::weights(&snapshot).is_err());
    snapshot.paper_initialization = None;
    domain::forward::weights(&snapshot).unwrap(); // Legacy real snapshot shape still follows its original rules.
}
