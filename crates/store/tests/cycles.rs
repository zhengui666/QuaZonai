//! Formal startup against real PostgreSQL/PGMQ. Parent native capability/data
//! records are explicit fixtures, not T42 or production data attestations.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{
    brief::{BriefState, BriefUpdate},
    control::ListQuery,
    cycles::*,
    Id, SchemaV1,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::StoreError;

async fn counts(pool: &PgPool) -> (i64, i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.research_cycles),(SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM app.run_events),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.command_receipts WHERE operation='CYCLE_START')")
        .fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn formal_freeze_closes_brief_and_context_with_the_original_command_receipt(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    let before = store.project(&actor, f.data.project).await.unwrap();
    let frozen = store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    assert_eq!(frozen.resource.brief.state, BriefState::Frozen);
    assert!(frozen.resource.brief.frozen_at.is_some());
    assert_ne!(frozen.resource.brief.revision, f.brief.revision);
    assert_eq!(
        frozen.resource.execution_context,
        f.freeze.execution_context
    );
    let project = store.project(&actor, f.data.project).await.unwrap();
    assert_eq!(project.current_brief_id, Some(f.brief.id));
    assert_ne!(project.revision, before.revision);
    let replay = store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(replay.resource).unwrap(),
        serde_json::to_value(&frozen.resource).unwrap()
    );
    let request = BriefUpdate {
        schema_version: SchemaV1,
        expected_revision: frozen.resource.brief.revision,
        content: f.brief.content.clone(),
        bindings: f.brief.bindings.clone(),
    };
    assert!(store
        .update_brief(&actor, "mutate", f.brief.id, &request)
        .await
        .is_err());
    let mutation = sqlx::query("UPDATE app.brief_execution_contexts SET runtime_revision=runtime_revision+1 WHERE brief_id=$1")
        .bind(f.brief.id.as_uuid()).execute(&pool).await.unwrap_err();
    assert_eq!(
        mutation.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
    let visible = store.frozen_brief(&actor, f.brief.id).await.unwrap();
    assert_eq!(visible.execution_context, f.freeze.execution_context);
    let stored: Vec<(String, String)> =
        sqlx::query_as("SELECT origin,pit_status FROM app.dataset_revisions WHERE id=ANY($1)")
            .bind(vec![
                f.data.discovery.as_uuid(),
                f.data.validation.as_uuid(),
                f.data.sealed.as_uuid(),
            ])
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        stored
            .iter()
            .all(|(origin, pit)| origin == "FIXTURE" && pit == "UNVERIFIED"),
        "freeze must not convert fixture data into REAL/PIT evidence"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn freeze_rejects_revision_or_input_mismatch_without_a_half_frozen_context(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    let mut invalid = f.freeze.clone();
    invalid.expected_revision = "99".to_owned().try_into().unwrap();
    assert!(matches!(
        store
            .freeze_brief(&actor, "stale", f.brief.id, &invalid)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    invalid = f.freeze.clone();
    invalid.execution_context.validation_input_set_id =
        invalid.execution_context.discovery_input_set_id;
    assert!(store
        .freeze_brief(&actor, "wrong-input", f.brief.id, &invalid)
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.brief_execution_contexts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        store.brief(&actor, f.brief.id).await.unwrap().state,
        BriefState::Draft
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cycle_run_event_admission_queue_and_receipt_are_created_once(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let (a, b) = tokio::join!(
        store.start_cycle(&actor, "start", &request),
        store.start_cycle(&actor, "start", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        serde_json::to_value(&a.resource).unwrap(),
        serde_json::to_value(b.resource).unwrap()
    );
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 1));
    assert_eq!(a.resource.cycle.initial_run_id, Some(a.resource.run.id));
    assert_eq!(a.resource.run.cycle_id, Some(a.resource.cycle.id));
    assert_eq!(a.resource.cycle.reserved_experiments, 0);
    assert!(a.resource.cycle.reserved_cpu_seconds.get() > 0);
    let events = &a.resource.cycle.available_actions;
    assert_eq!(
        events,
        &[
            CycleReadAction::ViewBrief,
            CycleReadAction::ViewRuns,
            CycleReadAction::ViewExperiments
        ]
    );
    let page = store
        .cycles(&actor, f.data.project, &ListQuery::default())
        .await
        .unwrap();
    assert_eq!(page.items[0].id, a.resource.cycle.id);
    assert_eq!(
        store
            .cycle(&actor, a.resource.cycle.id)
            .await
            .unwrap()
            .initial_run_id,
        Some(a.resource.run.id)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn failure_after_queue_enqueue_rolls_back_the_entire_official_start_command(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    sqlx::query("CREATE FUNCTION app.fail_startup_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='injected failure after native queue publication'; END $$")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_startup_fixture BEFORE INSERT ON app.cycle_startups FOR EACH ROW EXECUTE FUNCTION app.fail_startup_fixture()")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        store.start_cycle(&actor, "atomic", &request).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));
    sqlx::query("DROP TRIGGER fail_startup_fixture ON app.cycle_startups")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !store
            .start_cycle(&actor, "atomic", &request)
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn frozen_inputs_do_not_retain_permission_after_revocation(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'TEST_REVOKED','test the current license authority')")
        .bind(f.data.grant.as_uuid()).execute(&pool).await.unwrap();
    assert!(store
        .start_cycle(&actor, "revoked", &request)
        .await
        .is_err());
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));
    assert_eq!(
        store.brief(&actor, f.brief.id).await.unwrap().state,
        BriefState::Frozen
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn daily_cycle_quota_and_paused_project_are_checked_in_the_start_transaction(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    let mut request = cycle_support::start_request(&store, &actor, &f).await;
    for number in 0..f.brief.content.budget.max_cycles_per_day {
        store
            .start_cycle(&actor, &format!("daily-{number}"), &request)
            .await
            .unwrap();
    }
    assert!(matches!(
        store.start_cycle(&actor, "quota", &request).await,
        Err(StoreError::Domain(domain::DomainError::BudgetExhausted(
            "cycles_per_day"
        )))
    ));
    let revision: i64 =
        sqlx::query_scalar("UPDATE app.projects SET state='PAUSED' WHERE id=$1 RETURNING revision")
            .bind(f.data.project.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    request.request.expected_revision = revision.to_string().try_into().unwrap();
    assert!(matches!(
        store.start_cycle(&actor, "paused", &request).await,
        Err(StoreError::Domain(domain::DomainError::AdmissionClosed))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn caller_cannot_commit_an_execution_context_without_freezing_the_brief(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    let c = &f.freeze.execution_context;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.brief_execution_contexts(brief_id,project_id,runtime_id,runtime_revision,discovery_input_set_id,validation_input_set_id,sealed_input_set_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(f.brief.id.as_uuid()).bind(f.data.project.as_uuid()).bind(c.runtime_id.as_uuid()).bind(c.runtime_revision.get() as i64)
        .bind(c.discovery_input_set_id.as_uuid()).bind(c.validation_input_set_id.as_uuid()).bind(c.sealed_input_set_id.as_uuid())
        .execute(&mut *tx).await.unwrap();
    let error = tx.commit().await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
}

#[test]
fn public_start_and_freeze_contracts_do_not_accept_success_or_runtime_override_fields() {
    let start = json!({"schema_version":1,"brief_id":Id::new(),"expected_revision":"1"});
    for field in [
        "outcome",
        "budget_snapshot",
        "state",
        "runtime_id",
        "run_id",
        "trigger",
        "wake_id",
    ] {
        let mut injected = start.clone();
        injected[field] = json!("injected");
        assert!(serde_json::from_value::<CycleStartV1>(injected).is_err());
    }
    let freeze: Value = json!({"schema_version":1,"expected_revision":"1","execution_context":{"schema_version":1,"runtime_id":Id::new(),"runtime_revision":"1","discovery_input_set_id":Id::new(),"validation_input_set_id":Id::new(),"sealed_input_set_id":Id::new()}});
    assert!(serde_json::from_value::<BriefFreezeV1>(freeze.clone()).is_ok());
    let mut injected = freeze;
    injected["capabilities"] = json!({"status":"AVAILABLE"});
    assert!(serde_json::from_value::<BriefFreezeV1>(injected).is_err());
}
