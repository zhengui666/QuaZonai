//! One connected, account-waived native research loop. The Provider scripts model
//! responses only: all DATA_VALIDATE/compile/forecast/validation bytes come from OCI.
use super::{actual_probe, mission_support, published, responses, scientific_catalog, support};
use contracts::{
    artifacts::ResearchArtifactKind,
    execution::NativeTaskParametersV1,
    experiments::{ExperimentProposalV1, ExperimentSource},
    runs::{RunSnapshotV1, RunState},
    runtime_jobs::{JobSpecV1, ResultManifestV1},
    science::{NativeAlphaValidationResultV1, NativeForecastResultV1},
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
use std::{collections::BTreeMap, sync::Arc, time::Duration};
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
) {
    tokio::time::timeout(
        Duration::from_secs(110),
        worker.process_mission_message(
            message.clone(),
            "connected-native-mission",
            shutdown.clone(),
        ),
    )
    .await
    .expect("bounded native Mission turn/tick")
    .unwrap();
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

async fn scenario(pool: PgPool) {
    let mut remote = support::Fixture::open().await;
    let scientific_catalog::Prepared {
        store,
        actor,
        data,
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
    let binary = std::env::var_os("QUAZONAI_NATIVE_SERVER_BIN")
        .expect("the built native server CLI is required");
    let deployment = CodexDeployment::new(CodexDeploymentConfig {
        schema_version: SchemaV1,
        binary: std::env::var_os("CODEX_NATIVE_BIN")
            .expect("the pinned official Codex binary is required")
            .into(),
        executable_path: std::env::var("PATH").unwrap(),
        bindings: vec![CodexDeploymentBinding {
            reference: profile.home_binding.unwrap(),
            label: "Native scientific fixture".into(),
            profile_origin: profile.profile_origin,
            home: home.clone(),
            codex_home: home.clone(),
            working_directory: home,
            environment_names: vec![],
        }],
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
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
        scientific_catalog::SOURCE.into(),
    )
    .await;
    let parameters=scientific_catalog::upload(&store,&actor,&data.objects,data.data.project,ResearchArtifactKind::Parameters,
        serde_json::json!({"schema_version":1,"dataset_revision_id":data.data.discovery,
            "parameters":{"schema_version":1,"fast_period":2,"slow_period":5,"label_horizon_observations":5,"total_fuel":"1000000"}}).to_string()).await;
    let proposal_report=scientific_catalog::upload(&store,&actor,&data.objects,data.data.project,ResearchArtifactKind::Report,
        r#"{"schema_version":1,"hypothesis":"Observe the exact close-minus-previous source on the native fixture; no market claim."}"#.into()).await;
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
    mission_tick(&worker, &mission, &shutdown).await;
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
    let (_, model) = published(
        &pool,
        &data.objects,
        compiled.id,
        compiled.active_attempt_id.unwrap(),
        "qz.wasm_model",
    )
    .await;
    let mut model = job::signals::WasmSignal::new(&model, 2, 100_000).unwrap();
    assert_eq!(
        model
            .predict([3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap(),
        2.0
    );
    assert_eq!(
        model
            .predict([1.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap(),
        -2.0
    );
    mission_tick(&worker, &mission, &shutdown).await;
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
    let (_, prediction) = published(
        &pool,
        &data.objects,
        predicted.id,
        predicted.active_attempt_id.unwrap(),
        "qz.native_forecast",
    )
    .await;
    let forecast: NativeForecastResultV1 = serde_json::from_slice(&prediction).unwrap();
    assert!(forecast.consumed_fuel.get() > 0);
    let parameter_size = prediction_spec
        .inputs
        .iter()
        .find_map(|input| match input {
            contracts::runtime_jobs::RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == prediction_spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &data
            .objects
            .read(prediction_spec.parameters_artifact_id, parameter_size)
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::EvaluateAlpha { request, .. } = task else {
        panic!("native forecast task required");
    };
    let root = catalog.path().join("window-0");
    let oracle = tokio::task::spawn_blocking(move || {
        let observed = job::catalog::load_catalog(&root, &request.selection).unwrap();
        let mut expected = BTreeMap::new();
        for series in observed.series {
            for (index, bar) in series.bars.iter().enumerate() {
                let prediction = (index + 1 >= request.parameters.slow_period as usize)
                    .then(|| bar.close.as_f64() - series.bars[index - 1].close.as_f64());
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
    mission_tick(&worker, &mission, &shutdown).await; // Records the unqualified Alpha.
    mission_tick(&worker, &mission, &shutdown).await; // Admits actual independent Validation.
    let validation = scientific_run(&pool, experiment, "validation").await;
    let (validated, _) = execute(
        &pool,
        &store,
        &actor,
        &mut remote,
        (&worker, &data.objects),
        validation,
        shutdown.clone(),
    )
    .await;
    let (_, raw) = published(
        &pool,
        &data.objects,
        validated.id,
        validated.active_attempt_id.unwrap(),
        "qz.alpha_validation",
    )
    .await;
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
    let source_report:String=sqlx::query_scalar(
        "SELECT a.id::text FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.schema_name='qz.alpha_validation'",
    ).bind(validated.active_attempt_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        provider.request_count(),
        3,
        "science must run outside the native model tool loop"
    );
    mission_tick(&worker, &mission, &shutdown).await; // Publishes original bounded feedback request.
    assert_eq!(provider.request_count(), 3);
    mission_tick(&worker, &mission, &shutdown).await; // Reopens App Server and resumes original Thread.
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
    assert_eq!(facts, (2, 2, 1, thread));
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
    remote.assert_private_logs();
    println!(
        "{}",
        serde_json::json!({
            "scope":"real native Parquet/MCP/compile/forecast/validation/Evaluation/original-Thread; controlled Provider, not account or market acceptance",
            "scientific_runs":3,"native_provider_requests":provider.request_count(),
            "original_thread_preserved":true,"validation_observations":measured.unique_test_observations,
            "actual_evaluation_decision":decision,"qualification_granted":false,
        })
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_mcp_science_returns_to_the_original_thread(pool: PgPool) {
    tokio::time::timeout(Duration::from_secs(300), Box::pin(scenario(pool)))
        .await
        .expect("connected native research acceptance deadline");
}
