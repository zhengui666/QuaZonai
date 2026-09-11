//! A Cycle copy cannot enlarge the locked Operator Brief. Historical facts stay.
mod support;
use contracts::{DbCounter, Id};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::{turns::TurnOutcome, turns::UsageReceipt, Store, StoreError};
use support::*;

async fn cycle(pool: &PgPool, f: &Fixture, snapshot: Value) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot) SELECT $1,project_id,brief_id,(SELECT max(ordinal)+1 FROM app.research_cycles WHERE project_id=$2),'OPERATOR','RUNNING',$3 FROM app.research_cycles WHERE id=$4")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(snapshot).bind(f.cycle.as_uuid())
        .execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_budget_field_is_bound_to_the_frozen_brief_before_reservation(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let store = Store::from_pool(pool.clone());
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
        let cycle = cycle(&pool, &f, changed).await;
        let (run, _, fence, deadline) =
            mission(&pool, f.project, cycle, f.input_set, f.profile).await;
        let mut request = f.request(field);
        request.deadline_at = deadline;
        assert!(
            matches!(
                store.reserve_turn(run, &fence, &request).await,
                Err(StoreError::Invalid("frozen_budget_snapshot_mismatch"))
            ),
            "{field}"
        );
    }
    let reserved: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_model_turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((reserved, queued), (0, 0));
    // The unmodified frozen copy still admits work; this is not a blanket deny.
    store
        .reserve_turn(f.run, &f.fence, &f.request("original"))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn mismatched_legacy_snapshot_blocks_new_dispatch_without_erasing_real_usage(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let store = Store::from_pool(pool.clone());
    let mut snapshot = serde_json::to_value(&f.budget).unwrap();
    snapshot["max_tokens"] = json!("1000");
    let cycle = cycle(&pool, &f, snapshot).await;
    let (run, session, fence, deadline) =
        mission(&pool, f.project, cycle, f.input_set, f.profile).await;
    let reservation = Id::new();
    // An old, already persisted reservation from before the authority repair.
    // Test-only SQL is not a production admission path.
    sqlx::query("INSERT INTO app.model_turn_reservations(id,project_id,cycle_id,run_id,session_id,attempt_id,owner_epoch,profile_revision,ordinal,command_key,turn_kind,reserved_tokens,reserved_cost,cost_currency,request_artifact_id,deadline_at) VALUES($1,$2,$3,$4,$5,$6,1,1,1,'historical','RESEARCH',40,1.25,'USD',$7,$8)")
        .bind(reservation.as_uuid()).bind(f.project.as_uuid()).bind(cycle.as_uuid()).bind(run.as_uuid())
        .bind(session.as_uuid()).bind(fence.attempt_id.as_uuid()).bind(f.artifact.as_uuid()).bind(deadline)
        .execute(&pool).await.unwrap();
    assert!(matches!(
        store.claim_turn_dispatch(reservation, &fence).await,
        Err(StoreError::Invalid("frozen_budget_snapshot_mismatch"))
    ));
    let sent: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_dispatches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sent, 0);
    // Simulate evidence that the old software already sent a request. The fix
    // must not delete actual native usage merely because its old cap was wrong.
    sqlx::query("INSERT INTO app.model_turn_dispatches(reservation_id,owner_epoch,rpc_request_id) VALUES($1,1,'historical-rpc')")
        .bind(reservation.as_uuid()).execute(&pool).await.unwrap();
    store
        .bind_native_turn(reservation, &fence, "historical-native")
        .await
        .unwrap();
    store
        .settle_turn(
            reservation,
            &fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(200).unwrap(),
                actual_cost: Some("1.5".parse().unwrap()),
                currency: Some("USD".into()),
                reason_code: "NATIVE_COMPLETED".into(),
            },
        )
        .await
        .unwrap();
    let used: i64 = sqlx::query_scalar(
        "SELECT actual_tokens::bigint FROM app.model_turn_receipts WHERE reservation_id=$1",
    )
    .bind(reservation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(used, 200);
}
