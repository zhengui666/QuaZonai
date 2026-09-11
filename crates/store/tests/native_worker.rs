//! Native Run/Attempt orchestration with actual PostgreSQL, queue and object publication.
//! Controlled terminal reports are never claims of real native computation or qualification.
#[path = "support/native_tasks.rs"]
mod support;
use contracts::{
    lifecycle::RunCancelV1,
    runs::RunState,
    runtime_jobs::{RuntimeJobState, RuntimeJobStatusV1},
    DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use store::{
    lifecycle::{
        native::NativePayloads, ClaimResult, NativeOutcome, NextRuntimeAction, TerminalObservation,
    },
    StoreError,
};
use support::*;

#[sqlx::test(migrations = "../../migrations")]
async fn takeover_retains_the_original_spec_and_only_one_first_send_permit(pool: PgPool) {
    let f = setup(&pool).await;
    let run = start(&f, "stable-spec", &f.request).await.unwrap().resource;
    let msg = message(&f, run.id).await;
    let old = lease(&f, &msg, "before-crash", 1).await;
    let original = f.data.store.native_job(run.id, &old.fence).await.unwrap();
    let exact = serde_json::to_value(&original.spec).unwrap();
    wait_expired(&pool, old.fence.attempt_id).await;
    let current = lease(&f, &msg, "after-takeover", 30).await;
    assert_eq!(current.fence.attempt_id, old.fence.attempt_id);
    assert!(current.fence.owner_epoch > old.fence.owner_epoch);
    let recovered = f
        .data
        .store
        .native_job(run.id, &current.fence)
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(&recovered.spec).unwrap(), exact);
    assert_eq!(recovered.spec.owner_epoch, old.fence.owner_epoch);
    assert_eq!(
        recovered.spec.external_job_id,
        original.spec.external_job_id
    );
    assert!(matches!(
        f.data.store.begin_run_dispatch(run.id, &old.fence).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    assert!(f
        .data
        .store
        .begin_run_dispatch(run.id, &current.fence)
        .await
        .unwrap());
    assert!(!f
        .data
        .store
        .begin_run_dispatch(run.id, &current.fence)
        .await
        .unwrap());
    let reconciling = f
        .data
        .store
        .native_job(run.id, &current.fence)
        .await
        .unwrap();
    assert!(matches!(reconciling.action, NextRuntimeAction::Reconcile));
    let attempts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_native_attempts WHERE run_id=$1)")
        .bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(attempts, (1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn accepted_is_not_running_and_manifest_publication_precedes_idempotent_queue_archive(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    let run = start(&f, "actual-receipt", &f.request)
        .await
        .unwrap()
        .resource;
    let msg = message(&f, run.id).await;
    let lease = lease(&f, &msg, "result-owner", 30).await;
    let job = f.data.store.native_job(run.id, &lease.fence).await.unwrap();
    assert!(f
        .data
        .store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    let accepted = RuntimeJobStatusV1 {
        schema_version: SchemaV1,
        run_id: run.id,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id.clone(),
        state: RuntimeJobState::Accepted,
        has_result: false,
        submitted_at: clock(&pool).await,
        started_at: None,
        finished_at: None,
    };
    f.data
        .store
        .observe_native_accepted(run.id, &lease.fence, &accepted)
        .await
        .unwrap();
    assert_eq!(
        f.data
            .store
            .get_run(&f.data.actor, run.id)
            .await
            .unwrap()
            .state,
        RunState::Dispatching
    );
    assert!(matches!(
        f.data.store.acknowledge_run(&msg).await,
        Err(StoreError::Conflict)
    ));
    let (manifest, outputs) = result(&pool, &f, &job).await;
    let first = adopt(&f, &lease, &manifest, outputs.clone()).await.unwrap();
    assert_eq!(first.resource.state, RunState::Succeeded);
    assert!(!first.replayed);
    let durable: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_native_outputs WHERE attempt_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$2),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$2),(SELECT count(*) FROM pgmq.q_runs)")
        .bind(lease.fence.attempt_id.as_uuid()).bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(durable, (1, 1, 2, 1));
    let replay = adopt(&f, &lease, &manifest, outputs.clone()).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, first.resource);
    let observed = f
        .data
        .store
        .claim_run(&msg, "redelivery-after-commit", 30)
        .await
        .unwrap();
    assert!(matches!(observed, ClaimResult::Terminal(_)));
    let stored: (uuid::Uuid, i64, String) = sqlx::query_as("SELECT a.id,a.byte_count,a.origin FROM app.artifacts a JOIN app.run_native_outputs o ON o.artifact_id=a.id WHERE o.attempt_id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
    let id: Id = stored.0.to_string().try_into().unwrap();
    assert_eq!(stored.2, "FIXTURE");
    assert_eq!(
        f.data
            .objects
            .read(id, DbCounter::new(stored.1 as u64).unwrap())
            .unwrap(),
        outputs[0].1
    );
    // Recreating a Store client is a replay check, not a claimed OS crash test.
    let recreated = store::Store::from_pool(pool.clone());
    recreated.acknowledge_run(&msg).await.unwrap();
    recreated.acknowledge_run(&msg).await.unwrap();
    let queues: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM pgmq.a_runs)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queues, (0, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn different_terminal_bytes_and_old_owner_cannot_publish_or_replace_a_result(pool: PgPool) {
    let f = setup(&pool).await;
    let run = start(&f, "publication-binding", &f.request)
        .await
        .unwrap()
        .resource;
    let msg = message(&f, run.id).await;
    let old = lease(&f, &msg, "original-owner", 1).await;
    let old_job = f.data.store.native_job(run.id, &old.fence).await.unwrap();
    assert!(f
        .data
        .store
        .begin_run_dispatch(run.id, &old.fence)
        .await
        .unwrap());
    wait_expired(&pool, old.fence.attempt_id).await;
    let current = lease(&f, &msg, "current-owner", 30).await;
    let job = f
        .data
        .store
        .native_job(run.id, &current.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.external_job_id, old_job.spec.external_job_id);
    let (manifest, outputs) = result(&pool, &f, &job).await;
    assert!(matches!(
        adopt(&f, &old, &manifest, outputs.clone()).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    adopt(&f, &current, &manifest, outputs.clone())
        .await
        .unwrap();
    assert!(matches!(
        adopt(&f, &old, &manifest, outputs.clone()).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    let reading = f.data.objects.clone();
    let conflict = f
        .data
        .store
        .publish_native_result(
            run.id,
            &current.fence,
            serde_json::to_vec_pretty(&manifest).unwrap(),
            NativePayloads::Verified(outputs),
            move |id, size| data::read(reading, id, size),
            |_| async { panic!("a conflicting terminal must not write native files") },
        )
        .await;
    assert!(matches!(conflict, Err(StoreError::Conflict)));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_native_outputs WHERE attempt_id=$1")
            .bind(current.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_winner_keeps_the_manifest_for_audit_but_does_not_publish_success_payloads(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    let run = start(&f, "cancel-result", &f.request)
        .await
        .unwrap()
        .resource;
    let msg = message(&f, run.id).await;
    let lease = lease(&f, &msg, "cancel-owner", 30).await;
    let job = f.data.store.native_job(run.id, &lease.fence).await.unwrap();
    assert!(f
        .data
        .store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    let (manifest, outputs) = result(&pool, &f, &job).await;
    let current = f.data.store.get_run(&f.data.actor, run.id).await.unwrap();
    let cancelled = f
        .data
        .store
        .cancel_run(
            &f.data.actor,
            "request-cancel",
            run.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    assert_eq!(cancelled.resource.state, RunState::CancelRequested);
    let adopted = adopt(&f, &lease, &manifest, outputs).await.unwrap();
    assert_eq!(adopted.resource.state, RunState::Cancelled);
    let facts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_native_outputs WHERE attempt_id=$1),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$2),(SELECT count(*) FROM app.qualifications)")
        .bind(lease.fence.attempt_id.as_uuid()).bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 1, 0));
    f.data.store.acknowledge_run(&msg).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn elapsed_sent_deadline_records_cancel_intent_before_accepting_a_native_tombstone(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    let mut request = f.request.clone();
    request.limits.wall_seconds = 1;
    request.limits.cpu_seconds = DbCounter::new(1).unwrap();
    let run = start(&f, "deadline", &request).await.unwrap().resource;
    let msg = message(&f, run.id).await;
    let lease = lease(&f, &msg, "deadline-owner", 30).await;
    let original = f.data.store.native_job(run.id, &lease.fence).await.unwrap();
    assert!(f
        .data
        .store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while clock(&pool).await < run.deadline_at {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let due = f.data.store.native_job(run.id, &lease.fence).await.unwrap();
    assert_eq!(due.run.state, RunState::CancelRequested);
    assert!(matches!(due.action, NextRuntimeAction::Cancel));
    assert_eq!(
        serde_json::to_value(&due.spec).unwrap(),
        serde_json::to_value(&original.spec).unwrap()
    );
    assert!(matches!(
        f.data.store.acknowledge_run(&msg).await,
        Err(StoreError::Conflict)
    ));
    let terminal = f
        .data
        .store
        .accept_run_terminal(
            run.id,
            &lease.fence,
            &TerminalObservation {
                schema_version: SchemaV1,
                external_job_id: due.spec.external_job_id,
                outcome: NativeOutcome::ConfirmedAbsent,
                manifest_artifact_id: None,
                failure_class: None,
                failure_code: None,
                observed_at: clock(&pool).await,
            },
        )
        .await
        .unwrap();
    assert_eq!(terminal.resource.state, RunState::Cancelled);
    f.data.store.acknowledge_run(&msg).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn unsubmitted_cancellation_and_publication_failure_do_not_create_remote_success(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    let run = start(&f, "not-sent", &f.request).await.unwrap().resource;
    let msg = message(&f, run.id).await;
    let lease = lease(&f, &msg, "unsent-owner", 30).await;
    let current = f.data.store.get_run(&f.data.actor, run.id).await.unwrap();
    f.data
        .store
        .cancel_run(
            &f.data.actor,
            "cancel-unsent",
            run.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    let terminal = f
        .data
        .store
        .settle_unsubmitted_native_run(run.id, &lease.fence)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(terminal.state, RunState::Cancelled);
    let state: String =
        sqlx::query_scalar("SELECT runtime_state FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "UNKNOWN");
    let other = start(&f, "file-failure", &f.request)
        .await
        .unwrap()
        .resource;
    let other_msg = message(&f, other.id).await;
    let other_lease = support::lease(&f, &other_msg, "file-owner", 30).await;
    let job = f
        .data
        .store
        .native_job(other.id, &other_lease.fence)
        .await
        .unwrap();
    f.data
        .store
        .begin_run_dispatch(other.id, &other_lease.fence)
        .await
        .unwrap();
    let (manifest, outputs) = result(&pool, &f, &job).await;
    let reading = f.data.objects.clone();
    assert!(matches!(
        f.data
            .store
            .publish_native_result(
                other.id,
                &other_lease.fence,
                serde_json::to_vec(&manifest).unwrap(),
                NativePayloads::Verified(outputs.clone()),
                move |id, size| data::read(reading, id, size),
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let rows: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1)")
        .bind(other.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(rows, (0, 0));
    assert!(matches!(
        f.data.store.acknowledge_run(&other_msg).await,
        Err(StoreError::Conflict)
    ));
    adopt(&f, &other_lease, &manifest, outputs).await.unwrap();
}
