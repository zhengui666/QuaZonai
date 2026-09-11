//! Actual Worker, native TCP transport, PostgreSQL/PGMQ and immutable object regression.
//! Remote reports here are controlled fault fixtures, never real science or qualification.
#[path = "support/worker_runtime.rs"]
mod native;
#[path = "../../../crates/store/tests/support/native_tasks.rs"]
mod tasks;

use contracts::{
    lifecycle::RunCancelV1, runs::RunState, runtime_jobs::RuntimeInputV1, Id, SchemaV1,
};
use native::{Behavior, Harness};
use server::worker::{Worker, WorkerFailure};
use sqlx::PgPool;
use std::time::Duration;
use tokio::{sync::watch, task::JoinHandle};

struct Driver {
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<(), WorkerFailure>>,
}
impl Driver {
    fn start(worker: Worker) -> Self {
        let (stop, receiver) = watch::channel(false);
        Self {
            stop,
            task: tokio::spawn(worker.run(receiver)),
        }
    }
    async fn finish(mut self) {
        self.stop.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(10), &mut self.task)
            .await
            .expect("native Worker did not drain bounded work")
            .unwrap()
            .unwrap();
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.task.abort();
    }
}

async fn terminal(pool: &PgPool, run: Id) -> String {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let state: String = sqlx::query_scalar("SELECT state FROM app.runs WHERE id=$1")
                .bind(run.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
            if matches!(state.as_str(), "SUCCEEDED" | "FAILED" | "CANCELLED") {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("native Worker did not reach its expected terminal state")
}

async fn queried(harness: &Harness) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while harness.counts().queries == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Worker did not reconcile the original native identity");
}

#[sqlx::test(migrations = "../../migrations")]
async fn actual_worker_loop_reconciles_lost_submit_ack_without_reposting_and_archives_after_publication(
    pool: PgPool,
) {
    let harness = native::setup(&pool, Behavior::LostSubmitAck).await;
    let f = &harness.fixture;
    let run = tasks::start(f, "worker-lost-ack", &f.request)
        .await
        .unwrap()
        .resource;
    let driver = Driver::start(harness.worker.clone());
    assert_eq!(terminal(&pool, run.id).await, "SUCCEEDED");
    driver.finish().await;
    let counts = harness.counts();
    assert_eq!(counts.submits, 1);
    assert_eq!(counts.uploads, 1);
    assert!(counts.queries >= 1);
    assert_eq!(counts.output_reads, 1);
    assert_eq!(counts.cancels, 0);
    let facts: (i64, i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM pgmq.a_runs)")
        .bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 2, 0, 1));
    let origin: String = sqlx::query_scalar(
        "SELECT origin FROM app.artifacts WHERE producer_run_id=$1 AND kind='DATA_QUALITY'",
    )
    .bind(run.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(origin, "FIXTURE");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_worker_recovers_the_exact_sent_attempt_after_real_lease_expiry(pool: PgPool) {
    let harness = native::setup(&pool, Behavior::LostSubmitAck).await;
    let f = &harness.fixture;
    let run = tasks::start(f, "worker-recovery", &f.request)
        .await
        .unwrap()
        .resource;
    let message = tasks::message(f, run.id).await;
    let old = tasks::lease(f, &message, "exited-driver", 1).await;
    let job = f.data.store.native_job(run.id, &old.fence).await.unwrap();
    let transport = harness.transport(&old);
    for input in &job.spec.inputs {
        if let RuntimeInputV1::Artifact {
            artifact_id,
            byte_count,
            storage_version,
            ..
        } = input
        {
            let bytes = f.data.objects.read(*artifact_id, *byte_count).unwrap();
            transport
                .upload_object(*artifact_id, storage_version, bytes)
                .await
                .unwrap();
        }
    }
    assert!(f
        .data
        .store
        .begin_run_dispatch(run.id, &old.fence)
        .await
        .unwrap());
    assert!(matches!(
        transport.submit_job(&job.spec).await,
        Err(server::runtime_transport::RuntimeRequestError::Unavailable)
    ));
    // No live driver owns the lease now. This is an actual lease-expiry/recovery
    // test, not a claim that the test killed a production OS process.
    tasks::wait_expired(&pool, old.fence.attempt_id).await;
    let (_stop, receiver) = watch::channel(false);
    tokio::time::timeout(
        Duration::from_secs(10),
        harness
            .worker
            .process_message(message, "recovered-driver", receiver),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        harness.received_spec(&job.spec.external_job_id),
        serde_json::to_value(&job.spec).unwrap()
    );
    assert_eq!(harness.counts().submits, 1);
    assert_eq!(harness.counts().uploads, 1);
    let stored = f.data.store.get_run(&f.data.actor, run.id).await.unwrap();
    assert_eq!(stored.state, RunState::Succeeded);
    assert_eq!(stored.active_attempt_id, Some(old.fence.attempt_id));
    let epoch: i64 = sqlx::query_scalar("SELECT owner_epoch FROM app.run_attempts WHERE id=$1")
        .bind(old.fence.attempt_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(epoch as u64 > old.fence.owner_epoch.get());
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_404_is_not_cancellation_and_only_the_matching_durable_tombstone_closes_the_run(
    pool: PgPool,
) {
    let harness = native::setup(&pool, Behavior::MissingUntilCancelled).await;
    let f = &harness.fixture;
    let run = tasks::start(f, "worker-missing", &f.request)
        .await
        .unwrap()
        .resource;
    let driver = Driver::start(harness.worker.clone());
    queried(&harness).await;
    let current = f.data.store.get_run(&f.data.actor, run.id).await.unwrap();
    assert!(!current.state.is_terminal());
    let receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(receipts, 0);
    assert_eq!(harness.counts().cancels, 0);
    let requested = f
        .data
        .store
        .cancel_run(
            &f.data.actor,
            "worker-cancel",
            run.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    assert_eq!(requested.resource.state, RunState::CancelRequested);
    assert_eq!(terminal(&pool, run.id).await, "CANCELLED");
    driver.finish().await;
    let counts = harness.counts();
    assert_eq!(counts.submits, 1);
    assert_eq!(counts.cancels, 1);
    assert_eq!(counts.output_reads, 0);
    let manifest: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT result_manifest_artifact_id FROM app.run_attempts WHERE run_id=$1",
    )
    .bind(run.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(manifest.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_native_payload_becomes_a_failed_run_without_publishing_bad_scientific_bytes(
    pool: PgPool,
) {
    let harness = native::setup(&pool, Behavior::InvalidPayload).await;
    let f = &harness.fixture;
    let run = tasks::start(f, "worker-invalid-output", &f.request)
        .await
        .unwrap()
        .resource;
    let driver = Driver::start(harness.worker.clone());
    assert_eq!(terminal(&pool, run.id).await, "FAILED");
    driver.finish().await;
    let error: (String, String) =
        sqlx::query_as("SELECT error_class,error_code FROM app.run_attempts WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        error,
        ("INVALID_INPUT".into(), "NATIVE_OUTPUT_INVALID".into())
    );
    let facts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_native_outputs o JOIN app.run_attempts a ON a.id=o.attempt_id WHERE a.run_id=$1),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1),(SELECT count(*) FROM app.qualifications)")
        .bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 1, 0));
    assert_eq!(harness.counts().submits, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn stopping_a_worker_does_not_cancel_or_acknowledge_an_unknown_remote_job(pool: PgPool) {
    let harness = native::setup(&pool, Behavior::MissingUntilCancelled).await;
    let f = &harness.fixture;
    let run = tasks::start(f, "worker-stop", &f.request)
        .await
        .unwrap()
        .resource;
    let driver = Driver::start(harness.worker.clone());
    queried(&harness).await;
    driver.finish().await;
    assert_eq!(harness.counts().submits, 1);
    assert_eq!(harness.counts().cancels, 0);
    assert!(!f
        .data
        .store
        .get_run(&f.data.actor, run.id)
        .await
        .unwrap()
        .state
        .is_terminal());
    let facts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM pgmq.a_runs)")
        .bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 1, 0));
}
