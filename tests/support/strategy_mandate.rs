//! Controlled native metadata plus real Store execution-assumption and mandate commands.
#[path = "execution_assumptions.rs"]
mod execution;
#[path = "runtime_observation.rs"]
pub mod observation;
use contracts::{
    portfolio::AllocationInputV1,
    runtime::{RuntimeArtifactSchemaV1, RuntimeProbeOutcomeV1},
    strategy_portfolio::{
        StrategyAllocationMethodV1, StrategyMandateContentV1, StrategyMandateCreateV1,
    },
    SchemaV1,
};
pub use execution::{data, prepare as execution_request};
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
        serde_json::from_str(include_str!("../contracts/allocation-input.json")).unwrap();
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
