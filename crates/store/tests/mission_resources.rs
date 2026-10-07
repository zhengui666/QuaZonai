//! Real PostgreSQL transaction tests; no synthetic model or container execution.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_fixture;
mod runtime_support {
    fn optional(
        mut value: contracts::runtime::RuntimeCapabilitiesV1,
    ) -> contracts::runtime::RuntimeCapabilitiesV1 {
        for name in ["optional-cpu-budget", "optional-output-budget"] {
            value.engine_versions.insert(name.into(), "1".into());
        }
        value
    }
    pub fn capabilities(
        now: chrono::DateTime<chrono::Utc>,
    ) -> contracts::runtime::RuntimeCapabilitiesV1 {
        optional(super::runtime_fixture::capabilities(now))
    }
    pub fn portfolio_capabilities(
        now: chrono::DateTime<chrono::Utc>,
    ) -> contracts::runtime::RuntimeCapabilitiesV1 {
        optional(super::runtime_fixture::portfolio_capabilities(now))
    }
}
use sqlx::PgPool;
use store::{
    lifecycle::{ClaimResult, RunLease, RunMessage},
    Store,
};

async fn setup(pool: &PgPool) -> (Store, RunLease) {
    setup_budget(pool, false).await
}
async fn setup_budget(pool: &PgPool, unlimited: bool) -> (Store, RunLease) {
    let (store, actor) = research_support::operator(pool).await;
    let mut fixture = cycle_support::setup(pool, &store, &actor).await;
    if unlimited {
        let mut content = fixture.brief.content.clone();
        content.budget.max_cpu_seconds = None;
        content.budget.max_output_bytes = None;
        content.budget.max_wall_seconds = None;
        content.budget.max_tokens = None;
        fixture.brief = store
            .update_brief(
                &actor,
                "unlimited-resource-fixture",
                fixture.brief.id,
                &contracts::brief::BriefUpdate {
                    schema_version: contracts::SchemaV1,
                    expected_revision: fixture.brief.revision,
                    content,
                    bindings: fixture.brief.bindings.clone(),
                },
            )
            .await
            .unwrap()
            .resource;
        fixture.freeze.expected_revision = fixture.brief.revision;
    }
    let (store, _, fixture, _, preparation) =
        mission_support::start(store, actor, fixture, false).await;
    mission_support::complete(pool, &store, &fixture, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store
        .read_mission_messages(30, 100)
        .await
        .unwrap()
        .remove(0);
    let Some(ClaimResult::Leased(lease)) = store
        .claim_mission(&message, "resource-owner", 60)
        .await
        .unwrap()
    else {
        panic!("lease");
    };
    (store, *lease)
}
#[sqlx::test(migrations = "../../migrations")]
async fn closed_launch_usage_is_retained_across_same_epoch_reopen(pool: PgPool) {
    let (store, lease) = setup(&pool).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    let (first, used) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    assert_eq!(used, Some(0));
    store
        .begin_mission_resource_launch(run, owner, first.id)
        .await
        .unwrap();
    store
        .bind_mission_resource(run, owner, first.id, "fixture-cgroup-identity")
        .await
        .unwrap();
    assert!(!store
        .checkpoint_mission_resource(run, owner, first.id, Some(600_000_000), false, false)
        .await
        .unwrap());
    assert!(store.mission_open_resources(run, owner).await.is_err());
    assert!(store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .is_err());
    assert!(store
        .checkpoint_mission_resource(run, owner, first.id, Some(599_999_999), false, false)
        .await
        .is_err());
    store
        .checkpoint_mission_resource(run, owner, first.id, Some(650_000_000), true, false)
        .await
        .unwrap();
    assert!(store
        .checkpoint_mission_resource(run, owner, first.id, Some(650_000_001), true, true)
        .await
        .is_err());
    store
        .checkpoint_mission_resource(run, owner, first.id, Some(650_000_000), true, true)
        .await
        .unwrap();
    let (second, used) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    assert_eq!(used, Some(650_000_000));
    assert_ne!(first.name(), second.name());
}
#[sqlx::test(migrations = "../../migrations")]
async fn only_expired_taken_over_owner_resources_are_reclaimable(pool: PgPool) {
    let (store, lease) = setup(&pool).await;
    let run = lease.run.id;
    let (old, _) = store
        .reserve_mission_resource(run, &lease.fence, "DOCKER")
        .await
        .unwrap();
    store
        .begin_mission_resource_launch(run, &lease.fence, old.id)
        .await
        .unwrap();
    store
        .bind_mission_resource(run, &lease.fence, old.id, "fixture-docker-id")
        .await
        .unwrap();
    store
        .begin_mission_resource_execution(run, &lease.fence, old.id)
        .await
        .unwrap();
    store
        .checkpoint_mission_resource(run, &lease.fence, old.id, Some(700_000_000), false, false)
        .await
        .unwrap();
    let (message_id, read_count): (i64, i32) =
        sqlx::query_as("SELECT msg_id,read_ct FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(run.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    let message = RunMessage {
        message_id,
        read_count,
        run_id: run,
    };
    assert!(matches!(
        store
            .claim_mission(&message, "replacement", 60)
            .await
            .unwrap(),
        Some(ClaimResult::Busy)
    ));
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(store
        .mission_open_resources(run, &lease.fence)
        .await
        .is_err());
    let Some(ClaimResult::Leased(replacement)) = store
        .claim_mission(&message, "replacement", 60)
        .await
        .unwrap()
    else {
        panic!("takeover");
    };
    let recovered = store
        .mission_open_resources(run, &replacement.fence)
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, old.id);
    assert!(store
        .checkpoint_mission_resource(run, &lease.fence, old.id, Some(800_000_000), true, false)
        .await
        .is_err());
    store
        .checkpoint_mission_resource(
            run,
            &replacement.fence,
            old.id,
            Some(900_000_000),
            true,
            false,
        )
        .await
        .unwrap();
    assert!(store
        .reserve_mission_resource(run, &replacement.fence, "DOCKER")
        .await
        .is_err());
    store
        .checkpoint_mission_resource(
            run,
            &replacement.fence,
            old.id,
            Some(900_000_000),
            true,
            true,
        )
        .await
        .unwrap();
    let (new, used) = store
        .reserve_mission_resource(run, &replacement.fence, "DOCKER")
        .await
        .unwrap();
    assert_eq!(used, Some(900_000_000));
    assert!(new.owner_epoch > old.owner_epoch);
}
#[sqlx::test(migrations = "../../migrations")]
async fn cumulative_limit_requests_cancel_and_cannot_be_reset(pool: PgPool) {
    let (store, lease) = setup(&pool).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    let (resource, _) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    store
        .begin_mission_resource_launch(run, owner, resource.id)
        .await
        .unwrap();
    let grant = lease.limits.cpu_seconds.unwrap().get() * 1_000_000_000;
    assert!(store
        .checkpoint_mission_resource(run, owner, resource.id, Some(grant), true, false)
        .await
        .unwrap());
    assert!(store
        .checkpoint_mission_resource(run, owner, resource.id, Some(grant), true, true)
        .await
        .unwrap());
    assert!(store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .is_err());
    let state: String = sqlx::query_scalar("SELECT state FROM app.runs WHERE id=$1")
        .bind(run.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "CANCEL_REQUESTED");
}

