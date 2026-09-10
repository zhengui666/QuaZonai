//! Transaction-composition regressions against real PostgreSQL/PGMQ. The shared
//! fixture is relational test input, not a production Brief-freeze or Agent flow.
mod support;

use contracts::{lifecycle::JobLimitsV1, runs::RunKind, DbCounter, Id, Revision, SchemaV1};
use domain::DomainError;
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;
use store::{lifecycle::RunSubmission, Store, StoreError};

async fn setup(pool: &PgPool) -> (support::Fixture, RunSubmission) {
    let fixture = support::fixture(pool, support::budget()).await;
    let runtime = Id::new();
    sqlx::query(
        "INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,\
         allowed_capabilities,protocol_version,enabled) \
         VALUES($1,'transaction fixture','https://runtime.example','SYSTEM_CA',\
         'fixture-credential',ARRAY['ALPHA_EVALUATE'],'1',true)",
    )
    .bind(runtime.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    let request = RunSubmission {
        cycle_id: Id::new(),
        input_set_id: fixture.input_set,
        runtime_id: runtime,
        runtime_revision: Revision::INITIAL,
        kind: RunKind::AlphaEvaluate,
        limits: JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 1,
            cpu_seconds: DbCounter::new(100).unwrap(),
            wall_seconds: 3600,
            memory_mib: 1024,
            output_bytes: DbCounter::new(4096).unwrap(),
        },
    };
    (fixture, request)
}

