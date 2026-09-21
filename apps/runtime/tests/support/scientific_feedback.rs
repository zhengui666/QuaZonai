//! Connected, account-waived native research and independent-review paths.
//! Only Provider decisions are scripted; scientific bytes come from the real OCI jobs.
use super::{actual_probe, mission_support, published, responses, scientific_catalog, support};
use contracts::{
    artifacts::ResearchArtifactKind,
    catalogs::RuntimeCatalogMetadataV1,
    codex::{CodexConnectionCreateV1, CodexProfileCreateV1},
    control::ListQuery,
    cycles::CodexProfileChoiceV1,
    evidence::Decision,
    execution::NativeTaskParametersV1,
    experiments::{ExperimentProposalV1, ExperimentSource},
    research::{ArtifactInputRole, DataPartition},
    runs::{RunSnapshotV1, RunState},
    runtime_jobs::{JobSpecV1, ResultManifestV1, RuntimeInputV1},
    science::{
        NativeAlphaSealedRequestV1, NativeAlphaSealedResultV1, NativeAlphaValidationResultV1,
        NativeForecastResultV1, NativeFrozenCalibrationV1,
    },
    Id, SchemaV1,
};
use integrations::{
    artifacts::ArtifactStore, authentication::random_capability, secrets::SecretVault,
};
use nautilus_model::instruments::Instrument;
use server::{
    codex_profiles::{CodexDeployment, CodexDeploymentBinding, CodexDeploymentConfig},
    worker::{mission::MissionLauncher, Worker},
    AppState, WebPolicy,
};
use sqlx::PgPool;
use std::{collections::BTreeMap, os::unix::fs::DirBuilderExt, sync::Arc, time::Duration};
use store::{authority::Actor, lifecycle::RunMessage, Store};
use tokio::{net::TcpListener, task::JoinHandle};

struct Api(JoinHandle<()>);
impl Drop for Api {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn run_spec(pool: &PgPool, run: &RunSnapshotV1) -> JobSpecV1 {
    let raw: serde_json::Value = sqlx::query_scalar(
        "SELECT spec_json FROM app.run_native_attempts WHERE run_id=$1 AND attempt_id=$2",
    )
    .bind(run.id.as_uuid())
    .bind(run.active_attempt_id.unwrap().as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let spec: JobSpecV1 = serde_json::from_value(raw).unwrap();
    assert_eq!(spec.run_id, run.id);
    assert_eq!(spec.image_ref, support::image());
    spec
}

async fn execute(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    remote: &mut support::Fixture,
    execution: (&Worker, &ArtifactStore),
    run: Id,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> (RunSnapshotV1, JobSpecV1) {
    let (worker, objects) = execution;
    remote.runs.push(run);
    let message = store
        .read_native_run_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == run)
        .expect("the original scientific Run must really queue");
    let message_id = message.message_id;
    tokio::time::timeout(
        Duration::from_secs(90),
        worker.process_message(message.clone(), "connected-native-science", shutdown),
    )
    .await
    .expect("bounded native science execution")
    .unwrap();
    let finished = store.get_run(actor, run).await.unwrap();
    assert_eq!(finished.state, RunState::Succeeded);
    let spec = run_spec(pool, &finished).await;
    let container = remote.native_container(&spec).await;
    assert_eq!(container.state.as_ref().unwrap().running, Some(false));
    let response = remote
        .client
        .get(remote.url(&["jobs", &spec.external_job_id, "result"]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let raw = response.bytes().await.unwrap();
    let manifest: ResultManifestV1 = serde_json::from_slice(&raw).unwrap();
    assert_eq!(manifest.run_id, run);
    assert_eq!(manifest.attempt_no, spec.attempt_no);
    let local:(String,i64)=sqlx::query_as(
        "SELECT id::text,byte_count FROM app.artifacts WHERE producer_run_id=$1 AND producer_attempt_id=$2 AND schema_name='qz.job_result'",
    ).bind(run.as_uuid()).bind(finished.active_attempt_id.unwrap().as_uuid()).fetch_one(pool).await.unwrap();
    let facts:(i64,i64,i64,i64)=sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2)",
    ).bind(run.as_uuid()).bind(message_id).fetch_one(pool).await.unwrap();
    assert_eq!(facts, (1, 1, 0, 1));
    // Read the exact native manifest publication using the actual Worker artifact path.
    assert!(
        objects
            .read(
                Id::try_from(local.0).unwrap(),
                support::count(local.1 as u64)
            )
            .unwrap()
            == raw
    );
    store.acknowledge_run(&message).await.unwrap();
    (finished, spec)
}

async fn mission_tick(
    worker: &Worker,
    message: &RunMessage,
    shutdown: &tokio::sync::watch::Receiver<bool>,
    stage: &'static str,
) {
    println!("native science Mission stage={stage}: begin");
    let owner = format!("connected-native-mission/{}", message.read_count);
    tokio::time::timeout(
        Duration::from_secs(110),
        worker.process_mission_message(message.clone(), &owner, shutdown.clone()),
    )
    .await
    .unwrap_or_else(|_| panic!("native Mission stage={stage}: deadline"))
    .unwrap_or_else(|error| panic!("native Mission stage={stage}: {error:?}"));
    println!("native science Mission stage={stage}: passed");
}

async fn scientific_run(pool: &PgPool, experiment: Id, stage: &str) -> Id {
    let sql = match stage {
        "compile" => {
            "SELECT compile_run_id::text FROM app.experiment_compilations WHERE experiment_id=$1"
        }
        "forecast" => "SELECT run_id::text FROM app.experiment_forecasts WHERE experiment_id=$1",
        "validation" => {
            "SELECT run_id::text FROM app.experiment_validations WHERE experiment_id=$1"
        }
        _ => panic!("fixed scientific stage expected"),
    };
    Id::try_from(
        sqlx::query_scalar::<_, String>(sqlx::SqlStr::from_static(sql))
            .bind(experiment.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap(),
    )
    .unwrap()
}

// Read the exact immutable task named by its JobSpec, never a replacement request.
fn task_parameters(objects: &ArtifactStore, spec: &JobSpecV1) -> NativeTaskParametersV1 {
    let parameters: Vec<_> = spec
        .inputs
        .iter()
        .filter_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                storage_version,
                byte_count,
                role: ArtifactInputRole::Parameters,
            } => Some((*artifact_id, *byte_count, storage_version.as_str())),
            _ => None,
        })
        .collect();
    let [(id, size, version)] = parameters.as_slice() else {
        panic!("exactly one native task parameter input required");
    };
    assert_eq!(*id, spec.parameters_artifact_id);
    assert_eq!(*version, "1");
    serde_json::from_slice(&objects.read(*id, *size).unwrap()).unwrap()
}

