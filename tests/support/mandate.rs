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
        ("portfolio-models", "2"),
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
        serde_json::from_str(include_str!("../contracts/allocation-input.json")).unwrap();
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
