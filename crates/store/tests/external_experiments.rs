//! Actual PostgreSQL/PGMQ transactions with controlled compiler observations.
//! These tests do not claim that rustc, Wasm or Nautilus was executed.
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
use contracts::{experiments::ExperimentOutcome, lifecycle::RunCancelV1, SchemaV1};
use external_support::{advance, evaluate, setup};
use sqlx::PgPool;
use store::{lifecycle::ClaimResult, StoreError};

#[sqlx::test(migrations = "../../migrations")]
async fn external_trial_follows_one_compile_to_one_evaluation_and_cancel_is_terminal(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compile = evaluate(&store, &actor, &f, &request).await.unwrap();
    let repeated = evaluate(&store, &actor, &f, &request).await.unwrap();
    assert!(repeated.replayed);
    assert_eq!(compile.resource.id, repeated.resource.id);
    assert_eq!(
        store
            .experiment(&actor, experiment.id)
            .await
            .unwrap()
            .run_id,
        Some(compile.resource.id)
    );
    assert!(matches!(
        store
            .experiment_evaluation(&actor, experiment.id, |id, size| f.read(id, size))
            .await,
        Err(StoreError::NotFound)
    ));
    experiment_support::complete_compilation(&pool, &store, &f, compile.resource.id).await;
    let compiler_message = store::lifecycle::RunMessage {
        message_id: sqlx::query_scalar(
            "SELECT initial_queue_message_id FROM app.run_admissions WHERE run_id=$1",
        )
        .bind(compile.resource.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        run_id: compile.resource.id,
        read_count: 1,
    };
    assert!(matches!(
        store.acknowledge_run(&compiler_message).await,
        Err(StoreError::Conflict)
    ));
    advance(&store, &f, compile.resource.id).await.unwrap();
    let running = store.experiment(&actor, experiment.id).await.unwrap();
    let evaluation = running.run_id.unwrap();
    assert_ne!(evaluation, compile.resource.id);
    assert_eq!(running.outcome, Some(ExperimentOutcome::Pending));
    assert!(matches!(
        store
            .experiment_evaluation(&actor, experiment.id, |id, size| f.read(id, size))
            .await,
        Err(StoreError::NotFound)
    ));
    advance(&store, &f, compile.resource.id).await.unwrap();
    store.acknowledge_run(&compiler_message).await.unwrap();
    assert_eq!(
        store
            .experiment(&actor, experiment.id)
            .await
            .unwrap()
            .run_id,
        Some(evaluation)
    );
    assert_eq!(
        evaluate(&store, &actor, &f, &request)
            .await
            .unwrap()
            .resource
            .id,
        compile.resource.id
    );
    let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM app.external_experiment_tasks),(SELECT count(*) FROM app.runs WHERE kind='AGENT_RESEARCH'),(SELECT used_experiments FROM app.research_cycles WHERE id=$1)").bind(experiment.cycle_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 1, 0, 1));
    let run = store.get_run(&actor, evaluation).await.unwrap();
    store
        .cancel_run(
            &actor,
            "external-cancel",
            evaluation,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == evaluation)
        .unwrap();
    match store
        .claim_native_run(&message, "cancelled-native", 60)
        .await
        .unwrap()
        .unwrap()
    {
        ClaimResult::Leased(lease) => {
            store
                .settle_unsubmitted_native_run(evaluation, &lease.fence)
                .await
                .unwrap();
        }
        ClaimResult::Terminal(_) => {}
        _ => panic!("terminal cancellation required"),
    }
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    advance(&store, &f, evaluation).await.unwrap();
    advance(&store, &f, evaluation).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
    let finished = store.experiment(&actor, experiment.id).await.unwrap();
    assert_eq!(finished.outcome, Some(ExperimentOutcome::Inconclusive));
    assert_eq!(
        finished.outcome_reason.as_deref(),
        Some("EXTERNAL_EVALUATION_CANCELLED")
    );
    assert!(finished.conclusion_artifact_id.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn compilation_cancel_consumes_trial_without_starting_science(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource;
    store
        .cancel_run(
            &actor,
            "cancel-compiler",
            compiled.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: compiled.revision,
            },
        )
        .await
        .unwrap();
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == compiled.id)
        .unwrap();
    match store
        .claim_native_run(&message, "cancel-compiler", 60)
        .await
        .unwrap()
        .unwrap()
    {
        ClaimResult::Leased(lease) => {
            store
                .settle_unsubmitted_native_run(compiled.id, &lease.fence)
                .await
                .unwrap();
        }
        ClaimResult::Terminal(_) => {}
        _ => panic!("terminal cancellation required"),
    }
    advance(&store, &f, compiled.id).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.external_experiment_tasks),(SELECT used_experiments FROM app.research_cycles WHERE id=$1)").bind(experiment.cycle_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_limits_and_changed_idempotent_intent_do_not_duplicate_trials(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let mut invalid = request.clone();
    invalid.request.evaluation_limits.experiments = 1;
    assert!(evaluate(&store, &actor, &f, &invalid).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE cycle_id=$1")
        .bind(experiment.cycle_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let compiled = evaluate(&store, &actor, &f, &request).await.unwrap();
    let mut changed = request.clone();
    changed.request.evaluation_limits.wall_seconds += 1;
    assert!(matches!(
        evaluate(&store, &actor, &f, &changed).await,
        Err(StoreError::IdempotencyConflict)
    ));
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs WHERE cycle_id=$1),(SELECT reserved_experiments FROM app.research_cycles WHERE id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$2)")
        .bind(experiment.cycle_id.as_uuid()).bind(compiled.resource.id.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1));
}

