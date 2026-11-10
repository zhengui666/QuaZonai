//! Controlled relational setup, not proof of OCI execution or REAL qualification.
#[path = "brief.rs"]
mod brief_support;
#[path = "runtime_observation.rs"]
pub mod observation;
#[path = "research.rs"]
pub mod research_support;
use contracts::{portfolio::*, runtime::RuntimeProbeOutcomeV1, Id, SchemaV1};
use sqlx::PgPool;
use store::{authority::Actor, Store};

pub async fn request(pool: &PgPool, store: &Store, actor: &Actor) -> MandateCreateV1 {
    let mut data = research_support::setup(pool, store, actor).await;
    sqlx::query("UPDATE app.runtime_integrations SET allowed_capabilities=ARRAY['PORTFOLIO_BUILD'] WHERE id=$1").bind(data.runtime.as_uuid()).execute(pool).await.unwrap();
    let mut cap = observation::configured_capabilities(pool, data.runtime).await;
    for (name, version) in [
        ("clarabel", CLARABEL_VERSION),
        ("ndarray", FIXED_ENSEMBLE_VERSION),
        ("ndarray-stats", SAMPLE_COVARIANCE_VERSION),
        ("portfolio-models", "4"),
    ] {
        cap.engine_versions.insert(name.into(), version.into());
    }
    cap.solver_capabilities = vec!["CONVEX_QP".into()];
    let assumptions = Id::new();
    sqlx::query("INSERT INTO app.execution_assumptions(id,venue_capability_ref,engine_image_ref,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref) SELECT $1,venue_capability_ref,$2,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref FROM app.execution_assumptions WHERE id=$3")
        .bind(assumptions.as_uuid()).bind(&cap.image_refs[0].image_ref).bind(data.assumptions.as_uuid()).execute(pool).await.unwrap();
    data.assumptions = assumptions;
    let brief = brief_support::request(store, actor, &data).await;
    let revision = observation::publish(
        pool,
        data.runtime,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(cap),
        },
        chrono::Duration::seconds(60),
    )
    .await;
    let allocation: AllocationInputV1 =
        serde_json::from_str(r###"{"schema_version":1,"forecasts":{"schema_version":1,"decision_asof_ns":"20000000000","forecast_asof_ns":"10000000000","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","max_input_age_seconds":10,"instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"members":[{"alpha_id":"01990000-0000-7000-8000-000000000011","alpha_version_id":"01990000-0000-7000-8000-000000000012","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]},{"alpha_id":"01990000-0000-7000-8000-000000000021","alpha_version_id":"01990000-0000-7000-8000-000000000022","forecast_unit":"RETURN_PER_HORIZON","horizon_kind":"FIXED_BARS","horizon_value":"4","base_currency":"USD","asof_ns":"10000000000","available_ns":"20000000000","ensemble_weight":"0.5","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"forecasts":[0.1,0.2]}]},"objective":"MIN_RISK","risk":"VARIANCE","base_currency":"USD","capital_assumption":"1000000","current_cash_weight":"0","exposure_tolerance":"0.000001","constraints":{"schema_version":1,"long_only":true,"min_cash_weight":"0","max_cash_weight":"0","min_asset_weight":"0","max_asset_weight":"1","max_gross_exposure":"1","min_net_exposure":"1","max_net_exposure":"1","max_turnover_per_rebalance":"2","max_participation":null,"max_ex_ante_risk":null,"group_bounds":[],"asset_overrides":[],"transaction_costs_ref":"01990000-0000-7000-8000-000000000001","liquidity_ref":null},"optimizer":{"schema_version":1,"adapter_kind":"CLARABEL_QP","upstream_class":"clarabel::solver::DefaultSolver","upstream_version":"0.11.1","parameters":{"schema_version":1,"risk_aversion":"1","max_iterations":200,"solver_tolerance":"0.0000000001","accept_inaccurate":false}},"alpha_ensemble":{"schema_version":1,"adapter_kind":"FIXED_WEIGHTED_FORECAST","upstream_class":"ndarray::ArrayBase::dot","upstream_version":"0.17.1","parameters":{}},"assets":[{"instrument_id":"ALPHA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-a"]},{"instrument_id":"BETA.EXAMPLE","currency":"USD","current_weight":"0.5","transaction_cost_rate":"0","available_notional":null,"groups":["sector-b"]}],"covariance_estimator":{"schema_version":1,"adapter_kind":"SAMPLE_COVARIANCE","upstream_class":"ndarray_stats::CorrelationExt::cov","upstream_version":"0.7.0","parameters":{"ddof":1}},"return_history":{"schema_version":1,"base_currency":"USD","horizon_kind":"FIXED_BARS","horizon_value":"4","instrument_ids":["ALPHA.EXAMPLE","BETA.EXAMPLE"],"bar_types":["ALPHA.EXAMPLE-1-MINUTE-LAST-EXTERNAL","BETA.EXAMPLE-1-MINUTE-LAST-EXTERNAL"],"end_ns":["100","200","300","400","500"],"available_ns":["100","200","300","400","500"],"asset_returns":[[0.0,2.0,0.0,2.0,1.0],[0.0,0.0,4.0,4.0,2.0]]}}"###).unwrap();
    let mut constraints = allocation.constraints;
    constraints.transaction_costs_ref = data.artifact;
    MandateCreateV1 {
        schema_version: SchemaV1,
        project_id: data.project,
        runtime_id: data.runtime,
        expected_runtime_revision: revision,
        content: MandateContentV1 {
            objective: allocation.objective,
            risk_measure: allocation.risk,
            base_currency: allocation.base_currency,
            capital_assumption: "100".parse().unwrap(),
            universe_version_id: data.universe,
            covariance_estimator: NativeModelRefV1::SampleCovariance {
                schema_version: SchemaV1,
                upstream_class: SAMPLE_COVARIANCE_CLASS.into(),
                upstream_version: SAMPLE_COVARIANCE_VERSION.into(),
                parameters: SampleCovarianceParametersV1 { ddof: 1 },
            },
            alpha_ensemble: allocation.alpha_ensemble,
            optimizer: allocation.optimizer,
            constraints,
            rebalance_schedule: RebalanceScheduleV1 {
                schema_version: SchemaV1,
                kind: RebalanceKind::Manual,
                interval_seconds: None,
                calendar_ref: None,
                timezone: "UTC".into(),
                session_offset_seconds: None,
                max_input_age_seconds: 60,
                target_ttl_seconds: 300,
            },
            required_evaluation_policy_id: brief.request.content.evaluation_policy_id,
            execution_assumptions_id: data.assumptions,
            exposure_tolerance: allocation.exposure_tolerance,
        },
    }
}
