mod support;
use chrono::Duration;
use contracts::{DbCounter, Id};
use domain::{admission::TurnKind, DomainError};
use sqlx::{PgPool, Row};
use store::{
    turns::{DispatchDecision, TurnOutcome, UsageReceipt},
    Store, StoreError,
};
use support::*;

fn assert_sqlstate(error: &sqlx::Error, expected: &str) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected),
        "{error:?}"
    );
}

fn used(outcome: TurnOutcome, tokens: u64, cost: &str) -> UsageReceipt {
    UsageReceipt {
        outcome,
        actual_tokens: DbCounter::new(tokens).unwrap(),
        actual_cost: Some(cost.parse().unwrap()),
        currency: Some("USD".into()),
        reason_code: "NATIVE_OBSERVED".into(),
    }
}
async fn complete(
    store: &Store,
    f: &Fixture,
    reservation: Id,
    outcome: TurnOutcome,
    tokens: u64,
    cost: &str,
) {
    assert!(matches!(
        store
            .claim_turn_dispatch(reservation, &f.fence)
            .await
            .unwrap(),
        DispatchDecision::Send { .. }
    ));
    store
        .bind_native_turn(reservation, &f.fence, &format!("native/{reservation}"))
        .await
        .unwrap();
    store
        .settle_turn(reservation, &f.fence, &used(outcome, tokens, cost))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_retries_reserve_once_and_publish_one_native_queue_message(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let store = Store::from_pool(pool.clone());
    let a = f.request("stable");
    let b = a.clone();
    let (first, second) = tokio::join!(
        store.reserve_turn(f.run, &f.fence, &a),
        store.reserve_turn(f.run, &f.fence, &b)
    );
    assert_eq!(first.unwrap(), second.unwrap());
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    let messages: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_model_turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((rows, messages), (1, 1));
    let mut changed = a;
    changed.tokens = DbCounter::new(41).unwrap();
    assert!(matches!(
        store.reserve_turn(f.run, &f.fence, &changed).await,
        Err(StoreError::Conflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_mission_cannot_open_a_second_unresolved_turn(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    s.reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    assert!(matches!(
        s.reserve_turn(f.run, &f.fence, &f.request("two")).await,
        Err(StoreError::TurnPending)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_missions_share_the_frozen_cycle_budget(pool: PgPool) {
    let mut b = budget();
    b.max_tokens = Some(DbCounter::new(60).unwrap());
    let f = fixture(&pool, b).await;
    let (other, _, fence, deadline) =
        mission(&pool, f.project, f.cycle, f.input_set, f.profile).await;
    let s = Store::from_pool(pool.clone());
    let a = f.request("one");
    let mut b = f.request("other");
    b.deadline_at = deadline;
    let (a, b) = tokio::join!(
        s.reserve_turn(f.run, &f.fence, &a),
        s.reserve_turn(other, &fence, &b)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let error = if a.is_err() {
        a.unwrap_err()
    } else {
        b.unwrap_err()
    };
    assert!(matches!(
        error,
        StoreError::Domain(DomainError::BudgetExhausted("tokens"))
    ));
    let sum: i64 =
        sqlx::query_scalar("SELECT sum(reserved_tokens)::bigint FROM app.model_turn_reservations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sum, 40);
}

#[sqlx::test(migrations = "../../migrations")]
async fn crash_after_dispatch_retains_identity_and_never_resends(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    assert!(matches!(
        s.claim_turn_dispatch(r.id, &f.fence).await.unwrap(),
        DispatchDecision::Send { .. }
    ));
    // A new Store/client simulates losing all worker memory after sending.
    let restarted = Store::from_pool(pool.clone());
    assert_eq!(
        restarted.claim_turn_dispatch(r.id, &f.fence).await.unwrap(),
        DispatchDecision::Reconcile {
            native_turn_id: None
        }
    );
    sqlx::query("UPDATE app.run_attempts SET owner_epoch=2,worker_owner_id='worker-B' WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let mut taken = f.fence.clone();
    taken.owner_epoch = taken.owner_epoch.next().unwrap();
    taken.worker_owner_id = "worker-B".into();
    assert!(matches!(
        restarted.bind_native_turn(r.id, &f.fence, "native-1").await,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
    assert_eq!(
        restarted.claim_turn_dispatch(r.id, &taken).await.unwrap(),
        DispatchDecision::Reconcile {
            native_turn_id: None
        }
    );
    restarted
        .bind_native_turn(r.id, &taken, "native-1")
        .await
        .unwrap();
    assert_eq!(
        restarted.claim_turn_dispatch(r.id, &taken).await.unwrap(),
        DispatchDecision::Reconcile {
            native_turn_id: Some("native-1".into())
        }
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn refund_requires_proof_that_no_dispatch_intent_exists(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    s.claim_turn_dispatch(r.id, &f.fence).await.unwrap();
    assert!(s
        .settle_turn(r.id, &f.fence, &used(TurnOutcome::NotSent, 0, "0"))
        .await
        .is_err());
    let remaining: i64 = sqlx::query_scalar(
        "SELECT reserved_tokens::bigint FROM app.model_turn_accounting WHERE reservation_id=$1",
    )
    .bind(r.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 40);
    assert!(s
        .settle_turn(r.id, &f.fence, &used(TurnOutcome::Failed, 0, "0"))
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn unsent_refund_releases_only_the_exact_reservation(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    s.settle_turn(r.id, &f.fence, &used(TurnOutcome::NotSent, 0, "0"))
        .await
        .unwrap();
    assert_eq!(
        s.claim_turn_dispatch(r.id, &f.fence).await.unwrap(),
        DispatchDecision::Settled
    );
    let row=sqlx::query("SELECT reserved_tokens::bigint,used_tokens::bigint,reserved_turns,used_turns FROM app.model_turn_accounting WHERE reservation_id=$1").bind(r.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<i64, _>("reserved_tokens"), 0);
    assert_eq!(row.get::<i64, _>("used_tokens"), 0);
    assert_eq!(row.get::<i32, _>("used_turns"), 0);
    s.reserve_turn(f.run, &f.fence, &f.request("two"))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn genuine_failed_turn_counts_once_and_settlement_is_immutable(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let mut q = f.request("repair");
    q.turn_kind = TurnKind::Repair;
    let r = s.reserve_turn(f.run, &f.fence, &q).await.unwrap();
    complete(&s, &f, r.id, TurnOutcome::Failed, 10, "0.1").await;
    s.settle_turn(r.id, &f.fence, &used(TurnOutcome::Failed, 10, "0.1"))
        .await
        .unwrap();
    assert!(matches!(
        s.settle_turn(r.id, &f.fence, &used(TurnOutcome::Succeeded, 10, "0.1"))
            .await,
        Err(StoreError::Conflict)
    ));
    let row=sqlx::query("SELECT used_tokens::bigint,used_turns,used_repair_turns FROM app.model_turn_accounting WHERE reservation_id=$1").bind(r.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<i64, _>("used_tokens"), 10);
    assert_eq!(row.get::<i32, _>("used_turns"), 1);
    assert_eq!(row.get::<i32, _>("used_repair_turns"), 1);
    q.command_key = "repair-two".into();
    assert!(matches!(
        s.reserve_turn(f.run, &f.fence, &q).await,
        Err(StoreError::Domain(DomainError::BudgetExhausted(
            "repair_turns"
        )))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn actual_overbudget_usage_is_retained_and_blocks_new_work(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    complete(&s, &f, r.id, TurnOutcome::Succeeded, 101, "11").await;
    assert!(matches!(
        s.reserve_turn(f.run, &f.fence, &f.request("two")).await,
        Err(StoreError::Domain(DomainError::BudgetExhausted("tokens")))
    ));
    let used: i64 = sqlx::query_scalar(
        "SELECT actual_tokens::bigint FROM app.model_turn_receipts WHERE reservation_id=$1",
    )
    .bind(r.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(used, 101);
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_queue_rolls_back_the_budget_and_reservation_together(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    sqlx::query("SELECT pgmq.drop_queue('model_turns')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .is_err());
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn deadline_and_lease_use_database_time_after_lock_wait(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
        .bind(f.project.as_uuid())
        .execute(&mut *blocker)
        .await
        .unwrap();
    let request = f.request("one");
    let future = s.reserve_turn(f.run, &f.fence, &request);
    let release = async {
        // Confirm that the Store transaction has actually started and blocked,
        // then expire the lease after its transaction timestamp. A NOW()-based
        // implementation would incorrectly admit this request after the wait.
        let mut waiting = false;
        for _ in 0..100 {
            waiting = sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT state FROM app.projects%')")
                .fetch_one(&pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(waiting, "Store must be blocked before expiring its lease");
        sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp() WHERE id=$1")
            .bind(f.fence.attempt_id.as_uuid())
            .execute(&mut *blocker)
            .await
            .unwrap();
        blocker.commit().await.unwrap();
    };
    let (outcome, ()) = tokio::join!(future, release);
    assert!(matches!(
        outcome,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn exact_mission_and_native_turn_identities_cannot_be_relabelled(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE app.model_turn_reservations SET reserved_tokens=1 WHERE id=$1")
            .bind(r.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM app.model_turn_reservations WHERE id=$1")
            .bind(r.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    let duplicate = sqlx::query("INSERT INTO app.codex_sessions(id,project_id,cycle_id,run_id,role,profile_id,profile_revision,thread_id,codex_version,protocol_schema_version,requested_settings,native_history_ref) SELECT uuidv7(),project_id,cycle_id,run_id,role,profile_id,profile_revision,'other-thread',codex_version,protocol_schema_version,requested_settings,native_history_ref FROM app.codex_sessions WHERE id=$1").bind(f.session.as_uuid()).execute(&pool).await.unwrap_err();
    assert_sqlstate(&duplicate, "23505");
    s.claim_turn_dispatch(r.id, &f.fence).await.unwrap();
    s.bind_native_turn(r.id, &f.fence, "one-native")
        .await
        .unwrap();
    assert!(matches!(
        s.bind_native_turn(r.id, &f.fence, "other-native").await,
        Err(StoreError::Conflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn queue_ack_requires_exact_committed_receipt_and_is_idempotent(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    let message: i64 = sqlx::query_scalar(
        "SELECT msg_id FROM pgmq.q_model_turns WHERE message->>'reservation_id'=$1",
    )
    .bind(r.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(matches!(
        s.acknowledge_settled_turn_message(message, r.id).await,
        Err(StoreError::TurnPending)
    ));
    complete(&s, &f, r.id, TurnOutcome::Succeeded, 10, "0.1").await;
    assert!(s
        .acknowledge_settled_turn_message(message, r.id)
        .await
        .unwrap());
    assert!(!s
        .acknowledge_settled_turn_message(message, r.id)
        .await
        .unwrap());
    let r2 = s
        .reserve_turn(f.run, &f.fence, &f.request("two"))
        .await
        .unwrap();
    complete(&s, &f, r2.id, TurnOutcome::Failed, 10, "0.1").await;
    assert!(matches!(
        s.acknowledge_settled_turn_message(message, r2.id).await,
        Err(StoreError::Conflict)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_model_turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn absent_cost_cap_does_not_disable_the_mission_turn_cap(pool: PgPool) {
    let mut b = budget();
    b.max_turns_per_mission = 2;
    b.max_tokens = None;
    b.max_cost_decimal = None;
    b.cost_currency = None;
    b.cost_enforcement = contracts::budget::CostEnforcement::Unavailable;
    let f = fixture(&pool, b).await;
    let s = Store::from_pool(pool.clone());
    for ordinal in 1..=2 {
        let mut q = f.request(&format!("turn-{ordinal}"));
        q.estimated_cost = None;
        let r = s.reserve_turn(f.run, &f.fence, &q).await.unwrap();
        s.claim_turn_dispatch(r.id, &f.fence).await.unwrap();
        s.bind_native_turn(r.id, &f.fence, &format!("native-{ordinal}"))
            .await
            .unwrap();
        s.settle_turn(
            r.id,
            &f.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Failed,
                actual_tokens: DbCounter::new(5).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "OBSERVED_FAILURE".into(),
            },
        )
        .await
        .unwrap();
    }
    let mut q = f.request("turn-3");
    q.estimated_cost = None;
    assert!(matches!(
        s.reserve_turn(f.run, &f.fence, &q).await,
        Err(StoreError::Domain(DomainError::BudgetExhausted(
            "mission_turns"
        )))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn no_dispatch_after_pause_but_existing_usage_can_be_reconciled(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    sqlx::query("UPDATE app.projects SET state='PAUSED' WHERE id=$1")
        .bind(f.project.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.claim_turn_dispatch(r.id, &f.fence).await,
        Err(StoreError::Domain(DomainError::AdmissionClosed))
    ));
    s.settle_turn(r.id, &f.fence, &used(TurnOutcome::NotSent, 0, "0"))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn cross_project_and_sealed_request_artifacts_are_rejected(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let foreign = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let mut q = f.request("cross");
    q.request_artifact_id = foreign.artifact;
    assert!(matches!(
        s.reserve_turn(f.run, &f.fence, &q).await,
        Err(StoreError::Invalid("request_artifact"))
    ));
    let sealed:uuid::Uuid=sqlx::query_scalar("INSERT INTO app.artifacts(project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,'PARAMETERS','application/json','fixture','1','LOCAL','sealed-test','1',1,'EVALUATOR_ONLY','FIXTURE','OPERATOR','AUDIT') RETURNING id::uuid").bind(f.project.as_uuid()).fetch_one(&pool).await.unwrap();
    q.request_artifact_id = Id::try_from(sealed.to_string()).unwrap();
    assert!(matches!(
        s.reserve_turn(f.run, &f.fence, &q).await,
        Err(StoreError::Invalid("request_artifact"))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn submicrosecond_deadlines_are_rejected_before_persistence(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let mut q = f.request("precision");
    q.deadline_at += Duration::nanoseconds(1);
    assert!(matches!(
        s.reserve_turn(f.run, &f.fence, &q).await,
        Err(StoreError::Invalid("timestamp_precision"))
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