// Original native registration metadata, independent of the Sealed task under test.
// These test-owned reads are never handed to the Reviewer model.
async fn registered_metadata(
    store: &Store,
    actor: &Actor,
    remote: &support::Fixture,
    dataset: Id,
) -> RuntimeCatalogMetadataV1 {
    let revision = store.get_dataset_revision(actor, dataset).await.unwrap();
    let source = store
        .get_data_source(actor, revision.source_id)
        .await
        .unwrap();
    let response = remote
        .client
        .get(remote.url(&["catalogs", &source.native_catalog_ref, "metadata"]))
        .query(&[("storage_version", &revision.storage_version)])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let metadata: RuntimeCatalogMetadataV1 = response.json().await.unwrap();
    assert_eq!(metadata.registered_ref, source.native_catalog_ref);
    assert_eq!(metadata.native_snapshot_ref, revision.native_snapshot_ref);
    assert_eq!(metadata.storage_version, revision.storage_version);
    assert_eq!(metadata.partition, revision.partition);
    assert_eq!(metadata.origin, revision.origin);
    assert_eq!(metadata.pit_status, revision.pit_status);
    assert_eq!(metadata.row_count, revision.row_count);
    assert_eq!(metadata.event_start, revision.event_start);
    assert_eq!(metadata.event_end, revision.event_end);
    assert_eq!(metadata.available_through, revision.available_through);
    assert_eq!(metadata.quality.datasets.len(), 1);
    metadata
}

// For the existing authored linear-price catalog, h-step return is h*step/close.
// This causal positive control changes code only, never measured results or policy.
const RECIPROCAL: &str = r#"#![no_std]
#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
#[no_mangle] pub extern "C" fn predict(c:f64,_p:f64,_f:f64,_s:f64,_v:f64,_o:f64,_h:f64,_l:f64)->f64 { 1.0/c }
"#;