/// Model a caller which has begun its larger domain transaction and staged a
/// new Cycle. Reuse the frozen fixture; do not introduce a runtime SQL seed API.
async fn stage_cycle(
    pool: &PgPool,
    fixture: &support::Fixture,
    cycle: Id,
) -> Transaction<'static, Postgres> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
        .bind(fixture.project.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let created = sqlx::query(
        "INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot) \
         SELECT $1,project_id,brief_id,2,'OPERATOR','RUNNING',budget_snapshot \
         FROM app.research_cycles WHERE id=$2",
    )
    .bind(cycle.as_uuid())
    .bind(fixture.cycle.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(created.rows_affected(), 1);
    tx
}

/// This pool query uses a different connection while the writer owns its Tx.
async fn visible_counts(pool: &PgPool, cycle: Id) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT \
         (SELECT count(*) FROM app.research_cycles WHERE id=$1),\
         (SELECT count(*) FROM app.runs WHERE cycle_id=$1),\
         (SELECT count(*) FROM app.run_admissions WHERE cycle_id=$1),\
         (SELECT count(*) FROM app.run_events e JOIN app.runs r ON r.id=e.run_id WHERE r.cycle_id=$1),\
         (SELECT count(*) FROM pgmq.q_runs)",
    )
    .bind(cycle.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn caller_commit_publishes_cycle_run_budget_event_and_queue_together(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    let store = Store::from_pool(pool.clone());
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (mut tx, admitted) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(!admitted.replayed);
    assert_eq!(admitted.resource.cycle_id, Some(request.cycle_id));
    assert_eq!(admitted.resource.last_event_seq.get(), 1);
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );
    assert!(store.read_run_messages(30, 100).await.unwrap().is_empty());
    let usage: (i64, i64, i64) = sqlx::query_as(
        "SELECT reserved_experiments::bigint,used_experiments::bigint,reserved_cpu_seconds::bigint \
         FROM app.research_cycles WHERE id=$1",
    )
    .bind(request.cycle_id.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(usage, (1, 0, 100));
    tx.commit().await.unwrap();

    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (1, 1, 1, 1, 1)
    );
    let replay = store.enqueue_run("initial", &request).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, admitted.resource);
    let messages = store.read_run_messages(30, 100).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].run_id, admitted.resource.id);
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (1, 1, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn fresh_admission_and_same_transaction_replay_never_commit_the_caller(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (tx, first) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    let (tx, replay) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, first.resource);
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );

    // Rollback must also discard the idempotency binding and budget reservation.
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (tx, retried) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(!retried.replayed);
    assert_ne!(retried.resource.id, first.resource.id);
    tx.commit().await.unwrap();
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (1, 1, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn queue_error_drops_the_whole_caller_transaction_and_can_retry(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    sqlx::raw_sql(
        "CREATE FUNCTION public.reject_initial_queue() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected queue failure'; END $$; \
         CREATE TRIGGER reject_initial_queue BEFORE INSERT ON pgmq.q_runs \
         FOR EACH ROW EXECUTE FUNCTION public.reject_initial_queue();",
    )
    .execute(&pool)
    .await
    .unwrap();
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    assert!(matches!(
        Store::enqueue_run_in_transaction(tx, "initial", &request).await,
        Err(StoreError::Database(_))
    ));
    // No caller-owned Tx survives the error, even if the caller handles it.
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );
    sqlx::query("DROP TRIGGER reject_initial_queue ON pgmq.q_runs")
        .execute(&pool)
        .await
        .unwrap();
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (tx, retried) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(!retried.replayed);
    tx.commit().await.unwrap();
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (1, 1, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn budget_rejection_and_invalid_key_discard_the_staged_cycle(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let mut invalid = request.clone();
    invalid.limits.experiments = fixture.budget.max_experiments + 1;
    assert!(matches!(
        Store::enqueue_run_in_transaction(tx, "initial", &invalid).await,
        Err(StoreError::Domain(DomainError::BudgetExhausted(
            "experiments"
        )))
    ));
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );

    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    assert!(matches!(
        Store::enqueue_run_in_transaction(tx, " invalid ", &request).await,
        Err(StoreError::Invalid("idempotency_key"))
    ));
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn conflicting_replay_discards_all_preceding_caller_writes(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (tx, admitted) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(!admitted.replayed);
    let mut different = request.clone();
    different.limits.experiments = 2;
    assert!(matches!(
        Store::enqueue_run_in_transaction(tx, "initial", &different).await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn deferred_commit_failure_does_not_publish_a_provisional_run(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    // A caller can add domain facts after admission. Their deferred failure must
    // still roll back the new Cycle, initial Run and native queue message.
    sqlx::query(
        "CREATE TABLE public.commit_barrier (cycle_id uuid NOT NULL \
         REFERENCES app.research_cycles(id) DEFERRABLE INITIALLY DEFERRED)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (mut tx, admitted) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(!admitted.replayed);
    sqlx::query("INSERT INTO public.commit_barrier(cycle_id) VALUES($1)")
        .bind(Id::new().as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    let error = tx.commit().await.unwrap_err();
    assert!(error
        .as_database_error()
        .is_some_and(|error| error.code().as_deref() == Some("23503")));
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );
    assert!(Store::from_pool(pool)
        .read_run_messages(30, 100)
        .await
        .unwrap()
        .is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn replay_of_committed_run_leaves_new_caller_changes_uncommitted(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (tx, initial) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let original: String = sqlx::query_scalar("SELECT name FROM app.projects WHERE id=$1")
        .bind(fixture.project.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE app.projects SET name='not committed by replay' WHERE id=$1")
        .bind(fixture.project.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    let (tx, replay) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, initial.resource);
    let visible: String = sqlx::query_scalar("SELECT name FROM app.projects WHERE id=$1")
        .bind(fixture.project.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(visible, original);
    tx.rollback().await.unwrap();
    let unchanged: String = sqlx::query_scalar("SELECT name FROM app.projects WHERE id=$1")
        .bind(fixture.project.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(unchanged, original);
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (1, 1, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelling_blocked_admission_rolls_back_the_staged_cycle(pool: PgPool) {
    let (fixture, request) = setup(&pool).await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.runtime_integrations WHERE id=$1 FOR UPDATE")
        .bind(request.runtime_id.as_uuid())
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    let mut tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let pending = request.clone();
    let task =
        tokio::spawn(
            async move { Store::enqueue_run_in_transaction(tx, "initial", &pending).await },
        );
    // Wait for the actual database lock, not an assumed scheduler sleep.
    let observed_wait = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event_type='Lock')",
            )
            .bind(backend)
            .fetch_one(&pool)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    task.abort();
    let outcome = task.await;
    blocker.rollback().await.unwrap();
    assert!(
        observed_wait.is_ok(),
        "admission must reach the held runtime lock"
    );
    assert!(matches!(outcome, Err(error) if error.is_cancelled()));
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (0, 0, 0, 0, 0)
    );

    let tx = stage_cycle(&pool, &fixture, request.cycle_id).await;
    let (tx, retried) = Store::enqueue_run_in_transaction(tx, "initial", &request)
        .await
        .unwrap();
    assert!(!retried.replayed);
    tx.commit().await.unwrap();
    assert_eq!(
        visible_counts(&pool, request.cycle_id).await,
        (1, 1, 1, 1, 1)
    );
}
