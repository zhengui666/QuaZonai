//! Real proposal/PGMQ/native-definition transactions. No compiler or science is mocked as run.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/experiments.rs"]
mod proposal_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{
    execution::NativeTaskParametersV1, lifecycle::JobLimitsV1, runtime_jobs::RuntimeInputV1,
    DbCounter, Id, SchemaV1,
};
use sqlx::{PgPool, Row};
use store::{
    lifecycle::{ClaimResult, RunLease},
    Store, StoreError,
};

#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
use experiment_support::{complete_compilation, setup};

#[path = "support/validation_publication.rs"]
mod validation_publication;

#[path = "support/cycle_selection.rs"]
mod cycle_selection;

#[path = "support/sealed_opportunities.rs"]
mod sealed_opportunities;

fn limits() -> JobLimitsV1 {
    JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 1,
        cpu_seconds: DbCounter::new(10).unwrap(),
        wall_seconds: 60,
        memory_mib: 1024,
        output_bytes: DbCounter::new(1024 * 1024).unwrap(),
    }
}

async fn trial_usage(pool: &PgPool, lease: &RunLease) -> (i64, i64) {
    sqlx::query_as(
        "SELECT reserved_experiments,used_experiments FROM app.research_cycles WHERE id=$1",
    )
    .bind(lease.run.cycle_id.unwrap().as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn result_turn(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
) -> Result<bool, StoreError> {
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    store
        .prepare_mission_result_turn(
            lease.run.id,
            &lease.fence,
            move |id, size| {
                let objects = reading.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            move |object| async move {
                writing
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_compiler_feedback_is_one_budgeted_repair_and_unknown_turns_do_not_continue(
    pool: PgPool,
) {
    use store::{
        lifecycle::{
            mission::NativeSessionReceipt, FailureClass, NativeOutcome, TerminalObservation,
        },
        turns::{TurnOutcome, UsageReceipt},
    };
    let (store, _actor, f, lease, experiment) = setup(&pool).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    let thread = Id::new().to_string();
    store
        .bind_mission_session(
            lease.run.id,
            &lease.fence,
            &NativeSessionReceipt {
                thread_id: thread.clone(),
                codex_version: "0.144.4".into(),
                protocol_schema_version: "v2".into(),
                requested_service_tier: None,
                effective: contracts::codex::CodexEffectiveSettingsV1 {
                    model: "controlled-native-model".into(),
                    provider: "controlled-native-provider".into(),
                    reasoning_effort: Some("medium".into()),
                    service_tier: None,
                },
            },
        )
        .await
        .unwrap();
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    store.prepare_initial_mission_turn(lease.run.id,&lease.fence,
        move |id,size| async move { reading.read(id,size).map_err(|_|StoreError::Integrity) },
        move |object| async move { writing.put(object.id,&object.bytes).map_err(|_|StoreError::Integrity) }).await.unwrap();
    let first = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap()
        .reservation;
    store
        .claim_turn_dispatch(first.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(first.id, &lease.fence, "controlled-initial")
        .await
        .unwrap();
    let compiler = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    let message = store
        .read_native_run_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == compiler)
        .unwrap();
    let Some(ClaimResult::Leased(compiling)) = store
        .claim_native_run(&message, "controlled-failed-compiler", 60)
        .await
        .unwrap()
    else {
        panic!("compiler lease required");
    };
    store
        .begin_run_dispatch(compiler, &compiling.fence)
        .await
        .unwrap();
    store
        .accept_run_terminal(
            compiler,
            &compiling.fence,
            &TerminalObservation {
                schema_version: SchemaV1,
                external_job_id: compiling.external_job_id.clone(),
                outcome: NativeOutcome::Failed,
                manifest_artifact_id: None,
                failure_class: Some(FailureClass::InvalidInput),
                failure_code: Some("MODEL_COMPILE_FAILED".into()),
                observed_at: sqlx::query_scalar("SELECT clock_timestamp()")
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        trial_usage(&pool, &lease).await,
        (0, 1),
        "failed compilation retains its trial"
    );
    assert_eq!(
        start(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .resource
            .id,
        compiler
    );
    assert_eq!(
        trial_usage(&pool, &lease).await,
        (0, 1),
        "replay cannot charge or refund"
    );
    assert!(
        !result_turn(&store, &f, &lease).await.unwrap(),
        "no receipt means no continuation"
    );
    store
        .settle_turn(
            first.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(12).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "controlled_completed".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .prepare_mission_result_turn(
                lease.run.id,
                &lease.fence,
                |_, _| async { panic!("failed compile must not read forecast or diagnostics") },
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1")
            .bind(lease.run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1, "publication failure rolls back reservation");
    let (a, b) = tokio::join!(
        result_turn(&store, &f, &lease),
        result_turn(&store, &f, &lease)
    );
    assert_ne!(a.unwrap(), b.unwrap());
    let latest = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap();
    assert_eq!(latest.reservation.ordinal, 2);
    assert_eq!(
        latest.reservation.turn_kind,
        domain::admission::TurnKind::Repair
    );
    let reading = f.objects.clone();
    let text=store.mission_turn_prompt(lease.run.id,&lease.fence,latest.reservation.id,
        move |id,size|async move {reading.read(id,size).map_err(|_|StoreError::Integrity)}).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
    assert_eq!(body["experiment_id"], experiment.to_string());
    assert_eq!(body["run_id"], compiler.to_string());
    assert_eq!(body["stage"], "COMPILATION");
    assert_eq!(body["execution_state"], "FAILED");
    assert_eq!(body["reason_code"], "RUNTIME_FAILED");
    assert_eq!(body["detailed_diagnostics_available"], false);
    assert!(body.get("forecast").is_none());
    assert!(!result_turn(&store, &f, &lease).await.unwrap());
    store
        .claim_turn_dispatch(latest.reservation.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(latest.reservation.id, &lease.fence, "controlled-feedback")
        .await
        .unwrap();
    store
        .settle_turn(
            latest.reservation.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(10).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "controlled_completed".into(),
            },
        )
        .await
        .unwrap();
    assert!(
        !result_turn(&store, &f, &lease).await.unwrap(),
        "same failure is delivered only once"
    );
    let facts:(i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_reservations WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_model_turns),(SELECT thread_id FROM app.codex_sessions WHERE run_id=$1)")
        .bind(lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (2, 2, thread));
    let mut stale = lease.clone();
    stale.fence.worker_owner_id = "not-owner".into();
    assert!(matches!(
        result_turn(&store, &f, &stale).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
}

async fn start(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let objects = f.objects.clone();
    store
        .start_experiment_compilation(
            lease.run.id,
            &lease.fence,
            experiment,
            &limits(),
            move |object| async move {
                objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn mission_cancellation_waits_for_admitted_compilation_but_not_future_forecast(pool: PgPool) {
    let (store, actor, f, lease, experiment) = setup(&pool).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    let compiler = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    let run = store.get_run(&actor, lease.run.id).await.unwrap();
    store
        .cancel_run(
            &actor,
            "cancel-parent",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    assert!(!store.complete_mission(run.id, &lease.fence).await.unwrap());
    assert_eq!(
        store.get_run(&actor, compiler).await.unwrap().state,
        contracts::runs::RunState::Queued
    );
    assert!(store
        .next_mission_experiment(run.id, &lease.fence)
        .await
        .unwrap()
        .is_none());
    complete_compilation(&pool, &store, &f, compiler).await;
    assert!(store.complete_mission(run.id, &lease.fence).await.unwrap());
    assert_eq!(
        store.get_run(&actor, run.id).await.unwrap().state,
        contracts::runs::RunState::Cancelled
    );
    assert_eq!(trial_usage(&pool, &lease).await, (0, 1));
    let untouched: (String,i64,i64) = sqlx::query_as("SELECT outcome,(SELECT count(*) FROM app.experiment_forecasts),(SELECT count(*) FROM app.model_turn_reservations) FROM app.experiments WHERE id=$1")
        .bind(experiment.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(untouched, ("PENDING".into(), 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn compilation_replay_retains_one_code_producer_and_never_mounts_market_data(pool: PgPool) {
    let (store, actor, f, lease, experiment) = setup(&pool).await;
    let before: i64 =
        sqlx::query_scalar("SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1")
            .bind(lease.run.cycle_id.unwrap().as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let (a, b) = tokio::join!(
        start(&store, &f, &lease, experiment),
        start(&store, &f, &lease, experiment)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert!(a.resource.deadline_at <= lease.run.deadline_at);
    let counts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.experiment_compilations WHERE experiment_id=$1),(SELECT count(*) FROM app.run_native_tasks WHERE run_id=$2),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$3),(SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$4)")
        .bind(experiment.as_uuid()).bind(a.resource.id.as_uuid()).bind(a.resource.id.to_string())
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1, before + 10));
    assert_eq!(trial_usage(&pool, &lease).await, (1, 0));
    let message = store
        .read_native_run_messages(60, 10)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == a.resource.id)
        .unwrap();
    let Some(ClaimResult::Leased(compiling)) = store
        .claim_native_run(&message, "native-compiler", 60)
        .await
        .unwrap()
    else {
        panic!("native compilation lease required")
    };
    let job = store
        .native_job(a.resource.id, &compiling.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.inputs.len(), 2);
    assert!(job
        .spec
        .inputs
        .iter()
        .all(|input| matches!(input, RuntimeInputV1::Artifact { .. })));
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &f.objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::CompileModel {
        code_artifact_id, ..
    } = task
    else {
        panic!("compile task required")
    };
    assert_eq!(
        store
            .experiment(&actor, experiment)
            .await
            .unwrap()
            .code_artifact_id,
        Some(code_artifact_id)
    );
    let changed = sqlx::query(
        "UPDATE app.experiments SET code_artifact_id=parameter_artifact_id WHERE id=$1",
    )
    .bind(experiment.as_uuid())
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        changed.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
    assert!(
        sqlx::query("DELETE FROM app.experiment_compilations WHERE experiment_id=$1")
            .bind(experiment.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(store
        .experiment(&actor, experiment)
        .await
        .unwrap()
        .run_id
        .is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn publication_and_fence_failures_leave_no_compilation_or_resource_charge(pool: PgPool) {
    let (store, _actor, f, lease, experiment) = setup(&pool).await;
    let before: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),(SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1)")
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(matches!(
        store
            .start_experiment_compilation(
                lease.run.id,
                &lease.fence,
                experiment,
                &limits(),
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let mut stale = lease.clone();
    stale.fence.owner_epoch = stale.fence.owner_epoch.next().unwrap();
    assert!(matches!(
        start(&store, &f, &stale, experiment).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    let mut invalid = limits();
    invalid.experiments = 0;
    assert!(matches!(
        store
            .start_experiment_compilation(
                lease.run.id,
                &lease.fence,
                experiment,
                &invalid,
                |_| async { panic!("invalid admission cannot publish") }
            )
            .await,
        Err(StoreError::Invalid("compilation_limits"))
    ));
    sqlx::raw_sql("CREATE FUNCTION public.reject_compilation_edge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected final compilation edge failure'; END $$; CREATE TRIGGER reject_edge BEFORE INSERT ON app.experiment_compilations FOR EACH ROW EXECUTE FUNCTION public.reject_compilation_edge();")
        .execute(&pool).await.unwrap();
    let mut published = None;
    let capture = &mut published;
    let objects = f.objects.clone();
    assert!(matches!(
        store
            .start_experiment_compilation(
                lease.run.id,
                &lease.fence,
                experiment,
                &limits(),
                move |object| async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)?;
                    *capture = Some((
                        object.id,
                        DbCounter::new(object.bytes.len() as u64).unwrap(),
                    ));
                    Ok(())
                }
            )
            .await,
        Err(StoreError::Database(_))
    ));
    let (id, size) =
        published.expect("native object publication preceded the injected edge failure");
    assert_eq!(f.objects.read(id, size).unwrap().len() as u64, size.get());
    sqlx::query("DROP TRIGGER reject_edge ON app.experiment_compilations")
        .execute(&pool)
        .await
        .unwrap();
    let after: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),(SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1)")
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    assert_eq!(trial_usage(&pool, &lease).await, (0, 0));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.experiment_compilations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        !start(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .replayed
    );
}

async fn forecast(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    let mut allocation = limits();
    allocation.experiments = 0;
    store
        .start_experiment_forecast(
            lease.run.id,
            &lease.fence,
            experiment,
            &allocation,
            move |id, size| {
                let objects = reading.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            move |object| async move {
                writing
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

// Controlled result bytes prove the real publication/producer transaction, not
// execution of rustc or Wasmi. Their actual execution has separate native tests.

async fn validation(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let mut allocation = limits();
    allocation.experiments = 0;
    store
        .start_experiment_validation(
            lease.run.id,
            &lease.fence,
            experiment,
            &allocation,
            |id, size| f.read(id, size),
            |object| async move {
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn formal_validation_keeps_the_original_trial_model_policy_and_complete_input(pool: PgPool) {
    let (store, actor, f, lease, experiment) = setup(&pool).await;
    let compilation = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    let model = complete_compilation(&pool, &store, &f, compilation).await;
    assert!(matches!(
        validation(&store, &f, &lease, experiment).await,
        Err(StoreError::Invalid("original_research_alpha_required"))
    ));
    let mut changed = experiment_support::capabilities(chrono::Utc::now());
    changed
        .image_refs
        .iter_mut()
        .find(|i| i.job_kind == contracts::runs::RunKind::AlphaEvaluate)
        .unwrap()
        .image_ref = format!("sha256:{}", "b".repeat(64));
    experiment_support::probe_capabilities(&store, &actor, &f, changed).await;
    assert!(matches!(
        forecast(&store, &f, &lease, experiment).await,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("frozen_alpha_image_changed")
        ))
    ));
    experiment_support::probe(&store, &actor, &f).await;
    let predicted = forecast(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_forecast(&pool, &store, &f, predicted).await;
    let alpha = store
        .prepare_research_alpha(lease.run.id, &lease.fence, experiment)
        .await
        .unwrap()
        .resource;
    let mut changed = experiment_support::capabilities(chrono::Utc::now());
    changed
        .engine_versions
        .insert("solow-cv".into(), "unrecognized".into());
    experiment_support::probe_capabilities(&store, &actor, &f, changed).await;
    assert!(matches!(
        validation(&store, &f, &lease, experiment).await,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("native_alpha_validation")
        ))
    ));
    experiment_support::probe(&store, &actor, &f).await;
    let before: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),reserved_cpu_seconds FROM app.research_cycles WHERE id=$1")
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    let mut allocation = limits();
    allocation.experiments = 0;
    assert!(store
        .start_experiment_validation(
            lease.run.id,
            &lease.fence,
            experiment,
            &allocation,
            |id, size| f.read(id, size),
            |_| async { Err(StoreError::Integrity) }
        )
        .await
        .is_err());
    let after: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),reserved_cpu_seconds FROM app.research_cycles WHERE id=$1")
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        before, after,
        "failed publication must not admit a partial stage"
    );
    let (a, b) = tokio::join!(
        validation(&store, &f, &lease, experiment),
        validation(&store, &f, &lease, experiment)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        a.resource.input_set_id,
        f.freeze.execution_context.validation_input_set_id
    );
    assert_eq!(trial_usage(&pool, &lease).await, (0, 1));
    let reserved_cpu: i64 =
        sqlx::query_scalar("SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1")
            .bind(lease.run.cycle_id.unwrap().as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        reserved_cpu,
        before.1 + 10,
        "concurrent validation reserves CPU exactly once"
    );
    let row = sqlx::query("SELECT v.alpha_version_id,v.policy_id,v.dataset_revision_id,t.access_class,a.access_class AS parameters_access,(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=v.run_id::text) AS queued FROM app.experiment_validations v JOIN app.run_native_tasks t ON t.run_id=v.run_id JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE v.experiment_id=$1")
        .bind(experiment.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        row.get::<uuid::Uuid, _>("alpha_version_id"),
        alpha.as_uuid()
    );
    assert_eq!(
        row.get::<uuid::Uuid, _>("policy_id"),
        f.brief.content.evaluation_policy_id.as_uuid()
    );
    assert_eq!(
        row.get::<uuid::Uuid, _>("dataset_revision_id"),
        f.data.validation.as_uuid()
    );
    assert_eq!(row.get::<String, _>("access_class"), "EVALUATOR_ONLY");
    assert_eq!(row.get::<String, _>("parameters_access"), "EVALUATOR_ONLY");
    assert_eq!(row.get::<i64, _>("queued"), 1);
    assert_eq!(
        store.experiment(&actor, experiment).await.unwrap().run_id,
        Some(predicted)
    );
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == a.resource.id)
        .unwrap();
    let Some(ClaimResult::Leased(native)) = store
        .claim_native_run(&message, "validation", 60)
        .await
        .unwrap()
    else {
        panic!("native validation lease required")
    };
    let job = store
        .native_job(a.resource.id, &native.fence)
        .await
        .unwrap();
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|i| match i {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let task: NativeTaskParametersV1 =
        serde_json::from_slice(&f.read(job.spec.parameters_artifact_id, size).await.unwrap())
            .unwrap();
    domain::execution::task(&job.spec, &task).unwrap();
    let NativeTaskParametersV1::ValidateAlpha {
        dataset_revision_id,
        model_artifact_id,
        request,
        ..
    } = task
    else {
        panic!("formal validation required")
    };
    assert_eq!(dataset_revision_id, f.data.validation);
    assert_eq!(model_artifact_id, model);
    assert_eq!(request.forecast.parameters.fast_period, 2);
    assert_eq!(request.forecast.parameters.slow_period, 5);
    assert_eq!(request.target_kind, f.brief.content.target_kind);
    let split: serde_json::Value =
        sqlx::query_scalar("SELECT split_policy FROM app.evaluation_policies WHERE id=$1")
            .bind(f.brief.content.evaluation_policy_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(serde_json::to_value(request.split_policy).unwrap(), split);
    for sql in [
        "UPDATE app.experiment_validations SET run_id=run_id WHERE experiment_id=$1",
        "DELETE FROM app.experiment_validations WHERE experiment_id=$1",
    ] {
        assert!(sqlx::query(sql)
            .bind(experiment.as_uuid())
            .execute(&pool)
            .await
            .is_err());
    }
    let published: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM app.evaluations)+(SELECT count(*) FROM app.qualifications)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        published, 0,
        "task admission is not a published evaluation or qualification"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn research_alpha_uses_the_original_forecast_once_without_qualification(pool: PgPool) {
    let (store, _, f, lease, experiment) = setup(&pool).await;
    let compilation = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    let model = complete_compilation(&pool, &store, &f, compilation).await;
    assert_eq!(trial_usage(&pool, &lease).await, (0, 1));
    let forecast = forecast(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    assert!(matches!(
        store
            .prepare_research_alpha(lease.run.id, &lease.fence, experiment)
            .await,
        Err(StoreError::Invalid("accepted_forecast_required"))
    ));
    experiment_support::complete_forecast(&pool, &store, &f, forecast).await;
    assert!(
        matches!(store.next_mission_experiment(lease.run.id,&lease.fence).await.unwrap(),Some(store::lifecycle::ExperimentWork::RecordAlpha(id)) if id==experiment)
    );
    sqlx::raw_sql("CREATE FUNCTION public.reject_research_version() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected Alpha publication failure'; END $$; CREATE TRIGGER reject_version BEFORE INSERT ON app.alpha_versions FOR EACH ROW EXECUTE FUNCTION public.reject_research_version();").execute(&pool).await.unwrap();
    assert!(store
        .prepare_research_alpha(lease.run.id, &lease.fence, experiment)
        .await
        .is_err());
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.alphas),(SELECT count(*) FROM app.alpha_versions),(SELECT count(*) FROM app.command_receipts WHERE operation='RESEARCH_ALPHA_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    sqlx::raw_sql("DROP TRIGGER reject_version ON app.alpha_versions; DROP FUNCTION public.reject_research_version();").execute(&pool).await.unwrap();
    let (a, b) = tokio::join!(
        store.prepare_research_alpha(lease.run.id, &lease.fence, experiment),
        store.prepare_research_alpha(lease.run.id, &lease.fence, experiment)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource, b.resource);
    assert_ne!(a.replayed, b.replayed);
    let facts=sqlx::query("SELECT alpha.lifecycle,v.model_artifact_id,(v.code_artifact_id=e.code_artifact_id AND v.root_lineage_id=family.root_lineage_id AND alpha.active_version_id=v.id) AS exact_bindings,v.signal_kind,v.forecast_unit,v.horizon_value,v.calibration_id,v.runtime_image_ref,e.outcome,(SELECT count(*) FROM app.qualifications WHERE alpha_version_id=v.id) AS qualifications FROM app.alpha_versions v JOIN app.alphas alpha ON alpha.id=v.alpha_id JOIN app.experiments e ON e.id=v.experiment_id JOIN app.experiment_families family ON family.id=e.family_id WHERE v.id=$1")
        .bind(a.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts.get::<String, _>("lifecycle"), "RESEARCH");
    assert_eq!(
        facts.get::<uuid::Uuid, _>("model_artifact_id"),
        model.as_uuid()
    );
    assert!(facts.get::<bool, _>("exact_bindings"));
    assert_eq!(facts.get::<String, _>("signal_kind"), "SCORE");
    assert_eq!(facts.get::<String, _>("forecast_unit"), "UNITLESS_SCORE");
    assert_eq!(facts.get::<Option<i64>, _>("horizon_value"), Some(5));
    assert_eq!(facts.get::<Option<uuid::Uuid>, _>("calibration_id"), None);
    let image: String =
        sqlx::query_scalar("SELECT image_ref FROM app.run_native_tasks WHERE run_id=$1")
            .bind(forecast.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(facts.get::<String, _>("runtime_image_ref"), image);
    assert_eq!(facts.get::<String, _>("outcome"), "PENDING");
    assert_eq!(facts.get::<i64, _>("qualifications"), 0);
    assert!(matches!(
        store.next_mission_experiment(lease.run.id, &lease.fence).await.unwrap(),
        Some(store::lifecycle::ExperimentWork::Validate(id)) if id == experiment
    ));
    assert!(
        sqlx::query("UPDATE app.alpha_versions SET signal_kind='EXPECTED_RETURN' WHERE id=$1")
            .bind(a.resource.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    let mut stale = lease.fence.clone();
    stale.worker_owner_id = "wrong-owner".into();
    assert!(matches!(
        store
            .prepare_research_alpha(lease.run.id, &stale, experiment)
            .await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn forecast_requires_accepted_model_and_replay_retains_one_trial(pool: PgPool) {
    let (store, actor, f, lease, experiment) = setup(&pool).await;
    assert!(
        matches!(store.next_mission_experiment(lease.run.id, &lease.fence).await.unwrap(), Some(store::lifecycle::ExperimentWork::Compile(id)) if id == experiment)
    );
    let compilation = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    assert!(matches!(
        forecast(&store, &f, &lease, experiment).await,
        Err(StoreError::Invalid("accepted_compilation_required"))
    ));
    assert!(store
        .next_mission_experiment(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .is_none());
    let model = complete_compilation(&pool, &store, &f, compilation).await;
    assert!(
        matches!(store.next_mission_experiment(lease.run.id, &lease.fence).await.unwrap(), Some(store::lifecycle::ExperimentWork::Forecast(id)) if id == experiment)
    );
    let before: (i64, i64) = sqlx::query_as(
        "SELECT reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1",
    )
    .bind(lease.run.cycle_id.unwrap().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        forecast(&store, &f, &lease, experiment),
        forecast(&store, &f, &lease, experiment)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(store
        .next_mission_experiment(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .is_none());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert!(a.resource.deadline_at <= lease.run.deadline_at);
    let after: (i64, i64) = sqlx::query_as(
        "SELECT reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1",
    )
    .bind(lease.run.cycle_id.unwrap().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, (before.0, before.1 + 10));
    assert_eq!(
        trial_usage(&pool, &lease).await,
        (0, 1),
        "forecast spends resources, not a second trial"
    );
    let proposal = store.experiment(&actor, experiment).await.unwrap();
    assert_eq!(proposal.run_id, Some(a.resource.id));
    assert_ne!(proposal.run_id, Some(compilation));
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.experiment_forecasts WHERE experiment_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$2)").bind(experiment.as_uuid()).bind(a.resource.id.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1));
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == a.resource.id)
        .unwrap();
    let Some(ClaimResult::Leased(science)) = store
        .claim_native_run(&message, "forecast-native", 60)
        .await
        .unwrap()
    else {
        panic!("forecast lease required");
    };
    let job = store
        .native_job(a.resource.id, &science.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.inputs.len(), 3);
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &f.objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap();
    domain::execution::task(&job.spec, &task).unwrap();
    let NativeTaskParametersV1::EvaluateAlpha {
        dataset_revision_id,
        model_artifact_id,
        request,
        ..
    } = task
    else {
        panic!("forecast operation required");
    };
    assert_eq!(dataset_revision_id, f.data.discovery);
    assert_eq!(model_artifact_id, model);
    assert_eq!(request.parameters.label_horizon_observations, 5);
    assert!(job.spec.inputs.iter().all(|input| !matches!(
        input,
        RuntimeInputV1::Dataset {
            role: contracts::research::DataPartition::Sealed,
            ..
        }
    )));
    assert!(
        sqlx::query("UPDATE app.experiments SET run_id=$2 WHERE id=$1")
            .bind(experiment.as_uuid())
            .bind(compilation.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM app.experiment_forecasts WHERE experiment_id=$1")
            .bind(experiment.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn forecast_parameter_and_late_publication_failures_do_not_charge_a_trial(pool: PgPool) {
    let (store, _actor, f, lease, experiment) = setup(&pool).await;
    let compilation = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    complete_compilation(&pool, &store, &f, compilation).await;
    let before:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1").bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    let parameters: uuid::Uuid =
        sqlx::query_scalar("SELECT parameter_artifact_id FROM app.experiments WHERE id=$1")
            .bind(experiment.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    for invalid in ["dataset", "horizon", "fuel", "periods", "model", "length"] {
        let reading = f.objects.clone();
        let mut allocation = limits();
        allocation.experiments = 0;
        let result = store
            .start_experiment_forecast(
                lease.run.id,
                &lease.fence,
                experiment,
                &allocation,
                move |id, size| {
                    let objects = reading.clone();
                    async move {
                        let raw = objects.read(id, size).map_err(|_| StoreError::Integrity)?;
                        if id.as_uuid() != parameters {
                            return Ok(raw);
                        }
                        let mut value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                        match invalid {
                            "dataset" => {
                                value["dataset_revision_id"] = serde_json::json!(f.data.sealed)
                            }
                            "horizon" => {
                                value["parameters"]["label_horizon_observations"] =
                                    serde_json::json!(6)
                            }
                            "fuel" => {
                                value["parameters"]["total_fuel"] = serde_json::json!("1000000001")
                            }
                            "periods" => value["parameters"]["fast_period"] = serde_json::json!(5),
                            "model" => value["model_artifact_id"] = serde_json::json!(Id::new()),
                            _ => return Ok(Vec::new()),
                        }
                        let mut changed = serde_json::to_vec(&value).unwrap();
                        assert!(changed.len() <= raw.len());
                        changed.resize(raw.len(), b' ');
                        Ok(changed)
                    }
                },
                |_| async { panic!("invalid inputs cannot publish") },
            )
            .await;
        match invalid {
            "dataset" => assert!(matches!(
                result,
                Err(StoreError::Invalid("forecast_discovery_dataset"))
            )),
            "horizon" => assert!(matches!(
                result,
                Err(StoreError::Invalid("forecast_label_horizon"))
            )),
            "model" => assert!(matches!(
                result,
                Err(StoreError::Invalid("forecast_parameters"))
            )),
            "length" => assert!(matches!(result, Err(StoreError::Integrity))),
            _ => assert!(
                matches!(result, Err(StoreError::Domain(_))),
                "invalid {invalid} did not reach domain validation"
            ),
        }
    }
    let mut stale = lease.clone();
    stale.fence.owner_epoch = stale.fence.owner_epoch.next().unwrap();
    assert!(matches!(
        forecast(&store, &f, &stale, experiment).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    sqlx::raw_sql("CREATE FUNCTION public.reject_forecast_edge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected final forecast edge failure'; END $$; CREATE TRIGGER reject_edge BEFORE INSERT ON app.experiment_forecasts FOR EACH ROW EXECUTE FUNCTION public.reject_forecast_edge();").execute(&pool).await.unwrap();
    assert!(matches!(
        forecast(&store, &f, &lease, experiment).await,
        Err(StoreError::Database(_))
    ));
    sqlx::query("DROP TRIGGER reject_edge ON app.experiment_forecasts")
        .execute(&pool)
        .await
        .unwrap();
    let after:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1").bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    assert!(
        !forecast(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .replayed
    );
}
