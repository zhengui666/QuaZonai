//! Scientific admission must bind the entire original frozen budget snapshot.
#[path = "../../../tests/support/runtime_observation.rs"]
mod runtime_observation;
mod support;
use contracts::{DbCounter, Id, Revision, SchemaV1};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::{lifecycle::RunSubmission, Store, StoreError};
use support::*;

async fn cycle(pool: &PgPool, f: &Fixture, snapshot: Value) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot) SELECT $1,project_id,brief_id,(SELECT max(ordinal)+1 FROM app.research_cycles WHERE project_id=$2),'OPERATOR','RUNNING',$3 FROM app.research_cycles WHERE id=$4")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(snapshot).bind(f.cycle.as_uuid()).execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_budget_field_is_bound_to_the_frozen_brief_before_reservation(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let store = Store::from_pool(pool.clone());
    let runtime = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'scientific budget fixture','https://runtime.example','SYSTEM_CA','fixture',ARRAY['DATA_VALIDATE'],'1',true)")
        .bind(runtime.as_uuid()).execute(&pool).await.unwrap();
    runtime_observation::ready(&pool, runtime).await;
    let mut request = RunSubmission {
        cycle_id: f.cycle,
        input_set_id: f.input_set,
        runtime_id: runtime,
        runtime_revision: Revision::INITIAL,
        kind: contracts::runs::RunKind::DataValidate,
        limits: contracts::lifecycle::JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: Some(DbCounter::new(1).unwrap()),
            wall_seconds: Some(60),
            memory_mib: Some(1024),
            output_bytes: Some(DbCounter::new(1024).unwrap()),
        },
    };
    for (field, value) in [
        ("max_tokens", json!("1000")),
        ("max_cost_decimal", json!("100")),
        ("cost_currency", json!("EUR")),
        ("cost_enforcement", json!("UNAVAILABLE")),
        ("max_turns_per_mission", json!(10)),
        ("max_repair_turns", json!(2)),
        ("max_experiments", json!(30)),
        ("max_parallel_runs", json!(4)),
        ("max_wall_seconds", json!(7200)),
        ("max_cpu_seconds", json!("14400")),
        ("max_memory_mib", json!(8192)),
        ("max_output_bytes", json!("134217728")),
        ("max_cycles_per_day", json!(6)),
        ("min_cycle_interval_seconds", json!(1)),
    ] {
        let mut changed = serde_json::to_value(&f.budget).unwrap();
        changed[field] = value;
        request.cycle_id = cycle(&pool, &f, changed).await;
        assert!(
            matches!(
                store.enqueue_run(field, &request).await,
                Err(StoreError::Invalid("frozen_budget_snapshot_mismatch"))
            ),
            "{field}"
        );
    }
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM pgmq.q_runs)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0));
    request.cycle_id = f.cycle;
    store
        .enqueue_run("original-science-budget", &request)
        .await
        .unwrap();
    let agent_facts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs WHERE kind='AGENT_RESEARCH'),(SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.model_turn_reservations)").fetch_one(&pool).await.unwrap();
    assert_eq!(agent_facts, (0, 0, 0));
}
