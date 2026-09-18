//! A real, quiescent control/Runtime checkpoint; not full T40 or account/market acceptance.
//! PostgreSQL, the native job image, sudo/tar and the built Worker CLI are mandatory.
#[path = "support/archive.rs"]
mod archive;
#[path = "../../job/tests/support/market.rs"]
mod market;
#[path = "../../server/tests/support/postgres.rs"]
mod postgres;
#[path = "support/oci.rs"]
mod support;
#[path = "../../../crates/store/tests/support/native_tasks.rs"]
mod tasks;

use contracts::{
    catalogs::RuntimeCatalogMetadataV1,
    data::DataValidateRequest,
    lifecycle::JobLimitsV1,
    research::{DataOrigin, DataPartition, InputItemV1, InputPurpose, InputSetCreate, PitStatus},
    runtime::{RuntimeCapabilitiesV1, RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    runtime_jobs::{
        JobSpecV1, ResultManifestV1, RuntimeInputV1, RuntimeJobState, RuntimeResultState,
    },
    settings::RuntimeUpdate,
    Id, SchemaV1,
};
use futures_util::FutureExt;
use integrations::{
    artifacts::ArtifactStore, authentication::random_capability, secrets::SecretVault,
};
use sqlx::{ConnectOptions, PgPool};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use store::{
    authority::Actor, lifecycle::native::NativeObjectPublication, runtime::ProbePreparation,
    Store,
};
use support::{count, Fixture};

struct Worker {
    child: Child,
    log: PathBuf,
}
impl Worker {
    fn start(binary: &Path, database: &str, state: &Path, origin: &str, log: PathBuf) -> Self {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&log)
            .unwrap();
        let address: std::net::SocketAddr =
            origin.strip_prefix("http://").unwrap().parse().unwrap();
        let targets = serde_json::json!([{ "origin": origin, "addresses": [address.to_string()] }]);
        let child = Command::new(binary)
            .args(["worker", "--development-http", "--parallelism", "1"])
            .arg("--state-dir")
            .arg(state)
            .env_clear()
            .env("DATABASE_URL", database)
            .env("RUNTIME_TARGETS", targets.to_string())
            .env("RUST_LOG", "warn")
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .spawn()
            .expect("built native Worker CLI must start");
        Self { child, log }
    }

    fn alive(&mut self) {
        assert!(self.child.try_wait().unwrap().is_none(), "the native Worker exited early; private diagnostics retained only in its owned fixture");
    }

    async fn deferred(&mut self) {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                self.alive();
                let log = fs::read_to_string(&self.log).unwrap();
                assert!(!log.contains(support::SECRET));
                if log.contains("worker task deferred for native reconciliation") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("missing local parameters must reach a real deferred Worker result");
    }

    async fn stop(&mut self) {
        self.alive();
        let sent = tokio::process::Command::new("kill")
            .env_clear()
            .args(["-TERM", &self.child.id().to_string()])
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        assert!(sent.status.success() && sent.stdout.is_empty() && sent.stderr.is_empty());
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "native Worker must exit normally on SIGTERM"
                    );
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("owned Worker shutdown deadline");
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

async fn expire(pool: &PgPool, attempt: Id) {
    tokio::time::timeout(
        Duration::from_secs(75),
        sqlx::query("SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM lease_expires_at-clock_timestamp()))+0.02) FROM app.run_attempts WHERE id=$1")
            .bind(attempt.as_uuid()).execute(pool),
    ).await.expect("the original database lease must expire without row edits").unwrap();
    let expired: bool = sqlx::query_scalar(
        "SELECT lease_expires_at<=clock_timestamp() FROM app.run_attempts WHERE id=$1",
    )
    .bind(attempt.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(expired);
}

async fn facts(pool: &PgPool, run: Id, message: i64) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM app.run_native_outputs o JOIN app.run_native_attempts n ON n.attempt_id=o.attempt_id WHERE n.run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2)")
        .bind(run.as_uuid()).bind(message).fetch_one(pool).await.unwrap()
}

async fn registration(pool: &PgPool, dataset: Id, input: Id) -> serde_json::Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('dataset',to_jsonb(d),'grant',to_jsonb(g),'source',to_jsonb(s),'evidence',to_jsonb(e),'input',to_jsonb(i)) FROM app.dataset_revisions d JOIN app.data_use_grants g ON g.id=d.data_use_grant_id JOIN app.data_sources s ON s.id=d.source_id JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id JOIN app.input_sets i ON i.id=$2 WHERE d.id=$1"
    ).bind(dataset.as_uuid()).bind(input.as_uuid()).fetch_one(pool).await.unwrap()
}

