//! Native CLI/TCP/Axum/Worker/PGMQ closure with controlled compiler output.
//! The separate Runtime suite executes Wasm and Nautilus; this tests admission,
//! producer identity, replay and cancellation, without claiming science success.
#[path = "support/client.rs"]
mod client;
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "../../../tests/support/external_experiments.rs"]
mod external_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
mod support;
use axum::http::StatusCode;
use contracts::{
    control::CommandResult,
    experiments::{ExperimentOutcome, ExperimentView},
    lifecycle::RunCancelV1,
    runs::RunSnapshotV1,
    SchemaV1,
};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};

#[sqlx::test(migrations = "../../migrations")]
async fn cli_compilation_success_is_not_evaluation_completion_and_worker_replay_preserves_chain(
    pool: PgPool,
) {
    cli_worker_trace(pool, false).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn cli_reads_original_successful_native_report_larger_than_one_mib(pool: PgPool) {
    cli_worker_trace(pool, true).await;
}

async fn cli_worker_trace(pool: PgPool, success: bool) {
    let native_profiles = external_support::unused_native_profiles(&pool).await;
    let f = support::fixture_with_runtime_targets(
        pool.clone(),
        Some(server::runtime_transport::RuntimeTargets::default()),
    )
    .await;
    let cookie = support::local_session(&f).await.cookie.unwrap();
    let objects = Arc::new(ArtifactStore::open(&f._state.path().join("artifacts")).unwrap());
    let (store, actor, data, experiment, mut intent) =
        external_support::setup_with_objects(&pool, objects).await;
    if success {
        intent.request.evaluation_limits.output_bytes =
            Some(contracts::DbCounter::new(4 * 1024 * 1024).unwrap());
    }
    let principal=client::browser(&f,&cookie,"external-cli-principal","/api/v2/machine-principals",json!({"schema_version":1,"name":"External science owner CLI","kind":"CLI","project_id":experiment.project_id,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential=client::browser(&f,&cookie,"external-cli-credential",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["RESEARCH_READ","RUN_READ","RUN_CANCEL"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token_file = f._state.path().join("external-owner-token");
    fs::write(&token_file, credential.body["token"].as_str().unwrap()).unwrap();
    fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();
    let (origin, _listener) = client::listen(&f).await;
    let id = experiment.id.to_string();
    let denied = client::invoke(
        &origin,
        &token_file,
        &[
            "--idempotency-key",
            "external-command",
            "experiment",
            "evaluate",
            &id,
        ],
        serde_json::to_value(&intent.request).unwrap(),
    )
    .await;
    assert!(!denied.status.success());
    let grant=client::invoke(&origin,&token_file,&["--idempotency-key","external-grant","operator-grant"],json!({"schema_version":1,"command":{"operation":"EXPERIMENT_EVALUATE","request":intent},"target_id":experiment.id})).await;
    assert!(
        grant.status.success(),
        "{}",
        String::from_utf8_lossy(&grant.stderr)
    );
    let grant: Value = serde_json::from_slice(&grant.stdout).unwrap();
    let grant = grant["resource"]["id"].as_str().unwrap();
    let args = [
        "--idempotency-key",
        "external-command",
        "--operator-grant",
        grant,
        "experiment",
        "evaluate",
        &id,
    ];
    let first = client::invoke(
        &origin,
        &token_file,
        &args,
        serde_json::to_value(&intent.request).unwrap(),
    )
    .await;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let admitted: CommandResult<RunSnapshotV1> = serde_json::from_slice(&first.stdout).unwrap();
    let replay = client::invoke(
        &origin,
        &token_file,
        &args,
        serde_json::to_value(&intent.request).unwrap(),
    )
    .await;
    assert!(replay.status.success());
    let replay: CommandResult<RunSnapshotV1> = serde_json::from_slice(&replay.stdout).unwrap();
    assert!(replay.replayed);
    assert_eq!(admitted.resource.id, replay.resource.id);
    experiment_support::complete_compilation(&pool, &store, &data, admitted.resource.id).await;
    let not_done = client::invoke(
        &origin,
        &token_file,
        &["experiment", "result", &id],
        Value::Null,
    )
    .await;
    assert!(!not_done.status.success());
    let worker = server::worker::Worker::new(
        store.clone(),
        SecretVault::open(
            &f._state.path().join("secrets"),
            &f._state.path().join("master.key"),
        )
        .unwrap(),
        ArtifactStore::open(&f._state.path().join("artifacts")).unwrap(),
        server::runtime_transport::RuntimeTargets::default(),
        1,
    )
    .unwrap();
    let (_shutdown, shutdown) = tokio::sync::watch::channel(false);
    let compiler_message = store::lifecycle::RunMessage {
        message_id: sqlx::query_scalar(
            "SELECT initial_queue_message_id FROM app.run_admissions WHERE run_id=$1",
        )
        .bind(admitted.resource.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        run_id: admitted.resource.id,
        read_count: 1,
    };
    worker
        .process_message(
            compiler_message,
            "external-compile-recovery",
            shutdown.clone(),
        )
        .await
        .unwrap();
    let follow = client::invoke(
        &origin,
        &token_file,
        &["experiment", "show", &id],
        Value::Null,
    )
    .await;
    assert!(follow.status.success());
    let follow: ExperimentView = serde_json::from_slice(&follow.stdout).unwrap();
    let evaluation = follow.run_id.unwrap();
    assert_ne!(evaluation, admitted.resource.id);
    assert_eq!(follow.outcome, Some(ExperimentOutcome::Pending));
    let not_done = client::invoke(
        &origin,
        &token_file,
        &["experiment", "result", &id],
        Value::Null,
    )
    .await;
    assert!(!not_done.status.success());
    let report_id = if success {
        Some(
            experiment_support::complete_external_evaluation(
                &pool,
                &store,
                &data,
                evaluation,
                Some(1024 * 1024 + 4096),
            )
            .await,
        )
    } else {
        let run = store.get_run(&actor, evaluation).await.unwrap();
        store
            .cancel_run(
                &actor,
                "cancel-evaluation",
                evaluation,
                &RunCancelV1 {
                    schema_version: SchemaV1,
                    expected_revision: run.revision,
                },
            )
            .await
            .unwrap();
        None
    };
    let message = store::lifecycle::RunMessage {
        message_id: sqlx::query_scalar(
            "SELECT initial_queue_message_id FROM app.run_admissions WHERE run_id=$1",
        )
        .bind(evaluation.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        run_id: evaluation,
        read_count: 1,
    };
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(store::StoreError::Conflict)
    ));
    worker
        .process_message(message, "external-evaluation-recovery", shutdown)
        .await
        .unwrap();
    let finished = client::invoke(
        &origin,
        &token_file,
        &["experiment", "show", &id],
        Value::Null,
    )
    .await;
    assert!(finished.status.success());
    let finished: ExperimentView = serde_json::from_slice(&finished.stdout).unwrap();
    assert_eq!(finished.run_id, Some(evaluation));
    assert_eq!(finished.outcome, Some(ExperimentOutcome::Inconclusive));
    assert_eq!(
        finished.outcome_reason.as_deref(),
        Some(if success {
            "EXTERNAL_EVALUATION_COMPLETE"
        } else {
            "EXTERNAL_EVALUATION_CANCELLED"
        })
    );
    assert_eq!(finished.conclusion_artifact_id, report_id);
    if let Some(report_id) = report_id {
        let metadata = store.artifact(&actor, report_id).await.unwrap();
        assert!(metadata.byte_count.get() > 1024 * 1024);
        let original: Value =
            serde_json::from_slice(&data.read(report_id, metadata.byte_count).await.unwrap())
                .unwrap();
        let output = client::invoke(
            &origin,
            &token_file,
            &["experiment", "result", &id],
            Value::Null,
        )
        .await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.len() > 1024 * 1024);
        let report: contracts::science::NativeExperimentEvaluationResultV1 =
            serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report.dataset_revision_id, data.data.validation);
        assert_eq!(serde_json::to_value(&report).unwrap(), original);
        let summary_output = client::invoke(
            &origin,
            &token_file,
            &["experiment", "summary", &id],
            Value::Null,
        )
        .await;
        assert!(
            summary_output.status.success(),
            "{}",
            String::from_utf8_lossy(&summary_output.stderr)
        );
        assert!(summary_output.stdout.len() < 1024 * 1024);
        let summary: contracts::experiment_summary::ExperimentSummaryV1 =
            serde_json::from_slice(&summary_output.stdout).unwrap();
        assert_eq!(summary.experiment_id, experiment.id);
        assert_eq!(summary.run_id, evaluation);
        assert_eq!(summary.report_artifact_id, report_id);
        assert_eq!(summary.dataset_revision_id, report.dataset_revision_id);
        assert_eq!(summary.model_artifact_id, report.model_artifact_id);
        assert_eq!(summary.folds.len(), report.folds.len());
        assert!(summary
            .folds
            .iter()
            .all(|f| f.equity_preview.points.len() <= 64));
        external_support::advance(&store, &data, evaluation)
            .await
            .unwrap();
        let again = store.experiment(&actor, experiment.id).await.unwrap();
        assert_eq!(again.conclusion_artifact_id, Some(report_id));
        let repeated = client::invoke(
            &origin,
            &token_file,
            &["experiment", "result", &id],
            Value::Null,
        )
        .await;
        assert!(repeated.status.success());
        assert_eq!(repeated.stdout, output.stdout);
    }
    if report_id.is_none() {
        let unavailable = client::invoke(
            &origin,
            &token_file,
            &["experiment", "summary", &id],
            Value::Null,
        )
        .await;
        assert!(!unavailable.status.success());
    }
    let cycle = store.cycle(&actor, experiment.cycle_id).await.unwrap();
    let close = contracts::cycles::CycleFinishExternalIntent {
        schema_version: SchemaV1,
        cycle_id: cycle.id,
        request: contracts::cycles::CycleFinishExternalV1 {
            schema_version: SchemaV1,
            expected_revision: cycle.revision,
        },
    };
    let grant = client::invoke(&origin,&token_file,&["--idempotency-key","external-close-grant","operator-grant"],json!({"schema_version":1,"command":{"operation":"CYCLE_FINISH_EXTERNAL","request":close},"target_id":cycle.id})).await;
    assert!(
        grant.status.success(),
        "{}",
        String::from_utf8_lossy(&grant.stderr)
    );
    let grant: Value = serde_json::from_slice(&grant.stdout).unwrap();
    let grant = grant["resource"]["id"].as_str().unwrap();
    let cycle_id = cycle.id.to_string();
    let closed = client::invoke(
        &origin,
        &token_file,
        &[
            "--idempotency-key",
            "external-close",
            "--operator-grant",
            grant,
            "cycle",
            "finish-external",
            &cycle_id,
        ],
        serde_json::to_value(&close.request).unwrap(),
    )
    .await;
    assert!(
        closed.status.success(),
        "{}",
        String::from_utf8_lossy(&closed.stderr)
    );
    let closed: CommandResult<contracts::cycles::CycleViewV1> =
        serde_json::from_slice(&closed.stdout).unwrap();
    assert_eq!(
        closed.resource.state,
        contracts::cycles::CycleState::Completed
    );
    assert!(closed.resource.next_action.is_none());
    if let Some(report_id) = report_id {
        use contracts::strategy_portfolio::{
            AlphaVersionEnvelopeV2, StrategyAlphaAdoptIntentV1, StrategyAlphaAdoptV1,
            StrategyAlphaVersionV1,
        };
        let adopt = StrategyAlphaAdoptIntentV1 {
            schema_version: SchemaV1,
            experiment_id: experiment.id,
            request: StrategyAlphaAdoptV1 {
                schema_version: SchemaV1,
                expected_revision: finished.revision,
                name: "CLI accepted weight strategy".into(),
                source_fold_index: 0,
            },
        };
        let body = serde_json::to_value(&adopt.request).unwrap();
        // Authenticated owner browsers and owner devices share operator
        // authority; the scoped machine CLI below still requires its grant.
        let browser_adopted = client::browser(
            &f,
            &cookie,
            "browser-adopt",
            &format!("/api/v2/experiments/{id}/adopt-alpha"),
            body.clone(),
        )
        .await;
        assert_eq!(browser_adopted.status, StatusCode::CREATED);
        let browser_adopted: CommandResult<StrategyAlphaVersionV1> =
            serde_json::from_value(browser_adopted.body).unwrap();
        assert!(!browser_adopted.replayed);
        assert_eq!(
            browser_adopted.resource.policy.source.report_artifact_id,
            report_id
        );
        assert_eq!(
            browser_adopted.resource.policy.source.evaluation_run_id,
            evaluation
        );
        let browser_replay = client::browser(
            &f,
            &cookie,
            "browser-adopt",
            &format!("/api/v2/experiments/{id}/adopt-alpha"),
            body.clone(),
        )
        .await;
        assert_eq!(browser_replay.status, StatusCode::CREATED);
        let browser_replay: CommandResult<StrategyAlphaVersionV1> =
            serde_json::from_value(browser_replay.body).unwrap();
        assert!(browser_replay.replayed);
        assert_eq!(
            serde_json::to_value(&browser_replay.resource).unwrap(),
            serde_json::to_value(&browser_adopted.resource).unwrap()
        );
        let denied = client::invoke(
            &origin,
            &token_file,
            &[
                "--idempotency-key",
                "alpha-without-grant",
                "experiment",
                "adopt-alpha",
                &id,
            ],
            body.clone(),
        )
        .await;
        assert!(!denied.status.success());
        let grant = client::invoke(&origin, &token_file, &["--idempotency-key", "alpha-grant", "operator-grant"],
            json!({"schema_version":1,"command":{"operation":"EXPERIMENT_ADOPT_ALPHA","request":adopt},"target_id":experiment.id})).await;
        assert!(
            grant.status.success(),
            "{}",
            String::from_utf8_lossy(&grant.stderr)
        );
        let grant: Value = serde_json::from_slice(&grant.stdout).unwrap();
        let args = [
            "--idempotency-key",
            "alpha-command",
            "--operator-grant",
            grant["resource"]["id"].as_str().unwrap(),
            "experiment",
            "adopt-alpha",
            &id,
        ];
        let adopted = client::invoke(&origin, &token_file, &args, body.clone()).await;
        assert!(
            adopted.status.success(),
            "{}",
            String::from_utf8_lossy(&adopted.stderr)
        );
        let adopted: CommandResult<StrategyAlphaVersionV1> =
            serde_json::from_slice(&adopted.stdout).unwrap();
        assert!(adopted.replayed);
        assert_eq!(
            serde_json::to_value(&adopted.resource).unwrap(),
            serde_json::to_value(&browser_adopted.resource).unwrap()
        );
        assert_eq!(adopted.resource.policy.source.report_artifact_id, report_id);
        assert_eq!(adopted.resource.policy.source.evaluation_run_id, evaluation);
        let replay = client::invoke(&origin, &token_file, &args, body).await;
        assert!(
            replay.status.success(),
            "{}",
            String::from_utf8_lossy(&replay.stderr)
        );
        let replay: CommandResult<StrategyAlphaVersionV1> =
            serde_json::from_slice(&replay.stdout).unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.resource.id, adopted.resource.id);
        let parent_alpha = adopted.resource.alpha_id.to_string();
        let view = client::invoke(
            &origin,
            &token_file,
            &["alpha", "show", &parent_alpha, "1"],
            Value::Null,
        )
        .await;
        assert!(
            view.status.success(),
            "{}",
            String::from_utf8_lossy(&view.stderr)
        );
        let view: AlphaVersionEnvelopeV2 = serde_json::from_slice(&view.stdout).unwrap();
        assert_eq!(
            serde_json::to_value(view).unwrap(),
            serde_json::to_value(&adopted.resource).unwrap()
        );
        let original_after = store.experiment(&actor, experiment.id).await.unwrap();
        assert_eq!(
            original_after.outcome,
            Some(ExperimentOutcome::Inconclusive)
        );
        assert_eq!(original_after.conclusion_artifact_id, Some(report_id));
        let versions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app.alpha_versions WHERE experiment_id=$1")
                .bind(experiment.id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(versions, 1);
    }
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM app.external_experiment_tasks)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 1));
    assert_eq!(
        external_support::unused_native_profiles(&pool).await,
        native_profiles
    );
}
