//! Native compilation and connected scientific feedback through the production Worker.
//! The standalone compilation case retains controlled preparation; the connected case
//! uses real catalog-backed computation and only a controlled account-waived provider.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
#[path = "support/oci.rs"]
mod support;

#[path = "../../job/tests/support/market_catalog.rs"]
mod market;
#[path = "../../server/tests/support/codex_responses.rs"]
#[allow(dead_code)] // Shared official Responses fixture also serves other protocol regressions.
mod responses;
#[path = "support/scientific_catalog.rs"]
mod scientific_catalog;
#[path = "support/scientific_feedback.rs"]
mod scientific_feedback;

use contracts::{
    execution::NativeModelCompilationV1,
    research::ArtifactInputRole,
    runtime::{RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    runtime_jobs::{JobSpecV1, ResultManifestV1, RuntimeInputV1},
    settings::RuntimeUpdate,
    Id, Revision, SchemaV1,
};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use server::{
    runtime_transport::{RuntimeTarget, RuntimeTargets, RuntimeTransport},
    worker::Worker,
};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, sync::Arc};
use store::{lifecycle::ClaimResult, runtime::ProbePreparation};
use support::{count, Fixture as RuntimeFixture, SECRET};

async fn configured_transport(
    snapshot: &store::lifecycle::RuntimeSnapshot,
    vault: Arc<SecretVault>,
    targets: &RuntimeTargets,
) -> RuntimeTransport {
    let snapshot = snapshot.clone();
    let targets = targets.clone();
    tokio::task::spawn_blocking(move || {
        let id = Id::try_from(snapshot.credential_ref.clone()).unwrap();
        let credential = vault.read(id, "RUNTIME").unwrap();
        assert!(snapshot.ca_certificate_ref.is_none());
        RuntimeTransport::new(&targets, &snapshot, &credential, None).unwrap()
    })
    .await
    .unwrap()
}

async fn actual_probe(
    store: &store::Store,
    actor: &store::authority::Actor,
    configuration: (Id, Revision),
    objects: Arc<ArtifactStore>,
    vault: Arc<SecretVault>,
    targets: &RuntimeTargets,
) {
    // Store binds identity before network I/O; credentials and endpoint are
    // resolved from that exact snapshot, never a pre-authenticated fixture client.
    let ProbePreparation::Pending(ticket) = store
        .prepare_runtime_probe(
            actor,
            &Id::new().to_string(),
            configuration.0,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: configuration.1,
            },
        )
        .await
        .unwrap()
    else {
        panic!("fresh native Runtime observation required");
    };
    let native = configured_transport(&ticket.snapshot, vault, targets).await;
    let capabilities = native.capabilities().await.unwrap();
    assert!(capabilities
        .image_refs
        .iter()
        .all(|image| image.image_ref == support::image()));
    store
        .complete_runtime_probe(
            *ticket,
            RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            move |id, bytes| async move {
                tokio::task::spawn_blocking(move || objects.put(id, &bytes))
                    .await
                    .map_err(|_| store::StoreError::Integrity)?
                    .map_err(|_| store::StoreError::Integrity)
            },
        )
        .await
        .unwrap();
}