async fn attempt(pool: &PgPool, run: Id) -> (String, i32, String, i64) {
    sqlx::query_as("SELECT id::text,attempt_no::int4,external_job_id,owner_epoch FROM app.run_attempts WHERE run_id=$1")
        .bind(run.as_uuid()).fetch_one(pool).await.unwrap()
}

async fn adopted(pool: &PgPool, run: Id, message: i64, worker: &mut Worker) {
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            worker.alive();
            let state: String = sqlx::query_scalar("SELECT state FROM app.runs WHERE id=$1")
                .bind(run.as_uuid()).fetch_one(pool).await.unwrap();
            assert!(!matches!(state.as_str(), "FAILED" | "CANCELLED"), "native recovery must adopt the genuine successful result, not synthesize a failure");
            let (_, receipts, _, queued, archived) = facts(pool, run, message).await;
            if state == "SUCCEEDED" && receipts == 1 && queued == 0 && archived == 1 { break; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("the recovered Worker must publish before acknowledging the original message");
}

async fn remote_bytes(
    runtime: &Fixture,
    spec: &JobSpecV1,
) -> (Vec<u8>, ResultManifestV1, Vec<Vec<u8>>) {
    let response = runtime
        .client
        .get(runtime.url(&["jobs", &spec.external_job_id, "result"]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let raw = response.bytes().await.unwrap().to_vec();
    let manifest: ResultManifestV1 = serde_json::from_slice(&raw).unwrap();
    assert_eq!(manifest.state, RuntimeResultState::Succeeded);
    assert!(!manifest.artifacts.is_empty());
    let mut outputs = Vec::new();
    for artifact in &manifest.artifacts {
        let response = runtime
            .client
            .get(runtime.url(&[
                "jobs",
                &spec.external_job_id,
                "artifacts",
                &artifact.storage_ref.to_string(),
            ]))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let bytes = response.bytes().await.unwrap().to_vec();
        assert_eq!(bytes.len() as u64, artifact.byte_count.get());
        outputs.push(bytes);
    }
    (raw, manifest, outputs)
}

async fn verify_publication(
    pool: &PgPool,
    objects: &ArtifactStore,
    run: Id,
    owner: Id,
    raw: &[u8],
    manifest: &ResultManifestV1,
    outputs: &[Vec<u8>],
) {
    let mapped: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT o.remote_storage_ref::text,o.artifact_id::text,a.byte_count FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 ORDER BY o.remote_storage_ref"
    ).bind(owner.as_uuid()).fetch_all(pool).await.unwrap();
    assert_eq!(mapped.len(), manifest.artifacts.len());
    for (descriptor, bytes) in manifest.artifacts.iter().zip(outputs) {
        let matches: Vec<_> = mapped
            .iter()
            .filter(|row| row.0 == descriptor.storage_ref.to_string())
            .collect();
        assert_eq!(matches.len(), 1);
        let row = matches[0];
        assert_eq!(row.2 as u64, descriptor.byte_count.get());
        assert!(
            objects
                .read(Id::try_from(row.1.clone()).unwrap(), descriptor.byte_count)
                .unwrap()
                == *bytes
        );
    }
    let stored: (String, i64) = sqlx::query_as(
        "SELECT id::text,byte_count FROM app.artifacts WHERE producer_run_id=$1 AND producer_attempt_id=$2 AND schema_name='qz.job_result'"
    ).bind(run.as_uuid()).bind(owner.as_uuid()).fetch_one(pool).await.unwrap();
    assert!(
        objects
            .read(Id::try_from(stored.0).unwrap(), count(stored.1 as u64))
            .unwrap()
            == raw
    );
    let origins: Vec<String> =
        sqlx::query_scalar("SELECT origin FROM app.artifacts WHERE producer_run_id=$1")
            .bind(run.as_uuid())
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(origins.len(), outputs.len() + 1);
    assert!(origins.iter().all(|origin| origin == "FIXTURE"));
}

async fn restore_directory(root: &Path, backup: &Path, label: &str) -> PathBuf {
    let archive = backup.join(format!("{label}.tar"));
    let retained = backup.join(format!("retained-{label}"));
    let before = fs::metadata(root).unwrap();
    archive::tar("joint-create", "--create", &archive, root).await;
    archive::tar("joint-compare-source", "--compare", &archive, root).await;
    fs::rename(root, &retained).unwrap();
    fs::create_dir(root).unwrap();
    fs::set_permissions(root, fs::Permissions::from_mode(before.mode() & 0o777)).unwrap();
    archive::tar("joint-extract", "--extract", &archive, root).await;
    assert_ne!(fs::metadata(root).unwrap().ino(), before.ino());
    archive::tar("joint-compare-restored", "--compare", &archive, root).await;
    archive::tar("joint-compare-retained", "--compare", &archive, &retained).await;
    retained
}

async fn checkpoint(
    pool: &PgPool,
    restored_name: &str,
    role: &str,
    database_attempted: &mut bool,
    role_attempted: &mut bool,
) {
    let binary = PathBuf::from(
        std::env::var_os("QUAZONAI_NATIVE_SERVER_BIN")
            .expect("explicit built native server binary is required"),
    );
    assert!(binary.is_absolute() && binary.is_file());
    let backup = tempfile::tempdir().unwrap();
    let key_root = tempfile::tempdir().unwrap();
    let key = key_root.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();

    // Nautilus bridges its synchronous Parquet API with block_in_place.
    // Keep that blocking work off the SQLx test's current-thread executor.
    let (catalog, metadata) = tokio::task::spawn_blocking(|| {
        let (catalog, request) = market::market("0", 8);
        fs::set_permissions(catalog.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let mut selection = request.selection;
        selection.bar_types.truncate(1);
        let observed = job::catalog::load_catalog(catalog.path(), &selection).unwrap();
        assert_eq!(observed.rows, 8);
        let [series] = observed.series.as_slice() else {
            panic!("one controlled native instrument expected");
        };
        let mut metadata = tasks::data::catalog_fixture::metadata();
        metadata.event_start =
            chrono::DateTime::from_timestamp_nanos(selection.event_start_ns.get() as i64);
        metadata.event_end =
            chrono::DateTime::from_timestamp_nanos(selection.event_end_ns.get() as i64);
        metadata.available_through =
            chrono::DateTime::from_timestamp_nanos(selection.decision_cutoff_ns.get() as i64);
        metadata.row_count = count(observed.rows as u64);
        metadata.universe.coverage_end = metadata.event_end;
        metadata.universe.instrument_definitions =
            vec![serde_json::to_value(&series.instrument).unwrap()];
        metadata.quality.checked_at = runtime::now();
        let quality = &mut metadata.quality.datasets[0];
        quality.selection = selection;
        quality.row_count = metadata.row_count;
        quality.first_event_ns = count(series.bars.first().unwrap().ts_event.as_u64());
        quality.last_event_ns = count(series.bars.last().unwrap().ts_event.as_u64());
        quality.available_through_ns = count(series.bars.last().unwrap().ts_init.as_u64());
        domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
        (catalog, metadata)
    })
    .await
    .expect("native catalog preparation must finish on the blocking pool");

    let mut remote = Fixture::open().await;
    remote.crash();
    let metadata_path = remote.directory.path().join("catalog.json");
    fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&remote.config_path).unwrap()).unwrap();
    config["catalogs"] =
        serde_json::json!([{ "root": catalog.path(), "metadata_file": metadata_path }]);
    fs::write(&remote.config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    remote.restart().await;
    let origin = format!("http://{}", remote.config.bind);
    let response = remote
        .client
        .get(remote.url(&["catalogs", &metadata.registered_ref, "metadata"]))
        .query(&[("storage_version", &metadata.storage_version)])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let observed_metadata = response.bytes().await.unwrap().to_vec();
    let actual: RuntimeCatalogMetadataV1 = serde_json::from_slice(&observed_metadata).unwrap();
    assert_eq!(actual.row_count, count(8));
    assert_eq!(actual.origin, DataOrigin::Fixture);
    assert_eq!(actual.pit_status, PitStatus::Unverified);

    let mut data = tasks::data::setup(pool, None).await;
    let state = data.directory.path().to_path_buf();
    fs::rename(state.join("objects"), state.join("artifacts")).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(state.join("secrets"))
        .unwrap();
    let vault = SecretVault::open(&state.join("secrets"), &key).unwrap();
    let secret = vault.put("RUNTIME", support::SECRET.as_bytes()).unwrap();
    let mut configuration = data.runtime.configuration.clone();
    configuration.endpoint = origin.clone();
    configuration.development_http = true;
    data.runtime = data
        .store
        .update_runtime(
            &data.actor,
            "joint-native-runtime",
            data.runtime.id,
            &RuntimeUpdate {
                schema_version: SchemaV1,
                expected_revision: data.runtime.revision,
                configuration,
                credential_ref: Some(secret),
                ca_certificate_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    drop(vault);
    let dataset_request = tasks::data::request(&data);
    let ticket = tasks::data::ticket(&data, "joint-native-metadata", &dataset_request).await;
    let dataset = tasks::data::complete(&data, ticket, observed_metadata.clone())
        .await
        .unwrap()
        .resource;
    let input = data
        .store
        .create_input_set(
            &data.actor,
            "joint-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: data.project,
                purpose: InputPurpose::Discovery,
                decision_cutoff: metadata.available_through,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset.id,
                    role: DataPartition::Discovery,
                }],
            },
        )
        .await
        .unwrap()
        .resource;
    let capabilities: RuntimeCapabilitiesV1 = remote
        .json(
            reqwest::Method::GET,
            &["capabilities"],
            None,
            &[reqwest::StatusCode::OK],
        )
        .await;
    tasks::probe(&data, capabilities.clone()).await;
    let f = tasks::Fixture {
        request: DataValidateRequest {
            schema_version: SchemaV1,
            project_id: data.project,
            input_set_id: input.header.id,
            runtime_id: data.runtime.id,
            expected_runtime_revision: data.runtime.revision,
            limits: JobLimitsV1 {
                schema_version: SchemaV1,
                experiments: 0,
                cpu_seconds: count(30),
                wall_seconds: 120,
                memory_mib: 512,
                output_bytes: count(65_536),
            },
        },
        data,
        dataset,
        capabilities,
    };
    let run = tasks::start(&f, "joint-original-run", &f.request)
        .await
        .unwrap()
        .resource;
    remote.runs.push(run.id);
    let message = tasks::message(&f, run.id).await;
    let lease = tasks::lease(&f, &message, "checkpoint-original-owner", 10).await;
    let job = f.data.store.native_job(run.id, &lease.fence).await.unwrap();
    for input in &job.spec.inputs {
        if let RuntimeInputV1::Artifact {
            artifact_id,
            byte_count,
            storage_version,
            ..
        } = input
        {
            assert_eq!(storage_version, "1");
            remote
                .object(
                    *artifact_id,
                    &f.data.objects.read(*artifact_id, *byte_count).unwrap(),
                )
                .await;
        }
    }
    assert!(f
        .data
        .store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    remote.submit(&job.spec).await;
    let status = remote.terminal(&job.spec).await;
    assert_eq!(status.state, RuntimeJobState::Succeeded);
    let (manifest_raw, manifest, output_bytes) = remote_bytes(&remote, &job.spec).await;
    let native = remote.native_container(&job.spec).await;
    assert_eq!(native.state.as_ref().unwrap().running, Some(false));
    let original_attempt = attempt(pool, run.id).await;
    let original_spec = serde_json::to_value(&job.spec).unwrap();
    let original_run = f.data.store.get_run(&f.data.actor, run.id).await.unwrap();
    let original_registration = registration(pool, f.dataset.id, f.request.input_set_id).await;
    assert!(!original_run.state.is_terminal());
    assert_eq!(
        facts(pool, run.id, message.message_id).await,
        (1, 0, 0, 1, 0)
    );
    let dispatch: String =
        sqlx::query_scalar("SELECT dispatch_state FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(dispatch, "SENT_UNKNOWN");

    // No API/Worker writes exist; the native task is terminal before its sole
    // Runtime writer is stopped. The database and all three directories form one checkpoint.
    let old_pid = remote.child.as_ref().unwrap().id();
    remote.crash();
    assert!(
        fs::metadata(remote.config.state_dir.join("journal.sqlite-wal"))
            .unwrap()
            .len()
            > 32
    );
    let dumped = tokio::time::timeout(
        Duration::from_secs(30),
        postgres::postgres_tool(pool, "pg_dump")
            .args(["--format=custom", "--no-owner", "--no-privileges"])
            .output(),
    )
    .await
    .expect("native database dump deadline")
    .unwrap();
    assert!(
        dumped.status.success() && dumped.stderr.is_empty(),
        "native checkpoint dump failed"
    );
    assert!(dumped.stdout.starts_with(b"PGDMP"));
    assert!(
        !state.join("master.key").exists(),
        "master key must remain outside the data archive"
    );
    let retained_state = restore_directory(&state, backup.path(), "control").await;
    let retained_remote =
        restore_directory(remote.directory.path(), backup.path(), "runtime").await;
    let retained_catalog = restore_directory(catalog.path(), backup.path(), "catalog").await;

    *database_attempted = true;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE {restored_name} TEMPLATE template0"
    )))
    .execute(pool)
    .await
    .unwrap();
    let restored = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect_with(
            pool.connect_options()
                .as_ref()
                .clone()
                .database(restored_name),
        )
        .await
        .unwrap();
    let mut restore = postgres::postgres_tool(&restored, "pg_restore")
        .args([
            "--dbname",
            "",
            "--single-transaction",
            "--exit-on-error",
            "--no-owner",
            "--no-privileges",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = restore.stdin.take().unwrap();
    use tokio::io::AsyncWriteExt;
    let (written, restored_output) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(
            async {
                stdin.write_all(&dumped.stdout).await?;
                stdin.shutdown().await
            },
            restore.wait_with_output()
        )
    })
    .await
    .expect("native database restore deadline");
    let restored_output = restored_output.unwrap();
    assert!(
        written.is_ok() && restored_output.status.success() && restored_output.stderr.is_empty(),
        "native checkpoint database restore failed"
    );
    let restored_store = Store::from_pool(restored.clone());
    assert_eq!(attempt(&restored, run.id).await, original_attempt);
    assert_eq!(
        facts(&restored, run.id, message.message_id).await,
        (1, 0, 0, 1, 0)
    );
    assert!(
        registration(&restored, f.dataset.id, f.request.input_set_id).await
            == original_registration
    );
    let checkpoint_auth = restored_store.authentication_snapshot().await.unwrap();
    let owner_url = restored.connect_options().to_url_lossy();
    let cutover = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(&binary)
            .args(["recover-access", "--recovery-id", &Id::new().to_string()])
            .env_clear()
            .env("DATABASE_URL", owner_url.as_str())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("native restored-access cutover deadline")
    .unwrap();
    assert!(
        cutover.status.success() && cutover.stderr.is_empty(),
        "native recovery access cutover failed"
    );
    let current_auth = restored_store.authentication_snapshot().await.unwrap();
    assert!(current_auth.epoch > checkpoint_auth.epoch);
    assert_eq!(current_auth.secret_ref, checkpoint_auth.secret_ref);
    let Actor::Browser { login_id } = &f.data.actor else {
        panic!("controlled operator fixture expected");
    };
    assert!(restored_store.browser_authority(*login_id).await.is_err());

    let password = random_capability();
    let ddl: String =
        sqlx::query_scalar("SELECT format('CREATE ROLE %I LOGIN PASSWORD %L',$1::text,$2::text)")
            .bind(role)
            .bind(&password)
            .fetch_one(pool)
            .await
            .unwrap();
    *role_attempted = true;
    assert!(sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(pool)
        .await
        .is_ok());
    restored_store
        .migrate_with_application_role(Some(role))
        .await
        .unwrap();
    let application = restored
        .connect_options()
        .as_ref()
        .clone()
        .username(role)
        .password(&password)
        .to_url_lossy();
    fs::copy(&key, state.join("master.key")).unwrap();
    fs::set_permissions(state.join("master.key"), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(fs::read(&key).unwrap() == fs::read(state.join("master.key")).unwrap());

    remote.restart().await;
    assert_ne!(remote.child.as_ref().unwrap().id(), old_pid);
    assert_eq!(remote.status(&job.spec).await, status);
    let (reopened_raw, reopened_manifest, reopened_bytes) = remote_bytes(&remote, &job.spec).await;
    assert!(reopened_raw == manifest_raw && reopened_bytes == output_bytes);
    assert!(
        serde_json::to_value(reopened_manifest).unwrap()
            == serde_json::to_value(&manifest).unwrap()
    );
    let response = remote
        .client
        .get(remote.url(&["catalogs", &metadata.registered_ref, "metadata"]))
        .query(&[("storage_version", &metadata.storage_version)])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.bytes().await.unwrap().as_ref() == observed_metadata.as_slice());

    // Withhold, never destroy or regenerate, an original local publication input.
    let parameter = state
        .join("artifacts")
        .join(job.spec.parameters_artifact_id.to_string());
    let retained_parameter = backup.path().join("withheld-parameters");
    fs::rename(&parameter, &retained_parameter).unwrap();
    expire(&restored, lease.fence.attempt_id).await;
    let mut missing = Worker::start(
        &binary,
        application.as_str(),
        &state,
        &origin,
        backup.path().join("missing.log"),
    );
    missing.deferred().await;
    assert_eq!(
        facts(&restored, run.id, message.message_id).await,
        (1, 0, 0, 1, 0)
    );
    let deferred_owner = attempt(&restored, run.id).await;
    assert_eq!(
        (&deferred_owner.0, deferred_owner.1, &deferred_owner.2),
        (&original_attempt.0, original_attempt.1, &original_attempt.2)
    );
    assert!(deferred_owner.3 > original_attempt.3);
    missing.stop().await;
    fs::rename(&retained_parameter, &parameter).unwrap();
    expire(&restored, lease.fence.attempt_id).await;
    let started = Instant::now();
    let mut worker = Worker::start(
        &binary,
        application.as_str(),
        &state,
        &origin,
        backup.path().join("recovered.log"),
    );
    adopted(&restored, run.id, message.message_id, &mut worker).await;
    worker.stop().await;
    let adoption_elapsed_ms = started.elapsed().as_millis();
    let current = attempt(&restored, run.id).await;
    assert_eq!(
        (&current.0, current.1, &current.2),
        (&original_attempt.0, original_attempt.1, &original_attempt.2)
    );
    assert!(current.3 > deferred_owner.3);
    let frozen: serde_json::Value =
        sqlx::query_scalar("SELECT spec_json FROM app.run_native_attempts WHERE attempt_id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&restored)
            .await
            .unwrap();
    assert!(frozen == original_spec);
    assert_eq!(
        facts(&restored, run.id, message.message_id).await,
        (1, 1, manifest.artifacts.len() as i64, 0, 1)
    );
    let objects = Arc::new(ArtifactStore::open(&state.join("artifacts")).unwrap());
    verify_publication(
        &restored,
        &objects,
        run.id,
        lease.fence.attempt_id,
        &manifest_raw,
        &manifest,
        &output_bytes,
    )
    .await;
    let same = remote.native_container(&job.spec).await;
    assert_eq!(same.id, native.id);
    assert_eq!(same.restart_count, native.restart_count);
    assert_eq!(same.state.as_ref().unwrap().running, Some(false));
    assert_eq!(
        same.state.as_ref().unwrap().started_at,
        native.state.as_ref().unwrap().started_at
    );
    assert_eq!(
        same.state.as_ref().unwrap().finished_at,
        native.state.as_ref().unwrap().finished_at
    );
    assert_eq!(
        facts(pool, run.id, message.message_id).await,
        (1, 0, 0, 1, 0)
    );
    assert_eq!(attempt(pool, run.id).await, original_attempt);
    assert!(
        serde_json::to_value(f.data.store.get_run(&f.data.actor, run.id).await.unwrap()).unwrap()
            == serde_json::to_value(original_run).unwrap()
    );
    for (label, retained) in [
        ("control", &retained_state),
        ("runtime", &retained_remote),
        ("catalog", &retained_catalog),
    ] {
        archive::tar(
            "joint-retained-after-adoption",
            "--compare",
            &backup.path().join(format!("{label}.tar")),
            retained,
        )
        .await;
    }

    // Use the existing trusted Store authentication fixture's verified-step input,
    // not another TOTP enrollment or a claim of actual account authentication.
    let snapshot = restored_store.authentication_snapshot().await.unwrap();
    let login = restored_store
        .login_with_verified_step(
            &snapshot,
            snapshot.database_now.timestamp() / 30 + 1,
            false,
            None,
        )
        .await
        .unwrap();
    let actor = Actor::Browser { login_id: login.id };
    // Historical adoption above uses the original frozen identity. A new task
    // still needs a fresh native capability observation after the recovery delay.
    let ticket = match restored_store
        .prepare_runtime_probe(
            &actor,
            "joint-post-restore-probe",
            f.request.runtime_id,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: f.request.expected_runtime_revision,
            },
        )
        .await
        .unwrap()
    {
        ProbePreparation::Pending(ticket) => *ticket,
        ProbePreparation::Replay(_) => panic!("new post-restore probe unexpectedly replayed"),
    };
    let capabilities: RuntimeCapabilitiesV1 = remote
        .json(
            reqwest::Method::GET,
            &["capabilities"],
            None,
            &[reqwest::StatusCode::OK],
        )
        .await;
    let publishing = objects.clone();
    restored_store
        .complete_runtime_probe(
            ticket,
            RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            move |id, bytes| tasks::publish_one(publishing, NativeObjectPublication { id, bytes }),
        )
        .await
        .unwrap();
    let reader = objects.clone();
    let writer = objects.clone();
    let next = restored_store
        .start_data_validation(
            &actor,
            "joint-new-restored-input-read",
            &f.request,
            move |id, size| tasks::data::read(reader.clone(), id, size),
            move |object| tasks::publish_one(writer, object),
        )
        .await
        .unwrap()
        .resource;
    assert_ne!(next.id, run.id);
    remote.runs.push(next.id);
    let next_message = restored_store
        .read_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == next.id)
        .expect("new genuine command must queue");
    let mut next_worker = Worker::start(
        &binary,
        application.as_str(),
        &state,
        &origin,
        backup.path().join("next.log"),
    );
    adopted(
        &restored,
        next.id,
        next_message.message_id,
        &mut next_worker,
    )
    .await;
    next_worker.stop().await;
    let document: serde_json::Value =
        sqlx::query_scalar("SELECT spec_json FROM app.run_native_attempts WHERE run_id=$1")
            .bind(next.id.as_uuid())
            .fetch_one(&restored)
            .await
            .unwrap();
    let next_spec: JobSpecV1 = serde_json::from_value(document).unwrap();
    let (next_raw, next_manifest, next_outputs) = remote_bytes(&remote, &next_spec).await;
    let next_attempt = attempt(&restored, next.id).await;
    verify_publication(
        &restored,
        &objects,
        next.id,
        Id::try_from(next_attempt.0).unwrap(),
        &next_raw,
        &next_manifest,
        &next_outputs,
    )
    .await;
    let next_container = remote.native_container(&next_spec).await;
    assert_ne!(next_container.id, native.id);
    assert_eq!(remote.status(&job.spec).await, status);
    assert!(
        registration(&restored, f.dataset.id, f.request.input_set_id).await
            == original_registration
    );
    assert!(
        registration(pool, f.dataset.id, f.request.input_set_id).await == original_registration
    );
    remote.assert_private_logs();
    for entry in ["missing.log", "recovered.log", "next.log"] {
        let log = fs::read_to_string(backup.path().join(entry)).unwrap();
        assert!(!log.contains(support::SECRET) && !log.contains(&password));
    }
    println!(
        "{}",
        serde_json::json!({
            "scope": "quiescent native control/Runtime checkpoint; FIXTURE data, no market or account acceptance",
            "original_attempt_preserved": true, "owner_epoch_advanced": true,
            "missing_local_input_prevented_ack": true,
            "original_output_count": output_bytes.len(), "unique_original_terminal_receipt": true,
            "new_native_validation_used_restored_catalog": true,
            "adoption_elapsed_ms": adoption_elapsed_ms,
            "network_request_count_observed": false
        })
    );
    restored.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn restored_control_and_runtime_adopt_the_original_native_result(pool: PgPool) {
    let suffix = Id::new().to_string().replace('-', "");
    let database = format!("joint_restore_{suffix}");
    let role = format!("joint_app_{suffix}");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1) OR EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$2)"
    ).bind(&database).bind(&role).fetch_one(&pool).await.unwrap();
    assert!(!exists, "refuse to touch a pre-existing database or role");
    let mut database_attempted = false;
    let mut role_attempted = false;
    let outcome = AssertUnwindSafe(tokio::time::timeout(
        Duration::from_secs(300),
        checkpoint(
            &pool,
            &database,
            &role,
            &mut database_attempted,
            &mut role_attempted,
        ),
    ))
    .catch_unwind()
    .await;
    // All owned child guards have dropped before removing only these fresh names.
    let database_clean = !database_attempted
        || sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {database} WITH (FORCE)"
        )))
        .execute(&pool)
        .await
        .is_ok();
    let role_clean = !role_attempted
        || sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE IF EXISTS {role}")))
            .execute(&pool)
            .await
            .is_ok();
    assert!(
        database_clean && role_clean,
        "owned native recovery resources failed cleanup"
    );
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(_)) => panic!("native joint-checkpoint acceptance deadline"),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
