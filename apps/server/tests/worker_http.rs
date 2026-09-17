//! Actual Worker, native TCP transport, PostgreSQL/PGMQ and immutable object regression.
//! Remote reports here are controlled fault fixtures, never real science or qualification.
#[path = "support/worker_runtime.rs"]
mod native;
#[path = "support/postgres.rs"]
mod postgres;
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
async fn a_restored_database_recovers_the_exact_sent_attempt_without_reposting(pool: PgPool) {
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
    // No driver is running during this archive. Restore both control database
    // and state files, while keeping the original remote task identity alive.
    let recovered_state = tempfile::tempdir().unwrap();
    let recovered_key = tempfile::tempdir().unwrap();
    let key_path = recovered_key.path().join("worker-master.key");
    std::fs::copy(f.data.directory.path().join("worker-master.key"), &key_path).unwrap();
    let archive = tempfile::NamedTempFile::new().unwrap();
    let saved = tokio::process::Command::new("tar")
        .env_clear()
        .arg("-C")
        .arg(f.data.directory.path())
        .arg("--exclude=./worker-master.key")
        .arg("-cf")
        .arg(archive.path())
        .arg(".")
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        saved.status.success() && saved.stderr.is_empty(),
        "test state archive failed"
    );
    let unpacked = tokio::process::Command::new("tar")
        .env_clear()
        .arg("-C")
        .arg(recovered_state.path())
        .arg("-xf")
        .arg(archive.path())
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        unpacked.status.success() && unpacked.stderr.is_empty(),
        "test state restore failed"
    );
    assert!(!recovered_state.path().join("worker-master.key").exists());
    let dump = postgres::postgres_tool(&pool, "pg_dump")
        .args(["--format=custom", "--no-owner", "--no-privileges"])
        .output()
        .await
        .unwrap();
    assert!(
        dump.status.success() && dump.stderr.is_empty(),
        "test archive failed"
    );
    assert!(dump.stdout.starts_with(b"PGDMP"));
    let database = format!("worker_restore_{}", Id::new().to_string().replace('-', ""));
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE {database} TEMPLATE template0"
    )))
    .execute(&pool)
    .await
    .unwrap();
    let restored = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect_with(pool.connect_options().as_ref().clone().database(&database))
        .await
        .unwrap();
    let mut child = postgres::postgres_tool(&restored, "pg_restore")
        .args([
            "--dbname",
            "",
            "--single-transaction",
            "--exit-on-error",
            "--no-owner",
            "--no-privileges",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    use tokio::io::AsyncWriteExt;
    let (written, output) = tokio::join!(
        async {
            input.write_all(&dump.stdout).await?;
            input.shutdown().await
        },
        child.wait_with_output()
    );
    let output = output.unwrap();
    assert!(
        written.is_ok() && output.status.success() && output.stderr.is_empty(),
        "test restore failed"
    );
    let restored_store = store::Store::from_pool(restored.clone());
    let root = recovered_state.path();
    let worker = Worker::new(
        restored_store.clone(),
        integrations::secrets::SecretVault::open(&root.join("worker-secrets"), &key_path).unwrap(),
        integrations::artifacts::ArtifactStore::open(&root.join("objects")).unwrap(),
        harness.targets.clone(),
        2,
    )
    .unwrap();
    tasks::wait_expired(&restored, old.fence.attempt_id).await;
    let (_stop, receiver) = watch::channel(false);
    tokio::time::timeout(
        Duration::from_secs(10),
        worker.process_message(message, "restored-driver", receiver),
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
    let stored = restored_store.get_run(&f.data.actor, run.id).await.unwrap();
    assert_eq!(stored.state, RunState::Succeeded);
    assert_eq!(stored.active_attempt_id, Some(old.fence.attempt_id));
    let epoch: i64 = sqlx::query_scalar("SELECT owner_epoch FROM app.run_attempts WHERE id=$1")
        .bind(old.fence.attempt_id.as_uuid())
        .fetch_one(&restored)
        .await
        .unwrap();
    assert!(epoch as u64 > old.fence.owner_epoch.get());
    assert_ne!(
        f.data
            .store
            .get_run(&f.data.actor, run.id)
            .await
            .unwrap()
            .state,
        RunState::Succeeded
    );
    drop(worker);
    restored.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {database} WITH (FORCE)"
    )))
    .execute(&pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn killed_worker_process_reconciles_original_job_without_resubmission(pool: PgPool) {
    use sqlx::ConnectOptions;
    use std::{os::unix::process::ExitStatusExt, process::Stdio};

    let harness = native::setup(&pool, Behavior::LostSubmitAck).await;
    harness.hold_results(true);
    let f = &harness.fixture;
    let mut request = f.request.clone();
    request.limits.wall_seconds = 180;
    let run = tasks::start(f, "killed-worker", &request)
        .await
        .unwrap()
        .resource;
    // Move only this test's owned fixtures into the deployed CLI state layout.
    // No parent in-process Worker or ArtifactStore is used after this point.
    let state = f.data.directory.path();
    for (from, to) in [
        ("worker-secrets", "secrets"),
        ("worker-master.key", "master.key"),
        ("objects", "artifacts"),
    ] {
        std::fs::rename(state.join(from), state.join(to)).unwrap();
    }
    let role = format!("worker_crash_{}", Id::new().to_string().replace('-', ""));
    let password = Id::new().to_string();
    let ddl: String =
        sqlx::query_scalar("SELECT format('CREATE ROLE %I LOGIN PASSWORD %L',$1::text,$2::text)")
            .bind(&role)
            .bind(&password)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .is_ok());
    f.data
        .store
        .migrate_with_application_role(Some(&role))
        .await
        .unwrap();
    let database = pool
        .connect_options()
        .as_ref()
        .clone()
        .username(&role)
        .password(&password)
        .to_url_lossy();
    let origin = &f.data.runtime.configuration.endpoint;
    let address: std::net::SocketAddr = origin.strip_prefix("http://").unwrap().parse().unwrap();
    let targets =
        serde_json::json!([{ "origin": origin, "addresses": [address.to_string()] }]).to_string();
    let launch = || {
        tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
            .args(["worker", "--development-http", "--parallelism", "1"])
            .arg("--state-dir")
            .arg(state)
            .env_clear()
            .env("DATABASE_URL", database.as_str())
            .env("RUNTIME_TARGETS", &targets)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap()
    };
    let mut first = launch();
    queried(&harness).await;
    assert!(first.try_wait().unwrap().is_none());
    assert_eq!(harness.counts().submits, 1);
    let original: (uuid::Uuid, String, i64) = sqlx::query_as(
        "SELECT id,external_job_id,owner_epoch FROM app.run_attempts WHERE run_id=$1",
    )
    .bind(run.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let original_spec = harness.received_spec(&original.1);
    first.kill().await.unwrap();
    assert_eq!(first.wait().await.unwrap().signal(), Some(9));
    tokio::time::timeout(
        Duration::from_secs(75),
        sqlx::query("SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM lease_expires_at-clock_timestamp()))+0.02) FROM app.run_attempts WHERE id=$1")
            .bind(original.0).execute(&pool),
    ).await.expect("the killed Worker's actual database lease must expire").unwrap();
    harness.hold_results(false);
    let mut recovered = launch();
    assert_eq!(terminal(&pool, run.id).await, "SUCCEEDED");
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs")
                .fetch_one(&pool)
                .await
                .unwrap();
            if queued == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("recovered Worker must acknowledge only after publication");
    recovered.kill().await.unwrap();
    assert_eq!(recovered.wait().await.unwrap().signal(), Some(9));
    let current: (uuid::Uuid, String, i64) = sqlx::query_as(
        "SELECT id,external_job_id,owner_epoch FROM app.run_attempts WHERE run_id=$1",
    )
    .bind(run.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(current.0, original.0);
    assert_eq!(current.1, original.1);
    assert!(current.2 > original.2);
    assert_eq!(harness.received_spec(&original.1), original_spec);
    let counts = harness.counts();
    assert_eq!(counts.submits, 1);
    assert_eq!(counts.uploads, 1);
    assert_eq!(counts.output_reads, 1);
    assert_eq!(counts.cancels, 0);
    let facts: (i64, i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM pgmq.a_runs)")
        .bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 2, 0, 1));
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {role}")))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
        .execute(&pool)
        .await
        .unwrap();
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

async fn assert_native_failure(pool: PgPool, behavior: Behavior, code: &str, manifest_count: i64) {
    let harness = native::setup(&pool, behavior).await;
    let f = &harness.fixture;
    let run = tasks::start(f, "strict-native-output", &f.request)
        .await
        .unwrap()
        .resource;
    let driver = Driver::start(harness.worker.clone());
    assert_eq!(terminal(&pool, run.id).await, "FAILED");
    driver.finish().await;
    let reason: (String, String) =
        sqlx::query_as("SELECT error_class,error_code FROM app.run_attempts WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason, ("INVALID_INPUT".to_owned(), code.to_owned()));
    let facts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_native_outputs o JOIN app.run_attempts a ON a.id=o.attempt_id WHERE a.run_id=$1),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1),(SELECT count(*) FROM app.qualifications),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM pgmq.a_runs)",
    ).bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, manifest_count, 0, 0, 1));
    assert_eq!(harness.counts().submits, 1);
    assert_eq!(harness.counts().result_reads, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_empty_typed_quality_report_is_not_successful_data_validation(pool: PgPool) {
    assert_native_failure(pool, Behavior::EmptyQuality, "NATIVE_OUTPUT_INVALID", 1).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_valid_quality_report_for_another_dataset_is_not_adopted(pool: PgPool) {
    assert_native_failure(pool, Behavior::ForeignQuality, "NATIVE_OUTPUT_INVALID", 1).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn remote_quality_cannot_expand_the_original_selection(pool: PgPool) {
    assert_native_failure(pool, Behavior::ChangedSelection, "NATIVE_OUTPUT_INVALID", 1).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn terminal_status_with_invalid_manifest_closes_as_invalid_input_without_fabricated_file(
    pool: PgPool,
) {
    assert_native_failure(
        pool,
        Behavior::InvalidManifest,
        "NATIVE_MANIFEST_INVALID",
        0,
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn terminal_status_with_oversized_manifest_is_not_retried_forever(pool: PgPool) {
    assert_native_failure(
        pool,
        Behavior::OversizedManifest,
        "NATIVE_MANIFEST_LIMIT",
        0,
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn unavailable_manifest_remains_unsettled_instead_of_becoming_invalid_input(pool: PgPool) {
    let harness = native::setup(&pool, Behavior::ResultUnavailable).await;
    let f = &harness.fixture;
    let run = tasks::start(f, "unavailable-manifest", &f.request)
        .await
        .unwrap()
        .resource;
    let message = tasks::message(f, run.id).await;
    let (_stop, receiver) = watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        harness
            .worker
            .process_message(message, "unavailable-result-owner", receiver),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(WorkerFailure::Runtime)));
    assert!(!f
        .data
        .store
        .get_run(&f.data.actor, run.id)
        .await
        .unwrap()
        .state
        .is_terminal());
    let facts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM pgmq.a_runs)",
    ).bind(run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0, 1, 0));
    assert_eq!(harness.counts().submits, 1);
    assert_eq!(harness.counts().result_reads, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn worker_deadline_commits_database_cancellation_before_its_native_rpc(pool: PgPool) {
    let harness = native::setup(&pool, Behavior::MissingUntilCancelled).await;
    let f = &harness.fixture;
    let mut request = f.request.clone();
    request.limits.wall_seconds = 2;
    request.limits.cpu_seconds = contracts::DbCounter::new(1).unwrap();
    let run = tasks::start(f, "native-deadline-intent", &request)
        .await
        .unwrap()
        .resource;
    let driver = Driver::start(harness.worker.clone());
    assert_eq!(terminal(&pool, run.id).await, "CANCELLED");
    driver.finish().await;
    // The actual native TCP handler independently asserted the database intent
    // and event were already committed when its cancel request arrived.
    assert_eq!(harness.counts().submits, 1);
    assert_eq!(harness.counts().cancels, 1);
    let due: bool = sqlx::query_scalar(
        "SELECT cancellation_requested_at>=deadline_at FROM app.runs WHERE id=$1",
    )
    .bind(run.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(due);
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
