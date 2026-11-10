use super::*;
use contracts::portfolio::*;

#[test]
fn native_conic_solution_meets_correlated_contribution_tolerance() {
    let input: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    let budget = RiskBudgetSettingsV1 {
        schema_version: contracts::SchemaV1,
        risky_gross_exposure: "1".parse().unwrap(),
        assets: input
            .assets
            .iter()
            .zip(["0.2", "0.8"])
            .map(|(a, share)| RiskBudgetAssetV1 {
                instrument_id: a.instrument_id.clone(),
                share: share.parse().unwrap(),
                sign: RiskBudgetSign::Long,
            })
            .collect(),
    };
    let native = risk_budgeting::solve(
        &input,
        &[vec![1.0, 1.0], vec![1.0, 5.0]],
        &Cholesky::new(DMatrix::from_row_slice(2, 2, &[1.0, 1.0, 1.0, 5.0]))
            .unwrap()
            .l(),
        &budget,
        domain::portfolio::optimizer_settings(&input.optimizer).unwrap(),
    )
    .unwrap();
    assert_eq!(native.status, clarabel::solver::SolverStatus::Solved);
    let w = &native.x;
    let variance = w[0] * w[0] + 2.0 * w[0] * w[1] + 5.0 * w[1] * w[1];
    let share = w[0] * (w[0] + w[1]) / variance;
    assert!(
        (share - 0.2).abs() <= 1e-6,
        "share={share:.16}, weights={:?}, status={:?}",
        &w[..2],
        native.status
    );
}

#[test]
fn native_conic_catalog_scale() {
    let input: AllocationInputV1 = serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###)
    .unwrap();
    let returns: Vec<Vec<f64>> = [1.0, 2.0]
        .into_iter()
        .map(|base| {
            (1..=18)
                .map(|i| (base + (i + 2) as f64 * 0.001) / (base + i as f64 * 0.001) - 1.0)
                .collect()
        })
        .collect();
    let covariance =
        domain::portfolio::sample_covariance(&input.covariance_estimator, &returns).unwrap();
    let budget = RiskBudgetSettingsV1 {
        schema_version: SchemaV1,
        risky_gross_exposure: "1".parse().unwrap(),
        assets: input
            .assets
            .iter()
            .map(|a| RiskBudgetAssetV1 {
                instrument_id: a.instrument_id.clone(),
                share: "0.5".parse().unwrap(),
                sign: RiskBudgetSign::Long,
            })
            .collect(),
    };
    let lower = Cholesky::new(DMatrix::from_fn(2, 2, |i, j| covariance[i][j]))
        .unwrap()
        .l();
    let native = risk_budgeting::solve(
        &input,
        &covariance,
        &lower,
        &budget,
        domain::portfolio::optimizer_settings(&input.optimizer).unwrap(),
    )
    .unwrap();
    let w = &native.x;
    let contribution = w[0] * (covariance[0][0] * w[0] + covariance[0][1] * w[1]);
    let variance = contribution + w[1] * (covariance[1][0] * w[0] + covariance[1][1] * w[1]);
    assert_eq!(native.status, clarabel::solver::SolverStatus::Solved);
    assert!(
        (contribution / variance - 0.5).abs() <= 1e-6,
        "share={}, covariance={covariance:?}, weights={:?}",
        contribution / variance,
        &w[..2]
    );
}
