//! Actual PostgreSQL cutover boundaries, original history and no hidden Agent work.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/experiments.rs"]
mod proposal_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
mod support;
use contracts::{control::ListQuery, Id};
use sqlx::PgPool;
use store::{
    lifecycle::{RunMessage, RunSubmission},
    Store, StoreError,
};

async fn agent_facts(pool: &PgPool) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_array((SELECT count(*) FROM app.runs WHERE kind='AGENT_RESEARCH'),(SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM app.mission_resources),(SELECT count(*) FROM app.cycle_selections),(SELECT count(*) FROM app.qualifications),(SELECT count(*) FROM pgmq.q_model_turns))").fetch_one(pool).await.unwrap()
}
async fn message(pool: &PgPool, run: Id) -> RunMessage {
    let message_id: i64 =
        sqlx::query_scalar("SELECT msg_id FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(run.to_string())
            .fetch_one(pool)
            .await
            .unwrap();
    RunMessage {
        message_id,
        run_id: run,
        read_count: 1,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn data_and_historical_ack_only_archive_their_exact_original_receipt(pool: PgPool) {
    let (store, actor, f, cycle, preparation) = mission_support::setup(&pool).await;
    let before = agent_facts(&pool).await;
    mission_support::complete(&pool, &store, &f, preparation, false).await;
    let prepared = message(&pool, preparation).await;
    store.acknowledge_run(&prepared).await.unwrap();
    store.acknowledge_run(&prepared).await.unwrap();
    assert_eq!(
        before,
        agent_facts(&pool).await,
        "data completion does not admit a researcher"
    );
    let parent =
        experiment_support::historical_alpha::parent(&pool, &store, &actor, &f, cycle).await;
    assert!(matches!(
        store
            .renew_run_lease(parent.run.id, &parent.fence, 60)
            .await,
        Err(StoreError::Invalid("legacy_agent_execution_retired"))
    ));
    assert!(matches!(
        store
            .prepare_run_runtime_probe(parent.run.id, &parent.fence)
            .await,
        Err(StoreError::Invalid("legacy_agent_execution_retired"))
    ));
    experiment_support::historical_alpha::terminal(&pool, &parent).await;
    let original_run = store.get_run(&actor, parent.run.id).await.unwrap();
    let before = agent_facts(&pool).await;
    let original = message(&pool, parent.run.id).await;
    store.acknowledge_run(&original).await.unwrap();
    store.acknowledge_run(&original).await.unwrap();
    assert_eq!(
        before,
        agent_facts(&pool).await,
        "ACK cannot freeze selection or admit review/held-out/model resources"
    );
    assert_eq!(
        original_run,
        store.get_run(&actor, parent.run.id).await.unwrap()
    );
    assert!(matches!(
        store.cycle_selection(&actor, cycle).await,
        Err(StoreError::NotFound)
    ));
    let historical = store
        .historical_agent_run(&actor, parent.run.id)
        .await
        .unwrap();
    assert_eq!(historical["source"], "CODEX");
    assert!(historical["session"].is_null());
    let mut request = RunSubmission {
        cycle_id: cycle,
        input_set_id: f.freeze.execution_context.discovery_input_set_id,
        runtime_id: f.freeze.execution_context.runtime_id,
        runtime_revision: f.freeze.execution_context.runtime_revision,
        kind: contracts::runs::RunKind::AgentResearch,
        limits: parent.limits.clone(),
    };
    assert!(matches!(
        store.enqueue_run("retired-new-job", &request).await,
        Err(StoreError::Invalid("legacy_agent_execution_retired"))
    ));
    request.kind = contracts::runs::RunKind::DataValidate;
    request.limits = experiment_support::limits();
    request.limits.experiments = 0;
    store
        .enqueue_run("still-explicit-science", &request)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_unknown_usage_stays_null_and_reads_never_relabel_or_mutate_rows(pool: PgPool) {
    let f = support::historical_fixture(&pool, support::budget()).await;
    let store = Store::from_pool(pool.clone());
    let actor = store::authority::Actor::Browser {
        login_id: store.local_browser().await.unwrap().id,
    };
    let session: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.codex_sessions WHERE run_id=$1")
            .bind(f.run.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let reservation = Id::new();
    sqlx::query("INSERT INTO app.model_turn_reservations(id,project_id,cycle_id,run_id,session_id,attempt_id,command_key,turn_kind,reserved_tokens,request_artifact_id,deadline_at,owner_epoch,profile_revision,ordinal) VALUES($1,$2,$3,$4,$5,$6,'original-unresolved','RESEARCH',NULL,$7,$8,1,1,65536)")
        .bind(reservation.as_uuid()).bind(f.project.as_uuid()).bind(f.cycle.as_uuid()).bind(f.run.as_uuid()).bind(session).bind(f.fence.attempt_id.as_uuid()).bind(f.artifact.as_uuid()).bind(f.deadline).execute(&pool).await.unwrap();
    let before: serde_json::Value = sqlx::query_scalar("SELECT jsonb_build_object('run',(SELECT to_jsonb(r) FROM app.runs r WHERE r.id=$1),'session',(SELECT to_jsonb(s) FROM app.codex_sessions s WHERE s.run_id=$1),'turn',(SELECT to_jsonb(t) FROM app.model_turn_reservations t WHERE t.id=$2))").bind(f.run.as_uuid()).bind(reservation.as_uuid()).fetch_one(&pool).await.unwrap();
    let run = store.historical_agent_run(&actor, f.run).await.unwrap();
    assert_eq!(run["source"], "CODEX");
    assert_eq!(run["session"]["id"], session.to_string());
    assert_eq!(run["session"]["codex_version"], "historical-fixture");
    let turns = store
        .historical_agent_turns(&actor, f.run, &ListQuery::default())
        .await
        .unwrap();
    assert_eq!(turns.items.len(), 1);
    assert_eq!(turns.items[0]["ordinal"], "65536");
    assert!(turns.items[0]["reserved_tokens"].is_null());
    assert!(turns.items[0]["terminal"].is_null());
    assert!(turns.items[0]["usage"].is_null());
    let profile: Id = run["session"]["profile_id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let profile = store
        .historical_agent_profile(&actor, profile)
        .await
        .unwrap();
    assert!(profile.get("codex_home_ref").is_none());
    store.migrate().await.unwrap();
    assert!(store.renew_run_lease(f.run, &f.fence, 60).await.is_err());
    assert!(store
        .observe_run_running(f.run, &f.fence, "historical-fixture")
        .await
        .is_err());
    assert!(store
        .cancel_run(
            &actor,
            "not-a-cutover-outcome",
            f.run,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: contracts::SchemaV1,
                expected_revision: store.get_run(&actor, f.run).await.unwrap().revision,
            }
        )
        .await
        .is_err());
    let after: serde_json::Value = sqlx::query_scalar("SELECT jsonb_build_object('run',(SELECT to_jsonb(r) FROM app.runs r WHERE r.id=$1),'session',(SELECT to_jsonb(s) FROM app.codex_sessions s WHERE s.run_id=$1),'turn',(SELECT to_jsonb(t) FROM app.model_turn_reservations t WHERE t.id=$2))").bind(f.run.as_uuid()).bind(reservation.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    assert!(store.begin_run_dispatch(f.run, &f.fence).await.is_err());
    assert_eq!(
        store.get_run(&actor, f.run).await.unwrap().state,
        contracts::runs::RunState::Running
    );
    assert!(matches!(
        store.historical_agent_run(&actor, Id::new()).await,
        Err(StoreError::NotFound)
    ));
}