async fn finish_request(
    store: &store::Store,
    actor: &store::authority::Actor,
    cycle: contracts::Id,
) -> contracts::cycles::CycleFinishExternalIntent {
    let current = store.cycle(actor, cycle).await.unwrap();
    contracts::cycles::CycleFinishExternalIntent {
        schema_version: SchemaV1,
        cycle_id: cycle,
        request: contracts::cycles::CycleFinishExternalV1 {
            schema_version: SchemaV1,
            expected_revision: current.revision,
        },
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_cycle_close_retains_unexecuted_proposals_and_replays_terminal_identity(
    pool: PgPool,
) {
    use contracts::cycles::{CycleOutcome, CycleState};
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    let closed = store
        .finish_external_cycle(&actor, "close-unexecuted", &finish)
        .await
        .unwrap();
    assert_eq!(closed.resource.state, CycleState::Completed);
    assert_eq!(closed.resource.outcome, Some(CycleOutcome::Inconclusive));
    assert!(closed.resource.next_action.is_none());
    assert!(closed.resource.ended_at.is_some());
    let replay = store
        .finish_external_cycle(&actor, "close-unexecuted", &finish)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.ended_at, closed.resource.ended_at);
    assert_eq!(replay.resource.revision, closed.resource.revision);
    let unexecuted = store.experiment(&actor, experiment.id).await.unwrap();
    assert_eq!(unexecuted.outcome, Some(ExperimentOutcome::Pending));
    assert!(unexecuted.run_id.is_none());
    assert!(unexecuted.conclusion_artifact_id.is_none());
    assert!(matches!(
        evaluate(&store, &actor, &f, &request).await,
        Err(StoreError::Domain(domain::DomainError::AdmissionClosed))
    ));
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE cycle_id=$1")
        .bind(experiment.cycle_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(runs, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_cycle_cannot_close_before_native_work_and_publication_are_settled(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource;
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    assert!(matches!(
        store
            .finish_external_cycle(&actor, "close-pending", &finish)
            .await,
        Err(StoreError::Invalid("external_cycle_work_pending"))
    ));
    store
        .cancel_run(
            &actor,
            "cancel-before-close",
            compiled.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: compiled.revision,
            },
        )
        .await
        .unwrap();
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == compiled.id)
        .unwrap();
    match store
        .claim_native_run(&message, "close-cancelled", 60)
        .await
        .unwrap()
        .unwrap()
    {
        ClaimResult::Leased(lease) => {
            store
                .settle_unsubmitted_native_run(compiled.id, &lease.fence)
                .await
                .unwrap();
        }
        ClaimResult::Terminal(_) => {}
        _ => panic!("terminal cancellation required"),
    }
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    assert!(matches!(
        store
            .finish_external_cycle(&actor, "close-unpublished", &finish)
            .await,
        Err(StoreError::Invalid("external_cycle_work_pending"))
    ));
    advance(&store, &f, compiled.id).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    let closed = store
        .finish_external_cycle(&actor, "close-settled", &finish)
        .await
        .unwrap();
    assert_eq!(
        closed.resource.state,
        contracts::cycles::CycleState::Completed
    );
    assert!(closed.resource.next_action.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_cycle_finish_and_trial_admission_serialize_on_original_cycle(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    let (admitted, closed) = tokio::join!(
        evaluate(&store, &actor, &f, &request),
        store.finish_external_cycle(&actor, "close-race", &finish)
    );
    assert_ne!(
        admitted.is_ok(),
        closed.is_ok(),
        "exactly one conflicting action must win"
    );
    let cycle = store.cycle(&actor, experiment.cycle_id).await.unwrap();
    if admitted.is_ok() {
        assert_eq!(cycle.state, contracts::cycles::CycleState::Running);
        assert_eq!(cycle.reserved_experiments, 1);
    } else {
        assert_eq!(cycle.state, contracts::cycles::CycleState::Completed);
        assert_eq!(cycle.reserved_experiments, 0);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn impossible_stage_allocations_are_rejected_before_any_trial_or_run(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    for compile in [false, true] {
        for dimension in 0..4 {
            let mut invalid = request.clone();
            let limits = if compile {
                &mut invalid.request.compile_limits
            } else {
                &mut invalid.request.evaluation_limits
            };
            match dimension {
                0 => limits.wall_seconds = f.brief.content.budget.max_wall_seconds + 1,
                1 => limits.memory_mib = f.brief.content.budget.max_memory_mib + 1,
                2 => {
                    limits.output_bytes =
                        contracts::DbCounter::new(f.brief.content.budget.max_output_bytes.get() + 1)
                            .unwrap()
                }
                _ => {
                    limits.wall_seconds = 1;
                    limits.cpu_seconds = contracts::DbCounter::new(3).unwrap();
                }
            }
            let error = evaluate(&store, &actor, &f, &invalid).await.unwrap_err();
            assert!(
                matches!(
                    error,
                    StoreError::Domain(
                        domain::DomainError::BudgetExhausted("job_resource_limit")
                            | domain::DomainError::CapabilityUnavailable("native_cpu_capacity")
                    )
                ),
                "{compile}/{dimension}: {error:?}"
            );
        }
    }
    let counts:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.runs WHERE cycle_id=$1),(SELECT count(*) FROM app.external_experiment_requests WHERE experiment_id=$2),(SELECT reserved_experiments FROM app.research_cycles WHERE id=$1),(SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1),(SELECT count(*) FROM app.command_receipts WHERE operation='EXPERIMENT_EVALUATE')")
        .bind(experiment.cycle_id.as_uuid()).bind(experiment.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 0));
}

async fn original_message(pool: &PgPool, run: contracts::Id) -> store::lifecycle::RunMessage {
    store::lifecycle::RunMessage {
        message_id: sqlx::query_scalar(
            "SELECT initial_queue_message_id FROM app.run_admissions WHERE run_id=$1",
        )
        .bind(run.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap(),
        run_id: run,
        read_count: 1,
    }
}

async fn assert_closed_continuation(
    pool: &PgPool,
    store: &store::Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    experiment: &contracts::experiments::ExperimentView,
    compiled: contracts::Id,
    reason: &str,
) {
    advance(store, f, compiled).await.unwrap();
    advance(store, f, compiled).await.unwrap();
    let view = store.experiment(actor, experiment.id).await.unwrap();
    assert_eq!(view.run_id, Some(compiled));
    assert_eq!(view.outcome, Some(ExperimentOutcome::Inconclusive));
    assert_eq!(view.outcome_reason.as_deref(), Some(reason));
    assert!(view.conclusion_artifact_id.is_none());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.external_experiment_tasks WHERE experiment_id=$1",
    )
    .bind(experiment.id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    store
        .acknowledge_run(&original_message(pool, compiled).await)
        .await
        .unwrap();
    let finish = finish_request(store, actor, experiment.cycle_id).await;
    assert_eq!(
        store
            .finish_external_cycle(actor, "finish-interrupted", &finish)
            .await
            .unwrap()
            .resource
            .state,
        contracts::cycles::CycleState::Completed
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_changed_runtime_revision_settles_original_trial_without_rebinding_or_poisoning_queue(
    pool: PgPool,
) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    let mut runtime = store.runtime(&actor, f.data.runtime).await.unwrap();
    runtime.configuration.name.push_str(" changed");
    // The relational setup has a legacy literal reference. Exercise the real
    // update with a native credential, not an unavailable or empty binding.
    let secrets = tempfile::tempdir().unwrap();
    let key_path = secrets.path().join("master.key");
    integrations::secrets::SecretVault::initialize_key(&key_path).unwrap();
    let vault = integrations::secrets::SecretVault::open(secrets.path(), &key_path).unwrap();
    let secret: &[u8] = b"external-runtime-revision-fixture";
    let credential = vault.put("RUNTIME", secret).unwrap();
    let original_admission: (i64, serde_json::Value) = sqlx::query_as(
        "SELECT runtime_revision,runtime_snapshot FROM app.run_admissions WHERE run_id=$1",
    )
    .bind(compiled.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let changed = store
        .update_runtime(
            &actor,
            "change-external-runtime",
            runtime.id,
            &contracts::settings::RuntimeUpdate {
                schema_version: SchemaV1,
                expected_revision: runtime.revision,
                configuration: runtime.configuration,
                credential_ref: Some(credential),
                ca_certificate_ref: None,
            },
            |refs| async move {
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].id, credential);
                assert_eq!(
                    refs[0].purpose,
                    contracts::settings::IntegrationSecretPurpose::Runtime
                );
                assert_eq!(
                    vault.read(refs[0].id, refs[0].purpose.code()).unwrap(),
                    secret
                );
                Ok(())
            },
        )
        .await
        .unwrap()
        .resource;
    assert_ne!(changed.revision, runtime.revision);
    assert_closed_continuation(
        &pool,
        &store,
        &actor,
        &f,
        &experiment,
        compiled,
        "EXTERNAL_CONTINUATION_RUNTIME_CHANGED",
    )
    .await;
    let retained_admission: (i64, serde_json::Value) = sqlx::query_as(
        "SELECT runtime_revision,runtime_snapshot FROM app.run_admissions WHERE run_id=$1",
    )
    .bind(compiled.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retained_admission, original_admission);
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_original_validation_grant_settles_without_reading_restricted_rows(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    let grant: uuid::Uuid =
        sqlx::query_scalar("SELECT data_use_grant_id FROM app.dataset_revisions WHERE id=$1")
            .bind(f.data.validation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    store
        .revoke_data_grant(
            &actor,
            "revoke-external-validation",
            grant.to_string().try_into().unwrap(),
            &contracts::data::DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: None,
                reason_code: "OWNER_REVOKED".into(),
                reason: "Controlled revocation during compilation".into(),
            },
        )
        .await
        .unwrap();
    assert_closed_continuation(
        &pool,
        &store,
        &actor,
        &f,
        &experiment,
        compiled,
        "EXTERNAL_CONTINUATION_AUTHORITY_CLOSED",
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn transient_storage_failure_keeps_original_queue_and_recovers_same_trial(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    let failed = store
        .advance_external_experiment(
            compiled,
            |_, _| async { Err(StoreError::StorageFull) },
            |_| async { panic!("no publication after failed source read") },
        )
        .await;
    assert!(matches!(failed, Err(StoreError::StorageFull)));
    assert_eq!(
        store
            .experiment(&actor, experiment.id)
            .await
            .unwrap()
            .outcome,
        Some(ExperimentOutcome::Pending)
    );
    assert!(matches!(
        store
            .acknowledge_run(&original_message(&pool, compiled).await)
            .await,
        Err(StoreError::Conflict)
    ));
    advance(&store, &f, compiled).await.unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    assert_ne!(evaluation, compiled);
    advance(&store, &f, compiled).await.unwrap();
    assert_eq!(
        store
            .experiment(&actor, experiment.id)
            .await
            .unwrap()
            .run_id,
        Some(evaluation)
    );
    store
        .acknowledge_run(&original_message(&pool, compiled).await)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn temporarily_unavailable_probe_retries_and_reprobe_restores_same_continuation(
    pool: PgPool,
) {
    use contracts::runtime::{RuntimeProbeFailure, RuntimeProbeOutcomeV1, RuntimeProbeRequestV1};
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    let store::runtime::ProbePreparation::Pending(ticket) = store
        .prepare_runtime_probe(
            &actor,
            "external-temporary-probe",
            f.data.runtime,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: f.freeze.execution_context.runtime_revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("fresh probe required");
    };
    store
        .complete_runtime_probe(
            *ticket,
            RuntimeProbeOutcomeV1::Unavailable {
                reason: RuntimeProbeFailure::Unavailable,
            },
            |id, bytes| {
                let objects = f.objects.clone();
                async move { objects.put(id, &bytes).map_err(|_| StoreError::Integrity) }
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        advance(&store, &f, compiled).await,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("runtime_probe_unavailable")
        ))
    ));
    assert_eq!(
        store
            .experiment(&actor, experiment.id)
            .await
            .unwrap()
            .outcome,
        Some(ExperimentOutcome::Pending)
    );
    assert!(matches!(
        store
            .acknowledge_run(&original_message(&pool, compiled).await)
            .await,
        Err(StoreError::Conflict)
    ));
    experiment_support::probe_capabilities(&store, &actor, &f, external_support::capabilities())
        .await;
    advance(&store, &f, compiled).await.unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    assert_ne!(evaluation, compiled);
    store
        .acknowledge_run(&original_message(&pool, compiled).await)
        .await
        .unwrap();
}

async fn fail_native_run(pool: &PgPool, store: &store::Store, run: contracts::Id) {
    let message = original_message(pool, run).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "controlled-native-failure", 60)
        .await
        .unwrap()
    else {
        panic!("native failure lease required");
    };
    store.native_job(run, &lease.fence).await.unwrap();
    assert!(store.begin_run_dispatch(run, &lease.fence).await.unwrap());
    store
        .accept_run_terminal(
            run,
            &lease.fence,
            &store::lifecycle::TerminalObservation {
                schema_version: SchemaV1,
                external_job_id: lease.external_job_id.clone(),
                outcome: store::lifecycle::NativeOutcome::Failed,
                manifest_artifact_id: None,
                failure_class: Some(store::lifecycle::FailureClass::InvalidInput),
                failure_code: Some("CONTROLLED_NATIVE_FAILURE".into()),
                observed_at: sqlx::query_scalar("SELECT clock_timestamp()")
                    .fetch_one(pool)
                    .await
                    .unwrap(),
            },
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn compiler_failure_publishes_terminal_interruption_and_allows_batch_close(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    fail_native_run(&pool, &store, compiled).await;
    assert_closed_continuation(
        &pool,
        &store,
        &actor,
        &f,
        &experiment,
        compiled,
        "EXTERNAL_EXECUTION_FAILED",
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn evaluation_failure_preserves_native_failure_without_fabricating_metrics(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    advance(&store, &f, compiled).await.unwrap();
    store
        .acknowledge_run(&original_message(&pool, compiled).await)
        .await
        .unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    fail_native_run(&pool, &store, evaluation).await;
    assert!(matches!(
        store
            .acknowledge_run(&original_message(&pool, evaluation).await)
            .await,
        Err(StoreError::Conflict)
    ));
    advance(&store, &f, evaluation).await.unwrap();
    advance(&store, &f, evaluation).await.unwrap();
    let outcome = store.experiment(&actor, experiment.id).await.unwrap();
    assert_eq!(outcome.run_id, Some(evaluation));
    assert_eq!(
        outcome.outcome_reason.as_deref(),
        Some("EXTERNAL_EXECUTION_FAILED")
    );
    assert!(outcome.conclusion_artifact_id.is_none());
    store
        .acknowledge_run(&original_message(&pool, evaluation).await)
        .await
        .unwrap();
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    store
        .finish_external_cycle(&actor, "finish-failed-evaluation", &finish)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn successful_native_report_is_adopted_then_associated_once_before_ack(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    let model = experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    advance(&store, &f, compiled).await.unwrap();
    store
        .acknowledge_run(&original_message(&pool, compiled).await)
        .await
        .unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    let report =
        experiment_support::complete_external_evaluation(&pool, &store, &f, evaluation, None).await;
    assert!(matches!(
        store
            .experiment_evaluation(&actor, experiment.id, |id, size| f.read(id, size))
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store
            .acknowledge_run(&original_message(&pool, evaluation).await)
            .await,
        Err(StoreError::Conflict)
    ));
    advance(&store, &f, evaluation).await.unwrap();
    advance(&store, &f, evaluation).await.unwrap();
    let view = store.experiment(&actor, experiment.id).await.unwrap();
    assert_eq!(view.run_id, Some(evaluation));
    assert_eq!(view.conclusion_artifact_id, Some(report));
    assert_eq!(view.outcome, Some(ExperimentOutcome::Inconclusive));
    assert_eq!(
        view.outcome_reason.as_deref(),
        Some("EXTERNAL_EVALUATION_COMPLETE")
    );
    let result = store
        .experiment_evaluation(&actor, experiment.id, |id, size| f.read(id, size))
        .await
        .unwrap();
    assert_eq!(result.model_artifact_id, model);
    assert_eq!(result.dataset_revision_id, f.data.validation);
    let metadata = store.artifact(&actor, report).await.unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&f.read(report, metadata.byte_count).await.unwrap()).unwrap();
    assert_eq!(serde_json::to_value(result).unwrap(), original);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.external_experiment_results WHERE experiment_id=$1",
    )
    .bind(experiment.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    store
        .acknowledge_run(&original_message(&pool, evaluation).await)
        .await
        .unwrap();
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    store
        .finish_external_cycle(&actor, "finish-successful-metrics", &finish)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn project_pause_between_stages_settles_the_original_execution(pool: PgPool) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    let project = store.project(&actor, experiment.project_id).await.unwrap();
    store
        .update_project(
            &actor,
            "pause-external-project",
            project.id,
            &contracts::control::ProjectUpdate {
                schema_version: SchemaV1,
                expected_revision: project.revision,
                name: project.name,
                description: project.description,
                state: contracts::runs::ProjectState::Paused,
            },
        )
        .await
        .unwrap();
    assert_closed_continuation(
        &pool,
        &store,
        &actor,
        &f,
        &experiment,
        compiled,
        "EXTERNAL_CONTINUATION_CLOSED",
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn competing_paid_trials_exhausting_remaining_cpu_settle_instead_of_poisoning_queue(
    pool: PgPool,
) {
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    let other = store
        .propose_experiment(
            &actor,
            "competing-proposal",
            &contracts::experiments::ExperimentProposalV1 {
                schema_version: SchemaV1,
                cycle_id: experiment.cycle_id,
                family_id: experiment.family_id,
                parent_experiment_id: Some(experiment.id),
                hypothesis: "Controlled competing trial budget".into(),
                expected_failure_modes: "Another admitted stage may consume remaining CPU".into(),
                proposal_artifact_id: experiment.proposal_artifact_id,
                parameter_artifact_id: experiment.parameter_artifact_id.unwrap(),
                code_artifact_id: experiment.code_artifact_id,
            },
        )
        .await
        .unwrap()
        .resource;
    let mut competing = request.clone();
    competing.experiment_id = other.id;
    competing.request.expected_revision = other.revision;
    competing.request.compile_limits.cpu_seconds = contracts::DbCounter::new(
        f.brief.content.budget.max_cpu_seconds.get()
            - request.request.compile_limits.cpu_seconds.get()
            - request.request.evaluation_limits.cpu_seconds.get(),
    )
    .unwrap();
    competing.request.compile_limits.wall_seconds = 300;
    let objects = &f.objects;
    let second = store
        .evaluate_experiment(
            &actor,
            "competing-evaluate",
            &competing,
            |id, size| f.read(id, size),
            |object| async move {
                objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, second).await;
    advance(&store, &f, compiled).await.unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    advance(&store, &f, second).await.unwrap();
    advance(&store, &f, second).await.unwrap();
    let interrupted = store.experiment(&actor, other.id).await.unwrap();
    assert_eq!(interrupted.run_id, Some(second));
    assert_eq!(
        interrupted.outcome_reason.as_deref(),
        Some("EXTERNAL_CONTINUATION_BUDGET_EXHAUSTED")
    );
    assert!(interrupted.conclusion_artifact_id.is_none());
    store
        .acknowledge_run(&original_message(&pool, second).await)
        .await
        .unwrap();
    store
        .acknowledge_run(&original_message(&pool, compiled).await)
        .await
        .unwrap();
    let run = store.get_run(&actor, evaluation).await.unwrap();
    store
        .cancel_run(
            &actor,
            "cancel-competing-evaluation",
            evaluation,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    advance(&store, &f, evaluation).await.unwrap();
    store
        .acknowledge_run(&original_message(&pool, evaluation).await)
        .await
        .unwrap();
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    store
        .finish_external_cycle(&actor, "finish-competing-budget", &finish)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn target_alpha_adoption_uses_accepted_fold_after_close_once_without_compute(pool: PgPool) {
    use contracts::{strategy_portfolio::*, Revision};
    let (store, actor, f, experiment, evaluation_request) = setup(&pool).await;
    let mut intent = StrategyAlphaAdoptIntentV1 {
        schema_version: SchemaV1,
        experiment_id: experiment.id,
        request: StrategyAlphaAdoptV1 {
            schema_version: SchemaV1,
            expected_revision: experiment.revision,
            name: "Original target policy".into(),
            source_fold_index: 0,
        },
    };
    assert!(matches!(
        store
            .adopt_experiment_alpha(&actor, "too-early", &intent, |id, size| f.read(id, size))
            .await,
        Err(StoreError::Invalid("accepted_external_evaluation_required"))
    ));
    let compiled = evaluate(&store, &actor, &f, &evaluation_request)
        .await
        .unwrap()
        .resource
        .id;
    let model = experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    advance(&store, &f, compiled).await.unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    let report =
        experiment_support::complete_external_evaluation(&pool, &store, &f, evaluation, None).await;
    advance(&store, &f, evaluation).await.unwrap();
    let original = store.experiment(&actor, experiment.id).await.unwrap();
    let result = store
        .experiment_evaluation(&actor, experiment.id, |id, size| f.read(id, size))
        .await
        .unwrap();
    assert!(matches!(
        store
            .adopt_experiment_alpha(&actor, "stale", &intent, |id, size| f.read(id, size))
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    intent.request.expected_revision = original.revision;
    let finish = finish_request(&store, &actor, experiment.cycle_id).await;
    store
        .finish_external_cycle(&actor, "finish-before-alpha", &finish)
        .await
        .unwrap();
    let budget_before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.research_cycles c WHERE id=$1")
            .bind(experiment.cycle_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let (first, competing) = tokio::join!(
        store.adopt_experiment_alpha(&actor, "alpha-a", &intent, |id, size| f.read(id, size)),
        store.adopt_experiment_alpha(&actor, "alpha-b", &intent, |id, size| f.read(id, size)),
    );
    let first = first.unwrap();
    let competing = competing.unwrap();
    assert_eq!(first.resource.id, competing.resource.id);
    assert_ne!(first.replayed, competing.replayed);
    let v = first.resource;
    assert_eq!(v.policy.source.experiment_id, experiment.id);
    assert_eq!(v.policy.source.evaluation_run_id, evaluation);
    assert_eq!(v.policy.source.report_artifact_id, report);
    let accepted = store
        .get_run(&actor, evaluation)
        .await
        .unwrap()
        .active_attempt_id
        .unwrap();
    assert_eq!(v.policy.source.accepted_attempt_id, accepted);
    assert_eq!(v.policy.model_artifact_id, model);
    assert_eq!(
        v.policy.code_artifact_id,
        experiment.code_artifact_id.unwrap()
    );
    assert_eq!(
        v.policy.parameter_artifact_id,
        experiment.parameter_artifact_id.unwrap()
    );
    assert_eq!(v.policy.dataset_revision_id, result.dataset_revision_id);
    assert_eq!(v.policy.feature_artifact_ids, result.feature_artifact_ids);
    assert_eq!(v.policy.model_abi, contracts::science::FEATURE_MODEL_ABI_V2);
    assert_eq!(
        v.policy.initialization.first_ordinal,
        result.folds[0].decisions[0].ordinal
    );
    assert_eq!(
        v.policy.initialization.first_event_ns,
        result.folds[0].decisions[0].event_ns
    );
    assert_eq!(
        v.policy.initialization.first_decision_ns,
        result.folds[0].decisions[0].decision_ns
    );
    let retry = store
        .adopt_experiment_alpha(&actor, "alpha-a", &intent, |_, _| async {
            panic!("receipt retry reads no source bytes")
        })
        .await
        .unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.resource.id, v.id);
    let read = store
        .alpha_version_v2(&actor, v.alpha_id, Revision::INITIAL)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        serde_json::to_value(&v).unwrap()
    );
    let versions = store
        .alpha_versions_v2(&actor, v.alpha_id, &Default::default())
        .await
        .unwrap();
    assert_eq!(versions.items.len(), 1);
    let wire = serde_json::to_value(&versions.items[0]).unwrap();
    assert_eq!(wire["output_kind"], "TARGET_WEIGHT");
    for absent in [
        "forecast_unit",
        "signal_kind",
        "horizon_kind",
        "calibration_id",
    ] {
        assert!(wire.get(absent).is_none());
    }
    let alphas = store
        .alphas(
            &actor,
            &contracts::research::ResearchListQuery {
                project_id: experiment.project_id,
                limit: 10,
                cursor: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        alphas.items[0].lifecycle,
        contracts::evidence::AlphaLifecycle::Research
    );
    assert_eq!(alphas.items[0].active_version_id, Some(v.id));
    let unchanged = store.experiment(&actor, experiment.id).await.unwrap();
    assert_eq!(
        serde_json::to_value(unchanged).unwrap(),
        serde_json::to_value(original).unwrap()
    );
    let budget_after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.research_cycles c WHERE id=$1")
            .bind(experiment.cycle_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(budget_before, budget_after);
    let counts: (i64, i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.alphas),(SELECT count(*) FROM app.alpha_versions),(SELECT count(*) FROM app.qualifications),(SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM app.runs)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 0, 0, 2));
    let mut changed = intent.clone();
    changed.request.name.push('!');
    assert!(matches!(
        store
            .adopt_experiment_alpha(&actor, "alpha-a", &changed, |id, size| f.read(id, size))
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        store
            .adopt_experiment_alpha(&actor, "different-name", &changed, |id, size| f
                .read(id, size))
            .await,
        Err(StoreError::Conflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn target_alpha_adoption_rejects_replaced_report_and_unknown_original_fold(pool: PgPool) {
    use contracts::strategy_portfolio::*;
    let (store, actor, f, experiment, request) = setup(&pool).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    advance(&store, &f, compiled).await.unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    let report =
        experiment_support::complete_external_evaluation(&pool, &store, &f, evaluation, None).await;
    advance(&store, &f, evaluation).await.unwrap();
    let original = store.experiment(&actor, experiment.id).await.unwrap();
    let mut intent = StrategyAlphaAdoptIntentV1 {
        schema_version: SchemaV1,
        experiment_id: experiment.id,
        request: StrategyAlphaAdoptV1 {
            schema_version: SchemaV1,
            expected_revision: original.revision,
            name: "Exact source only".into(),
            source_fold_index: 31,
        },
    };
    assert!(matches!(
        store
            .adopt_experiment_alpha(&actor, "bad-fold", &intent, |id, size| f.read(id, size))
            .await,
        Err(StoreError::Invalid("strategy_source_fold"))
    ));
    intent.request.source_fold_index = 0;
    // Same-sized UUID substitution reaches exact producer/content binding rather
    // than merely the file length guard. The stored bytes are never changed.
    let native_parameters: uuid::Uuid = sqlx::query_scalar(
        "SELECT parameters_artifact_id FROM app.run_native_tasks WHERE run_id=$1",
    )
    .bind(evaluation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let compilation_report: uuid::Uuid = sqlx::query_scalar("SELECT a.id FROM app.artifacts a JOIN app.run_native_outputs o ON o.artifact_id=a.id JOIN app.runs r ON r.active_attempt_id=o.attempt_id WHERE r.id=$1 AND a.schema_name='qz.model_compilation'")
        .bind(compiled.as_uuid()).fetch_one(&pool).await.unwrap();
    let substitutions = [
        (report, "model_artifact_id"),
        (report, "dataset_revision_id"),
        (
            contracts::Id::try_from(native_parameters.to_string()).unwrap(),
            "model_artifact_id",
        ),
        (
            contracts::Id::try_from(compilation_report.to_string()).unwrap(),
            "code_artifact_id",
        ),
        (
            experiment.parameter_artifact_id.unwrap(),
            "dataset_revision_id",
        ),
    ];
    for (source_artifact, field) in substitutions {
        let failed = store
            .adopt_experiment_alpha(&actor, field, &intent, |id, size| {
                let f = &f;
                async move {
                    let bytes = f.read(id, size).await?;
                    if id != source_artifact {
                        return Ok(bytes);
                    }
                    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    value[field] = serde_json::json!(contracts::Id::new());
                    let modified = serde_json::to_vec(&value).unwrap();
                    assert_eq!(modified.len(), bytes.len());
                    Ok(modified)
                }
            })
            .await;
        assert!(matches!(
            failed,
            Err(StoreError::Domain(_)) | Err(StoreError::Integrity)
        ));
    }
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.alphas),(SELECT count(*) FROM app.command_receipts WHERE operation='EXPERIMENT_ADOPT_ALPHA')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
    let accepted = store
        .adopt_experiment_alpha(&actor, "after-failure", &intent, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    assert!(!accepted.replayed);
}

#[path = "support/recorded_feature.rs"]
mod recorded_fixture;

#[sqlx::test(migrations = "../../migrations")]
async fn registered_features_reach_compile_continuation_and_adoption_with_actual_origin(
    pool: PgPool,
) {
    use contracts::{
        research::{DataOrigin, DataUse},
        strategy_portfolio::*,
    };
    let (descriptor, parts) = recorded_fixture::parts();
    let (store, actor, f, experiment, request) =
        external_support::setup_recorded(&pool, DataUse::Research, descriptor, parts).await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    advance(&store, &f, compiled).await.unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    let origin: String =
        sqlx::query_scalar("SELECT origin FROM app.run_native_tasks WHERE run_id=$1")
            .bind(evaluation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        origin, "REAL",
        "controlled REAL metadata plus registered REAL feature inputs retains its declared origin"
    );
    let feature_count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.external_experiment_requests q JOIN LATERAL jsonb_array_elements_text(q.feature_artifact_ids) ids ON true JOIN app.feature_artifact_sources f ON f.artifact_id=ids.value::uuid WHERE q.experiment_id=$1 AND f.dataset_revision_id=q.dataset_revision_id")
        .bind(experiment.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(feature_count, 2);
    experiment_support::complete_external_evaluation(&pool, &store, &f, evaluation, None).await;
    advance(&store, &f, evaluation).await.unwrap();
    let original = store.experiment(&actor, experiment.id).await.unwrap();
    let intent = StrategyAlphaAdoptIntentV1 {
        schema_version: SchemaV1,
        experiment_id: experiment.id,
        request: StrategyAlphaAdoptV1 {
            schema_version: SchemaV1,
            expected_revision: original.revision,
            name: "Controlled paired source policy".into(),
            source_fold_index: 0,
        },
    };
    let adopted = store
        .adopt_experiment_alpha(&actor, "recorded-alpha", &intent, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap()
        .resource;
    assert_eq!(adopted.policy.dataset_revision_id, f.data.validation);
    for feature in &adopted.policy.feature_artifact_ids {
        assert_eq!(
            store.artifact(&actor, *feature).await.unwrap().origin,
            DataOrigin::Real
        );
    }
    store
        .revoke_data_grant(
            &actor,
            "recorded-withdraw",
            f.data.grant,
            &contracts::data::DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: None,
                reason_code: "CONTROLLED_WITHDRAWAL".into(),
                reason: "Controlled paired source permission withdrawn".into(),
            },
        )
        .await
        .unwrap();
    assert!(store
        .adopt_experiment_alpha(&actor, "new-recorded-alpha-key", &intent, |id, size| f
            .read(id, size))
        .await
        .is_err());
    let replay = store
        .adopt_experiment_alpha(&actor, "recorded-alpha", &intent, |_, _| async {
            panic!("same receipt does not consume new data")
        })
        .await
        .unwrap();
    assert_eq!(replay.resource.id, adopted.id);
    assert!(replay.replayed);
    assert!(store
        .experiment_evaluation(&actor, experiment.id, |id, size| f.read(id, size))
        .await
        .is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn registered_feature_permission_withdrawal_closes_compile_continuation(pool: PgPool) {
    let (descriptor, parts) = recorded_fixture::parts();
    let (store, actor, f, experiment, request) = external_support::setup_recorded(
        &pool,
        contracts::research::DataUse::Research,
        descriptor,
        parts,
    )
    .await;
    let compiled = evaluate(&store, &actor, &f, &request)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(&pool, &store, &f, compiled).await;
    store
        .revoke_data_grant(
            &actor,
            "before-continuation",
            f.data.grant,
            &contracts::data::DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: None,
                reason_code: "CONTROLLED_WITHDRAWAL".into(),
                reason: "The paired feature use is no longer permitted".into(),
            },
        )
        .await
        .unwrap();
    advance(&store, &f, compiled).await.unwrap();
    let stopped = store.experiment(&actor, experiment.id).await.unwrap();
    assert_eq!(stopped.run_id, Some(compiled));
    assert_eq!(stopped.outcome, Some(ExperimentOutcome::Inconclusive));
    assert_eq!(
        stopped.outcome_reason.as_deref(),
        Some("EXTERNAL_CONTINUATION_AUTHORITY_CLOSED")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.external_experiment_tasks WHERE experiment_id=$1"
        )
        .bind(experiment.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}
