//! Real authoring/claim setup for an expired unsent Mission; no model process.
#![allow(dead_code)]
use super::{cycle_support, mission_support, research_support};
use contracts::{Id, SchemaV1};
use sqlx::PgPool;
use store::{
    authority::Actor,
    lifecycle::{mission::resources::MissionResource, ClaimResult, RunLease, RunMessage},
    Store,
};

pub struct ExpiredMission {
    pub store: Store,
    pub actor: Actor,
    pub fixture: cycle_support::Fixture,
    pub lease: RunLease,
    pub message: RunMessage,
    pub resource: MissionResource,
}

pub async fn setup(pool: &PgPool, launched: bool) -> ExpiredMission {
    let (store, actor) = research_support::operator(pool).await;
    let mut f = cycle_support::setup(pool, &store, &actor).await;
    let mut content = f.brief.content.clone();
    // Author a real short finite deadline; never rewrite immutable Run history.
    content.budget.max_wall_seconds = Some(10);
    content.budget.max_cpu_seconds = Some(contracts::DbCounter::new(100).unwrap());
    f.brief = store
        .update_brief(
            &actor,
            "short-expired-mission",
            f.brief.id,
            &contracts::brief::BriefUpdate {
                schema_version: SchemaV1,
                expected_revision: f.brief.revision,
                content,
                bindings: f.brief.bindings.clone(),
            },
        )
        .await
        .unwrap()
        .resource;
    f.freeze.expected_revision = f.brief.revision;
    let (store, actor, f, _, preparation) = mission_support::start(store, actor, f, false).await;
    mission_support::complete(pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store
        .read_mission_messages(30, 100)
        .await
        .unwrap()
        .remove(0);
    let Some(ClaimResult::Leased(lease)) = store
        .claim_mission(&message, "bootstrap-owner", 60)
        .await
        .unwrap()
    else {
        panic!("initial fenced owner");
    };
    let (resource, _) = store
        .reserve_mission_resource(lease.run.id, &lease.fence, "HOST")
        .await
        .unwrap();
    if launched {
        store
            .begin_mission_resource_launch(lease.run.id, &lease.fence, resource.id)
            .await
            .unwrap();
    }
    let deadline = lease.run.deadline_at.expect("finite fixture deadline");
    sqlx::query("SELECT pg_sleep((greatest(0,extract(epoch FROM ($1::timestamptz-clock_timestamp())))+0.02)::double precision)")
        .bind(deadline).execute(pool).await.unwrap();
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(pool).await.unwrap();
    let state: String =
        sqlx::query_scalar("SELECT dispatch_state FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(state, "NOT_SENT");
    ExpiredMission {
        store,
        actor,
        fixture: f,
        lease: *lease,
        message,
        resource,
    }
}

pub async fn assert_no_model_or_new_resource(pool: &PgPool, run: Id) {
    let facts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.codex_sessions WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.mission_resources WHERE run_id=$1)"
    ).bind(run.as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(facts, (0, 0, 1));
}
