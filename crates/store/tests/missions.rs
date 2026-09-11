//! Formal Cycle -> native result -> Mission admission. Model/OCI execution is
//! not claimed by the explicit native result fixture used in these PG tests.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{
    catalogs::RuntimeCatalogMetadataV1,
    control::ProjectUpdate,
    execution::NativeTaskParametersV1,
    runs::{ProjectState, RunKind, RunState},
    runtime_jobs::*,
    DbCounter, Id, Revision, SchemaV1,
};
use sqlx::PgPool;
use store::{
    authority::Actor,
    lifecycle::{native::NativePayloads, ClaimResult},
    Store, StoreError,
};

async fn mission_lease(store: &Store) -> store::lifecycle::RunLease {
    let messages = store.read_mission_messages(30, 100).await.unwrap();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert!(store
        .claim_native_run(message, "wrong-driver", 60)
        .await
        .unwrap()
        .is_none());
    let Some(ClaimResult::Leased(lease)) = store
        .claim_mission(message, "native-mission-fixture", 60)
        .await
        .unwrap()
    else {
        panic!("Mission lease required");
    };
    *lease
}

fn native_thread() -> store::lifecycle::mission::NativeSessionReceipt {
    store::lifecycle::mission::NativeSessionReceipt {
        thread_id: Id::new().to_string(),
        codex_version: "0.144.4".into(),
        protocol_schema_version: "v2".into(),
        requested_service_tier: None,
        effective: contracts::codex::CodexEffectiveSettingsV1 {
            model: "controlled-native-model".into(),
            provider: "controlled-native-provider".into(),
            reasoning_effort: Some("medium".into()),
            service_tier: None,
        },
    }
}

async fn setup(pool: &PgPool) -> (Store, Actor, cycle_support::Fixture, Id, Id) {
    let (store, actor) = research_support::operator(pool).await;
    let f = cycle_support::setup(pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let started = f
        .start(&store, &actor, "start", &request)
        .await
        .unwrap()
        .resource;
    (store, actor, f, started.cycle.id, started.run.id)
}

async fn complete(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    run: Id,
    invalid: bool,
) {
    assert!(
        !store.advance_initial_cycle(run).await.unwrap(),
        "queued preparation cannot admit a Mission"
    );
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == run)
        .unwrap();
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "cycle-preparation-fixture", 60)
        .await
        .unwrap()
    else {
        panic!("native lease required");
    };
    let job = store.native_job(run, &lease.fence).await.unwrap();
    assert!(store.begin_run_dispatch(run, &lease.fence).await.unwrap());
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
    let NativeTaskParametersV1::ValidateData { selections, .. } = serde_json::from_slice(
        &f.objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap() else {
        panic!("fixed validation required");
    };
    let selection = &selections[0];
    let (metadata, bytes): (uuid::Uuid, i64) = sqlx::query_as("SELECT e.native_metadata_artifact_id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1")
        .bind(selection.dataset_revision_id.as_uuid()).fetch_one(pool).await.unwrap();
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(
        &f.objects
            .read(
                metadata.to_string().try_into().unwrap(),
                DbCounter::new(bytes as u64).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut quality = metadata.quality;
    quality.checked_at = now;
    quality.datasets[0].dataset_revision_id = selection.dataset_revision_id;
    quality.datasets[0].selection = selection.selection.clone();
    if invalid {
        quality.datasets.clear();
    }
    let bytes = serde_json::to_vec(&quality).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::DataQuality,
        schema: job.spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        media_type: "application/json".into(),
    };
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id,
        input_set_id: job.run.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: runtime_support::capabilities(now).engine_versions,
        started_at: Some(job.submitted_not_before),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::new(
                (now - job.submitted_not_before).num_milliseconds().max(0) as u64,
            )
            .unwrap(),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: output.byte_count,
        },
        artifacts: vec![output.clone()],
        error: None,
    };
    let reading = f.objects.clone();
    let publishing = f.objects.clone();
    let result = store.publish_native_result(run,&lease.fence,serde_json::to_vec(&manifest).unwrap(),NativePayloads::Verified(vec![(output,bytes)]),
        move |id,size| async move { reading.read(id,size).map_err(|_|StoreError::Integrity) },
        move |batch| async move {
            for object in batch { publishing.put(object.id,&object.bytes).map_err(|_|StoreError::Integrity)?; }
            Ok(())
        }).await.unwrap();
    assert_eq!(
        result.resource.state,
        if invalid {
            RunState::Failed
        } else {
            RunState::Succeeded
        }
    );
}

