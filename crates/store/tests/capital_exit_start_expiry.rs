//! Real PostgreSQL lock races; fixture envelopes do not establish native execution.
mod capital_exit_support;
use capital_exit_support::*;
use chrono::{DateTime, Utc};
use contracts::{DbCounter, SchemaV1, capital_exit::*};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use std::time::Duration;
use store::{Store, StoreError};

#[derive(Clone, Copy)]
enum WaitAt {
    Cursor,
    Reservation,
    Queue,
}
#[derive(Clone, Copy)]
enum Expire {
    Preview,
    Assessment,
}

async fn clock(pool: &PgPool) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn after_deadline(pool: &PgPool, deadline: DateTime<Utc>) {
    sqlx::query("SELECT pg_sleep(GREATEST(0, EXTRACT(EPOCH FROM ($1::timestamptz-clock_timestamp()))::double precision)+0.05)")
        .bind(deadline).execute(pool).await.unwrap();
    assert!(clock(pool).await > deadline);
}
fn start_request(preview: &CapitalExitPreviewV1) -> CapitalExitStartV1 {
    CapitalExitStartV1 {
        schema_version: SchemaV1,
        preview_id: preview.id,
        expected_account_control_revision: preview.expected_account_control_revision.unwrap(),
        acknowledged_plan_artifact_id: preview.plan_artifact_id,
        expected_source_observation_id: preview.original_observation_id,
    }
}
async fn bounded_start_store(pool: &PgPool) -> Store {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _| {
            Box::pin(async move {
                // Match the production timeouts: expiry must reject, not a lock timeout.
                sqlx::query("SELECT set_config('lock_timeout','5s',false), set_config('statement_timeout','15s',false)")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    Store::from_pool(pool)
}
async fn block_start<'a>(
    pool: &'a PgPool,
    f: &Fixture,
    at: WaitAt,
) -> (Transaction<'a, Postgres>, i32, &'static str) {
    let mut blocker = pool.begin().await.unwrap();
    let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    let query = match at {
        WaitAt::Cursor => {
            sqlx::query(
                "SELECT source_id FROM app.native_account_cursors WHERE source_id=$1 FOR UPDATE",
            )
            .bind(f.source.as_uuid())
            .execute(&mut *blocker)
            .await
            .unwrap();
            "SELECT s.downstream_id,%"
        }
        WaitAt::Reservation => {
            sqlx::query("SELECT managed_account_key FROM app.managed_capital_reservations WHERE managed_account_key=$1 FOR UPDATE")
                .bind(&f.registration.managed_account_key).execute(&mut *blocker).await.unwrap();
            "SELECT r.*,%"
        }
        WaitAt::Queue => {
            sqlx::query("LOCK TABLE pgmq.q_capital_exits IN SHARE MODE")
                .execute(&mut *blocker)
                .await
                .unwrap();
            "SELECT pgmq.send('capital_exits',%"
        }
    };
    (blocker, pid, query)
}
async fn observe_wait(pool: &PgPool, blocker_pid: i32, query: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND $1=ANY(pg_blocking_pids(pid)) AND query LIKE $2)")
                .bind(blocker_pid).bind(query).fetch_one(pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("start must actually wait on the selected PostgreSQL lock");
}
async fn reservation_snapshot(pool: &PgPool, f: &Fixture) -> Value {
    sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM app.managed_capital_reservations r WHERE managed_account_key=$1",
    )
    .bind(&f.registration.managed_account_key)
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn committed_counts(pool: &PgPool) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.capital_exit_intents),(SELECT count(*) FROM app.command_receipts WHERE operation='CAPITAL_EXIT_START'),(SELECT count(*) FROM pgmq.q_capital_exits)")
        .fetch_one(pool).await.unwrap()
}
fn assert_expired(
    result: Result<contracts::control::CommandResult<CapitalExitViewV1>, StoreError>,
    expires: Expire,
) {
    match (result, expires) {
        (
            Err(StoreError::Domain(domain::DomainError::Invalid("capital_exit_preview_expired"))),
            Expire::Preview,
        )
        | (Err(StoreError::Invalid("capital_exit_preview_stale")), Expire::Assessment) => {}
        (other, _) => panic!("expired start must reject before any 202 result: {other:?}"),
    }
}

