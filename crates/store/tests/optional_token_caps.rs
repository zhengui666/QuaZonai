//! Uncapped application tokens retain immutable dispatch and usage evidence.
mod support;

use contracts::{budget::CostEnforcement, DbCounter, Id};
use domain::DomainError;
use sqlx::PgPool;
use store::{
    turns::{DispatchDecision, TurnOutcome, TurnRequest, TurnTerminal, UsageReceipt},
    Store, StoreError,
};
use support::{budget, fixture, Fixture};

fn unpriced_budget() -> contracts::budget::BudgetV1 {
    contracts::budget::BudgetV1 {
        max_tokens: None,
        max_cost_decimal: None,
        cost_currency: None,
        cost_enforcement: CostEnforcement::Unavailable,
        ..budget()
    }
}

fn request(f: &Fixture, key: &str) -> TurnRequest {
    TurnRequest {
        tokens: None,
        estimated_cost: None,
        ..f.request(key)
    }
}

fn usage(tokens: u64) -> UsageReceipt {
    UsageReceipt {
        outcome: TurnOutcome::Succeeded,
        actual_tokens: DbCounter::new(tokens).unwrap(),
        actual_cost: None,
        currency: None,
        reason_code: "NATIVE_OBSERVED".into(),
    }
}

async fn send(store: &Store, f: &Fixture, id: Id) {
    assert!(matches!(
        store.claim_turn_dispatch(id, &f.fence).await.unwrap(),
        DispatchDecision::Send { .. }
    ));
    store
        .bind_native_turn(id, &f.fence, &format!("native/{id}"))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn nullable_reservation_replays_exactly_without_an_infinite_grant(pool: PgPool) {
    let f = fixture(&pool, unpriced_budget()).await;
    let store = Store::from_pool(pool.clone());
    let request = request(&f, "uncapped");
    let (first, second) = tokio::join!(
        store.reserve_turn(f.run, &f.fence, &request),
        store.reserve_turn(f.run, &f.fence, &request),
    );
    let first = first.unwrap();
    assert_eq!(first, second.unwrap());
    assert_eq!(first.tokens, None);
    let pending: (Option<i64>, i32, i64, i64) = sqlx::query_as(
        "SELECT reserved_tokens::bigint,reserved_turns,(SELECT count(*) FROM app.model_turn_receipts),(SELECT count(*) FROM pgmq.q_model_turns) FROM app.model_turn_accounting WHERE reservation_id=$1",
    ).bind(first.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(pending, (None, 1, 0, 1));

    let changed = TurnRequest {
        tokens: Some(DbCounter::new(i64::MAX as u64).unwrap()),
        ..request
    };
    assert!(matches!(
        store.reserve_turn(f.run, &f.fence, &changed).await,
        Err(StoreError::Conflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn bounded_reservation_cannot_replay_as_uncapped(pool: PgPool) {
    let f = fixture(&pool, unpriced_budget()).await;
    let store = Store::from_pool(pool);
    let mut request = request(&f, "bounded");
    request.tokens = Some(DbCounter::new(40).unwrap());
    let original = store.reserve_turn(f.run, &f.fence, &request).await.unwrap();
    assert_eq!(original.tokens, request.tokens);
    request.tokens = None;
    assert!(matches!(
        store.reserve_turn(f.run, &f.fence, &request).await,
        Err(StoreError::Conflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn uncapped_usage_is_recorded_once_and_does_not_close_future_spending(pool: PgPool) {
    let f = fixture(&pool, unpriced_budget()).await;
    let store = Store::from_pool(pool.clone());
    let first = store
        .reserve_turn(f.run, &f.fence, &request(&f, "first"))
        .await
        .unwrap();
    send(&store, &f, first.id).await;
    assert!(matches!(
        store.claim_turn_dispatch(first.id, &f.fence).await.unwrap(),
        DispatchDecision::Reconcile {
            native_turn_id: Some(_)
        }
    ));
    let usage = usage(120);
    store.settle_turn(first.id, &f.fence, &usage).await.unwrap();
    store.settle_turn(first.id, &f.fence, &usage).await.unwrap();
    assert!(matches!(
        store.claim_turn_dispatch(first.id, &f.fence).await.unwrap(),
        DispatchDecision::Settled
    ));
    let settled: (i64, i64, i32, i32, i64) = sqlx::query_as(
        "SELECT reserved_tokens::bigint,used_tokens::bigint,reserved_turns,used_turns,(SELECT count(*) FROM app.model_turn_receipts) FROM app.model_turn_accounting WHERE reservation_id=$1",
    ).bind(first.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(settled, (0, 120, 0, 1, 1));
    let mut changed_usage = usage;
    changed_usage.actual_tokens = DbCounter::new(121).unwrap();
    assert!(matches!(
        store.settle_turn(first.id, &f.fence, &changed_usage).await,
        Err(StoreError::Conflict)
    ));
    let next = store
        .reserve_turn(f.run, &f.fence, &request(&f, "next"))
        .await
        .unwrap();
    assert_eq!(next.tokens, None);
    assert_eq!(next.ordinal, 2);
    send(&store, &f, next.id).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_uncapped_usage_cannot_be_refunded_or_replaced(pool: PgPool) {
    let f = fixture(&pool, unpriced_budget()).await;
    let store = Store::from_pool(pool.clone());
    let first = store
        .reserve_turn(f.run, &f.fence, &request(&f, "unknown"))
        .await
        .unwrap();
    send(&store, &f, first.id).await;
    let observed_at = sqlx::query_scalar("SELECT date_trunc('microseconds',clock_timestamp())")
        .fetch_one(&pool)
        .await
        .unwrap();
    store
        .observe_turn_terminal(
            first.id,
            &f.fence,
            &TurnTerminal {
                outcome: TurnOutcome::Succeeded,
                native_turn_id: Some(format!("native/{}", first.id)),
                reason_code: "NATIVE_OBSERVED".into(),
                observed_at,
            },
        )
        .await
        .unwrap();
    let unknown: (Option<i64>, i32, i64) = sqlx::query_as(
        "SELECT reserved_tokens::bigint,reserved_turns,(SELECT count(*) FROM app.model_turn_receipts) FROM app.model_turn_accounting WHERE reservation_id=$1",
    ).bind(first.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(unknown, (None, 1, 0));
    assert!(matches!(
        store
            .reserve_turn(f.run, &f.fence, &request(&f, "replacement"))
            .await,
        Err(StoreError::TurnPending)
    ));
    let message: i64 = sqlx::query_scalar("SELECT msg_id FROM pgmq.q_model_turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store
            .acknowledge_settled_turn_message(message, first.id)
            .await,
        Err(StoreError::TurnPending)
    ));
    store
        .settle_turn(first.id, &f.fence, &usage(17))
        .await
        .unwrap();
    assert!(store
        .acknowledge_settled_turn_message(message, first.id)
        .await
        .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn finite_cycle_cap_rejects_an_uncapped_turn_without_queueing(pool: PgPool) {
    let policy = contracts::budget::BudgetV1 {
        max_tokens: Some(DbCounter::new(100).unwrap()),
        ..unpriced_budget()
    };
    let f = fixture(&pool, policy).await;
    let store = Store::from_pool(pool.clone());
    assert!(matches!(
        store
            .reserve_turn(f.run, &f.fence, &request(&f, "invalid"))
            .await,
        Err(StoreError::Domain(DomainError::Invalid(
            "model_token_reservation"
        )))
    ));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM pgmq.q_model_turns)",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn proven_unsent_uncapped_turn_can_cancel_without_spending_or_resending(pool: PgPool) {
    let f = fixture(&pool, unpriced_budget()).await;
    let store = Store::from_pool(pool.clone());
    let first = store
        .reserve_turn(f.run, &f.fence, &request(&f, "unsent"))
        .await
        .unwrap();
    let receipt = UsageReceipt {
        outcome: TurnOutcome::NotSent,
        reason_code: "CONFIRMED_NOT_SENT".into(),
        ..usage(0)
    };
    store
        .settle_turn(first.id, &f.fence, &receipt)
        .await
        .unwrap();
    store
        .settle_turn(first.id, &f.fence, &receipt)
        .await
        .unwrap();
    assert!(matches!(
        store.claim_turn_dispatch(first.id, &f.fence).await.unwrap(),
        DispatchDecision::Settled
    ));
    let counts: (i64, i64, i32, i32, i64) = sqlx::query_as(
        "SELECT reserved_tokens::bigint,used_tokens::bigint,reserved_turns,used_turns,(SELECT count(*) FROM app.model_turn_dispatches) FROM app.model_turn_accounting WHERE reservation_id=$1",
    ).bind(first.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 0));
    let next = store
        .reserve_turn(f.run, &f.fence, &request(&f, "replacement"))
        .await
        .unwrap();
    assert_eq!(next.tokens, None);
    assert_eq!(next.ordinal, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn uncapped_tokens_do_not_bypass_cost_overrun_accounting(pool: PgPool) {
    let policy = contracts::budget::BudgetV1 {
        max_tokens: None,
        ..budget()
    };
    let f = fixture(&pool, policy).await;
    let store = Store::from_pool(pool);
    let mut request = f.request("priced");
    request.tokens = None;
    let first = store.reserve_turn(f.run, &f.fence, &request).await.unwrap();
    send(&store, &f, first.id).await;
    let receipt = UsageReceipt {
        actual_cost: Some("2".parse().unwrap()),
        currency: Some("USD".into()),
        ..usage(120)
    };
    store
        .settle_turn(first.id, &f.fence, &receipt)
        .await
        .unwrap();
    request.command_key = "after-cost-overrun".into();
    assert!(matches!(
        store.reserve_turn(f.run, &f.fence, &request).await,
        Err(StoreError::Domain(DomainError::BudgetExhausted(
            "estimated_cost"
        )))
    ));
}
