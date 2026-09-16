mod support;
use contracts::DbCounter;
use sqlx::PgPool;
use store::{
    turns::{DispatchDecision, TurnOutcome, TurnTerminal, UsageReceipt},
    Store, StoreError,
};
use support::*;

async fn terminal(pool: &PgPool) -> TurnTerminal {
    TurnTerminal {
        outcome: TurnOutcome::Succeeded,
        native_turn_id: Some("native-1".into()),
        reason_code: "NATIVE_COMPLETED".into(),
        observed_at: sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(pool)
            .await
            .unwrap(),
    }
}
fn usage() -> UsageReceipt {
    UsageReceipt {
        outcome: TurnOutcome::Succeeded,
        actual_tokens: DbCounter::new(17).unwrap(),
        actual_cost: Some("0.25".parse().unwrap()),
        currency: Some("USD".into()),
        reason_code: "NATIVE_COMPLETED".into(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn observed_terminal_without_usage_is_durable_and_does_not_release_budget(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let store = Store::from_pool(pool.clone());
    let r = store
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    store.claim_turn_dispatch(r.id, &f.fence).await.unwrap();
    store
        .bind_native_turn(r.id, &f.fence, "native-1")
        .await
        .unwrap();
    let t = terminal(&pool).await;
    store
        .observe_turn_terminal(r.id, &f.fence, &t)
        .await
        .unwrap();
    drop(store);
    let restarted = Store::from_pool(pool.clone());
    restarted
        .observe_turn_terminal(r.id, &f.fence, &t)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.model_turn_terminals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT reserved_tokens::bigint FROM app.model_turn_accounting WHERE reservation_id=$1"
        )
        .bind(r.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        40
    );
    assert!(matches!(
        restarted
            .reserve_turn(f.run, &f.fence, &f.request("two"))
            .await,
        Err(StoreError::TurnPending)
    ));
    assert_eq!(
        restarted.claim_turn_dispatch(r.id, &f.fence).await.unwrap(),
        DispatchDecision::Reconcile {
            native_turn_id: Some("native-1".into())
        }
    );
    restarted
        .settle_turn(r.id, &f.fence, &usage())
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT used_tokens::bigint FROM app.model_turn_accounting WHERE reservation_id=$1"
        )
        .bind(r.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        17
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn terminal_identity_and_outcome_cannot_be_replaced_by_late_usage(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    s.claim_turn_dispatch(r.id, &f.fence).await.unwrap();
    s.bind_native_turn(r.id, &f.fence, "native-1")
        .await
        .unwrap();
    let t = terminal(&pool).await;
    let wrong = TurnTerminal {
        native_turn_id: Some("unrelated-turn".into()),
        ..t.clone()
    };
    assert!(s
        .observe_turn_terminal(r.id, &f.fence, &wrong)
        .await
        .is_err());
    s.observe_turn_terminal(r.id, &f.fence, &t).await.unwrap();
    assert!(matches!(
        s.observe_turn_terminal(
            r.id,
            &f.fence,
            &TurnTerminal {
                outcome: TurnOutcome::Failed,
                ..t
            }
        )
        .await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        s.settle_turn(
            r.id,
            &f.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Failed,
                ..usage()
            }
        )
        .await,
        Err(StoreError::Conflict)
    ));
    assert!(sqlx::query("INSERT INTO app.model_turn_receipts(reservation_id,outcome,actual_tokens,actual_cost,cost_currency,usage_source,reason_code) VALUES($1,'FAILED',17,0.25,'USD','NATIVE_REPORT','FAILED')").bind(r.id.as_uuid()).execute(&pool).await.is_err());
    assert!(sqlx::query(
        "UPDATE app.model_turn_terminals SET outcome='FAILED' WHERE reservation_id=$1"
    )
    .bind(r.id.as_uuid())
    .execute(&pool)
    .await
    .is_err());
    s.settle_turn(r.id, &f.fence, &usage()).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_dispatch_cannot_be_relabelled_as_a_not_sent_terminal(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    s.claim_turn_dispatch(r.id, &f.fence).await.unwrap();
    let t = TurnTerminal {
        outcome: TurnOutcome::NotSent,
        native_turn_id: None,
        reason_code: "NOT_SENT".into(),
        ..terminal(&pool).await
    };
    assert!(s.observe_turn_terminal(r.id, &f.fence, &t).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.model_turn_terminals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT reserved_tokens::bigint FROM app.model_turn_accounting WHERE reservation_id=$1"
        )
        .bind(r.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        40
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn frozen_reservation_identity_records_epoch_profile_and_monotonic_ordinal(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let s = Store::from_pool(pool.clone());
    let r = s
        .reserve_turn(f.run, &f.fence, &f.request("one"))
        .await
        .unwrap();
    assert_eq!(r.owner_epoch, f.fence.owner_epoch);
    assert_eq!(r.profile_revision.get(), 1);
    assert_eq!(r.ordinal, 1);
    s.settle_turn(
        r.id,
        &f.fence,
        &UsageReceipt {
            outcome: TurnOutcome::NotSent,
            actual_tokens: DbCounter::ZERO,
            actual_cost: Some("0".parse().unwrap()),
            currency: Some("USD".into()),
            reason_code: "NOT_SENT".into(),
        },
    )
    .await
    .unwrap();
    let next = s
        .reserve_turn(f.run, &f.fence, &f.request("two"))
        .await
        .unwrap();
    assert_eq!(next.ordinal, 2);
    assert!(
        sqlx::query("UPDATE app.model_turn_reservations SET ordinal=3 WHERE id=$1")
            .bind(next.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE app.codex_sessions SET profile_revision=2 WHERE id=$1")
            .bind(f.session.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.model_turn_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}