async fn published(
    pool: &PgPool,
    objects: &ArtifactStore,
    run: Id,
    attempt: Id,
    schema: &str,
) -> (Id, Vec<u8>) {
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT a.id::text,a.byte_count,o.remote_storage_ref::text FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.producer_attempt_id=$1 AND a.producer_run_id=$2 AND a.schema_name=$3",
    )
    .bind(attempt.as_uuid())
    .bind(run.as_uuid())
    .bind(schema)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "one original output per schema and Attempt");
    let (artifact, size, remote) = rows.into_iter().next().unwrap();
    (
        Id::try_from(remote).unwrap(),
        objects
            .read(Id::try_from(artifact).unwrap(), count(size as u64))
            .unwrap(),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn experiment_compilation_runs_in_real_runtime_through_production_worker(pool: PgPool) {
    let mut remote = RuntimeFixture::open().await;
    let private = tempfile::tempdir().unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(private.path().join("secrets"))
        .unwrap();
    let key = private.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let vault = Arc::new(SecretVault::open(&private.path().join("secrets"), &key).unwrap());
    let credential = vault.put("RUNTIME", SECRET.as_bytes()).unwrap();
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: remote.origin.as_str().trim_end_matches('/').to_owned(),
            addresses: vec![remote.config.bind],
        }],
        true,
    )
    .unwrap();

    let (store, actor) = research_support::operator(&pool).await;
    // Create the assumptions with the built image before freezing anything.
    // The fixture's native gateway already verified that it advertises this image.
    let mut data =
        cycle_support::setup_with_native_image(&pool, &store, &actor, &support::image()).await;
    let current = store.runtime(&actor, data.data.runtime).await.unwrap();
    let mut configuration = current.configuration;
    configuration.endpoint = remote.origin.as_str().trim_end_matches('/').to_owned();
    configuration.development_http = true;
    let updated = store
        .update_runtime(
            &actor,
            "native-science-runtime",
            data.data.runtime,
            &RuntimeUpdate {
                schema_version: SchemaV1,
                expected_revision: current.revision,
                configuration,
                credential_ref: Some(credential),
                ca_certificate_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    data.freeze.execution_context.runtime_revision = updated.revision;
    actual_probe(
        &store,
        &actor,
        (
            data.data.runtime,
            data.freeze.execution_context.runtime_revision,
        ),
        data.objects.clone(),
        vault.clone(),
        &targets,
    )
    .await;

    let (store, actor, data, cycle, preparation) =
        mission_support::start(store, actor, data, false).await;
    // Relational/data-quality preparation only. It is not counted as native science.
    mission_support::complete(&pool, &store, &data, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let mission_message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(parent)) = store
        .claim_mission(&mission_message, "native-science-parent", 120)
        .await
        .unwrap()
    else {
        panic!("Mission lease required");
    };

    let experiment = experiment_support::propose(&pool, &store, &actor, &data, cycle).await;
    // The shared proposal helper includes a controlled probe. Replace it with
    // the real configured transport's observation before native admission.
    actual_probe(
        &store,
        &actor,
        (
            data.data.runtime,
            data.freeze.execution_context.runtime_revision,
        ),
        data.objects.clone(),
        vault,
        &targets,
    )
    .await;
    let compilation = experiment_support::start(&store, &data, &parent, experiment)
        .await
        .unwrap()
        .resource;
    remote.runs.push(compilation.id);

    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == compilation.id)
        .expect("compilation must queue one native Run");
    let message_id = message.message_id;
    let objects_path = data
        .directory
        .as_ref()
        .expect("owned cycle artifact directory")
        .path()
        .join("objects");
    let worker = Worker::new(
        store.clone(),
        SecretVault::open(&private.path().join("secrets"), &key).unwrap(),
        ArtifactStore::open(&objects_path).unwrap(),
        targets,
        1,
    )
    .unwrap();
    let (_stop, shutdown) = tokio::sync::watch::channel(false);
    worker
        .process_message(message, "native-science-compile-worker", shutdown)
        .await
        .unwrap();

    let finished = store.get_run(&actor, compilation.id).await.unwrap();
    assert_eq!(finished.state, contracts::runs::RunState::Succeeded);
    let attempt = finished.active_attempt_id.unwrap();
    let spec_json: serde_json::Value = sqlx::query_scalar(
        "SELECT spec_json FROM app.run_native_attempts WHERE run_id=$1 AND attempt_id=$2",
    )
    .bind(compilation.id.as_uuid())
    .bind(attempt.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let spec: JobSpecV1 = serde_json::from_value(spec_json).unwrap();
    assert_eq!(spec.run_id, compilation.id);
    assert_eq!(spec.image_ref, support::image());
    let container = remote.native_container(&spec).await;
    assert_eq!(container.state.as_ref().unwrap().running, Some(false));

    let (report_ref, report_bytes) = published(
        &pool,
        &data.objects,
        compilation.id,
        attempt,
        "qz.model_compilation",
    )
    .await;
    let report: NativeModelCompilationV1 = serde_json::from_slice(&report_bytes).unwrap();
    let (model_ref, model_bytes) = published(
        &pool,
        &data.objects,
        compilation.id,
        attempt,
        "qz.wasm_model",
    )
    .await;
    let code = spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                role: ArtifactInputRole::Code,
                ..
            } => Some(*artifact_id),
            _ => None,
        })
        .unwrap();
    assert_eq!(report.code_artifact_id, code);
    assert_eq!(report.model_storage_ref, model_ref);
    assert_eq!(report.module_bytes.get(), model_bytes.len() as u64);
    assert_eq!(report.target, "wasm32-unknown-unknown");
    // Execute the accepted module with the production native interpreter.
    // The submitted helper source computes close - previous_close. A header-only
    // or unrelated hard-coded module cannot satisfy these varied observations.
    let mut signal = job::signals::WasmSignal::new(&model_bytes, 4, 1_000_000).unwrap();
    for (close, previous, expected) in [
        (3.0, 1.0, 2.0),
        (1.0, 3.0, -2.0),
        (1.5, 1.25, 0.25),
        (7.0, 7.0, 0.0),
    ] {
        let actual = signal
            .predict([close, previous, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0])
            .unwrap();
        assert!((actual - expected).abs() < f64::EPSILON);
    }

    let response = remote
        .client
        .get(remote.url(&["jobs", &spec.external_job_id, "result"]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let raw = response.bytes().await.unwrap();
    let manifest: ResultManifestV1 = serde_json::from_slice(&raw).unwrap();
    assert_eq!(manifest.run_id, compilation.id);
    assert_eq!(manifest.attempt_no, spec.attempt_no);
    assert_eq!(manifest.artifacts.len(), 2);
    for (reference, bytes) in [(report_ref, &report_bytes), (model_ref, &model_bytes)] {
        assert!(manifest
            .artifacts
            .iter()
            .any(|a| a.storage_ref == reference && a.byte_count.get() == bytes.len() as u64));
    }
    let (receipt, size): (String, i64) = sqlx::query_as(
        "SELECT id::text,byte_count FROM app.artifacts WHERE producer_run_id=$1 AND producer_attempt_id=$2 AND schema_name='qz.job_result'",
    )
    .bind(compilation.id.as_uuid()).bind(attempt.as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(
        data.objects
            .read(Id::try_from(receipt).unwrap(), count(size as u64))
            .unwrap()
            == raw
    );
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2)",
    )
    .bind(compilation.id.as_uuid()).bind(message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 0, 1));
    remote.assert_private_logs();
    println!("native compilation: original source/Attempt/model, four Wasmi observations and unique receipt/ACK verified; not full T08");
}
