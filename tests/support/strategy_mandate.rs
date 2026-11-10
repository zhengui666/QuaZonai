//! Controlled native metadata plus real Store execution-assumption and mandate commands.
#[path = "execution_assumptions.rs"]
mod execution;
use contracts::{
    portfolio::AllocationInputV1,
    runtime::{RuntimeArtifactSchemaV1, RuntimeProbeOutcomeV1},
    strategy_portfolio::{
        StrategyAllocationMethodV1, StrategyMandateContentV1, StrategyMandateCreateV1,
    },
    SchemaV1,
};
pub use execution::{data, observation, prepare as execution_request};
use sqlx::PgPool;

pub async fn prepare(
    pool: &PgPool,
    data: data::Fixture,
) -> (data::Fixture, StrategyMandateCreateV1) {
    let (f, execution) = execution_request(pool, data).await;
    from_execution(pool, f, execution).await
}

pub async fn from_execution(
    pool: &PgPool,
    f: data::Fixture,
    execution: contracts::execution_assumptions::ExecutionAssumptionsCreateV1,
) -> (data::Fixture, StrategyMandateCreateV1) {
    let saved = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "strategy-execution",
            &execution,
            |id, size| data::read(f.objects.clone(), id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| store::StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    sqlx::query("UPDATE app.runtime_integrations SET allowed_capabilities=ARRAY['PORTFOLIO_SIMULATE','PORTFOLIO_BUILD','DATA_VALIDATE'] WHERE id=$1")
        .bind(f.runtime.id.as_uuid()).execute(pool).await.unwrap();
    let mut cap = observation::configured_capabilities(pool, f.runtime.id).await;
    cap.engine_versions
        .insert("strategy-composition".into(), "1".into());
    cap.engine_versions.insert(
        "nautilus".into(),
        contracts::portfolio::NAUTILUS_EXECUTION_VERSION.into(),
    );
    cap.artifact_schemas.push(RuntimeArtifactSchemaV1 {
        name: "qz.strategy_portfolio".into(),
        version: "1".into(),
    });
    let revision = observation::publish(
        pool,
        f.runtime.id,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(cap),
        },
        chrono::Duration::seconds(60),
    )
    .await;
    let allocation: AllocationInputV1 =
        serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###).unwrap();
    let mut constraints = allocation.constraints;
    constraints.group_bounds.clear();
    constraints.asset_overrides.clear();
    constraints.max_ex_ante_risk = None;
    constraints.max_participation = None;
    constraints.liquidity_ref = None;
    constraints.transaction_costs_ref = saved.fee_schedule_artifact_id;
    let dataset = f
        .store
        .get_dataset_revision(&f.actor, saved.dataset_revision_id)
        .await
        .unwrap();
    let request = StrategyMandateCreateV1 {
        schema_version: SchemaV1,
        project_id: f.project,
        runtime_id: f.runtime.id,
        expected_runtime_revision: revision,
        content: StrategyMandateContentV1 {
            schema_version: SchemaV1,
            allocation_method: StrategyAllocationMethodV1::FixedTargetWeights,
            base_currency: saved.settings.base_currency,
            capital_assumption: saved.settings.starting_capital,
            universe_version_id: dataset.universe_version_id,
            execution_assumptions_id: saved.id,
            constraints,
            exposure_tolerance: saved.settings.exposure_tolerance,
            max_input_age_seconds: 60,
            target_ttl_seconds: 300,
        },
    };
    (f, request)
}