async fn project_state(store: &Store, actor: &Actor, id: Id, state: ProjectState) {
    let p = store.project(actor, id).await.unwrap();
    store
        .update_project(
            actor,
            &Id::new().to_string(),
            id,
            &ProjectUpdate {
                schema_version: SchemaV1,
                expected_revision: p.revision,
                name: p.name,
                description: p.description,
                state,
            },
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_preparation_completions_admit_one_role_without_a_scientific_agent_capability(
    pool: PgPool,
) {
    let (store, actor, f, cycle, run) = setup(&pool).await;
    complete(&pool, &store, &f, run, false).await;
    project_state(&store, &actor, f.data.project, ProjectState::Paused).await;
    assert!(
        !store.advance_initial_cycle(run).await.unwrap(),
        "pause must retain the recovery notification"
    );
    project_state(&store, &actor, f.data.project, ProjectState::Active).await;
    let (one, two) = tokio::join!(
        store.advance_initial_cycle(run),
        store.advance_initial_cycle(run)
    );
    assert!(one.unwrap() && two.unwrap());
    let rows:Vec<(uuid::Uuid,uuid::Uuid,i64)> = sqlx::query_as("SELECT run_id,profile_id,profile_revision FROM app.run_missions WHERE cycle_id=$1 AND role='RESEARCHER'")
        .bind(cycle.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].1, rows[0].2),
        (
            f.researcher_profile.profile_id.as_uuid(),
            f.researcher_profile.expected_revision.get() as i64
        )
    );
    let mission: Id = rows[0].0.to_string().try_into().unwrap();
    let admitted = store.get_run(&actor, mission).await.unwrap();
    assert_eq!(admitted.kind, RunKind::AgentResearch);
    assert_eq!(admitted.state, RunState::Queued);
    assert!(!store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .iter()
        .any(|m| m.run_id == mission));
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$1),(SELECT count(*) FROM app.run_admissions WHERE run_id=$2),(SELECT count(*) FROM app.qualifications)")
        .bind(mission.to_string()).bind(mission.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 0));
    assert_eq!(
        store
            .cycle(&actor, cycle)
            .await
            .unwrap()
            .next_action
            .as_deref(),
        Some("RESEARCH_MISSION")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_data_validation_ends_the_cycle_without_creating_a_model_mission(pool: PgPool) {
    let (store, actor, f, cycle, run) = setup(&pool).await;
    complete(&pool, &store, &f, run, true).await;
    assert!(store.advance_initial_cycle(run).await.unwrap());
    assert!(store.advance_initial_cycle(run).await.unwrap());
    assert_eq!(
        store.cycle(&actor, cycle).await.unwrap().state,
        contracts::cycles::CycleState::Failed
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.run_missions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_edited_after_cycle_start_is_not_silently_used_by_the_mission(pool: PgPool) {
    let (store, actor, f, cycle, run) = setup(&pool).await;
    complete(&pool, &store, &f, run, false).await;
    sqlx::query("UPDATE app.codex_profiles SET name='new profile revision' WHERE id=$1")
        .bind(f.researcher_profile.profile_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.advance_initial_cycle(run).await.unwrap());
    let c = store.cycle(&actor, cycle).await.unwrap();
    assert_eq!(c.state, contracts::cycles::CycleState::WaitingInput);
    assert_eq!(
        c.next_action.as_deref(),
        Some("CODEX_PROFILE_REQUIRES_ATTENTION")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.run_missions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_revoked_input_rolls_back_mission_admission_but_retains_an_honest_cycle_status(
    pool: PgPool,
) {
    let (store, actor, f, cycle, run) = setup(&pool).await;
    complete(&pool, &store, &f, run, false).await;
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'REVOKED_FIXTURE','controlled license revocation')")
        .bind(f.data.grant.as_uuid()).execute(&pool).await.unwrap();
    assert!(store.advance_initial_cycle(run).await.unwrap());
    let c = store.cycle(&actor, cycle).await.unwrap();
    assert_eq!(c.state, contracts::cycles::CycleState::WaitingInput);
    let facts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.runs WHERE kind='AGENT_RESEARCH'),(SELECT count(*) FROM app.run_missions)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_native_account_operation_retains_the_preparation_notification_for_resume(pool: PgPool) {
    let (store, actor, f, cycle, run) = setup(&pool).await;
    complete(&pool, &store, &f, run, false).await;
    store
        .prepare_codex_account(
            &actor,
            "login",
            &contracts::codex::CodexAccountRequestV1 {
                schema_version: SchemaV1,
                profile_id: f.researcher_profile.profile_id,
                expected_revision: f.researcher_profile.expected_revision,
            },
            contracts::codex::CodexAccountActionV1::Login,
        )
        .await
        .unwrap();
    assert!(!store.advance_initial_cycle(run).await.unwrap());
    let c = store.cycle(&actor, cycle).await.unwrap();
    assert_eq!(c.state, contracts::cycles::CycleState::Running);
    assert_eq!(c.next_action.as_deref(), Some("WAITING_FOR_CODEX_ACCOUNT"));
    let facts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$1)")
        .bind(run.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn exhausted_cpu_admission_keeps_no_half_mission_and_finishes_the_cycle_honestly(
    pool: PgPool,
) {
    let (store, actor) = research_support::operator(&pool).await;
    let mut f = cycle_support::setup(&pool, &store, &actor).await;
    let mut content = f.brief.content.clone();
    content.budget.max_cpu_seconds = DbCounter::new(1).unwrap();
    f.brief = store
        .update_brief(
            &actor,
            "one-cpu-second",
            f.brief.id,
            &contracts::brief::BriefUpdate {
                schema_version: SchemaV1,
                expected_revision: f.brief.revision,
                content,
                bindings: f.brief.bindings.clone(),
            },
        )
        .await
        .unwrap()
        .resource;
    f.freeze.expected_revision = f.brief.revision;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let started = f
        .start(&store, &actor, "start", &request)
        .await
        .unwrap()
        .resource;
    complete(&pool, &store, &f, started.run.id, false).await;
    assert!(store.advance_initial_cycle(started.run.id).await.unwrap());
    let c = store.cycle(&actor, started.cycle.id).await.unwrap();
    assert_eq!(c.state, contracts::cycles::CycleState::Completed);
    assert_eq!(
        c.outcome,
        Some(contracts::cycles::CycleOutcome::BudgetExhausted)
    );
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM app.runs),(SELECT count(*) FROM pgmq.q_runs)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 1, 1));
    assert_eq!(c.reserved_cpu_seconds.get(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn observed_thread_survives_profile_edit_without_permitting_a_new_paid_turn(pool: PgPool) {
    let (store, _, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let lease = mission_lease(&store).await;
    let receipt = native_thread();
    assert!(matches!(
        store
            .bind_mission_session(lease.run.id, &lease.fence, &receipt)
            .await,
        Err(StoreError::Conflict)
    ));
    let job = store.mission_job(lease.run.id, &lease.fence).await.unwrap();
    assert!(job.session.is_none());
    assert_eq!(job.profile.profile.id, f.researcher_profile.profile_id);
    assert_eq!(
        job.profile.profile.revision,
        f.researcher_profile.expected_revision
    );
    assert_eq!(job.role, "RESEARCHER");
    assert_eq!(job.brief_id, f.brief.id);
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    sqlx::query(
        "UPDATE app.codex_profiles SET name='edited during native thread start' WHERE id=$1",
    )
    .bind(f.researcher_profile.profile_id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    let (one, two) = tokio::join!(
        store.bind_mission_session(lease.run.id, &lease.fence, &receipt),
        store.bind_mission_session(lease.run.id, &lease.fence, &receipt)
    );
    let bound = one.unwrap();
    assert_eq!(bound, two.unwrap());
    assert_eq!(bound.native, receipt);
    assert_eq!(bound.requested_settings.profile_id, job.profile.profile.id);
    assert_eq!(
        bound.requested_settings.profile_revision,
        job.profile.profile.revision
    );
    assert!(bound.requested_settings.use_default_model_settings);
    let recorded = serde_json::to_value(&bound.requested_settings).unwrap();
    for field in ["model", "reasoning_effort", "service_tier"] {
        assert!(
            recorded.get(field).is_none(),
            "defaults omit native overrides"
        );
    }
    let restored = store.mission_job(lease.run.id, &lease.fence).await.unwrap();
    assert_eq!(restored.session, Some(bound));
    assert_eq!(
        restored.profile.profile.revision,
        f.researcher_profile.expected_revision
    );
    assert_eq!(restored.profile.profile.name, job.profile.profile.name);
    assert!(!store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let mut replacement = receipt;
    replacement.thread_id = Id::new().to_string();
    assert!(matches!(
        store
            .bind_mission_session(lease.run.id, &lease.fence, &replacement)
            .await,
        Err(StoreError::Conflict)
    ));
    let request = store::turns::TurnRequest {
        command_key: "forbidden-new-turn".into(),
        turn_kind: domain::admission::TurnKind::Research,
        tokens: DbCounter::new(1).unwrap(),
        estimated_cost: None,
        request_artifact_id: Id::new(),
        deadline_at: lease.run.deadline_at,
    };
    assert!(matches!(
        store
            .reserve_turn(lease.run.id, &lease.fence, &request)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let facts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.model_turn_reservations)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn changed_profile_before_native_start_never_grants_a_dispatch_permit(pool: PgPool) {
    let (store, _, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let lease = mission_lease(&store).await;
    sqlx::query("UPDATE app.codex_profiles SET name='edited before native start' WHERE id=$1")
        .bind(f.researcher_profile.profile_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.begin_run_dispatch(lease.run.id, &lease.fence).await,
        Err(StoreError::RevisionConflict { .. })
    ));
    assert!(matches!(
        store
            .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
            .await,
        Err(StoreError::Conflict)
    ));
    assert_eq!(
        store
            .mission_job(lease.run.id, &lease.fence)
            .await
            .unwrap()
            .lease
            .action,
        store::lifecycle::NextRuntimeAction::PrepareDispatch
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn lost_owner_cannot_bind_or_replace_the_canonical_thread(pool: PgPool) {
    let (store, _, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let lease = mission_lease(&store).await;
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    let receipt = native_thread();
    assert!(matches!(
        store
            .bind_mission_session(lease.run.id, &lease.fence, &receipt)
            .await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    let (msg_id, read_ct): (i64, i32) =
        sqlx::query_as("SELECT msg_id,read_ct FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(lease.run.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    let message = store::lifecycle::RunMessage {
        message_id: msg_id,
        read_count: read_ct,
        run_id: lease.run.id,
    };
    let Some(ClaimResult::Leased(replacement)) = store
        .claim_mission(&message, "replacement-owner", 60)
        .await
        .unwrap()
    else {
        panic!("takeover required");
    };
    assert_eq!(replacement.fence.attempt_id, lease.fence.attempt_id);
    assert!(replacement.fence.owner_epoch > lease.fence.owner_epoch);
    assert!(!store
        .begin_run_dispatch(lease.run.id, &replacement.fence)
        .await
        .unwrap());
    assert!(store
        .mission_job(lease.run.id, &replacement.fence)
        .await
        .unwrap()
        .session
        .is_none());
    let bound = store
        .bind_mission_session(lease.run.id, &replacement.fence, &receipt)
        .await
        .unwrap();
    assert_eq!(bound.native, receipt);
    assert!(matches!(
        store
            .bind_mission_session(lease.run.id, &lease.fence, &receipt)
            .await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_late_thread_receipt_does_not_undo_cancellation(pool: PgPool) {
    let (store, actor, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let lease = mission_lease(&store).await;
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let current = store.get_run(&actor, lease.run.id).await.unwrap();
    store
        .cancel_run(
            &actor,
            "cancel-mission",
            lease.run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    let bound = store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    let job = store.mission_job(lease.run.id, &lease.fence).await.unwrap();
    assert_eq!(job.session, Some(bound));
    assert_eq!(job.lease.run.state, RunState::CancelRequested);
    assert_eq!(
        job.lease.action,
        store::lifecycle::NextRuntimeAction::Cancel
    );
    assert!(!store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_first_turn_is_reserved_after_native_binding_without_faking_running(pool: PgPool) {
    let (store, actor, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let lease = mission_lease(&store).await;
    let parameters: uuid::Uuid = sqlx::query_scalar(
        "SELECT parameters_artifact_id FROM app.run_native_tasks WHERE run_id=$1",
    )
    .bind(preparation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let request = store::turns::TurnRequest {
        command_key: "first-native-turn".into(),
        turn_kind: domain::admission::TurnKind::Research,
        tokens: DbCounter::new(1).unwrap(),
        estimated_cost: f
            .brief
            .content
            .budget
            .cost_currency
            .as_ref()
            .map(|currency| domain::admission::CostEstimate {
                currency: currency.clone(),
                amount: "0.01".parse().unwrap(),
            }),
        request_artifact_id: parameters.to_string().try_into().unwrap(),
        deadline_at: lease.run.deadline_at,
    };
    assert!(matches!(
        store
            .reserve_turn(lease.run.id, &lease.fence, &request)
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    let first = store
        .reserve_turn(lease.run.id, &lease.fence, &request)
        .await
        .unwrap();
    assert!(matches!(
        store
            .claim_turn_dispatch(first.id, &lease.fence)
            .await
            .unwrap(),
        store::turns::DispatchDecision::Send { .. }
    ));
    assert!(matches!(
        store
            .claim_turn_dispatch(first.id, &lease.fence)
            .await
            .unwrap(),
        store::turns::DispatchDecision::Reconcile {
            native_turn_id: None
        }
    ));
    assert_eq!(
        store.get_run(&actor, lease.run.id).await.unwrap().state,
        RunState::Dispatching
    );
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM app.model_turn_dispatches),(SELECT count(*) FROM app.model_turn_receipts)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_changed_science_connection_is_not_mislabelled_as_the_frozen_runtime(pool: PgPool) {
    let (store, actor, f, cycle, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    sqlx::query("UPDATE app.runtime_integrations SET name='edited after preparation' WHERE id=$1")
        .bind(f.data.runtime.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let c = store.cycle(&actor, cycle).await.unwrap();
    assert_eq!(c.state, contracts::cycles::CycleState::WaitingInput);
    assert_eq!(
        c.next_action.as_deref(),
        Some("RESEARCH_INPUTS_REQUIRE_ATTENTION")
    );
    assert!(store
        .read_mission_messages(30, 100)
        .await
        .unwrap()
        .is_empty());
}