#[sqlx::test(migrations = "../../migrations")]
async fn reservation_before_launch_can_close_without_inventing_execution(pool: PgPool) {
    let (store, lease) = setup(&pool).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    let (resource, _) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    assert!(!resource.launch_requested);
    let pending = store.mission_open_resources(run, owner).await.unwrap();
    assert_eq!(pending[0].id, resource.id);
    store
        .checkpoint_mission_resource(run, owner, resource.id, Some(0), true, true)
        .await
        .unwrap();
    let (next, used) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    assert_eq!(used, Some(0));
    store
        .begin_mission_resource_launch(run, owner, next.id)
        .await
        .unwrap();
    // After the durable start intent, absence is no longer proof of zero CPU.
    assert!(store.mission_open_resources(run, owner).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_finite_session_keeps_original_execution_semantics(pool: PgPool) {
    let (store, lease) = setup(&pool).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    assert!(lease.limits.wall_seconds.is_some());
    assert!(store
        .mission_resource_accounting_required(run, owner)
        .await
        .unwrap());
    store.begin_run_dispatch(run, owner).await.unwrap();
    let receipt = store::lifecycle::mission::NativeSessionReceipt {
        thread_id: contracts::Id::new().to_string(),
        codex_version: "0.144.4".into(),
        protocol_schema_version: "v2".into(),
        requested_service_tier: None,
        effective: contracts::codex::CodexEffectiveSettingsV1 {
            model: "controlled-native-model".into(),
            provider: "controlled-native-provider".into(),
            reasoning_effort: Some("medium".into()),
            service_tier: None,
        },
    };
    store
        .bind_mission_session(run, owner, &receipt)
        .await
        .unwrap();
    assert!(!store
        .mission_resource_accounting_required(run, owner)
        .await
        .unwrap());
    // Directly requesting a new ledger must still refuse unknown historical CPU.
    assert!(store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn docker_create_is_not_a_start_permit(pool: PgPool) {
    let (store, lease) = setup(&pool).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    let (resource, _) = store
        .reserve_mission_resource(run, owner, "DOCKER")
        .await
        .unwrap();
    assert!(store
        .begin_mission_resource_execution(run, owner, resource.id)
        .await
        .is_err());
    store
        .begin_mission_resource_launch(run, owner, resource.id)
        .await
        .unwrap();
    assert!(store
        .begin_mission_resource_execution(run, owner, resource.id)
        .await
        .is_err());
    store
        .bind_mission_resource(run, owner, resource.id, "fixture-created-id")
        .await
        .unwrap();
    assert!(store
        .checkpoint_mission_resource(run, owner, resource.id, Some(1), false, false)
        .await
        .is_err());
    store
        .checkpoint_mission_resource(run, owner, resource.id, Some(0), true, true)
        .await
        .unwrap();
    let (_, used) = store
        .reserve_mission_resource(run, owner, "DOCKER")
        .await
        .unwrap();
    assert_eq!(used, Some(0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unlimited_cpu_recovers_with_null_usage_without_resetting_it_to_zero(pool: PgPool) {
    let (store, lease) = setup_budget(&pool, true).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    assert!(lease.limits.cpu_seconds.is_none());
    assert!(lease.limits.wall_seconds.is_none());
    assert!(lease.limits.output_bytes.is_none());
    let (first, used) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    assert_eq!(used, Some(0));
    store
        .begin_mission_resource_launch(run, owner, first.id)
        .await
        .unwrap();
    store
        .checkpoint_mission_resource(run, owner, first.id, Some(100), false, false)
        .await
        .unwrap();
    // Adapter has confirmed the exact resource stopped but final stats vanished.
    store
        .checkpoint_mission_resource(run, owner, first.id, None, false, true)
        .await
        .unwrap();
    let row:(Option<i64>,bool,bool,bool)=sqlx::query_as("SELECT cpu_nanoseconds,accounting_unknown,final_accounted,closed FROM app.mission_resources WHERE id=$1").bind(first.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(row, (None, true, false, true));
    let (next, used) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    assert_eq!(used, None);
    store
        .begin_mission_resource_launch(run, owner, next.id)
        .await
        .unwrap();
    store
        .checkpoint_mission_resource(run, owner, next.id, Some(200), true, true)
        .await
        .unwrap();
    let (_, used) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    assert_eq!(used, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn finite_cpu_unknown_closure_blocks_further_research(pool: PgPool) {
    let (store, lease) = setup(&pool).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    let (resource, _) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    store
        .begin_mission_resource_launch(run, owner, resource.id)
        .await
        .unwrap();
    store
        .checkpoint_mission_resource(run, owner, resource.id, None, false, true)
        .await
        .unwrap();
    assert!(store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_before_send_waits_for_cleanup_without_creating_a_model_session(pool: PgPool) {
    let (store, lease) = setup_budget(&pool, true).await;
    let run = lease.run.id;
    let owner = &lease.fence;
    let (resource, _) = store
        .reserve_mission_resource(run, owner, "HOST")
        .await
        .unwrap();
    sqlx::query("UPDATE app.runs SET state='CANCEL_REQUESTED',cancellation_requested_at=clock_timestamp() WHERE id=$1")
        .bind(run.as_uuid()).execute(&pool).await.unwrap();
    assert!(!store.complete_mission(run, owner).await.unwrap());
    let denied = sqlx::query("UPDATE app.runs SET state='CANCELLED' WHERE id=$1")
        .bind(run.as_uuid())
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(denied
        .to_string()
        .contains("mission terminal requires confirmed resource cleanup"));
    let before: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.codex_sessions WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1)"
    ).bind(run.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, (0, 0, 0));
    // Reserved but never launched: native cleanup-only has proof of zero use.
    store
        .checkpoint_mission_resource(run, owner, resource.id, Some(0), true, true)
        .await
        .unwrap();
    assert!(store.complete_mission(run, owner).await.unwrap());
    let after: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.codex_sessions WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1)"
    ).bind(run.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(after, (0, 0, 1));
}

#[path = "../../../tests/support/expired_mission_resource.rs"]
mod expired;

#[sqlx::test(migrations = "../../migrations")]
async fn expired_unsent_launched_resource_gets_cleanup_lease_and_keeps_unknown_cpu(pool: PgPool) {
    let f = expired::setup(&pool, true).await;
    let Some(ClaimResult::Leased(replacement)) = f
        .store
        .claim_mission(&f.message, "expired-bootstrap-cleanup", 60)
        .await
        .unwrap()
    else {
        panic!("expired bootstrap must be leased for cleanup, not terminally skipped");
    };
    assert!(replacement.fence.owner_epoch > f.lease.fence.owner_epoch);
    assert_eq!(
        f.store
            .mission_open_resources(f.lease.run.id, &replacement.fence)
            .await
            .unwrap()[0]
            .id,
        f.resource.id
    );
    assert!(!f
        .store
        .complete_mission(f.lease.run.id, &replacement.fence)
        .await
        .unwrap());
    // Trusted native adapter has confirmed stop + launch revocation. Missing
    // final CPU remains unknown; it does not authorize another research grant.
    f.store
        .checkpoint_mission_resource(
            f.lease.run.id,
            &replacement.fence,
            f.resource.id,
            None,
            false,
            true,
        )
        .await
        .unwrap();
    assert!(f
        .store
        .reserve_mission_resource(f.lease.run.id, &replacement.fence, "HOST")
        .await
        .is_err());
    assert!(f
        .store
        .complete_mission(f.lease.run.id, &replacement.fence)
        .await
        .unwrap());
    expired::assert_no_model_or_new_resource(&pool, f.lease.run.id).await;
    let facts: (String, Option<i64>, bool, bool) = sqlx::query_as(
        "SELECT r.state,resource.cpu_nanoseconds,resource.accounting_unknown,resource.closed FROM app.runs r JOIN app.mission_resources resource ON resource.run_id=r.id WHERE r.id=$1"
    ).bind(f.lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, ("FAILED".into(), None, true, true));
}
