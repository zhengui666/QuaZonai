//! PostgreSQL regressions for retry-vs-authority and frozen admission boundaries.
mod support;
use chrono::{DateTime, Duration, Utc};
use contracts::{DbCounter, Id, Revision};
use domain::DomainError;
use sqlx::PgPool;
use store::{
    turns::{TurnOutcome, TurnTerminal, UsageReceipt, WorkerFence},
    Store, StoreError,
};
use support::*;

fn usage() -> UsageReceipt {
    UsageReceipt {
        outcome: TurnOutcome::Succeeded,
        actual_tokens: DbCounter::new(17).unwrap(),
        actual_cost: Some("0.25".parse().unwrap()),
        currency: Some("USD".into()),
        reason_code: "NATIVE_COMPLETED".into(),
    }
}

async fn prepared(pool: &PgPool) -> (Fixture, Store, Id) {
    let f = fixture(pool, budget()).await;
    let store = Store::from_pool(pool.clone());
    let reservation = store
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    store
        .claim_turn_dispatch(reservation.id, &f.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(reservation.id, &f.fence, "native-1")
        .await
        .unwrap();
    (f, store, reservation.id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn committed_receipt_retry_survives_expiry_and_takeover_without_rebilling(pool: PgPool) {
    let (f, store, reservation) = prepared(&pool).await;
    store
        .settle_turn(reservation, &f.fence, &usage())
        .await
        .unwrap();
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp() WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    store
        .settle_turn(reservation, &f.fence, &usage())
        .await
        .unwrap();
    sqlx::query("UPDATE app.run_attempts SET worker_owner_id='worker-B',owner_epoch=owner_epoch+1,lease_expires_at=clock_timestamp()+interval '1 minute' WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    store
        .settle_turn(reservation, &f.fence, &usage())
        .await
        .unwrap();
    assert!(matches!(
        store
            .settle_turn(
                reservation,
                &f.fence,
                &UsageReceipt {
                    actual_tokens: DbCounter::new(18).unwrap(),
                    ..usage()
                }
            )
            .await,
        Err(StoreError::Conflict)
    ));
    let foreign = WorkerFence {
        attempt_id: Id::new(),
        ..f.fence.clone()
    };
    assert!(matches!(
        store.settle_turn(reservation, &foreign, &usage()).await,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
    let (count, tokens): (i64, i64) = sqlx::query_as(
        "SELECT count(*),sum(actual_tokens)::bigint FROM app.model_turn_receipts WHERE reservation_id=$1",
    )
    .bind(reservation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((count, tokens), (1, 17));
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_receipt_still_requires_the_current_worker_fence(pool: PgPool) {
    let (f, store, reservation) = prepared(&pool).await;
    sqlx::query("UPDATE app.run_attempts SET worker_owner_id='worker-B',owner_epoch=owner_epoch+1 WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store.settle_turn(reservation, &f.fence, &usage()).await,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let current = WorkerFence {
        worker_owner_id: "worker-B".into(),
        owner_epoch: Revision::try_from("2".to_owned()).unwrap(),
        ..f.fence
    };
    store
        .settle_turn(reservation, &current, &usage())
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn receipt_committed_during_lock_wait_is_rechecked_after_fence_expiry(pool: PgPool) {
    let (f, store, reservation) = prepared(&pool).await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
        .bind(f.project.as_uuid())
        .execute(&mut *blocker)
        .await
        .unwrap();
    // This transaction represents the first successful native report whose
    // response is lost. The retry cannot see these rows before it commits.
    sqlx::query("INSERT INTO app.model_turn_terminals(reservation_id,native_turn_id,outcome,reason_code,observed_at) VALUES($1,'native-1','SUCCEEDED','NATIVE_COMPLETED',clock_timestamp())")
        .bind(reservation.as_uuid()).execute(&mut *blocker).await.unwrap();
    sqlx::query("INSERT INTO app.model_turn_receipts(reservation_id,outcome,actual_tokens,actual_cost,cost_currency,usage_source,reason_code) VALUES($1,'SUCCEEDED',17,0.25,'USD','NATIVE_REPORT','NATIVE_COMPLETED')")
        .bind(reservation.as_uuid()).execute(&mut *blocker).await.unwrap();
    let expected = usage();
    let retry = store.settle_turn(reservation, &f.fence, &expected);
    let release = async {
        let mut waiting = false;
        for _ in 0..100 {
            waiting = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT state FROM app.projects%')")
                .fetch_one(&pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(waiting, "retry must wait behind the committing report");
        sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp() WHERE id=$1")
            .bind(f.fence.attempt_id.as_uuid())
            .execute(&mut *blocker)
            .await
            .unwrap();
        blocker.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(retry, release);
    result.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn draft_brief_cannot_authorize_a_new_model_turn(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let draft = Id::new();
    let cycle = Id::new();
    sqlx::query("INSERT INTO app.research_briefs(id,project_id,version,hypothesis,economic_rationale,universe_version_id,target_kind,horizon_kind,horizon_value,base_currency,evaluation_policy_id,execution_assumptions_id,budget,stop_rule,state) SELECT $1,b.project_id,b.version+1,b.hypothesis,b.economic_rationale,b.universe_version_id,b.target_kind,b.horizon_kind,b.horizon_value,b.base_currency,b.evaluation_policy_id,b.execution_assumptions_id,b.budget,b.stop_rule,'DRAFT' FROM app.research_briefs b JOIN app.research_cycles c ON c.brief_id=b.id WHERE c.id=$2")
        .bind(draft.as_uuid()).bind(f.cycle.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot) VALUES($1,$2,$3,2,'OPERATOR','RUNNING',$4)")
        .bind(cycle.as_uuid()).bind(f.project.as_uuid()).bind(draft.as_uuid())
        .bind(serde_json::to_value(&f.budget).unwrap()).execute(&pool).await.unwrap();
    let (run, _, fence, deadline) = mission(&pool, f.project, cycle, f.input_set, f.profile).await;
    let mut request = f.request("draft");
    request.deadline_at = deadline;
    let store = Store::from_pool(pool.clone());
    assert!(matches!(
        store.reserve_turn(run, &fence, &request).await,
        Err(StoreError::Domain(DomainError::AdmissionClosed))
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

async fn terminal(pool: &PgPool) -> TurnTerminal {
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    TurnTerminal {
        outcome: TurnOutcome::Succeeded,
        native_turn_id: Some("native-1".into()),
        reason_code: "NATIVE_COMPLETED".into(),
        observed_at: now,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn terminal_retry_survives_expiry_and_takeover_but_never_changes_facts_or_refunds(
    pool: PgPool,
) {
    let (f, store, reservation) = prepared(&pool).await;
    let observed = terminal(&pool).await;
    store
        .observe_turn_terminal(reservation, &f.fence, &observed)
        .await
        .unwrap();
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp() WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    store
        .observe_turn_terminal(reservation, &f.fence, &observed)
        .await
        .unwrap();
    sqlx::query("UPDATE app.run_attempts SET worker_owner_id='worker-B',owner_epoch=owner_epoch+1,lease_expires_at=clock_timestamp()+interval '1 minute' WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    store
        .observe_turn_terminal(reservation, &f.fence, &observed)
        .await
        .unwrap();
    for changed in [
        TurnTerminal {
            outcome: TurnOutcome::Failed,
            ..observed.clone()
        },
        TurnTerminal {
            native_turn_id: Some("different-native".into()),
            ..observed.clone()
        },
        TurnTerminal {
            reason_code: "DIFFERENT_REASON".into(),
            ..observed.clone()
        },
        TurnTerminal {
            observed_at: observed.observed_at + Duration::microseconds(1),
            ..observed.clone()
        },
    ] {
        assert!(matches!(
            store
                .observe_turn_terminal(reservation, &f.fence, &changed)
                .await,
            Err(StoreError::Conflict)
        ));
    }
    let foreign = WorkerFence {
        attempt_id: Id::new(),
        ..f.fence.clone()
    };
    assert!(matches!(
        store
            .observe_turn_terminal(reservation, &foreign, &observed)
            .await,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
    let (terminals, receipts, reserved): (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_terminals),(SELECT count(*) FROM app.model_turn_receipts),reserved_tokens::bigint FROM app.model_turn_accounting WHERE reservation_id=$1")
        .bind(reservation.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!((terminals, receipts, reserved), (1, 0, 40));
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_terminal_still_requires_the_current_worker_fence(pool: PgPool) {
    let (f, store, reservation) = prepared(&pool).await;
    let observed = terminal(&pool).await;
    sqlx::query("UPDATE app.run_attempts SET worker_owner_id='worker-B',owner_epoch=owner_epoch+1 WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store
            .observe_turn_terminal(reservation, &f.fence, &observed)
            .await,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_terminals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let current = WorkerFence {
        worker_owner_id: "worker-B".into(),
        owner_epoch: Revision::try_from("2".to_owned()).unwrap(),
        ..f.fence
    };
    store
        .observe_turn_terminal(reservation, &current, &observed)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn terminal_committed_during_lock_wait_is_rechecked_even_after_lease_loss(pool: PgPool) {
    for same in [true, false] {
        let (f, store, reservation) = prepared(&pool).await;
        let observed = terminal(&pool).await;
        let mut blocker = pool.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(f.project.as_uuid())
            .execute(&mut *blocker)
            .await
            .unwrap();
        sqlx::query("INSERT INTO app.model_turn_terminals(reservation_id,native_turn_id,outcome,reason_code,observed_at) VALUES($1,'native-1','SUCCEEDED','NATIVE_COMPLETED',$2)")
            .bind(reservation.as_uuid()).bind(observed.observed_at).execute(&mut *blocker).await.unwrap();
        let expected = if same {
            observed.clone()
        } else {
            TurnTerminal {
                reason_code: "CONTRADICTORY_RETRY".into(),
                ..observed
            }
        };
        let retry = store.observe_turn_terminal(reservation, &f.fence, &expected);
        let release = async {
            let mut waiting = false;
            for _ in 0..100 {
                waiting = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT state FROM app.projects%')")
                    .fetch_one(&pool).await.unwrap();
                if waiting {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            assert!(
                waiting,
                "retry must actually wait behind the first terminal report"
            );
            sqlx::query(
                "UPDATE app.run_attempts SET lease_expires_at=clock_timestamp() WHERE id=$1",
            )
            .bind(f.fence.attempt_id.as_uuid())
            .execute(&mut *blocker)
            .await
            .unwrap();
            blocker.commit().await.unwrap();
        };
        let (result, ()) = tokio::join!(retry, release);
        if same {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(StoreError::Conflict)));
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirmed_not_sent_terminal_retry_remains_read_only_after_expiry(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let store = Store::from_pool(pool.clone());
    let reservation = store
        .reserve_turn(f.run, &f.fence, &f.request("not-sent"))
        .await
        .unwrap();
    let observed = TurnTerminal {
        outcome: TurnOutcome::NotSent,
        native_turn_id: None,
        reason_code: "CONFIRMED_NOT_SENT".into(),
        ..terminal(&pool).await
    };
    store
        .observe_turn_terminal(reservation.id, &f.fence, &observed)
        .await
        .unwrap();
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp() WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    store
        .observe_turn_terminal(reservation.id, &f.fence, &observed)
        .await
        .unwrap();
    let (receipts, reserved): (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_receipts),reserved_tokens::bigint FROM app.model_turn_accounting WHERE reservation_id=$1")
        .bind(reservation.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!((receipts, reserved), (0, 40));
}