async fn scenario(pool: PgPool, independent: Option<Decision>) {
    let mut remote = support::Fixture::open().await;
    let scientific_catalog::Prepared {
        store,
        actor,
        mut data,
        vault,
        targets,
        catalog,
    } = Box::pin(scientific_catalog::prepare(&pool, &mut remote)).await;
    let root = data.directory.as_ref().unwrap().path().to_path_buf();
    let home = root.join("native");
    let provider = responses::Provider::start(&home).await;
    let profile = store
        .codex_profile(&actor, data.researcher_profile.profile_id)
        .await
        .unwrap();
    let mut bindings = vec![CodexDeploymentBinding {
        reference: profile.home_binding.clone().unwrap(),
        label: "Native scientific researcher fixture".into(),
        profile_origin: profile.profile_origin,
        home: home.clone(),
        codex_home: home.clone(),
        working_directory: home.clone(),
        environment_names: vec![],
    }];
    if independent.is_some() {
        let reviewer_home = root.join("native-reviewer");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&reviewer_home)
            .unwrap();
        // Share only this fixture's credential-free Provider configuration.
        // No auth, sessions, Threads or research history enter the separate home.
        std::fs::copy(home.join("config.toml"), reviewer_home.join("config.toml")).unwrap();
        let reviewer = store
            .create_codex_profile(
                &actor,
                "native-independent-profile",
                &CodexProfileCreateV1 {
                    schema_version: SchemaV1,
                    name: "Native account-waived independent Reviewer".into(),
                    home_binding: format!("native-reviewer-{}", Id::new()),
                    profile_origin: profile.profile_origin,
                    connection: CodexConnectionCreateV1::System {},
                    model_settings: profile.model_settings.clone(),
                },
                |binding| async move {
                    domain::codex::settings::home_binding(&binding.home_binding)?;
                    Ok(())
                },
            )
            .await
            .unwrap()
            .resource;
        assert_ne!(reviewer.id, profile.id);
        assert_ne!(reviewer.home_binding, profile.home_binding);
        assert_ne!(reviewer_home, home);
        data.reviewer_profile = CodexProfileChoiceV1 {
            profile_id: reviewer.id,
            expected_revision: reviewer.revision,
        };
        bindings.push(CodexDeploymentBinding {
            reference: reviewer.home_binding.unwrap(),
            label: "Native scientific independent Reviewer fixture".into(),
            profile_origin: reviewer.profile_origin,
            home: reviewer_home.clone(),
            codex_home: reviewer_home.clone(),
            working_directory: reviewer_home,
            environment_names: vec![],
        });
    }
    let binary = std::env::var_os("QUAZONAI_NATIVE_SERVER_BIN")
        .expect("the built native server CLI is required");
    let deployment = CodexDeployment::new(CodexDeploymentConfig {
        schema_version: SchemaV1,
        binary: std::env::var_os("CODEX_NATIVE_BIN")
            .expect("the pinned official Codex binary is required")
            .into(),
        executable_path: std::env::var("PATH").unwrap(),
        bindings,
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://localhost:{}", address.port());
    let cookie_material = format!("{}{}", random_capability(), random_capability());
    let app = server::router(
        AppState::new(
            store.clone(),
            SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
            WebPolicy::new(&origin, address, true).unwrap(),
        )
        .with_artifact_store(ArtifactStore::open(&root.join("objects")).unwrap())
        .with_runtime_targets(targets.clone()),
        cookie_material
            .as_bytes()
            .try_into()
            .expect("native cookie key from private random bytes"),
    );
    let _api = Api(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let launcher = Arc::new(
        MissionLauncher::new(
            deployment,
            root.join("workspaces"),
            binary.into(),
            origin,
            true,
        )
        .unwrap(),
    );
    let worker = Worker::new(
        store.clone(),
        SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
        ArtifactStore::open(&root.join("objects")).unwrap(),
        targets.clone(),
        1,
    )
    .unwrap()
    .with_missions(launcher);
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
    let (store, actor, data, cycle, preparation) =
        mission_support::start(store, actor, data, false).await;
    let (_stop, shutdown) = tokio::sync::watch::channel(false);
    // Initial DATA_VALIDATE is real too; no complete_* scientific fixtures.
    execute(
        &pool,
        &store,
        &actor,
        &mut remote,
        (&worker, &data.objects),
        preparation,
        shutdown.clone(),
    )
    .await;
    let mission = store
        .read_mission_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id != preparation)
        .expect("real data-quality completion must admit a Mission");
    assert_eq!(
        store
            .get_run(&actor, mission.run_id)
            .await
            .unwrap()
            .cycle_id,
        Some(cycle)
    );

    let reciprocal = independent.is_some();
    let signal = if reciprocal {
        RECIPROCAL
    } else {
        scientific_catalog::SOURCE
    };
    let policy = store
        .evaluation_policy(&actor, data.brief.content.evaluation_policy_id)
        .await
        .unwrap();
    let code = scientific_catalog::upload(
        &store,
        &actor,
        &data.objects,
        data.data.project,
        ResearchArtifactKind::Code,
        signal.into(),
    )
    .await;
    let parameters=scientific_catalog::upload(&store,&actor,&data.objects,data.data.project,ResearchArtifactKind::Parameters,
        serde_json::json!({"schema_version":1,"dataset_revision_id":data.data.discovery,
            "parameters":{"schema_version":1,"fast_period":2,"slow_period":5,"label_horizon_observations":5,"total_fuel":"1000000"}}).to_string()).await;
    let proposal_report=scientific_catalog::upload(&store,&actor,&data.objects,data.data.project,ResearchArtifactKind::Report,
        serde_json::json!({"schema_version":1,"hypothesis":if reciprocal {
            "Causal reciprocal-close positive control on the authored linear-price fixture, not market evidence."
        } else {
            "Observe the exact close-minus-previous source on the native fixture; no market claim."
        }}).to_string()).await;
    provider.submit_experiment(ExperimentProposalV1 {
        schema_version: SchemaV1,
        cycle_id: cycle,
        family_id: policy.selection_rule.family_id,
        parent_experiment_id: None,
        hypothesis: "Measure the uploaded source on the actual native catalog".into(),
        expected_failure_modes:
            "Constant-signal or insufficient calibration may be inconclusive; never assume PASS"
                .into(),
        proposal_artifact_id: proposal_report,
        parameter_artifact_id: parameters,
        code_artifact_id: Some(code),
    });
    mission_tick(&worker, &mission, &shutdown, "proposal-and-compile").await;
    assert_eq!(provider.request_count(), 3);
    let experiment = provider.proposed_experiment();
    let authored = store.experiment(&actor, experiment).await.unwrap();
    assert_eq!(authored.trial_source, ExperimentSource::Codex);
    assert_eq!(authored.cycle_id, cycle);
    assert_eq!(authored.author_run_id, Some(mission.run_id));
    assert_eq!(authored.code_artifact_id, Some(code));
    let thread: String =
        sqlx::query_scalar("SELECT thread_id FROM app.codex_sessions WHERE run_id=$1")
            .bind(mission.run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let compilation = scientific_run(&pool, experiment, "compile").await;
    let (compiled, _) = execute(
        &pool,
        &store,
        &actor,
        &mut remote,
        (&worker, &data.objects),
        compilation,
        shutdown.clone(),
    )
    .await;
    let model_output = published(
        &pool,
        &data.objects,
        compiled.id,
        compiled.active_attempt_id.unwrap(),
        "qz.wasm_model",
    )
    .await;
    let model_id = model_output.artifact_id;
    let module_bytes = model_output.bytes;
    let mut model = job::signals::WasmSignal::new(&module_bytes, 2, 100_000).unwrap();
    assert_eq!(
        model
            .predict([3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap(),
        if reciprocal { 1.0 / 3.0 } else { 2.0 }
    );
    assert_eq!(
        model
            .predict([1.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap(),
        if reciprocal { 1.0 } else { -2.0 }
    );
    mission_tick(&worker, &mission, &shutdown, "forecast-admission").await;
    let forecast = scientific_run(&pool, experiment, "forecast").await;
    let (predicted, prediction_spec) = execute(
        &pool,
        &store,
        &actor,
        &mut remote,
        (&worker, &data.objects),
        forecast,
        shutdown.clone(),
    )
    .await;
    let forecast_output = published(
        &pool,
        &data.objects,
        predicted.id,
        predicted.active_attempt_id.unwrap(),
        "qz.native_forecast",
    )
    .await;
    let forecast: NativeForecastResultV1 = serde_json::from_slice(&forecast_output.bytes).unwrap();
    assert!(forecast.consumed_fuel.get() > 0);
    let task = task_parameters(&data.objects, &prediction_spec);
    let NativeTaskParametersV1::EvaluateAlpha { request, .. } = task else {
        panic!("native forecast task required");
    };
    let root = catalog.path().join("window-0");
    let oracle = tokio::task::spawn_blocking(move || {
        let observed = job::catalog::load_catalog(&root, &request.selection).unwrap();
        let mut expected = BTreeMap::new();
        for series in observed.series {
            for (index, bar) in series.bars.iter().enumerate() {
                let prediction =
                    (index + 1 >= request.parameters.slow_period as usize).then(|| {
                        if reciprocal {
                            1.0 / bar.close.as_f64()
                        } else {
                            bar.close.as_f64() - series.bars[index - 1].close.as_f64()
                        }
                    });
                expected.insert(
                    (series.instrument.id().to_string(), index as u32),
                    (bar.ts_event.as_u64(), prediction),
                );
            }
        }
        expected
    })
    .await
    .unwrap();
    assert_eq!(forecast.points.len(), oracle.len());
    for point in &forecast.points {
        let expected = &oracle[&(point.instrument_id.clone(), point.ordinal)];
        assert_eq!(point.event_ns.get(), expected.0);
        assert_eq!(point.forecast, expected.1);
    }
    mission_tick(&worker, &mission, &shutdown, "record-alpha").await;
    mission_tick(&worker, &mission, &shutdown, "validation-admission").await;
    let validation = scientific_run(&pool, experiment, "validation").await;
    let (validated, validation_spec) = execute(
        &pool,
        &store,
        &actor,
        &mut remote,
        (&worker, &data.objects),
        validation,
        shutdown.clone(),
    )
    .await;
    let validation_output = published(
        &pool,
        &data.objects,
        validated.id,
        validated.active_attempt_id.unwrap(),
        "qz.alpha_validation",
    )
    .await;
    let source_report_id = validation_output.artifact_id;
    let raw = validation_output.bytes;
    let measured: NativeAlphaValidationResultV1 = serde_json::from_slice(&raw).unwrap();
    assert!(
        measured.consumed_fuel.get() > 0
            && measured.unique_test_observations.get() > 0
            && !measured.folds.is_empty()
    );
    let native_count = measured
        .folds
        .iter()
        .flat_map(|fold| {
            fold.test_points
                .iter()
                .map(move |point| (fold.instrument_id.clone(), point.observation.ordinal))
        })
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert_eq!(measured.unique_test_observations.get(), native_count as u64);
    let (evaluation, report, decision): (String, String, String) = sqlx::query_as(
        "SELECT id::text,report_artifact_id::text,decision FROM app.evaluations WHERE run_id=$1",
    )
    .bind(validation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    if independent.is_some() {
        assert_eq!(
            decision, "PASS",
            "positive control must really pass the unchanged native policy"
        );
    }
    let source_report:String=sqlx::query_scalar(
        "SELECT a.id::text FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.schema_name='qz.alpha_validation'",
    ).bind(validated.active_attempt_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(source_report, source_report_id.to_string());
    assert_eq!(
        provider.request_count(),
        3,
        "science must run outside the native model tool loop"
    );
    mission_tick(&worker, &mission, &shutdown, "prepare-feedback").await;
    assert_eq!(provider.request_count(), 3);

    // The first native process is closed. Reopening it must be a new delivery,
    // not a second credential issuance under the original unexpired owner fence.
    let before: (String, i32, i64) = sqlx::query_as(
        "SELECT id::text,attempt_no::int4,owner_epoch FROM app.run_attempts WHERE run_id=$1",
    )
    .bind(mission.run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let pending: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.run_id=$1),(SELECT count(*) FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id WHERE p.run_id=$1)",
    )
    .bind(mission.run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending, (2, 1, 1));
    tokio::time::timeout(
        Duration::from_secs(75),
        sqlx::query(
            "SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM GREATEST(a.lease_expires_at,q.vt)-clock_timestamp()))+0.02) FROM app.run_attempts a JOIN pgmq.q_runs q ON q.msg_id=$2 WHERE a.id=$1",
        )
        .bind(Id::try_from(before.0.clone()).unwrap().as_uuid())
        .bind(mission.message_id)
        .execute(&pool),
    )
    .await
    .expect("the original lease and queue visibility must expire without row edits")
    .unwrap();
    let eligible: bool = sqlx::query_scalar(
        "SELECT a.lease_expires_at<=clock_timestamp() AND q.vt<=clock_timestamp() FROM app.run_attempts a JOIN pgmq.q_runs q ON q.msg_id=$2 WHERE a.id=$1",
    )
    .bind(Id::try_from(before.0.clone()).unwrap().as_uuid())
    .bind(mission.message_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(eligible);
    let redelivered = store
        .read_mission_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == mission.run_id)
        .expect("the same Mission must be redelivered by native PGMQ");
    assert_eq!(redelivered.message_id, mission.message_id);
    assert_eq!(redelivered.read_count, mission.read_count + 1);
    mission_tick(&worker, &redelivered, &shutdown, "consume-feedback").await;
    let after: (String, i32, i64) = sqlx::query_as(
        "SELECT id::text,attempt_no::int4,owner_epoch FROM app.run_attempts WHERE run_id=$1",
    )
    .bind(mission.run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((&after.0, after.1), (&before.0, before.1));
    assert_eq!(after.2, before.2 + 1);
    let credentials: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.machine_principals WHERE run_id=$1),(SELECT count(*) FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id WHERE p.run_id=$1)",
    )
    .bind(mission.run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(credentials, (1, 2));
    assert_eq!(provider.request_count(), 4);
    assert!(provider.saw_previous_context());
    let observation = provider.scientific_observation();
    assert_eq!(observation["run_id"], validation.to_string());
    assert_eq!(
        observation["attempt_id"],
        validated.active_attempt_id.unwrap().to_string()
    );
    assert_eq!(observation["evaluation"]["id"], evaluation);
    assert_eq!(observation["evaluation"]["report_artifact_id"], report);
    assert_eq!(observation["evaluation"]["decision"], decision);
    assert_eq!(
        observation["evaluation"]["policy_id"],
        data.brief.content.evaluation_policy_id.to_string()
    );
    assert_eq!(
        observation["evaluation"]["input_set_id"],
        data.freeze
            .execution_context
            .validation_input_set_id
            .to_string()
    );
    let selected = measured
        .folds
        .iter()
        .find(|fold| fold.fold_index == 0 && fold.instrument_id == "EUR/USD.SIM")
        .unwrap();
    let metric = selected
        .metrics
        .iter()
        .find(|metric| metric.kind == contracts::science::NativeAlphaMetricKind::PearsonIc)
        .unwrap();
    assert_eq!(
        observation["evaluation"]["selection_metric"]["value"],
        serde_json::to_value(metric.value).unwrap()
    );
    assert_eq!(
        observation["evaluation"]["selection_metric"]["status"],
        serde_json::to_value(metric.status).unwrap()
    );
    assert_eq!(
        observation["evaluation"]["selection_metric"]["source_artifact_id"],
        source_report
    );
    assert_eq!(
        observation["evaluation"]["selection_metric"]["observation_count"],
        selected.test_points.len().to_string()
    );
    assert_eq!(
        store.get_run(&actor, mission.run_id).await.unwrap().state,
        RunState::Succeeded
    );
    let facts:(i64,i64,i64,String)=sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT thread_id FROM app.codex_sessions WHERE run_id=$1)",
    ).bind(mission.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (2, 2, 1, thread.clone()));
    let receipt_counts:(i64,i64,i64)=sqlx::query_as(
        "SELECT (SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$1),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$1),(SELECT count(*) FROM app.qualifications)",
    ).bind(mission.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(receipt_counts, (0, 1, 0));
    store.acknowledge_run(&mission).await.unwrap();
    assert_eq!(provider.request_count(), 4);
    let summary:(String,i64)=sqlx::query_as(
        "SELECT a.id::text,a.byte_count FROM app.model_turn_summaries s JOIN app.model_turn_reservations r ON r.id=s.reservation_id JOIN app.artifacts a ON a.id=s.artifact_id WHERE r.run_id=$1 AND r.ordinal=2",
    ).bind(mission.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(
        &data
            .objects
            .read(
                Id::try_from(summary.0).unwrap(),
                support::count(summary.1 as u64),
            )
            .unwrap(),
    )
    .unwrap();
    let text = summary["text"].as_str().unwrap();
    assert!(text.contains(&evaluation) && text.contains(&report) && text.contains(&decision));

    if let Some(review_decision) = independent {
        // A native scientific PASS must enter the normal selection/ACK path.
        let selection = store.cycle_selection(&actor, cycle).await.unwrap();
        assert_eq!(selection.selected_count.get(), 1);
        let chosen: (String, String) = sqlx::query_as(
            "SELECT review_alpha_version_id::text,evaluation_id::text FROM app.cycle_selection_trials WHERE cycle_id=$1 AND experiment_id=$2 AND selected",
        )
        .bind(cycle.as_uuid())
        .bind(experiment.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(chosen.1, evaluation);
        let alpha = Id::try_from(chosen.0).unwrap();
        let messages = store.read_mission_messages(60, 100).await.unwrap();
        assert_eq!(
            messages.len(),
            1,
            "research ACK must admit exactly one independent Reviewer"
        );
        let reviewer = &messages[0];
        assert_ne!(reviewer.run_id, mission.run_id);
        let role: (String, String) =
            sqlx::query_as("SELECT role,profile_id::text FROM app.run_missions WHERE run_id=$1")
                .bind(reviewer.run_id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_ne!(
            data.reviewer_profile.profile_id,
            data.researcher_profile.profile_id
        );
        assert_ne!(role.1, data.researcher_profile.profile_id.to_string());
        assert_eq!(
            role,
            (
                "INDEPENDENT_REVIEWER".into(),
                data.reviewer_profile.profile_id.to_string()
            )
        );
        let original_evaluation: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(e) FROM app.evaluations e WHERE id=$1")
                .bind(Id::try_from(evaluation.clone()).unwrap().as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        let parameter_locator = store.artifact_content(&actor, parameters).await.unwrap();
        let parameter_bytes = data
            .objects
            .read(parameters, parameter_locator.metadata.byte_count)
            .unwrap();
        // Build the complete oracle from original records, not the materialized
        // Reviewer file or the projector being exercised by this scenario.
        let evaluation_view = store
            .evaluation(&actor, Id::try_from(evaluation.clone()).unwrap())
            .await
            .unwrap();
        assert_eq!(evaluation_view.run_id, validated.id);
        assert_eq!(evaluation_view.policy_id, policy.id);
        let parent_alpha: String =
            sqlx::query_scalar("SELECT alpha_id::text FROM app.alpha_versions WHERE id=$1")
                .bind(alpha.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        let versions = store
            .alpha_versions(
                &actor,
                Id::try_from(parent_alpha).unwrap(),
                &ListQuery {
                    cursor: None,
                    limit: 100,
                },
            )
            .await
            .unwrap();
        assert!(versions.next_cursor.is_none());
        let original_alpha = versions
            .items
            .iter()
            .find(|item| Some(item.id) == evaluation_view.subject_alpha_version_id)
            .unwrap();
        let selected_alpha = versions.items.iter().find(|item| item.id == alpha).unwrap();
        assert_eq!(original_alpha.experiment_id, experiment);
        assert_eq!(selected_alpha.experiment_id, experiment);
        assert_eq!(original_alpha.code_artifact_id, code);
        assert_eq!(selected_alpha.code_artifact_id, code);
        assert_eq!(original_alpha.model_artifact_id, Some(model_id));
        assert_eq!(selected_alpha.model_artifact_id, Some(model_id));
        let metric_page = store
            .evaluation_metrics(
                &actor,
                evaluation_view.id,
                &ListQuery {
                    cursor: None,
                    limit: 100,
                },
            )
            .await
            .unwrap();
        assert!(
            metric_page.next_cursor.is_none(),
            "compare every native metric"
        );
        let mut context_metrics = metric_page.items;
        assert_eq!(
            context_metrics.len(),
            measured
                .folds
                .iter()
                .map(|fold| fold.metrics.len())
                .sum::<usize>()
        );
        for value in &context_metrics {
            assert_eq!(value.evaluation_id, evaluation_view.id);
            assert_eq!(value.source_artifact_id.to_string(), source_report);
        }
        context_metrics.sort_by(|a, b| {
            (&a.metric_code, &a.scope, &a.method_id).cmp(&(&b.metric_code, &b.scope, &b.method_id))
        });
        let review_policy = store
            .evaluation_policy(&actor, selection.policy_id)
            .await
            .unwrap();
        let context = serde_json::json!({
            "schema_version":1,"experiment_id":experiment,"alpha_version_id":alpha,
            "original_alpha_version_id":original_alpha.id,
            "cycle_id":cycle,"source_cycle_id":authored.cycle_id,
            "hypothesis":authored.hypothesis,
            "expected_failure_modes":authored.expected_failure_modes,
            "validation_evaluation_id":evaluation_view.id,
            "policy_id":evaluation_view.policy_id,"input_set_id":evaluation_view.input_set_id,
            "execution_status":evaluation_view.execution_status,
            "evidence_status":evaluation_view.evidence_status,"decision":evaluation_view.decision,
            "origin":evaluation_view.origin,
            "signal_kind":selected_alpha.signal_kind,"horizon_kind":selected_alpha.horizon_kind,
            "horizon_value":selected_alpha.horizon_value,"forecast_unit":selected_alpha.forecast_unit,
            "runtime_image_ref":selected_alpha.runtime_image_ref,
            "validation_policy":policy,"review_policy":review_policy,
            "valid_until":evaluation_view.valid_until,"metrics":context_metrics,
            "qualification":"NOT_GRANTED"
        });
        provider.review_science(responses::scientific_review::OriginalScience {
            experiment,
            alpha,
            code,
            parameters,
            source: signal.into(),
            parameter_document: serde_json::from_slice(&parameter_bytes).unwrap(),
            context,
            decision: review_decision,
        });
        mission_tick(
            &worker,
            reviewer,
            &shutdown,
            "independent-review-original-science",
        )
        .await;
        let reviewed_requests = provider.request_count();
        assert!((6..=12).contains(&reviewed_requests));
        let reviewed: (String, String, String, String) = sqlx::query_as(
            "SELECT s.thread_id,r.decision,t.alpha_version_id::text,r.summary_artifact_id::text FROM app.mission_reviews r JOIN app.mission_review_turns t ON t.reservation_id=r.reservation_id JOIN app.codex_sessions s ON s.run_id=t.run_id WHERE t.run_id=$1",
        )
        .bind(reviewer.run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(reviewed.0, thread);
        assert_eq!(serde_json::to_value(review_decision).unwrap(), reviewed.1);
        assert_eq!(reviewed.2, alpha.to_string());
        let read_context = provider.reviewed_science();
        let review_directory = data
            .directory
            .as_ref()
            .unwrap()
            .path()
            .join("workspaces")
            .join(reviewer.run_id.to_string())
            .join(format!("review-{experiment}"));
        assert_eq!(
            std::fs::read(review_directory.join(code.to_string())).unwrap(),
            signal.as_bytes()
        );
        assert_eq!(
            std::fs::read(review_directory.join(parameters.to_string())).unwrap(),
            parameter_bytes
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(review_directory.join(alpha.to_string())).unwrap()
            )
            .unwrap(),
            read_context
        );
        let summary_id = Id::try_from(reviewed.3).unwrap();
        // Reviewer summaries retain their existing EVALUATOR_ONLY access. The
        // test reads the exact private publication, not a new browser permission.
        assert!(matches!(
            store.artifact_content(&actor, summary_id).await,
            Err(store::StoreError::NotFound)
        ));
        let summary_bytes: i64 = sqlx::query_scalar(
            "SELECT a.byte_count FROM app.artifacts a JOIN app.model_turn_summaries s ON s.artifact_id=a.id JOIN app.model_turn_reservations r ON r.id=s.reservation_id WHERE a.id=$1 AND r.run_id=$2 AND a.producer_run_id=$2 AND a.producer_attempt_id=r.attempt_id AND a.schema_name='qz.mission_summary' AND a.schema_version='1' AND a.access_class='EVALUATOR_ONLY' AND a.storage_backend='LOCAL' AND a.storage_version='1' AND a.storage_object_ref=a.id::text",
        )
        .bind(summary_id.as_uuid())
        .bind(reviewer.run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        let summary: serde_json::Value = serde_json::from_slice(
            &data
                .objects
                .read(
                    summary_id,
                    support::count(u64::try_from(summary_bytes).unwrap()),
                )
                .unwrap(),
        )
        .unwrap();
        let answer: serde_json::Value =
            serde_json::from_str(summary["text"].as_str().unwrap()).unwrap();
        assert_eq!(answer["alpha_version_id"], alpha.to_string());
        assert_eq!(
            answer["decision"],
            serde_json::to_value(review_decision).unwrap()
        );
        let mut scopes: Vec<String> = sqlx::query_scalar(
            "SELECT c.scope_codes FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id WHERE p.run_id=$1 ORDER BY c.issued_at DESC LIMIT 1",
        )
        .bind(reviewer.run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        scopes.sort_unstable();
        assert_eq!(scopes, ["EVIDENCE_READ", "RESEARCH_READ", "RUN_READ"]);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.qualifications")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );

        if review_decision == Decision::Pass {
            assert!(!store
                .get_run(&actor, reviewer.run_id)
                .await
                .unwrap()
                .state
                .is_terminal());
            mission_tick(&worker, reviewer, &shutdown, "admit-original-sealed").await;
            let held: (String, String, String) = sqlx::query_as(
                "SELECT task.run_id::text,task.alpha_version_id::text,task.validation_evaluation_id::text FROM app.mission_sealed_evaluations held JOIN app.mission_review_turns review ON review.reservation_id=held.review_reservation_id JOIN app.sealed_evaluation_tasks task ON task.run_id=held.run_id WHERE review.run_id=$1",
            )
            .bind(reviewer.run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(held.1, alpha.to_string());
            assert_eq!(held.2, evaluation);
            let sealed = Id::try_from(held.0).unwrap();
            let (finished, sealed_spec) = execute(
                &pool,
                &store,
                &actor,
                &mut remote,
                (&worker, &data.objects),
                sealed,
                shutdown.clone(),
            )
            .await;
            assert_eq!(
                finished.input_set_id,
                data.freeze.execution_context.sealed_input_set_id
            );
            assert!(sealed_spec.inputs.iter().any(|input| matches!(input,
                contracts::runtime_jobs::RuntimeInputV1::Dataset {
                    revision_id, role: contracts::research::DataPartition::Sealed, ..
                } if *revision_id == data.data.sealed)));
            // Compare against original compilation/Validation and registration,
            // not against claims made by the Sealed output itself.
            let NativeTaskParametersV1::ValidateAlpha {
                dataset_revision_id,
                model_artifact_id,
                request: original_validation,
                ..
            } = task_parameters(&data.objects, &validation_spec)
            else {
                panic!("original Validation task required");
            };
            assert_eq!(dataset_revision_id, data.data.validation);
            assert_eq!(model_artifact_id, model_id);
            let source_parameters: serde_json::Value =
                serde_json::from_slice(&parameter_bytes).unwrap();
            assert_eq!(
                serde_json::to_value(&original_validation.forecast.parameters).unwrap(),
                source_parameters["parameters"]
            );
            let calibration = store.alpha_calibration(&actor, alpha).await.unwrap();
            assert_eq!(selected_alpha.calibration_id, Some(calibration.id));
            assert_eq!(calibration.validation.id, evaluation_view.id);
            assert_eq!(calibration.train_input_set_id, evaluation_view.input_set_id);
            // Calibration is a local projection of the original Validation,
            // not a remote Runtime output. Read its actual recorded identity.
            let calibration_id = calibration.model_artifact_id;
            assert!(matches!(
                store.artifact_content(&actor, calibration_id).await,
                Err(store::StoreError::NotFound)
            ));
            let calibration_size: i64 = sqlx::query_scalar(
                "SELECT a.byte_count FROM app.artifacts a JOIN app.calibrations c ON c.model_artifact_id=a.id WHERE c.id=$1 AND c.validation_evaluation_id=$2 AND a.id=$3 AND a.producer_run_id=$4 AND a.producer_attempt_id=$5 AND a.kind='MODEL' AND a.schema_name='qz.alpha_calibration' AND a.schema_version='1' AND a.access_class='EVALUATOR_ONLY' AND a.origin='FIXTURE' AND a.storage_backend='LOCAL' AND a.storage_version='1' AND a.storage_object_ref=a.id::text AND NOT EXISTS(SELECT 1 FROM app.run_native_outputs o WHERE o.artifact_id=a.id)",
            )
            .bind(calibration.id.as_uuid())
            .bind(evaluation_view.id.as_uuid())
            .bind(calibration_id.as_uuid())
            .bind(validated.id.as_uuid())
            .bind(validated.active_attempt_id.unwrap().as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
            let calibration_bytes = data
                .objects
                .read(
                    calibration_id,
                    support::count(u64::try_from(calibration_size).unwrap()),
                )
                .unwrap();
            let calibration_document: NativeFrozenCalibrationV1 =
                serde_json::from_slice(&calibration_bytes).unwrap();
            assert_eq!(
                calibration_document.source_report_artifact_id.to_string(),
                source_report
            );
            assert_eq!(
                calibration_document.horizon_observations.get(),
                u64::from(
                    original_validation
                        .forecast
                        .parameters
                        .label_horizon_observations
                )
            );
            let discovery_metadata =
                registered_metadata(&store, &actor, &remote, data.data.discovery).await;
            let validation_metadata =
                registered_metadata(&store, &actor, &remote, data.data.validation).await;
            let sealed_metadata =
                registered_metadata(&store, &actor, &remote, data.data.sealed).await;
            assert_eq!(sealed_metadata.partition, DataPartition::Sealed);
            let original_input = store
                .input_set(&actor, data.freeze.execution_context.sealed_input_set_id)
                .await
                .unwrap();
            let cutoff = u64::try_from(
                original_input
                    .header
                    .decision_cutoff
                    .timestamp_nanos_opt()
                    .unwrap(),
            )
            .unwrap();
            let mut forecast = original_validation.forecast;
            forecast.selection = sealed_metadata.quality.datasets[0].selection.clone();
            forecast.selection.decision_cutoff_ns =
                support::count(cutoff.min(forecast.selection.decision_cutoff_ns.get()));
            let expected_task = NativeTaskParametersV1::EvaluateSealedAlpha {
                schema_version: SchemaV1,
                dataset_revision_id: data.data.sealed,
                model_artifact_id: model_id,
                calibration_artifact_id: Some(calibration_id),
                request: Box::new(NativeAlphaSealedRequestV1 {
                    schema_version: SchemaV1,
                    forecast,
                    target_kind: original_validation.target_kind,
                    research_available_through_ns: discovery_metadata.quality.datasets[0]
                        .available_through_ns
                        .max(validation_metadata.quality.datasets[0].available_through_ns),
                }),
            };
            assert_eq!(
                serde_json::to_value(task_parameters(&data.objects, &sealed_spec)).unwrap(),
                serde_json::to_value(expected_task).unwrap()
            );
            let parameter_size: i64 = sqlx::query_scalar(
                "SELECT byte_count FROM app.artifacts WHERE id=$1 AND schema_name='qz.native_task' AND schema_version='1' AND access_class='EVALUATOR_ONLY'",
            )
            .bind(sealed_spec.parameters_artifact_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
            let expected_inputs = vec![
                RuntimeInputV1::Dataset {
                    revision_id: data.data.sealed,
                    registered_ref: sealed_metadata.registered_ref,
                    storage_version: sealed_metadata.storage_version,
                    role: DataPartition::Sealed,
                },
                RuntimeInputV1::Artifact {
                    artifact_id: model_id,
                    storage_version: "1".into(),
                    byte_count: support::count(module_bytes.len() as u64),
                    role: ArtifactInputRole::Model,
                },
                RuntimeInputV1::Artifact {
                    artifact_id: calibration_id,
                    storage_version: "1".into(),
                    byte_count: support::count(calibration_bytes.len() as u64),
                    role: ArtifactInputRole::Model,
                },
                RuntimeInputV1::Artifact {
                    artifact_id: sealed_spec.parameters_artifact_id,
                    storage_version: "1".into(),
                    byte_count: support::count(u64::try_from(parameter_size).unwrap()),
                    role: ArtifactInputRole::Parameters,
                },
            ];
            assert_eq!(sealed_spec.inputs, expected_inputs);
            assert_eq!(sealed_spec.image_ref, selected_alpha.runtime_image_ref);
            let sealed_output = published(
                &pool,
                &data.objects,
                sealed,
                finished.active_attempt_id.unwrap(),
                "qz.alpha_sealed",
            )
            .await;
            let sealed_result: NativeAlphaSealedResultV1 =
                serde_json::from_slice(&sealed_output.bytes).unwrap();
            assert!(sealed_result.forecast.consumed_fuel.get() > 0);
            assert!(!sealed_result.assets.is_empty());
            assert_eq!(
                sealed_result.calibration_source_report_artifact_id,
                Some(Id::try_from(source_report.clone()).unwrap())
            );
            // Sealed evaluations are private; provenance belongs to their original
            // report Artifact, not a column or public projection of evaluations.
            let sealed_evaluation: (String, String, String, String) = sqlx::query_as(
                "SELECT e.id::text,e.decision,report.origin,e.evidence_status FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id JOIN app.artifacts report ON report.id=e.report_artifact_id AND report.id=e.method_versions_artifact_id AND report.project_id=e.project_id AND report.producer_run_id=e.run_id AND report.producer_attempt_id=$2 AND report.schema_name='qz.alpha_evaluation' AND report.schema_version='1' AND report.access_class='EVALUATOR_ONLY' WHERE e.run_id=$1 AND e.subject_alpha_version_id=$3 AND e.input_set_id=$4 AND e.evaluation_kind='SEALED' AND e.execution_status='SUCCEEDED'",
            )
            .bind(sealed.as_uuid())
            .bind(finished.active_attempt_id.unwrap().as_uuid())
            .bind(alpha.as_uuid())
            .bind(finished.input_set_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(sealed_evaluation.2, "FIXTURE");
            assert_eq!(sealed_evaluation.3, "VALID");
            assert!(matches!(
                store
                    .evaluation(&actor, Id::try_from(sealed_evaluation.0).unwrap())
                    .await,
                Err(store::StoreError::NotFound)
            ));
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.sealed_opportunities")
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                1
            );
            println!(
                "native independent review: real Sealed decision={}, fixture grants no qualification",
                sealed_evaluation.1
            );
        } else {
            assert_eq!(review_decision, Decision::Reject);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.mission_sealed_evaluations")
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.sealed_evaluation_tasks")
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                0
            );
        }

        assert_eq!(
            store.get_run(&actor, reviewer.run_id).await.unwrap().state,
            RunState::Succeeded
        );
        let usage: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT sum(used_tokens)::bigint FROM app.model_turn_accounting WHERE cycle_id=$1),(SELECT sum(receipt.actual_tokens)::bigint FROM app.model_turn_reservations r JOIN app.model_turn_receipts receipt ON receipt.reservation_id=r.id WHERE r.cycle_id=$1)",
        )
        .bind(cycle.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(usage.0, usage.1);
        assert_eq!(usage.0, reviewed_requests as i64 * 12);
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM app.mission_review_turns WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2)",
        )
        .bind(reviewer.run_id.as_uuid())
        .bind(reviewer.message_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 0, 1));
        store.acknowledge_run(reviewer).await.unwrap();
        assert_eq!(provider.request_count(), reviewed_requests);
        let preserved: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(e) FROM app.evaluations e WHERE id=$1")
                .bind(Id::try_from(evaluation.clone()).unwrap().as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(preserved, original_evaluation);
        let unchanged = published(
            &pool,
            &data.objects,
            validated.id,
            validated.active_attempt_id.unwrap(),
            "qz.alpha_validation",
        )
        .await;
        assert_eq!(unchanged.artifact_id, source_report_id);
        assert_eq!(unchanged.bytes, raw);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.qualifications")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        println!("native independent review: decision={review_decision:?}, different Thread, exact native code/parameters/metrics, unique review and no qualification");
    }
    remote.assert_private_logs();
    println!(
        "{}",
        serde_json::json!({
            "scope":"real native Parquet/MCP/compile/forecast/validation/Evaluation/original-Thread; controlled Provider, not account or market acceptance",
            "scientific_runs":if independent == Some(Decision::Pass) {4} else {3},
            "native_provider_requests":provider.request_count(),
            "independent_review_decision":independent,
            "original_thread_preserved":true,"original_attempt_preserved":true,
            "lease_takeover_observed":true,"validation_observations":measured.unique_test_observations,
            "actual_evaluation_decision":decision,"qualification_granted":false,
        })
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_mcp_science_returns_to_the_original_thread(pool: PgPool) {
    // Only the Worker's existing sanitized StoreError Display is enabled.
    // Never enable SQL, native messages, credentials or ambient RUST_LOG output.
    let _ = tracing_subscriber::fmt()
        .with_env_filter("off,server::worker=warn")
        .with_test_writer()
        .with_ansi(false)
        .try_init();
    tokio::time::timeout(Duration::from_secs(300), Box::pin(scenario(pool, None)))
        .await
        .expect("connected native research acceptance deadline");
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_review_pass_admits_original_sealed_science(pool: PgPool) {
    tokio::time::timeout(
        Duration::from_secs(300),
        Box::pin(scenario(pool, Some(Decision::Pass))),
    )
    .await
    .expect("native independent PASS and Sealed acceptance deadline");
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_review_reject_preserves_scientific_pass_without_sealed(pool: PgPool) {
    tokio::time::timeout(
        Duration::from_secs(300),
        Box::pin(scenario(pool, Some(Decision::Reject))),
    )
    .await
    .expect("native independent REJECT acceptance deadline");
}
