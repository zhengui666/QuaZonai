//! Native non-account scientific acceptance. This first stage proves that a Store-frozen
//! experiment compilation is executed by the real Runtime/OCI through the production Worker.
//! Forecast/validation and same-Thread feedback are added in this PR before delivery.
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "support/oci.rs"]
mod support;

use contracts::{
    execution::NativeModelCompilationV1,
    runtime::{RuntimeCapabilitiesV1, RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    settings::RuntimeUpdate,
    Id, SchemaV1,
};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use server::{
    runtime_transport::{RuntimeTarget, RuntimeTargets},
    worker::Worker,
};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt};
use store::{lifecycle::ClaimResult, runtime::ProbePreparation};
use support::{Fixture as RuntimeFixture, SECRET};

async fn actual_probe(
    remote: &RuntimeFixture,
    store: &store::Store,
    actor: &store::authority::Actor,
    data: &cycle_support::Fixture,
) {
    let capabilities: RuntimeCapabilitiesV1 = remote
        .json(
            reqwest::Method::GET,
            &["capabilities"],
            None,
            &[reqwest::StatusCode::OK],
        )
        .await;
    let ProbePreparation::Pending(ticket) = store
        .prepare_runtime_probe(
            actor,
            &Id::new().to_string(),
            data.data.runtime,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: data.freeze.execution_context.runtime_revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("fresh native Runtime observation required");
    };
    let objects = data.objects.clone();
    store
        .complete_runtime_probe(
            *ticket,
            RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            move |id, bytes| async move {
                objects
                    .put(id, &bytes)
                    .map_err(|_| store::StoreError::Integrity)
            },
        )
        .await
        .unwrap();
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
    let vault = SecretVault::open(&private.path().join("secrets"), &key).unwrap();
    let credential = vault.put("RUNTIME", SECRET.as_bytes()).unwrap();

    let (store, actor) = research_support::operator(&pool).await;
    let mut data = cycle_support::setup(&pool, &store, &actor).await;
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

    let (store, actor, data, cycle, preparation) =
        mission_support::start(store, actor, data, false).await;
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
    actual_probe(&remote, &store, &actor, &data).await;
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
    let objects_path = data
        .directory
        .as_ref()
        .expect("owned cycle artifact directory")
        .path()
        .join("objects");
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: remote.origin.as_str().trim_end_matches('/').to_owned(),
            addresses: vec![remote.config.bind],
        }],
        true,
    )
    .unwrap();
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
    let container = remote.native_container(
        &store
            .native_job(
                compilation.id,
                &store
                    .claim_native_run(
                        &store
                            .read_native_run_messages(1, 100)
                            .await
                            .unwrap()
                            .into_iter()
                            .find(|m| m.run_id == compilation.id)
                            .unwrap_or_else(|| panic!("terminal queue message already archived")),
                        "post-terminal-inspection",
                        1,
                    )
                    .await
                    .ok()
                    .and_then(|claim| match claim {
                        Some(ClaimResult::Leased(lease)) => Some(lease.fence.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("terminal native job is intentionally archived")),
            )
            .await
            .unwrap()
            .spec,
    )
    .await;
    assert_eq!(container.state.as_ref().unwrap().running, Some(false));

    let (report_id, report_bytes): (uuid::Uuid, i64) = sqlx::query_as(
        "SELECT a.id,a.byte_count FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id JOIN app.run_attempts t ON t.id=o.attempt_id WHERE t.run_id=$1 AND a.schema_name='qz.model_compilation'",
    )
    .bind(compilation.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let report = data
        .objects
        .read(
            Id::try_from(report_id.to_string()).unwrap(),
            contracts::DbCounter::new(report_bytes as u64).unwrap(),
        )
        .unwrap();
    let report: NativeModelCompilationV1 = serde_json::from_slice(&report).unwrap();
    assert!(!report.rustc_version.contains("controlled observation"));
    assert!(report.module_bytes.get() > 8);
    remote.assert_private_logs();
}
