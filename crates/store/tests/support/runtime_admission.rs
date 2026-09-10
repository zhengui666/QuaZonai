//! Real Store admission/dispatch transactions with explicitly controlled observations.
use super::*;
use contracts::runtime::{RuntimeProbeFailure, RuntimeProbeOutcomeV1};

async fn counts(pool: &PgPool) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.run_events),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.command_receipts)")
        .fetch_one(pool).await.unwrap()
}
fn standalone(fixture: &support::Fixture, request: &RunSubmission) -> StandaloneRunSubmission {
    let mut limits = request.limits.clone();
    limits.experiments = 0;
    StandaloneRunSubmission {
        project_id: fixture.project,
        input_set_id: request.input_set_id,
        runtime_id: request.runtime_id,
        runtime_revision: request.runtime_revision,
        kind: RunKind::DataValidate,
        limits,
        max_parallel_runs: 2,
    }
}
async fn reject_both(
    pool: &PgPool,
    store: &Store,
    fixture: &support::Fixture,
    request: &RunSubmission,
) {
    let before = counts(pool).await;
    let budget_before = usage(pool, fixture.cycle).await;
    assert!(matches!(
        store.enqueue_run("rejected-cycle", request).await,
        Err(StoreError::Domain(DomainError::CapabilityUnavailable(_)))
    ));
    assert!(matches!(
        store
            .enqueue_standalone_run("rejected-standalone", &standalone(fixture, request))
            .await,
        Err(StoreError::Domain(DomainError::CapabilityUnavailable(_)))
    ));
    assert_eq!(counts(pool).await, before);
    assert_eq!(usage(pool, fixture.cycle).await, budget_before);
}
async fn unavailable(pool: &PgPool, runtime: Id) {
    runtime_observation::publish(
        pool,
        runtime,
        RuntimeProbeOutcomeV1::Unavailable {
            reason: RuntimeProbeFailure::Unavailable,
        },
        Duration::seconds(60),
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_expired_and_failed_observations_cannot_consume_budget_or_queue_work(pool: PgPool) {
    let (store, fixture, mut request, _) = setup(&pool).await;
    let unchecked = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) SELECT $1,'unprobed admission fixture',endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled FROM app.runtime_integrations WHERE id=$2")
        .bind(unchecked.as_uuid()).bind(request.runtime_id.as_uuid()).execute(&pool).await.unwrap();
    request.runtime_id = unchecked;
    reject_both(&pool, &store, &fixture, &request).await;
    let capabilities = runtime_observation::configured_capabilities(&pool, unchecked).await;
    runtime_observation::publish(
        &pool,
        unchecked,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(capabilities),
        },
        Duration::milliseconds(1),
    )
    .await;
    // Observe the actual database deadline; do not mutate the append-only record.
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let expired: bool = sqlx::query_scalar("SELECT clock_timestamp() >= valid_until FROM app.runtime_probe_observations WHERE runtime_id=$1 ORDER BY observed_at DESC,id DESC LIMIT 1")
                .bind(unchecked.as_uuid()).fetch_one(&pool).await.unwrap();
            if expired { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    reject_both(&pool, &store, &fixture, &request).await;
    unavailable(&pool, unchecked).await;
    reject_both(&pool, &store, &fixture, &request).await;
    runtime_observation::ready(&pool, unchecked).await;
    let first = store.enqueue_run("rejected-cycle", &request).await.unwrap();
    let admin = standalone(&fixture, &request);
    let second = store
        .enqueue_standalone_run("rejected-standalone", &admin)
        .await
        .unwrap();
    assert!(!first.replayed && !second.replayed);
    let before = counts(&pool).await;
    let reserved = usage(&pool, fixture.cycle).await;
    unavailable(&pool, unchecked).await;
    let replay = store.enqueue_run("rejected-cycle", &request).await.unwrap();
    let admin_replay = store
        .enqueue_standalone_run("rejected-standalone", &admin)
        .await
        .unwrap();
    assert!(replay.replayed && admin_replay.replayed);
    assert_eq!(replay.resource, first.resource);
    assert_eq!(admin_replay.resource, second.resource);
    assert_eq!(counts(&pool).await, before);
    assert_eq!(usage(&pool, fixture.cycle).await, reserved);
}

#[sqlx::test(migrations = "../../migrations")]
async fn actual_runtime_wall_memory_and_output_capacity_gate_every_admission(pool: PgPool) {
    let (store, fixture, request, _) = setup(&pool).await;
    for dimension in 0..3 {
        let mut capabilities =
            runtime_observation::configured_capabilities(&pool, request.runtime_id).await;
        match dimension {
            0 => capabilities.max_wall_seconds = request.limits.wall_seconds - 1,
            1 => capabilities.max_memory_mib = request.limits.memory_mib - 1,
            _ => {
                capabilities.max_output_bytes =
                    DbCounter::new(request.limits.output_bytes.get() - 1).unwrap()
            }
        }
        runtime_observation::publish(
            &pool,
            request.runtime_id,
            RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            Duration::seconds(60),
        )
        .await;
        reject_both(&pool, &store, &fixture, &request).await;
    }
    let mut capabilities =
        runtime_observation::configured_capabilities(&pool, request.runtime_id).await;
    capabilities.max_wall_seconds = request.limits.wall_seconds;
    capabilities.max_memory_mib = request.limits.memory_mib;
    capabilities.max_output_bytes = request.limits.output_bytes;
    runtime_observation::publish(
        &pool,
        request.runtime_id,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(capabilities),
        },
        Duration::seconds(60),
    )
    .await;
    assert!(store
        .enqueue_run("exact-cycle-capacity", &request)
        .await
        .is_ok());
    assert!(store
        .enqueue_standalone_run("exact-standalone-capacity", &standalone(&fixture, &request))
        .await
        .is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_send_rechecks_readiness_and_limits_but_sent_unknown_keeps_its_identity(
    pool: PgPool,
) {
    let (store, _, request, _) = setup(&pool).await;
    let run = store
        .enqueue_run("dispatch-health", &request)
        .await
        .unwrap()
        .resource;
    let lease = leased(&store, &message(&store, run.id).await, "readiness-owner").await;
    unavailable(&pool, request.runtime_id).await;
    assert!(matches!(
        store.begin_run_dispatch(run.id, &lease.fence).await,
        Err(StoreError::Domain(DomainError::CapabilityUnavailable(_)))
    ));
    for dimension in 0..3 {
        let mut capabilities =
            runtime_observation::configured_capabilities(&pool, request.runtime_id).await;
        match dimension {
            0 => capabilities.max_wall_seconds = request.limits.wall_seconds - 1,
            1 => capabilities.max_memory_mib = request.limits.memory_mib - 1,
            _ => {
                capabilities.max_output_bytes =
                    DbCounter::new(request.limits.output_bytes.get() - 1).unwrap()
            }
        }
        runtime_observation::publish(
            &pool,
            request.runtime_id,
            RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            Duration::seconds(60),
        )
        .await;
        assert!(matches!(
            store.begin_run_dispatch(run.id, &lease.fence).await,
            Err(StoreError::Domain(DomainError::CapabilityUnavailable(_)))
        ));
        let state: String =
            sqlx::query_scalar("SELECT dispatch_state FROM app.run_attempts WHERE id=$1")
                .bind(lease.fence.attempt_id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(state, "NOT_SENT");
    }
    runtime_observation::ready(&pool, request.runtime_id).await;
    assert!(store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    unavailable(&pool, request.runtime_id).await;
    assert!(!store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    let state: String =
        sqlx::query_scalar("SELECT dispatch_state FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "SENT_UNKNOWN");
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM app.run_attempts WHERE run_id=$1")
        .bind(run.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 1);
}
