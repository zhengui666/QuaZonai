//! Formal Cycle/PGMQ/issuance + official App Server bootstrap. Only the market
//! preparation and model response are fixtures; no paid-account/T42 claim.
#![cfg(feature = "native-codex")]
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "support/codex_responses.rs"]
mod responses;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{research::DataOrigin, runs::RunState, DbCounter, Id, SchemaV1};
use integrations::secrets::SecretVault;
use server::{
    codex_profiles::{CodexDeployment, CodexDeploymentBinding, CodexDeploymentConfig},
    worker::{
        mission::{MissionLauncher, TurnProgress},
        Worker,
    },
    AppState, WebPolicy,
};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, sync::Arc};
use store::{
    authority::Actor,
    lifecycle::{ClaimResult, RunLease, RunMessage},
    turns::{DispatchDecision, Reservation, TurnOutcome, TurnRequest},
    Store,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tower_sessions::cookie::Key;
use tower_sessions_sqlx_store::PostgresStore;

struct Fixture {
    store: Store,
    actor: Actor,
    data: cycle_support::Fixture,
    lease: RunLease,
    message: RunMessage,
    launcher: Arc<MissionLauncher>,
    vault: Arc<SecretVault>,
    provider: responses::Provider,
    root: tempfile::TempDir,
    http: JoinHandle<()>,
    runtime_http: JoinHandle<()>,
    runtime_targets: server::runtime_transport::RuntimeTargets,
    runtime_probes: Arc<std::sync::atomic::AtomicUsize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.http.abort();
        self.runtime_http.abort();
    }
}

async fn fixture(pool: &PgPool) -> Fixture {
    fixture_with_cost(pool, false).await
}

async fn fixture_with_cost(pool: &PgPool, priced: bool) -> Fixture {
    fixture_with_selection(pool, priced, 2, DataOrigin::Fixture).await
}