async fn expiry_race(pool: PgPool, at: WaitAt, expires: Expire) {
    let f = setup(&pool, true).await;
    let store = bounded_start_store(&pool).await;
    let deadline = clock(&pool).await + chrono::Duration::seconds(3);
    let mut assessment = f.assessment();
    if matches!(expires, Expire::Preview) {
        assessment.valid_until = deadline;
    }
    f.store
        .submit_capital_exit_assessment(&f.actor, "assessment-1", &assessment, |_| async { Ok(()) })
        .await
        .unwrap();
    let preview = f.preview("lock-preview").await;
    // The mutable owner projection and immutable preview have independent guards.
    // Isolate each deadline without changing any original evidence or receipt.
    let assessment_deadline = match expires {
        Expire::Preview => deadline + chrono::Duration::seconds(60),
        Expire::Assessment => deadline,
    };
    sqlx::query("UPDATE app.managed_capital_reservations SET latest_assessment=jsonb_set(latest_assessment,'{valid_until}',to_jsonb($2::timestamptz)) WHERE managed_account_key=$1")
        .bind(&f.registration.managed_account_key).bind(assessment_deadline).execute(&pool).await.unwrap();
    let before = reservation_snapshot(&pool, &f).await;
    let request = start_request(&preview);
    let (blocker, pid, query) = block_start(&pool, &f, at).await;
    let start = store.start_capital_exit(
        &f.operator,
        f.observation.binding.project_id,
        "lock-start",
        &request,
    );
    let release = async {
        observe_wait(&pool, pid, query).await;
        // Prove that initial admission preceded expiry; never assume a sleep
        // scheduled the start, and never count statement/lock timeout as rejection.
        assert!(clock(&pool).await < deadline);
        after_deadline(&pool, deadline).await;
        blocker.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(start, release);
    assert_expired(result, expires);
    assert_eq!(committed_counts(&pool).await, (0, 0, 0));
    assert_eq!(reservation_snapshot(&pool, &f).await, before);

    let retry = store
        .start_capital_exit(
            &f.operator,
            f.observation.binding.project_id,
            "lock-start",
            &request,
        )
        .await;
    assert_expired(retry, expires);
    assert_eq!(committed_counts(&pool).await, (0, 0, 0));
    assert_eq!(reservation_snapshot(&pool, &f).await, before);

    // Rollback leaves the key unused: a newly assessed/acknowledged plan can
    // start once, without a phantom receipt or a stranded account reservation.
    let mut fresh = f.assessment();
    fresh.external_message_id = "assessment-2".into();
    fresh.sequence = DbCounter::new(2).unwrap();
    f.store
        .submit_capital_exit_assessment(&f.actor, "assessment-2", &fresh, |_| async { Ok(()) })
        .await
        .unwrap();
    let fresh_preview = f.preview("fresh-preview").await;
    let fresh_request = start_request(&fresh_preview);
    let accepted = store
        .start_capital_exit(
            &f.operator,
            f.observation.binding.project_id,
            "lock-start",
            &fresh_request,
        )
        .await
        .unwrap();
    assert!(!accepted.replayed);
    assert_eq!(accepted.resource.account_control_epoch.get(), 1);
    assert_eq!(accepted.resource.account_control_revision.get(), 2);
    assert_eq!(accepted.resource.funds.reserved_amount, money("100"));
    assert_eq!(committed_counts(&pool).await, (1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_start_preview_expiry_during_cursor_lock_rolls_back(pool: PgPool) {
    expiry_race(pool, WaitAt::Cursor, Expire::Preview).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_start_assessment_expiry_during_reservation_lock_rolls_back(pool: PgPool) {
    expiry_race(pool, WaitAt::Reservation, Expire::Assessment).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_start_expiry_during_queue_lock_rolls_back(pool: PgPool) {
    expiry_race(pool, WaitAt::Queue, Expire::Preview).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_start_fresh_lock_wait_and_expired_replay_keep_original_receipt(pool: PgPool) {
    let f = setup(&pool, true).await;
    let store = bounded_start_store(&pool).await;
    let mut assessment = f.assessment();
    assessment.valid_until = clock(&pool).await + chrono::Duration::seconds(3);
    f.store
        .submit_capital_exit_assessment(&f.actor, "assessment-1", &assessment, |_| async { Ok(()) })
        .await
        .unwrap();
    let preview = f.preview("lock-preview").await;
    let request = start_request(&preview);
    let (blocker, pid, query) = block_start(&pool, &f, WaitAt::Reservation).await;
    let start = store.start_capital_exit(
        &f.operator,
        f.observation.binding.project_id,
        "lock-start",
        &request,
    );
    let release = async {
        observe_wait(&pool, pid, query).await;
        assert!(clock(&pool).await < preview.valid_until);
        blocker.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(start, release);
    let accepted = result.unwrap();
    assert!(!accepted.replayed);
    assert_eq!(committed_counts(&pool).await, (1, 1, 1));
    let original_receipt: Value = sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM app.command_receipts r WHERE operation='CAPITAL_EXIT_START'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(original_receipt["response_status"], 202);
    let reserved = reservation_snapshot(&pool, &f).await;
    after_deadline(&pool, preview.valid_until).await;
    let replay = store
        .start_capital_exit(
            &f.operator,
            f.observation.binding.project_id,
            "lock-start",
            &request,
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(&replay.resource).unwrap(),
        serde_json::to_value(&accepted.resource).unwrap()
    );
    assert_eq!(reservation_snapshot(&pool, &f).await, reserved);
    assert_eq!(committed_counts(&pool).await, (1, 1, 1));
    let replayed_receipt: Value = sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM app.command_receipts r WHERE operation='CAPITAL_EXIT_START'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(replayed_receipt, original_receipt);
}