async fn fixture_with_selection(
    pool: &PgPool,
    priced: bool,
    candidates: u16,
    origin: DataOrigin,
) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    for name in ["native", "workspaces", "secrets"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.path().join(name))
            .unwrap();
    }
    let key = root.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let secrets = root.path().join("secrets");
    let vault = Arc::new(SecretVault::open(&secrets, &key).unwrap());
    let secret = integrations::authentication::random_capability();
    let reference = vault.put("RUNTIME", secret.as_bytes()).unwrap();
    let bearer = format!("Bearer {secret}");
    let runtime_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let runtime_address = runtime_listener.local_addr().unwrap();
    let runtime_origin = format!("http://{runtime_address}");
    let runtime_probes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = runtime_probes.clone();
    let runtime_app = axum::Router::new().route(
        "/runtime/v1/capabilities",
        axum::routing::get(move |headers: axum::http::HeaderMap| {
            let count = count.clone();
            let bearer = bearer.clone();
            async move {
                if headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|h| h.to_str().ok())
                    != Some(bearer.as_str())
                {
                    return Err(axum::http::StatusCode::UNAUTHORIZED);
                }
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(axum::Json(experiment_support::capabilities(
                    chrono::Utc::now(),
                )))
            }
        }),
    );
    let runtime_http = tokio::spawn(async move {
        axum::serve(runtime_listener, runtime_app).await.unwrap();
    });
    let runtime_targets = server::runtime_transport::RuntimeTargets::new(
        vec![server::runtime_transport::RuntimeTarget {
            origin: runtime_origin.clone(),
            addresses: vec![runtime_address],
        }],
        true,
    )
    .unwrap();
    let objects = Arc::new(
        integrations::artifacts::ArtifactStore::open(&root.path().join("objects")).unwrap(),
    );
    let (store, actor) = research_support::operator(pool).await;
    let mut data =
        cycle_support::setup_with_policy(pool, &store, &actor, objects, origin, |policy| {
            policy.selection.candidate_count = candidates;
            if candidates == 1 {
                // Both controlled origins use the same passing scientific criterion.
                // Only REAL/PIT declarations may reach qualification registration.
                policy.sealed_metric_requirements[0].threshold_low = Some("0.1".parse().unwrap());
            }
        })
        .await;
    if candidates == 1 {
        // Explicit same Profile is allowed; independent native Thread is still
        // mandatory and checked below. No second HOME or account is invented.
        data.reviewer_profile = data.researcher_profile;
    }
    let current = store.runtime(&actor, data.data.runtime).await.unwrap();
    let mut configuration = current.configuration;
    configuration.endpoint = runtime_origin;
    configuration.development_http = true;
    let updated = store
        .update_runtime(
            &actor,
            "native-mission-runtime",
            data.data.runtime,
            &contracts::settings::RuntimeUpdate {
                schema_version: SchemaV1,
                expected_revision: current.revision,
                configuration,
                credential_ref: Some(reference),
                ca_certificate_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    data.freeze.execution_context.runtime_revision = updated.revision;
    experiment_support::probe(&store, &actor, &data).await;
    let (store, actor, data, _, preparation) =
        mission_support::start(store, actor, data, priced).await;
    mission_support::complete(pool, &store, &data, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store.read_mission_messages(60, 10).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(lease)) = store
        .claim_mission(&message, "bootstrap-first", 120)
        .await
        .unwrap()
    else {
        panic!("native Mission lease required");
    };
    let home = root.path().join("native");
    let provider = responses::Provider::start(&home).await;
    let profile = store
        .codex_profile(&actor, data.researcher_profile.profile_id)
        .await
        .unwrap();
    let deployment = CodexDeployment::new(CodexDeploymentConfig {
        schema_version: SchemaV1,
        binary: std::env::var_os("CODEX_NATIVE_BIN")
            .expect("pinned official binary required")
            .into(),
        executable_path: std::env::var("PATH").unwrap(),
        bindings: vec![CodexDeploymentBinding {
            reference: profile.home_binding.unwrap(),
            label: "Native bootstrap fixture".into(),
            profile_origin: profile.profile_origin,
            home: home.clone(),
            codex_home: home.clone(),
            working_directory: home,
            environment_names: vec![],
        }],
    })
    .unwrap();
    let sessions = PostgresStore::new(pool.clone());
    sessions.migrate().await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let app = server::router(
        AppState::new(
            store.clone(),
            SecretVault::open(&secrets, &key).unwrap(),
            WebPolicy::new(&origin, address, true).unwrap(),
        )
        .with_artifact_store(
            integrations::artifacts::ArtifactStore::open(&root.path().join("objects")).unwrap(),
        ),
        Key::generate(),
    );
    let http = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let launcher = MissionLauncher::new(
        deployment,
        root.path().join("workspaces"),
        env!("CARGO_BIN_EXE_server").into(),
        origin,
        true,
    )
    .unwrap();
    Fixture {
        store,
        actor,
        data,
        lease: *lease,
        message,
        launcher: Arc::new(launcher),
        vault,
        provider,
        root,
        http,
        runtime_http,
        runtime_targets,
        runtime_probes,
    }
}

async fn takeover(f: &Fixture, pool: &PgPool, owner: &str) -> RunLease {
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(f.lease.fence.attempt_id.as_uuid()).execute(pool).await.unwrap();
    let Some(ClaimResult::Leased(lease)) =
        f.store.claim_mission(&f.message, owner, 120).await.unwrap()
    else {
        panic!("same Attempt takeover required");
    };
    assert_eq!(lease.fence.attempt_id, f.lease.fence.attempt_id);
    lease.as_ref().clone()
}

fn daemon(f: &Fixture) -> Worker {
    Worker::new(
        f.store.clone(),
        SecretVault::open(
            &f.root.path().join("secrets"),
            &f.root.path().join("master.key"),
        )
        .unwrap(),
        integrations::artifacts::ArtifactStore::open(&f.root.path().join("objects")).unwrap(),
        f.runtime_targets.clone(),
        1,
    )
    .unwrap()
}

async fn visible(f: &Fixture, pool: &PgPool) {
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(f.lease.fence.attempt_id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("SELECT pgmq.set_vt('runs',$1,0)")
        .bind(f.message.message_id)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn daemon_cancellation_without_turns_does_not_open_a_native_thread(pool: PgPool) {
    let f = fixture(&pool).await;
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    f.store
        .cancel_run(
            &f.actor,
            "cancel-before-thread",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    visible(&f, &pool).await;
    let (_stop, receiver) = tokio::sync::watch::channel(false);
    daemon(&f)
        .with_missions(f.launcher.clone())
        .process_mission_message(f.message.clone(), "cancel-without-native", receiver)
        .await
        .unwrap();
    let facts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$1)")
        .bind(f.message.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0, 1));
    assert_eq!(f.provider.request_count(), 0);
    let selection = f
        .store
        .cycle_selection(&f.actor, f.lease.run.cycle_id.unwrap())
        .await
        .unwrap();
    assert_eq!(selection.research_run_id, run.id);
    assert_eq!(selection.trial_count.get(), 0);
    assert_eq!(
        selection.status,
        contracts::cycles::SelectionStatus::Inconclusive
    );
    assert_eq!(
        f.store.get_run(&f.actor, run.id).await.unwrap().state,
        RunState::Cancelled
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn daemon_cancellation_settles_unsent_turn_without_reopening_original_thread(pool: PgPool) {
    let f = fixture(&pool).await;
    let connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    let session = connection.session.id;
    let reserved = prepare(
        &f,
        &f.lease,
        "never-sent",
        responses::FIRST_PROMPT,
        f.lease.run.deadline_at,
    )
    .await;
    connection.client.close().await.unwrap();
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    f.store
        .cancel_run(
            &f.actor,
            "cancel-unsent-original",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    visible(&f, &pool).await;
    let (_stop, receiver) = tokio::sync::watch::channel(false);
    daemon(&f)
        .with_missions(f.launcher.clone())
        .process_mission_message(f.message.clone(), "settle-without-reopening", receiver)
        .await
        .unwrap();
    let receipt: (uuid::Uuid,String,i64,String) = sqlx::query_as("SELECT r.session_id,t.outcome,t.actual_tokens,t.usage_source FROM app.model_turn_reservations r JOIN app.model_turn_receipts t ON t.reservation_id=r.id WHERE r.id=$1")
        .bind(reserved.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        receipt,
        (
            session.as_uuid(),
            "NOT_SENT".into(),
            0,
            "CONFIRMED_NOT_SENT".into()
        )
    );
    let facts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_bindings),(SELECT count(*) FROM app.model_turn_summaries),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$1)")
        .bind(f.message.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0, 1));
    assert_eq!(f.provider.request_count(), 0);
    let selection = f
        .store
        .cycle_selection(&f.actor, f.lease.run.cycle_id.unwrap())
        .await
        .unwrap();
    assert_eq!(selection.research_run_id, run.id);
    assert_eq!(selection.trial_count.get(), 0);
    assert_eq!(
        selection.status,
        contracts::cycles::SelectionStatus::Inconclusive
    );
    assert_eq!(
        f.store.get_run(&f.actor, run.id).await.unwrap().state,
        RunState::Cancelled
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn settled_native_mission_publishes_validation_then_returns_to_original_thread(pool: PgPool) {
    settled_scientific_protocol(pool, DataOrigin::Fixture).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn controlled_real_declaration_registers_original_reviewed_qualification(pool: PgPool) {
    // Real PG/files/App Server protocol, controlled market/model results. This
    // proves original association and transaction gates, NOT REAL acceptance.
    settled_scientific_protocol(pool, DataOrigin::Real).await;
}

async fn settled_scientific_protocol(pool: PgPool, origin: DataOrigin) {
    let declared_origin = serde_json::to_value(origin).unwrap();
    let f = fixture_with_selection(&pool, false, 1, origin).await;
    let experiment = experiment_support::propose(
        &pool,
        &f.store,
        &f.actor,
        &f.data,
        f.lease.run.cycle_id.unwrap(),
    )
    .await;
    // Actual expiry, never an edited observation or an increased production TTL.
    tokio::time::timeout(std::time::Duration::from_secs(65), async {
        loop {
            let expired: bool = sqlx::query_scalar("SELECT valid_until<=clock_timestamp() FROM app.runtime_probe_observations WHERE runtime_id=$1 ORDER BY observed_at DESC,id DESC LIMIT 1")
                .bind(f.data.data.runtime.as_uuid()).fetch_one(&pool).await.unwrap();
            if expired { break; }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }).await.expect("native probe did not expire");
    f.provider.initial_request();
    visible(&f, &pool).await;
    let worker = daemon(&f).with_missions(f.launcher.clone());
    let (_stop, receiver) = tokio::sync::watch::channel(false);
    let first = tokio::time::timeout(
        std::time::Duration::from_secs(150),
        worker.process_mission_message(f.message.clone(), "automatic-compile", receiver.clone()),
    )
    .await
    .unwrap();
    first.unwrap();
    assert!(f.runtime_probes.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    assert_eq!(f.provider.request_count(), 1);
    let compiler: uuid::Uuid = sqlx::query_scalar(
        "SELECT compile_run_id FROM app.experiment_compilations WHERE experiment_id=$1",
    )
    .bind(experiment.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let compiler: Id = compiler.to_string().try_into().unwrap();
    let compiled = f.store.get_run(&f.actor, compiler).await.unwrap();
    assert_eq!(compiled.state, RunState::Queued);
    visible(&f, &pool).await;
    worker
        .process_mission_message(
            f.message.clone(),
            "await-original-compiler",
            receiver.clone(),
        )
        .await
        .unwrap();
    assert_eq!(f.provider.request_count(), 1);
    let model = experiment_support::complete_compilation(&pool, &f.store, &f.data, compiler).await;
    visible(&f, &pool).await;
    worker
        .process_mission_message(f.message.clone(), "automatic-forecast", receiver.clone())
        .await
        .unwrap();
    let (forecast, bound_model): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT run_id,model_artifact_id FROM app.experiment_forecasts WHERE experiment_id=$1",
    )
    .bind(experiment.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bound_model, model.as_uuid());
    assert_eq!(
        f.store
            .experiment(&f.actor, experiment)
            .await
            .unwrap()
            .run_id
            .unwrap()
            .as_uuid(),
        forecast
    );
    visible(&f, &pool).await;
    worker
        .process_mission_message(
            f.message.clone(),
            "await-original-forecast",
            receiver.clone(),
        )
        .await
        .unwrap();
    assert_eq!(f.provider.request_count(), 1);
    let counts: (i64,i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.experiment_compilations WHERE experiment_id=$1),(SELECT count(*) FROM app.experiment_forecasts WHERE experiment_id=$1),(SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$2),(SELECT count(*) FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.run_id=$2),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$2),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$3)")
        .bind(experiment.as_uuid()).bind(f.lease.run.id.as_uuid()).bind(f.message.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1, 1, 0, 1));
    let scientific_run: Id = forecast.to_string().try_into().unwrap();
    experiment_support::complete_forecast(&pool, &f.store, &f.data, scientific_run).await;
    let original_thread: String =
        sqlx::query_scalar("SELECT thread_id FROM app.codex_sessions WHERE run_id=$1")
            .bind(f.lease.run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    visible(&f, &pool).await;
    worker
        .process_mission_message(f.message.clone(), "record-research-alpha", receiver.clone())
        .await
        .unwrap();
    let research:(String,i64,String)=sqlx::query_as("SELECT a.lifecycle,(SELECT count(*) FROM app.qualifications WHERE alpha_version_id=v.id),e.outcome FROM app.alpha_versions v JOIN app.alphas a ON a.id=v.alpha_id JOIN app.experiments e ON e.id=v.experiment_id WHERE e.id=$1")
        .bind(experiment.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(research, ("RESEARCH".into(), 0, "PENDING".into()));
    assert_eq!(f.provider.request_count(), 1);
    visible(&f, &pool).await;
    worker
        .process_mission_message(f.message.clone(), "automatic-validation", receiver.clone())
        .await
        .unwrap();
    let validation: uuid::Uuid =
        sqlx::query_scalar("SELECT run_id FROM app.experiment_validations WHERE experiment_id=$1")
            .bind(experiment.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let validation: Id = validation.to_string().try_into().unwrap();
    assert_eq!(
        f.store.get_run(&f.actor, validation).await.unwrap().state,
        RunState::Queued
    );
    visible(&f, &pool).await;
    worker
        .process_mission_message(f.message.clone(), "await-validation", receiver.clone())
        .await
        .unwrap();
    assert_eq!(f.provider.request_count(), 1);
    assert!(f.store.acknowledge_run(&f.message).await.is_err());
    let raw =
        experiment_support::complete_validation(&pool, &f.store, &f.data, validation, 1000, 0.8)
            .await;
    visible(&f, &pool).await;
    worker
        .process_mission_message(
            f.message.clone(),
            "await-evaluation-publication",
            receiver.clone(),
        )
        .await
        .unwrap();
    assert_eq!(f.provider.request_count(), 1);
    assert!(f.store.acknowledge_run(&f.message).await.is_err());
    let message_id: i64 = sqlx::query_scalar(
        "SELECT initial_queue_message_id FROM app.run_admissions WHERE run_id=$1",
    )
    .bind(validation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let native_message = RunMessage {
        message_id,
        run_id: validation,
        read_count: 1,
    };
    let object_names = || {
        fs::read_dir(f.root.path().join("objects"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before_publication = object_names();
    sqlx::raw_sql("CREATE FUNCTION public.reject_validation_publication() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected calibrated version publication failure'; END $$; CREATE TRIGGER reject_validation BEFORE INSERT ON app.alpha_versions FOR EACH ROW EXECUTE FUNCTION public.reject_validation_publication();").execute(&pool).await.unwrap();
    assert!(worker
        .process_message(
            native_message.clone(),
            "publication-fault",
            receiver.clone()
        )
        .await
        .is_err());
    assert_eq!(
        f.store.get_run(&f.actor, validation).await.unwrap().state,
        RunState::Succeeded
    );
    assert!(f.store.acknowledge_run(&native_message).await.is_err());
    assert_eq!(
        object_names(),
        before_publication,
        "both unpublished objects were reclaimed without changing original evidence"
    );
    assert_eq!(f.provider.request_count(), 1);
    sqlx::raw_sql("DROP TRIGGER reject_validation ON app.alpha_versions; DROP FUNCTION public.reject_validation_publication();").execute(&pool).await.unwrap();
    worker
        .process_message(
            native_message.clone(),
            "publication-recovery",
            receiver.clone(),
        )
        .await
        .unwrap();
    // PGMQ cannot redeliver an archived message. The ACK entry point, not a
    // fresh claim of that removed queue row, owns acknowledgement replay.
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.calibrations c JOIN app.evaluations e ON e.id=c.validation_evaluation_id JOIN app.artifacts a ON a.id=c.model_artifact_id WHERE e.run_id=$1 AND a.producer_run_id=e.run_id AND a.access_class='EVALUATOR_ONLY' AND a.origin=$2")
        .bind(validation.as_uuid()).bind(declared_origin.as_str().unwrap()).fetch_one(&pool).await.unwrap(), 1);
    f.store.acknowledge_run(&native_message).await.unwrap();
    let (evaluation, report): (uuid::Uuid, uuid::Uuid) =
        sqlx::query_as("SELECT id,report_artifact_id FROM app.evaluations WHERE run_id=$1")
            .bind(validation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let derived: uuid::Uuid = sqlx::query_scalar("SELECT v.id FROM app.alpha_versions v JOIN app.calibrations c ON c.id=v.calibration_id WHERE c.validation_evaluation_id=$1")
        .bind(evaluation).fetch_one(&pool).await.unwrap();
    let calibrated = f
        .store
        .alpha_calibration(&f.actor, derived.to_string().try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(calibrated.validation.id.as_uuid(), evaluation);
    assert_ne!(
        calibrated
            .validation
            .subject_alpha_version_id
            .map(Id::as_uuid),
        Some(derived)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.alpha_versions WHERE experiment_id=$1"
        )
        .bind(experiment.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    let public = f.store.experiment(&f.actor, experiment).await.unwrap();
    assert_eq!(
        public.outcome,
        Some(contracts::experiments::ExperimentOutcome::Supported)
    );
    assert!(f
        .store
        .artifact(&f.actor, report.to_string().try_into().unwrap())
        .await
        .is_err());
    let before_feedback = takeover(&f, &pool, "feedback-publication-fault").await;
    assert!(matches!(
        f.store
            .prepare_mission_result_turn(
                f.lease.run.id,
                &before_feedback.fence,
                |_, _| async { panic!("formal feedback must not read any report bytes") },
                |_| async { Err(store::StoreError::Integrity) },
            )
            .await,
        Err(store::StoreError::Integrity)
    ));
    assert!(!f
        .store
        .complete_mission(f.lease.run.id, &before_feedback.fence)
        .await
        .unwrap());
    visible(&f, &pool).await;
    worker
        .process_mission_message(f.message.clone(), "prepare-formal-result", receiver.clone())
        .await
        .unwrap();
    assert_eq!(
        f.provider.request_count(),
        1,
        "preparation is not a model call"
    );
    let feedback_lease = takeover(&f, &pool, "inspect-result-request").await;
    let latest = f
        .store
        .mission_turn_checkpoint(f.lease.run.id, &feedback_lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap();
    assert_eq!(latest.reservation.ordinal, 2);
    assert_eq!(
        latest.reservation.turn_kind,
        domain::admission::TurnKind::Research
    );
    let objects = f.data.objects.clone();
    let prompt = f
        .store
        .mission_turn_prompt(
            f.lease.run.id,
            &feedback_lease.fence,
            latest.reservation.id,
            move |id, size| async move {
                objects
                    .read(id, size)
                    .map_err(|_| store::StoreError::Integrity)
            },
        )
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(prompt.lines().nth(1).unwrap()).unwrap();
    assert_eq!(body["run_id"], validation.to_string());
    assert_eq!(body["stage"], "VALIDATION");
    assert_eq!(body["origin"], declared_origin);
    assert_eq!(body["formal_evaluation"], "PUBLISHED");
    assert_eq!(body["evaluation"]["id"], evaluation.to_string());
    assert_eq!(body["evaluation"]["report_artifact_id"], report.to_string());
    assert_eq!(body["evaluation"]["decision"], "PASS");
    assert_eq!(body["evaluation"]["unexpired_at_feedback"], true);
    assert_eq!(
        body["evaluation"]["selection_metric"]["metric_code"],
        "PEARSON_IC"
    );
    assert_eq!(body["evaluation"]["selection_metric"]["value"], 0.8);
    assert_eq!(
        body["evaluation"]["selection_metric"]["source_artifact_id"],
        raw.to_string()
    );
    for excluded in ["forecast", "folds", "points", "calibration"] {
        assert!(body.get(excluded).is_none());
        assert!(body["evaluation"].get(excluded).is_none());
    }
    sqlx::raw_sql("CREATE FUNCTION public.reject_reviewer() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.role='INDEPENDENT_REVIEWER' THEN RAISE EXCEPTION 'controlled Reviewer admission failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_reviewer BEFORE INSERT ON app.run_missions FOR EACH ROW EXECUTE FUNCTION public.reject_reviewer();")
        .execute(&pool).await.unwrap();
    visible(&f, &pool).await;
    let stopped_at_ack = tokio::time::timeout(
        std::time::Duration::from_secs(150),
        worker.process_mission_message(
            f.message.clone(),
            "resume-with-real-result",
            receiver.clone(),
        ),
    )
    .await
    .unwrap();
    assert!(stopped_at_ack.is_err());
    assert_eq!(f.provider.request_count(), 2);
    assert!(f.provider.saw_previous_context());
    assert_eq!(
        f.store
            .get_run(&f.actor, f.lease.run.id)
            .await
            .unwrap()
            .state,
        RunState::Succeeded
    );
    let partial:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.cycle_selections),(SELECT count(*) FROM app.run_missions WHERE role='INDEPENDENT_REVIEWER')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        partial,
        (0, 0),
        "failed Reviewer creation rolls back selection and its Run/queue reservation"
    );
    sqlx::raw_sql(
        "DROP TRIGGER reject_reviewer ON app.run_missions; DROP FUNCTION public.reject_reviewer();",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        f.store.acknowledge_run(&f.message),
        f.store.acknowledge_run(&f.message)
    );
    a.unwrap();
    b.unwrap();
    // Two actual competing ACKs share the same original native completion and
    // create one selection/Reviewer without another research model request.
    let cycle = f.lease.run.cycle_id.unwrap();
    let selection = f.store.cycle_selection(&f.actor, cycle).await.unwrap();
    assert_eq!(selection.research_run_id, f.lease.run.id);
    assert_eq!(
        (
            selection.trial_count.get(),
            selection.eligible_count.get(),
            selection.selected_count.get()
        ),
        (1, 1, 1)
    );
    assert_eq!(
        selection.status,
        contracts::cycles::SelectionStatus::Complete
    );
    let trials = f
        .store
        .cycle_selection_trials(&f.actor, cycle, &Default::default())
        .await
        .unwrap()
        .items;
    assert_eq!(trials.len(), 1);
    assert_eq!(trials[0].experiment_id, experiment);
    assert_eq!(trials[0].evaluation_id.map(Id::as_uuid), Some(evaluation));
    assert_eq!(trials[0].execution_run_id, Some(validation));
    assert_eq!(
        trials[0]
            .selection_metric
            .as_ref()
            .unwrap()
            .source_artifact_id,
        raw
    );
    f.store.acknowledge_run(&f.message).await.unwrap();
    assert_eq!(
        serde_json::to_value(f.store.cycle_selection(&f.actor, cycle).await.unwrap()).unwrap(),
        serde_json::to_value(selection).unwrap()
    );
    let archived: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$1")
        .bind(f.message.message_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(archived, 1);
    assert_eq!(f.provider.request_count(), 2);
    let facts:(i64,i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT thread_id FROM app.codex_sessions WHERE run_id=$1)")
        .bind(f.lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (2, 2, 1, original_thread));
    let evidence: (i64,i64,i64,serde_json::Value) = sqlx::query_as("SELECT (SELECT count(*) FROM app.evaluations WHERE run_id=$1),(SELECT count(*) FROM app.qualifications),(SELECT used_experiments FROM app.research_cycles WHERE id=$2),observation FROM app.run_terminal_receipts WHERE run_id=$3")
        .bind(validation.as_uuid()).bind(f.lease.run.cycle_id.unwrap().as_uuid()).bind(f.lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!((evidence.0, evidence.1, evidence.2), (1, 0, 1));
    assert_eq!(evidence.3["formal_evaluation"], "PUBLISHED");
    assert_eq!(evidence.3["formal_evaluation_count"], "1");
    let summaries:i64=sqlx::query_scalar("SELECT count(*) FROM app.model_turn_summaries s JOIN app.model_turn_reservations r ON r.id=s.reservation_id WHERE r.run_id=$1")
        .bind(f.lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        summaries, 2,
        "each settled native reply has one original producer-bound public report"
    );
    let messages = f.store.read_mission_messages(60, 10).await.unwrap();
    assert_eq!(
        messages.len(),
        1,
        "Research ACK durably creates one independent Reviewer"
    );
    let review = &messages[0];
    assert_ne!(review.run_id, f.lease.run.id);
    let role: (String, uuid::Uuid) =
        sqlx::query_as("SELECT role,profile_id FROM app.run_missions WHERE run_id=$1")
            .bind(review.run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        role,
        (
            "INDEPENDENT_REVIEWER".into(),
            f.data.reviewer_profile.profile_id.as_uuid()
        )
    );
    let original_tokens:i64=sqlx::query_scalar("SELECT sum(receipt.actual_tokens)::bigint FROM app.model_turn_reservations r JOIN app.model_turn_receipts receipt ON receipt.reservation_id=r.id WHERE r.run_id=$1")
        .bind(f.lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(original_tokens > 0);
    f.store.acknowledge_run(&f.message).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.run_missions WHERE cycle_id=$1")
            .bind(cycle.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(150),
        worker.process_mission_message(
            review.clone(),
            "independent-native-review",
            receiver.clone(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let reviewed_requests = f.provider.request_count();
    assert!(
        (4..=8).contains(&reviewed_requests),
        "one independent Turn uses native input-file tools before its answer"
    );
    assert!(!f
        .store
        .get_run(&f.actor, review.run_id)
        .await
        .unwrap()
        .state
        .is_terminal());
    assert!(f.store.acknowledge_run(review).await.is_err());
    let before_sealed: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),reserved_cpu_seconds FROM app.research_cycles WHERE id=$1")
        .bind(cycle.as_uuid()).fetch_one(&pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION public.reject_review_sealed() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled Sealed continuation failure'; END $$; CREATE TRIGGER reject_review_sealed BEFORE INSERT ON app.mission_sealed_evaluations FOR EACH ROW EXECUTE FUNCTION public.reject_review_sealed();")
        .execute(&pool).await.unwrap();
    assert!(worker
        .process_mission_message(
            review.clone(),
            "independent-native-review",
            receiver.clone()
        )
        .await
        .is_err());
    assert_eq!(sqlx::query_as::<_, (i64, i64)>("SELECT (SELECT count(*) FROM app.runs),reserved_cpu_seconds FROM app.research_cycles WHERE id=$1")
        .bind(cycle.as_uuid()).fetch_one(&pool).await.unwrap(), before_sealed);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.sealed_evaluation_tasks")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::raw_sql("DROP TRIGGER reject_review_sealed ON app.mission_sealed_evaluations; DROP FUNCTION public.reject_review_sealed();")
        .execute(&pool).await.unwrap();
    // The real Worker above has refreshed Runtime capabilities. Now isolate
    // a late publication failure, not an unrelated pre-publication rejection.
    let Some(ClaimResult::Leased(review_lease)) = f
        .store
        .claim_mission(review, "independent-native-review", 60)
        .await
        .unwrap()
    else {
        panic!("original Reviewer lease");
    };
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()+interval '1 second' WHERE id=$1")
        .bind(review_lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    let delayed = std::sync::atomic::AtomicBool::new(false);
    let stale_rejection = f
        .store
        .prepare_review_sealed(
            review.run_id,
            &review_lease.fence,
            |id, size| f.data.read(id, size),
            |_| async {
                delayed.store(true, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                Err(store::StoreError::Domain(
                    domain::DomainError::CapabilityUnavailable("controlled_delayed_publication"),
                ))
            },
        )
        .await;
    assert!(delayed.load(std::sync::atomic::Ordering::SeqCst));
    assert!(stale_rejection.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM app.research_cycles WHERE id=$1")
            .bind(cycle.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        "RUNNING",
        "expired ownership cannot turn a late preparation failure into WAITING_INPUT"
    );
    worker
        .process_mission_message(
            review.clone(),
            "independent-native-review",
            receiver.clone(),
        )
        .await
        .unwrap();
    assert_eq!(f.provider.request_count(), reviewed_requests);
    let held: (uuid::Uuid, uuid::Uuid, uuid::Uuid, String, i64, String) = sqlx::query_as("SELECT task.alpha_version_id,task.validation_evaluation_id,r.cycle_id,r.state,(admission.limits->>'experiments')::bigint,parameters.created_by FROM app.mission_sealed_evaluations held JOIN app.mission_review_turns review ON review.reservation_id=held.review_reservation_id JOIN app.sealed_evaluation_tasks task ON task.run_id=held.run_id JOIN app.runs r ON r.id=task.run_id JOIN app.run_admissions admission ON admission.run_id=r.id JOIN app.run_native_tasks native ON native.run_id=r.id JOIN app.artifacts parameters ON parameters.id=native.parameters_artifact_id WHERE review.run_id=$1")
        .bind(review.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(held.0, trials[0].review_alpha_version_id.unwrap().as_uuid());
    assert_eq!(held.1, evaluation);
    assert_eq!(held.2, cycle.as_uuid());
    assert_eq!(
        (held.3.as_str(), held.4, held.5.as_str()),
        ("QUEUED", 0, "RUNTIME")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.command_receipts WHERE operation='ALPHA_EVALUATE'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.sealed_opportunities")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        f.store
            .get_run(&f.actor, review.run_id)
            .await
            .unwrap()
            .state,
        RunState::Succeeded
    );
    let reviewed:(String,String,uuid::Uuid)=sqlx::query_as("SELECT s.thread_id,r.decision,t.alpha_version_id FROM app.mission_reviews r JOIN app.mission_review_turns t ON t.reservation_id=r.reservation_id JOIN app.codex_sessions s ON s.run_id=t.run_id WHERE t.run_id=$1")
        .bind(review.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_ne!(reviewed.0, facts.3);
    assert_eq!(reviewed.1, "PASS");
    assert_eq!(
        Some(reviewed.2),
        trials[0].review_alpha_version_id.map(Id::as_uuid)
    );
    let copied: serde_json::Value = serde_json::from_slice(
        &fs::read(
            f.root
                .path()
                .join("workspaces")
                .join(review.run_id.to_string())
                .join(format!("review-{experiment}"))
                .join(reviewed.2.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(copied["origin"], declared_origin);
    assert_eq!(copied["signal_kind"], "SCORE");
    assert_eq!(copied["qualification"], "NOT_GRANTED");
    assert_eq!(
        copied["review_policy"]["id"],
        f.data.brief.content.evaluation_policy_id.to_string()
    );
    for excluded in [
        "calibration",
        "points",
        "forecast",
        "conversation",
        "credentials",
    ] {
        assert!(copied.get(excluded).is_none());
    }
    let scopes:Vec<String>=sqlx::query_scalar("SELECT c.scope_codes FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id WHERE p.run_id=$1 ORDER BY c.issued_at DESC LIMIT 1")
        .bind(review.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(!scopes
        .iter()
        .any(|s| s == "ARTIFACT_SUBMIT" || s == "EXPERIMENT_SUBMIT"));
    let accumulated:(i64,i64)=sqlx::query_as("SELECT (SELECT sum(used_tokens)::bigint FROM app.model_turn_accounting WHERE cycle_id=$1),(SELECT sum(receipt.actual_tokens)::bigint FROM app.model_turn_reservations r JOIN app.model_turn_receipts receipt ON receipt.reservation_id=r.id WHERE r.cycle_id=$1)")
        .bind(cycle.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(accumulated.0, accumulated.1);
    assert!(
        accumulated.0 > original_tokens,
        "Reviewer spending does not reset Researcher usage"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.qualifications")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    f.store.acknowledge_run(review).await.unwrap();
    // ACK itself is idempotent; a stale worker must not claim an archived
    // message again. Both paths leave native model usage unchanged.
    assert!(worker
        .process_mission_message(review.clone(), "review-ack-replay", receiver)
        .await
        .is_err());
    assert_eq!(f.provider.request_count(), reviewed_requests);
    let (sealed_run, sealed_message): (uuid::Uuid, i64) = sqlx::query_as("SELECT held.run_id,admission.initial_queue_message_id FROM app.mission_sealed_evaluations held JOIN app.mission_review_turns turn ON turn.reservation_id=held.review_reservation_id JOIN app.run_admissions admission ON admission.run_id=held.run_id WHERE turn.run_id=$1")
        .bind(review.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    let sealed_run: Id = sealed_run.to_string().try_into().unwrap();
    let message = RunMessage {
        run_id: sealed_run,
        message_id: sealed_message,
        read_count: 0,
    };
    let Some(ClaimResult::Leased(lease)) = f
        .store
        .claim_native_run(&message, "controlled-sealed-result", 60)
        .await
        .unwrap()
    else {
        panic!("original automatic Sealed task");
    };
    // Native protocol/result associations are real; these scientific bytes are
    // explicitly controlled, not a real market or OCI acceptance claim.
    experiment_support::complete_sealed(&pool, &f.store, &f.data, *lease).await;
    let objects = f.data.objects.clone();
    let sealed_evaluation = f
        .store
        .publish_scientific_result(
            sealed_run,
            |id, size| f.data.read(id, size),
            move |object| {
                let objects = objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| store::StoreError::Integrity)
                }
            },
        )
        .await
        .unwrap()
        .unwrap()
        .resource;
    let result: (String, String) = sqlx::query_as("SELECT e.decision,a.origin FROM app.evaluations e JOIN app.artifacts a ON a.id=e.report_artifact_id WHERE e.id=$1")
        .bind(sealed_evaluation.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        (result.0.as_str(), result.1.as_str()),
        ("PASS", declared_origin.as_str().unwrap())
    );
    f.store.acknowledge_run(&message).await.unwrap();
    f.store.acknowledge_run(&message).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.qualifications")
            .fetch_one(&pool)
            .await
            .unwrap(),
        i64::from(origin == DataOrigin::Real),
        "only original reviewed REAL/PIT declarations may register a qualification"
    );
    if origin == DataOrigin::Real {
        let original: (uuid::Uuid, uuid::Uuid, String, bool) = sqlx::query_as("SELECT q.qualifying_evaluation_id,a.active_version_id,a.lifecycle,q.valid_until<=e.valid_until AND q.valid_until<=v.valid_until FROM app.qualifications q JOIN app.alpha_versions av ON av.id=q.alpha_version_id JOIN app.alphas a ON a.id=av.alpha_id JOIN app.evaluations e ON e.id=q.qualifying_evaluation_id JOIN app.sealed_evaluation_tasks t ON t.run_id=e.run_id JOIN app.evaluations v ON v.id=t.validation_evaluation_id")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(
            original,
            (
                sealed_evaluation.as_uuid(),
                derived,
                "QUALIFIED".into(),
                true
            )
        );
    }
    assert_eq!(f.provider.request_count(), reviewed_requests);
}

#[sqlx::test(migrations = "../../migrations")]
async fn daemon_recovers_summary_then_terminal_ack_without_another_model_request(pool: PgPool) {
    let f = fixture(&pool).await;
    visible(&f, &pool).await;
    f.provider.initial_request();
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let disabled = daemon(&f);
    assert!(matches!(
        disabled
            .process_mission_message(f.message.clone(), "disabled", receiver.clone())
            .await,
        Err(server::worker::WorkerFailure::TaskKind)
    ));
    let (result, ()) = tokio::join!(disabled.run(receiver), async {
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        stop.send(true).unwrap();
    });
    result.unwrap();
    let reads: i32 = sqlx::query_scalar("SELECT read_ct FROM pgmq.q_runs WHERE msg_id=$1")
        .bind(f.message.message_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reads, f.message.read_count);
    assert_eq!(f.provider.request_count(), 0);

    // Force the crash-after-usage/before-summary boundary, independently of
    // whether shutdown wins the in-flight public-summary read or publication.
    sqlx::raw_sql("CREATE FUNCTION public.reject_summary_publication() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected summary publication failure'; END $$; CREATE TRIGGER reject_summary BEFORE INSERT ON app.model_turn_summaries FOR EACH ROW EXECUTE FUNCTION public.reject_summary_publication();")
        .execute(&pool).await.unwrap();
    let worker = daemon(&f).with_missions(f.launcher.clone());
    let (stop, receiver) = tokio::sync::watch::channel(false);
    tokio::time::timeout(std::time::Duration::from_secs(150), async {
        let (result, ()) = tokio::join!(worker.clone().run(receiver), async {
            loop {
                let used: Option<i64> = sqlx::query_scalar("SELECT t.actual_tokens FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.run_id=$1")
                    .bind(f.lease.run.id.as_uuid()).fetch_optional(&pool).await.unwrap();
                if let Some(used) = used {
                    assert_eq!(used, 12);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            stop.send(true).unwrap();
        });
        result.unwrap();
    }).await.expect("daemon did not settle the actual native initial Turn");
    assert_eq!(f.provider.request_count(), 1);
    let before: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2)")
        .bind(f.lease.run.id.as_uuid()).bind(f.message.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(before, (1, 1, 1));
    let absent: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_summaries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(absent, 0);
    sqlx::raw_sql("DROP TRIGGER reject_summary ON app.model_turn_summaries; DROP FUNCTION public.reject_summary_publication();").execute(&pool).await.unwrap();
    sqlx::raw_sql(&format!("CREATE FUNCTION public.reject_mission_archive() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.message->>'run_id'='{}' THEN RAISE EXCEPTION 'injected Mission ACK failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_archive BEFORE INSERT ON pgmq.a_runs FOR EACH ROW EXECUTE FUNCTION public.reject_mission_archive();",f.lease.run.id)).execute(&pool).await.unwrap();
    visible(&f, &pool).await;
    let (_stop, receiver) = tokio::sync::watch::channel(false);
    assert!(matches!(
        worker
            .process_mission_message(f.message.clone(), "daemon-restarted", receiver.clone())
            .await,
        Err(server::worker::WorkerFailure::Store)
    ));
    assert_eq!(f.provider.request_count(), 1);
    let recovered:i64=sqlx::query_scalar("SELECT count(*) FROM app.model_turn_summaries s JOIN app.model_turn_reservations r ON r.id=s.reservation_id WHERE r.run_id=$1")
        .bind(f.lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        recovered, 1,
        "restart recovers only the original public answer, with no paid Turn"
    );
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    assert_eq!(run.state, contracts::runs::RunState::Succeeded);
    let terminals: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(terminals, 1);
    sqlx::raw_sql("DROP TRIGGER reject_archive ON pgmq.a_runs; DROP FUNCTION public.reject_mission_archive();").execute(&pool).await.unwrap();
    worker
        .process_mission_message(f.message.clone(), "terminal-ack-restarted", receiver)
        .await
        .unwrap();
    f.store.acknowledge_run(&f.message).await.unwrap();
    assert_eq!(f.provider.request_count(), 1);
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$1")
        .bind(f.message.message_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn daemon_renews_pending_mission_without_starving_science_and_shutdown_keeps_unknown_usage(
    pool: PgPool,
) {
    let f = fixture(&pool).await;
    let experiment = experiment_support::propose(
        &pool,
        &f.store,
        &f.actor,
        &f.data,
        f.lease.run.cycle_id.unwrap(),
    )
    .await;
    visible(&f, &pool).await;
    f.provider.initial_request();
    f.provider.slow_response();
    let worker = daemon(&f).with_missions(f.launcher.clone());
    let (stop, receiver) = tokio::sync::watch::channel(false);
    tokio::time::timeout(std::time::Duration::from_secs(150), async {
        let (result, ()) = tokio::join!(worker.run(receiver), async {
            while f.provider.request_count() == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let first: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT lease_expires_at FROM app.run_attempts WHERE id=$1")
                .bind(f.lease.fence.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
            // A real terminal scientific message is replayed while the single
            // Mission slot is occupied. It must still be selected and archived.
            let replay: i64 = sqlx::query_scalar("SELECT pgmq.send('runs',jsonb_build_object('schema_version',1,'run_id',s.initial_run_id)) FROM app.cycle_startups s WHERE s.cycle_id=$1")
                .bind(f.lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let archived: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pgmq.a_runs WHERE msg_id=$1)")
                        .bind(replay).fetch_one(&pool).await.unwrap();
                    if archived { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }).await.expect("waiting Mission starved the scientific queue");
            tokio::time::timeout(std::time::Duration::from_secs(12), async {
                loop {
                    let renewed: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT lease_expires_at FROM app.run_attempts WHERE id=$1")
                        .bind(f.lease.fence.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
                    if renewed > first { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }).await.expect("Mission driver did not renew its own lease");
            stop.send(true).unwrap();
        });
        result.unwrap();
    }).await.expect("daemon did not stop its bounded Mission driver");
    assert_eq!(f.provider.request_count(), 1);
    let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2)")
        .bind(f.lease.run.id.as_uuid()).bind(f.message.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 0, 1));
    let compiled: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app.experiment_compilations WHERE experiment_id=$1)",
    )
    .bind(experiment.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !compiled,
        "unsettled native Turn cannot start scientific work"
    );
    assert_eq!(
        f.runtime_probes.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    assert!(!run.state.is_terminal() && run.cancellation_requested_at.is_none());
}

#[test]
fn worker_cli_requires_complete_mission_configuration_before_connecting() {
    for arguments in [
        vec!["--codex-deployment", "/missing/deployment.json"],
        vec!["--mission-api-origin", "https://localhost"],
        vec!["--mission-workspaces", "/missing/workspaces"],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_server"))
            .env_clear()
            .args([
                "worker",
                "--database-url",
                "postgresql://localhost/not-used",
            ])
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8(output.stderr)
            .unwrap()
            .contains("required arguments"));
    }
}

async fn prepare(
    f: &Fixture,
    lease: &RunLease,
    command: &str,
    prompt: &str,
    deadline: chrono::DateTime<chrono::Utc>,
) -> Reservation {
    let reading = f.data.objects.clone();
    let publishing = f.data.objects.clone();
    f.store
        .prepare_mission_turn(
            lease.run.id,
            &lease.fence,
            &TurnRequest {
                command_key: command.into(),
                turn_kind: domain::admission::TurnKind::Research,
                tokens: DbCounter::new(100).unwrap(),
                estimated_cost: f.data.brief.content.budget.cost_currency.as_ref().map(
                    |currency| domain::admission::CostEstimate {
                        amount: "0.01".parse().unwrap(),
                        currency: currency.clone(),
                    },
                ),
                request_artifact_id: Id::new(),
                deadline_at: deadline,
            },
            prompt,
            move |id, size| async move {
                reading
                    .read(id, size)
                    .map_err(|_| store::StoreError::Integrity)
            },
            move |object| async move {
                publishing
                    .put(object.id, &object.bytes)
                    .map_err(|_| store::StoreError::Integrity)
            },
        )
        .await
        .unwrap()
}

async fn turn(
    f: &Fixture,
    lease: &RunLease,
    connection: &mut server::worker::mission::MissionConnection,
    command: &str,
    prompt: &str,
    baseline: i64,
) {
    let before = f
        .store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(before.accounted_tokens.get(), baseline as u64);
    let reserved = prepare(f, lease, command, prompt, lease.run.deadline_at).await;
    let checkpoint = f
        .store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap();
    let latest = checkpoint.latest.unwrap();
    assert_eq!(latest.reservation, reserved);
    assert!(
        !latest.sent
            && latest.native_turn_id.is_none()
            && latest.terminal.is_none()
            && latest.receipt.is_none()
    );
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    let result = connection
        .drive_turn(
            &f.store,
            f.data.objects.clone(),
            lease.run.id,
            &lease.fence,
            &shutdown,
        )
        .await
        .unwrap();
    let TurnProgress::Settled(receipt) = &result else {
        panic!("native receipt required");
    };
    assert_eq!(receipt.actual_tokens.get(), 12);
    assert_eq!(receipt.outcome, TurnOutcome::Succeeded);
    assert!(receipt.actual_cost.is_none() && receipt.currency.is_none());
    let count = f.provider.request_count();
    assert_eq!(
        connection
            .drive_turn(
                &f.store,
                f.data.objects.clone(),
                lease.run.id,
                &lease.fence,
                &shutdown
            )
            .await
            .unwrap(),
        result
    );
    assert_eq!(f.provider.request_count(), count);
    let checkpoint = f
        .store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(checkpoint.accounted_tokens.get(), baseline as u64 + 12);
    let latest = checkpoint.latest.unwrap();
    assert!(latest.sent);
    assert!(latest.native_turn_id.is_some());
    assert!(latest.terminal.is_some() && latest.receipt.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn lost_native_send_ack_is_not_retried_and_expired_turn_persists_cancel(pool: PgPool) {
    let f = fixture(&pool).await;
    let mut connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    let deadline = sqlx::query_scalar("SELECT clock_timestamp()+interval '1 second'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let reserved = prepare(&f, &f.lease, "unknown", responses::FIRST_PROMPT, deadline).await;
    assert!(matches!(
        f.store
            .claim_turn_dispatch(reserved.id, &f.lease.fence)
            .await
            .unwrap(),
        DispatchDecision::Send { .. }
    ));
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    assert_eq!(
        connection
            .drive_turn(
                &f.store,
                f.data.objects.clone(),
                f.lease.run.id,
                &f.lease.fence,
                &shutdown
            )
            .await
            .unwrap(),
        TurnProgress::Unresolved
    );
    assert_eq!(f.provider.request_count(), 0);
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    assert_eq!(
        connection
            .drive_turn(
                &f.store,
                f.data.objects.clone(),
                f.lease.run.id,
                &f.lease.fence,
                &shutdown
            )
            .await
            .unwrap(),
        TurnProgress::Unresolved
    );
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    assert_eq!(run.state, RunState::CancelRequested);
    assert!(run.cancellation_requested_at.is_some());
    let item = f
        .store
        .mission_turn_checkpoint(run.id, &f.lease.fence)
        .await
        .unwrap();
    assert_eq!(item.accounted_tokens.get(), 0);
    let latest = item.latest.unwrap();
    assert!(latest.sent && latest.native_turn_id.is_none() && latest.receipt.is_none());
    assert_eq!(f.provider.request_count(), 0);
    connection.client.close().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_terminal_without_usage_preserves_first_observation_and_budget(pool: PgPool) {
    let f = fixture(&pool).await;
    let mut connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    let reserved = prepare(
        &f,
        &f.lease,
        "lost-usage",
        responses::FIRST_PROMPT,
        f.lease.run.deadline_at,
    )
    .await;
    let DispatchDecision::Send { rpc_request_id } = f
        .store
        .claim_turn_dispatch(reserved.id, &f.lease.fence)
        .await
        .unwrap()
    else {
        panic!("send permit required");
    };
    let turn = connection
        .client
        .start_turn(
            &rpc_request_id,
            &connection.session.native.thread_id,
            responses::FIRST_PROMPT,
        )
        .await
        .unwrap();
    f.store
        .bind_native_turn(reserved.id, &f.lease.fence, &turn.id)
        .await
        .unwrap();
    // Consume and deliberately lose the real usage event before the driver sees it.
    let usage = responses::completed(
        &mut connection.client,
        &connection.session.native.thread_id,
        &turn.id,
    )
    .await;
    assert_eq!(usage.total, 12);
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    assert_eq!(
        connection
            .drive_turn(
                &f.store,
                f.data.objects.clone(),
                f.lease.run.id,
                &f.lease.fence,
                &shutdown
            )
            .await
            .unwrap(),
        TurnProgress::Unresolved
    );
    assert!(
        f.store
            .mission_turn_checkpoint(f.lease.run.id, &f.lease.fence)
            .await
            .unwrap()
            .latest
            .unwrap()
            .terminal
            .is_none(),
        "a reconstructed list status alone cannot prove a terminal"
    );
    // The helper above did observe the real completed notification. Persist that
    // exact fact, but deliberately withhold its usage to exercise later recovery.
    f.store
        .observe_mission_turn_terminal(
            reserved.id,
            &f.lease.fence,
            TurnOutcome::Succeeded,
            "NATIVE_TURN_COMPLETED",
        )
        .await
        .unwrap();
    let mut first = None;
    for _ in 0..2 {
        assert_eq!(
            connection
                .drive_turn(
                    &f.store,
                    f.data.objects.clone(),
                    f.lease.run.id,
                    &f.lease.fence,
                    &shutdown
                )
                .await
                .unwrap(),
            TurnProgress::Unresolved
        );
        let checkpoint = f
            .store
            .mission_turn_checkpoint(f.lease.run.id, &f.lease.fence)
            .await
            .unwrap();
        assert_eq!(checkpoint.accounted_tokens.get(), 0);
        let latest = checkpoint.latest.unwrap();
        assert!(latest.receipt.is_none());
        let terminal = latest.terminal.unwrap();
        assert_eq!(terminal.outcome, TurnOutcome::Succeeded);
        assert_eq!(terminal.native_turn_id.as_deref(), Some(turn.id.as_str()));
        if let Some(first) = &first {
            assert_eq!(first, &terminal);
        } else {
            first = Some(terminal);
        }
    }
    assert_eq!(f.provider.request_count(), 1);
    connection.client.close().await.unwrap();
    let credentials: i64 = sqlx::query_scalar("SELECT count(*) FROM app.machine_credentials")
        .fetch_one(&pool)
        .await
        .unwrap();
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    f.store
        .cancel_run(
            &f.actor,
            "cancel-before-native-recovery",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    visible(&f, &pool).await;
    daemon(&f)
        .with_missions(f.launcher.clone())
        .process_mission_message(f.message.clone(), "cancelled-original-thread", shutdown)
        .await
        .unwrap();
    assert_eq!(
        f.provider.request_count(),
        1,
        "recovery cannot send another model turn"
    );
    let recovered: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.machine_credentials),(SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.model_turn_receipts WHERE reservation_id=$1),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2)")
        .bind(reserved.id.as_uuid()).bind(f.message.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(recovered, (credentials, 1, 0, 0));
    assert_eq!(
        f.store.get_run(&f.actor, run.id).await.unwrap().state,
        RunState::CancelRequested
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn unpriced_native_driver_refuses_cost_capped_send_without_spending(pool: PgPool) {
    let f = fixture_with_cost(&pool, true).await;
    let mut connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    prepare(
        &f,
        &f.lease,
        "unknown-cost",
        responses::FIRST_PROMPT,
        f.lease.run.deadline_at,
    )
    .await;
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    assert!(matches!(
        connection
            .drive_turn(
                &f.store,
                f.data.objects.clone(),
                f.lease.run.id,
                &f.lease.fence,
                &shutdown
            )
            .await,
        Err(server::worker::WorkerFailure::Codex("COST_UNAVAILABLE", _))
    ));
    assert_eq!(f.provider.request_count(), 0);
    let latest = f
        .store
        .mission_turn_checkpoint(f.lease.run.id, &f.lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap();
    assert!(!latest.sent && latest.receipt.is_none() && latest.terminal.is_none());
    connection.client.close().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn partial_usage_before_failed_tool_continuation_is_not_a_final_receipt(pool: PgPool) {
    let f = fixture(&pool).await;
    f.provider.fail_after_tool();
    let mut connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    prepare(
        &f,
        &f.lease,
        "partial-usage",
        responses::FIRST_PROMPT,
        f.lease.run.deadline_at,
    )
    .await;
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    let result = connection
        .drive_turn(
            &f.store,
            f.data.objects.clone(),
            f.lease.run.id,
            &f.lease.fence,
            &shutdown,
        )
        .await
        .unwrap();
    connection.client.close().await.unwrap();
    assert_eq!(
        f.provider.request_count(),
        2,
        "the native tool continuation really reached upstream"
    );
    assert_eq!(result, TurnProgress::Unresolved);
    let checkpoint = f
        .store
        .mission_turn_checkpoint(f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    assert_eq!(checkpoint.accounted_tokens.get(), 0);
    let latest = checkpoint.latest.unwrap();
    assert!(latest.sent && latest.receipt.is_none());
    assert_eq!(latest.terminal.unwrap().outcome, TurnOutcome::Failed);
}

async fn token_limit(pool: PgPool, failed_before_driver: bool) {
    let f = fixture(&pool).await;
    f.provider.exceed_tokens_before_tool();
    if failed_before_driver {
        f.provider.fail_after_tool();
    }
    let mut connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    let reserved = prepare(
        &f,
        &f.lease,
        "token-limit",
        responses::FIRST_PROMPT,
        f.lease.run.deadline_at,
    )
    .await;
    if failed_before_driver {
        let DispatchDecision::Send { rpc_request_id } = f
            .store
            .claim_turn_dispatch(reserved.id, &f.lease.fence)
            .await
            .unwrap()
        else {
            panic!("unique send permit required");
        };
        let thread = connection.session.native.thread_id.clone();
        let turn = connection
            .client
            .start_turn(&rpc_request_id, &thread, responses::FIRST_PROMPT)
            .await
            .unwrap();
        f.store
            .bind_native_turn(reserved.id, &f.lease.fence, &turn.id)
            .await
            .unwrap();
        // Query only native identities/status (itemsView=notLoaded). The wire
        // buffers its public usage notification while these RPCs are pending;
        // no driver is running yet, no hidden history or messages are read.
        let mut read_states = std::collections::BTreeSet::new();
        let ready = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                if f.provider.request_count() < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
                // A native list RPC may still be unavailable just after start.
                // Retry only this bounded read; never infer absence or resend.
                let turns = match connection.client.turns(&thread).await {
                    Ok(turns) => turns,
                    Err(server::codex_native::NativeFailure::Rejected(-32603)) => {
                        read_states.insert("REJECTED_-32603".to_owned());
                        Vec::new()
                    }
                    Err(error) => panic!("native status query failed: {error:?}"),
                };
                for actual in turns.iter().filter(|actual| actual.id == turn.id) {
                    read_states.insert(format!("{:?}", actual.status));
                }
                if let Some(actual) = turns
                    .iter()
                    .find(|actual| actual.id == turn.id && actual.status.terminal())
                {
                    // The live snapshot can report Failed, while reconstructed
                    // history can report Completed for this same failed Turn.
                    // Neither projection replaces its canonical notification.
                    assert!(matches!(
                        actual.status,
                        server::codex_native::TurnStatus::Completed
                            | server::codex_native::TurnStatus::Failed
                    ));
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        })
        .await;
        assert!(
            ready.is_ok(),
            "native status timeout: requests={}, read_states={read_states:?}",
            f.provider.request_count()
        );
        assert_eq!(f.provider.request_count(), 2);
        let checkpoint = f
            .store
            .mission_turn_checkpoint(f.lease.run.id, &f.lease.fence)
            .await
            .unwrap();
        assert!(checkpoint.latest.unwrap().terminal.is_none());
    }
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    let result = connection
        .drive_turn(
            &f.store,
            f.data.objects.clone(),
            f.lease.run.id,
            &f.lease.fence,
            &shutdown,
        )
        .await;
    connection.client.close().await.unwrap();
    assert_eq!(result.unwrap(), TurnProgress::Unresolved);
    // Native usage notification and tool continuation are asynchronous. The
    // second request can already be in flight; interruption cannot undo it.
    assert!((1..=2).contains(&f.provider.request_count()));
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    assert_eq!(run.state, RunState::CancelRequested);
    let events = f
        .store
        .run_events(&f.actor, run.id, DbCounter::ZERO, 100)
        .await
        .unwrap();
    let event = events
        .events
        .iter()
        .find(|event| event.event_type == "mission.token_limit")
        .unwrap();
    assert_eq!(
        event.payload,
        serde_json::json!({"schema_version":1,"reservation_id":reserved.id,"observed_tokens":"120","reserved_tokens":"100"})
    );
    let checkpoint = f
        .store
        .mission_turn_checkpoint(run.id, &f.lease.fence)
        .await
        .unwrap();
    assert_eq!(checkpoint.accounted_tokens, DbCounter::ZERO);
    let latest = checkpoint.latest.unwrap();
    assert!(latest.receipt.is_none());
    let terminal = latest.terminal.unwrap();
    if failed_before_driver {
        assert_eq!(terminal.outcome, TurnOutcome::Failed);
        assert!(terminal.observed_at <= event.occurred_at);
    } else {
        assert_eq!(terminal.outcome, TurnOutcome::Cancelled);
        assert!(terminal.observed_at >= run.cancellation_requested_at.unwrap());
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_token_limit_commits_stop_before_interrupt_and_keeps_partial_usage_unsettled(
    pool: PgPool,
) {
    token_limit(pool, false).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_native_turns_late_partial_usage_still_closes_spending(pool: PgPool) {
    token_limit(pool, true).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_interrupt_follows_committed_deadline_and_does_not_invent_usage(pool: PgPool) {
    let f = fixture(&pool).await;
    let mut connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    f.provider.slow_response();
    let deadline = sqlx::query_scalar("SELECT clock_timestamp()+interval '2 seconds'")
        .fetch_one(&pool)
        .await
        .unwrap();
    prepare(&f, &f.lease, "interrupt", responses::FIRST_PROMPT, deadline).await;
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    let result = connection
        .drive_turn(
            &f.store,
            f.data.objects.clone(),
            f.lease.run.id,
            &f.lease.fence,
            &shutdown,
        )
        .await
        .unwrap();
    assert_eq!(result, TurnProgress::Unresolved);
    let run = f.store.get_run(&f.actor, f.lease.run.id).await.unwrap();
    assert_eq!(run.state, RunState::CancelRequested);
    let intent = run.cancellation_requested_at.unwrap();
    assert!(intent >= deadline);
    let checkpoint = f
        .store
        .mission_turn_checkpoint(run.id, &f.lease.fence)
        .await
        .unwrap();
    assert_eq!(checkpoint.accounted_tokens.get(), 0);
    let latest = checkpoint.latest.unwrap();
    assert!(latest.sent && latest.native_turn_id.is_some() && latest.receipt.is_none());
    let terminal = latest.terminal.unwrap();
    assert_eq!(terminal.outcome, TurnOutcome::Cancelled);
    assert!(terminal.observed_at >= intent);
    // The deadline may interrupt native startup before its first upstream call.
    assert!(f.provider.request_count() <= 1);
    connection.client.close().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn bootstrap_mints_one_credential_binds_before_turn_and_resumes_the_original_native_thread(
    pool: PgPool,
) {
    let f = fixture(&pool).await;
    let mut connection = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    let session = connection.session.clone();
    assert_eq!(session.native.effective.model, "gpt-5.4");
    assert_eq!(session.native.effective.provider, "local_fixture");
    assert!(session.requested_settings.model.is_none());
    assert!(session.requested_settings.reasoning_effort.is_none());
    assert!(session.requested_settings.service_tier.is_none());
    assert_eq!(f.provider.request_count(), 0);
    assert_eq!(
        f.store
            .get_run(&f.actor, f.lease.run.id)
            .await
            .unwrap()
            .state,
        RunState::Dispatching
    );
    let work = f
        .root
        .path()
        .join("workspaces")
        .join(f.lease.run.id.to_string());
    assert!(work.join(".git").is_dir());
    assert!(!work.join("auth.json").exists());
    fs::write(work.join("original.txt"), "original Mission workspace").unwrap();
    assert!(f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .is_err());
    turn(
        &f,
        &f.lease,
        &mut connection,
        "first",
        responses::FIRST_PROMPT,
        0,
    )
    .await;
    connection.client.close().await.unwrap();
    let next = takeover(&f, &pool, "bootstrap-takeover").await;
    let mut recovered = f
        .launcher
        .open(&f.store, f.vault.clone(), next.run.id, &next.fence)
        .await
        .unwrap();
    assert_eq!(session, recovered.session);
    assert_eq!(
        fs::read_to_string(work.join("original.txt")).unwrap(),
        "original Mission workspace"
    );
    turn(
        &f,
        &next,
        &mut recovered,
        "second",
        responses::SECOND_PROMPT,
        12,
    )
    .await;
    recovered.client.close().await.unwrap();
    assert_eq!(f.provider.request_count(), 2);
    assert!(f.provider.saw_previous_context());
    let facts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.machine_principals WHERE kind='MISSION'),(SELECT count(*) FROM app.machine_credentials WHERE issued_by='MISSION_SERVICE'),(SELECT sum(actual_tokens)::bigint FROM app.model_turn_receipts)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 2, 24));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_start_never_creates_a_replacement_session(pool: PgPool) {
    let f = fixture(&pool).await;
    assert!(f
        .store
        .begin_run_dispatch(f.lease.run.id, &f.lease.fence)
        .await
        .unwrap());
    let next = takeover(&f, &pool, "unknown-start-owner").await;
    assert!(f
        .launcher
        .open(&f.store, f.vault.clone(), next.run.id, &next.fence)
        .await
        .is_err());
    assert_eq!(f.provider.request_count(), 0);
    let facts: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.machine_credentials WHERE issued_by='MISSION_SERVICE')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0));
    assert!(!f
        .root
        .path()
        .join("workspaces")
        .join(next.run.id.to_string())
        .exists());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_passive_thread_without_native_rollout_is_not_rebuilt_on_takeover(pool: PgPool) {
    let f = fixture(&pool).await;
    let first = f
        .launcher
        .open(&f.store, f.vault.clone(), f.lease.run.id, &f.lease.fence)
        .await
        .unwrap();
    let original = first.session;
    first.client.close().await.unwrap();
    let next = takeover(&f, &pool, "empty-thread-owner").await;
    assert!(f
        .launcher
        .open(&f.store, f.vault.clone(), next.run.id, &next.fence)
        .await
        .is_err());
    let actual = f
        .store
        .mission_job(next.run.id, &next.fence)
        .await
        .unwrap()
        .session
        .unwrap();
    assert_eq!(actual, original);
    assert_eq!(f.provider.request_count(), 0);
}
