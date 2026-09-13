//! Formal startup against real PostgreSQL/PGMQ. Parent native capability/data
//! records are explicit fixtures, not T42 or production data attestations.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{
    brief::{BriefState, BriefUpdate},
    control::ListQuery,
    cycles::*,
    Id, SchemaV1,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::StoreError;

async fn counts(pool: &PgPool) -> (i64, i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.research_cycles),(SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM app.run_events),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.command_receipts WHERE operation='CYCLE_START')")
        .fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn freeze_rejects_validation_fold_in_sealed_policy_without_admission(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let f = cycle_support::setup_with_policy(
        &pool,
        &store,
        &actor,
        objects,
        contracts::research::DataOrigin::Fixture,
        cycle_support::Liquidity::None,
        |policy| {
            policy.sealed_metric_requirements[0].scope = "asset:0/fold:0".into();
        },
    )
    .await;
    let before = counts(&pool).await;
    let error = store
        .freeze_brief(
            &actor,
            "invalid-sealed",
            f.brief.id,
            &f.freeze,
            |id, size| f.read(id, size),
        )
        .await
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("NATIVE_ALPHA_POLICY_UNSUPPORTED"),
        "{error:?}"
    );
    assert_eq!(
        store.brief(&actor, f.brief.id).await.unwrap().state,
        BriefState::Draft
    );
    assert_eq!(counts(&pool).await, before);
    let contexts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.brief_execution_contexts WHERE brief_id=$1")
            .bind(f.brief.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(contexts, 0);
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE idempotency_key='invalid-sealed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn formal_freeze_closes_brief_and_context_with_the_original_command_receipt(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    let before = store.project(&actor, f.data.project).await.unwrap();
    let frozen = store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    assert_eq!(frozen.resource.brief.state, BriefState::Frozen);
    assert!(frozen.resource.brief.frozen_at.is_some());
    assert_ne!(frozen.resource.brief.revision, f.brief.revision);
    assert_eq!(
        frozen.resource.execution_context,
        f.freeze.execution_context
    );
    let project = store.project(&actor, f.data.project).await.unwrap();
    assert_eq!(project.current_brief_id, Some(f.brief.id));
    assert_ne!(project.revision, before.revision);
    let replay = store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(replay.resource).unwrap(),
        serde_json::to_value(&frozen.resource).unwrap()
    );
    let request = BriefUpdate {
        schema_version: SchemaV1,
        expected_revision: frozen.resource.brief.revision,
        content: f.brief.content.clone(),
        bindings: f.brief.bindings.clone(),
    };
    assert!(store
        .update_brief(&actor, "mutate", f.brief.id, &request)
        .await
        .is_err());
    let mutation = sqlx::query("UPDATE app.brief_execution_contexts SET runtime_revision=runtime_revision+1 WHERE brief_id=$1")
        .bind(f.brief.id.as_uuid()).execute(&pool).await.unwrap_err();
    assert_eq!(
        mutation.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
    let visible = store.frozen_brief(&actor, f.brief.id).await.unwrap();
    assert_eq!(visible.execution_context, f.freeze.execution_context);
    let stored: Vec<(String, String)> =
        sqlx::query_as("SELECT origin,pit_status FROM app.dataset_revisions WHERE id=ANY($1)")
            .bind(vec![
                f.data.discovery.as_uuid(),
                f.data.validation.as_uuid(),
                f.data.sealed.as_uuid(),
            ])
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        stored
            .iter()
            .all(|(origin, pit)| origin == "FIXTURE" && pit == "UNVERIFIED"),
        "freeze must not convert fixture data into REAL/PIT evidence"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn freeze_rejects_revision_or_input_mismatch_without_a_half_frozen_context(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    let mut invalid = f.freeze.clone();
    invalid.expected_revision = "99".to_owned().try_into().unwrap();
    assert!(matches!(
        store
            .freeze_brief(&actor, "stale", f.brief.id, &invalid, |id, size| f
                .read(id, size))
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    invalid = f.freeze.clone();
    invalid.execution_context.validation_input_set_id =
        invalid.execution_context.discovery_input_set_id;
    assert!(store
        .freeze_brief(&actor, "wrong-input", f.brief.id, &invalid, |id, size| f
            .read(id, size))
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.brief_execution_contexts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        store.brief(&actor, f.brief.id).await.unwrap().state,
        BriefState::Draft
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn freeze_rechecks_only_registered_validation_metadata_and_keeps_failure_atomic(
    pool: PgPool,
) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    let metadata: uuid::Uuid = sqlx::query_scalar("SELECT native_metadata_artifact_id FROM app.dataset_registration_evidence WHERE dataset_revision_id=$1")
        .bind(f.data.validation.as_uuid()).fetch_one(&pool).await.unwrap();
    for missing in [true, false] {
        let result = store
            .freeze_brief(&actor, "metadata", f.brief.id, &f.freeze, |id, size| {
                let fixture = &f;
                async move {
                    assert_eq!(id.as_uuid(), metadata, "no raw market or Sealed reads");
                    if missing {
                        return Err(StoreError::Integrity);
                    }
                    let mut bytes = fixture.read(id, size).await?;
                    bytes.fill(b'x');
                    Ok(bytes)
                }
            })
            .await;
        assert!(result.is_err());
        assert_eq!(
            store.brief(&actor, f.brief.id).await.unwrap().state,
            BriefState::Draft
        );
        let published: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM app.brief_execution_contexts)+(SELECT count(*) FROM app.command_receipts WHERE operation='BRIEF_FREEZE')")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(published, 0);
    }
    store
        .freeze_brief(&actor, "metadata", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    assert!(
        store
            .freeze_brief(&actor, "metadata", f.brief.id, &f.freeze, |_, _| async {
                panic!("exact receipt replay must not reread or gain new authority")
            })
            .await
            .unwrap()
            .replayed
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cycle_run_event_admission_queue_and_receipt_are_created_once(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let (a, b) = tokio::join!(
        f.start(&store, &actor, "start", &request),
        f.start(&store, &actor, "start", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        serde_json::to_value(&a.resource).unwrap(),
        serde_json::to_value(b.resource).unwrap()
    );
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 1));
    assert_eq!(a.resource.cycle.initial_run_id, Some(a.resource.run.id));
    assert_eq!(
        a.resource.cycle.researcher_profile,
        Some(f.researcher_profile)
    );
    assert_eq!(a.resource.cycle.reviewer_profile, Some(f.reviewer_profile));
    assert_eq!(a.resource.run.cycle_id, Some(a.resource.cycle.id));
    assert_eq!(a.resource.cycle.reserved_experiments, 0);
    assert!(a.resource.cycle.reserved_cpu_seconds.get() > 0);
    let events = &a.resource.cycle.available_actions;
    assert_eq!(
        events,
        &[
            CycleReadAction::ViewBrief,
            CycleReadAction::ViewRuns,
            CycleReadAction::ViewExperiments
        ]
    );
    let page = store
        .cycles(&actor, f.data.project, &ListQuery::default())
        .await
        .unwrap();
    assert_eq!(page.items[0].id, a.resource.cycle.id);
    assert_eq!(
        store
            .cycle(&actor, a.resource.cycle.id)
            .await
            .unwrap()
            .initial_run_id,
        Some(a.resource.run.id)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cycle_choices_are_explicit_revision_locked_and_never_rewritten_by_profile_updates(
    pool: PgPool,
) {
    use contracts::codex::{CodexConnectionUpdateV1, CodexProfileUpdateV1};
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let mut invalid = request.clone();
    invalid.request.reviewer_profile.profile_id = Id::new();
    assert!(matches!(
        f.start(&store, &actor, "missing-profile", &invalid).await,
        Err(StoreError::NotFound)
    ));
    invalid = request.clone();
    invalid.request.reviewer_profile.expected_revision = "99".to_owned().try_into().unwrap();
    assert!(matches!(
        f.start(&store, &actor, "stale-profile", &invalid).await,
        Err(StoreError::RevisionConflict { .. })
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));

    let first = f
        .start(&store, &actor, "start", &request)
        .await
        .unwrap()
        .resource;
    let old = store
        .codex_profile(&actor, f.researcher_profile.profile_id)
        .await
        .unwrap();
    let updated = store
        .update_codex_profile(
            &actor,
            "change-profile",
            old.id,
            &CodexProfileUpdateV1 {
                schema_version: SchemaV1,
                expected_revision: old.revision,
                name: "changed native profile".into(),
                connection: CodexConnectionUpdateV1::System {},
                model_settings: old.model_settings,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_ne!(updated.revision, f.researcher_profile.expected_revision);
    let replay = f.start(&store, &actor, "start", &request).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(&replay.resource).unwrap(),
        serde_json::to_value(&first).unwrap()
    );
    let visible = store.cycle(&actor, first.cycle.id).await.unwrap();
    assert_eq!(visible.researcher_profile, Some(f.researcher_profile));
    assert_eq!(visible.reviewer_profile, Some(f.reviewer_profile));
    for (statement, code) in [
        ("INSERT INTO app.cycle_startups(cycle_id,project_id,initial_run_id) SELECT cycle_id,project_id,initial_run_id FROM app.cycle_startups WHERE cycle_id=$1", "23514"),
        ("INSERT INTO app.cycle_startups SELECT cycle_id,project_id,initial_run_id,created_at,researcher_profile_id,researcher_profile_revision,reviewer_profile_id,reviewer_profile_revision FROM app.cycle_startups WHERE cycle_id=$1", "23514"),
    ] {
        // BEFORE INSERT must reject missing/stale choices before uniqueness is
        // checked; updating the source profile above makes its old revision stale.
        let error = sqlx::query(statement).bind(first.cycle.id.as_uuid()).execute(&pool).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(), Some(code));
    }
    let mut current = request.clone();
    current.request.expected_revision = store
        .project(&actor, f.data.project)
        .await
        .unwrap()
        .revision;
    assert!(matches!(
        f.start(&store, &actor, "stale-after-edit", &current).await,
        Err(StoreError::RevisionConflict { .. })
    ));
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 1));
    let mutation = sqlx::query(
        "UPDATE app.cycle_startups SET researcher_profile_revision=$2 WHERE cycle_id=$1",
    )
    .bind(first.cycle.id.as_uuid())
    .bind(updated.revision.get() as i64)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        mutation.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
    let mut changed = request.clone();
    changed.request.researcher_profile.expected_revision = updated.revision;
    assert!(
        f.start(&store, &actor, "start", &changed).await.is_err(),
        "same command key cannot select a different profile revision"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cycle_cannot_start_while_native_account_change_is_in_flight(pool: PgPool) {
    use contracts::codex::{CodexAccountActionV1, CodexAccountRequestV1};
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    store
        .prepare_codex_account(
            &actor,
            "login",
            &CodexAccountRequestV1 {
                schema_version: SchemaV1,
                profile_id: f.reviewer_profile.profile_id,
                expected_revision: f.reviewer_profile.expected_revision,
            },
            CodexAccountActionV1::Login,
        )
        .await
        .unwrap();
    assert!(matches!(
        f.start(&store, &actor, "during-login", &request).await,
        Err(StoreError::Conflict)
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn initial_cycle_run_has_one_native_definition_with_exact_discovery_parameters(pool: PgPool) {
    use contracts::{
        execution::NativeTaskParametersV1,
        research::{ArtifactInputRole, DataPartition},
        runtime_jobs::RuntimeInputV1,
    };
    use store::lifecycle::ClaimResult;
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(
            &actor,
            "freeze-native",
            f.brief.id,
            &f.freeze,
            |id, size| f.read(id, size),
        )
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let started = f
        .start(&store, &actor, "native-start", &request)
        .await
        .unwrap();
    let messages = store.read_native_run_messages(30, 100).await.unwrap();
    assert_eq!(
        messages.len(),
        1,
        "Cycle preparation must be selectable by the native driver"
    );
    assert_eq!(messages[0].run_id, started.resource.run.id);
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&messages[0], "cycle-native-owner", 30)
        .await
        .unwrap()
    else {
        panic!("a genuine native Cycle preparation lease is required");
    };
    let job = store
        .native_job(started.resource.run.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(job.run.cycle_id, Some(started.resource.cycle.id));
    assert_eq!(
        job.spec.input_set_id,
        f.freeze.execution_context.discovery_input_set_id
    );
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|item| match item {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                role: ArtifactInputRole::Parameters,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let bytes = f
        .objects
        .read(job.spec.parameters_artifact_id, size)
        .unwrap();
    let parameters: NativeTaskParametersV1 = serde_json::from_slice(&bytes).unwrap();
    domain::execution::task(&job.spec, &parameters).unwrap();
    let NativeTaskParametersV1::ValidateData { selections, .. } = parameters else {
        panic!("Cycle startup must use fixed validation parameters");
    };
    assert_eq!(selections.len(), 1);
    assert_eq!(selections[0].dataset_revision_id, f.data.discovery);
    assert_eq!(
        job.spec.inputs.len(),
        2,
        "unrelated InputSet artifacts must not enter the job"
    );
    assert!(
        matches!(&job.spec.inputs[0], RuntimeInputV1::Dataset { revision_id, role:DataPartition::Discovery, .. } if *revision_id == f.data.discovery)
    );
    assert!(job.spec.inputs.iter().all(|item| !matches!(
        item,
        RuntimeInputV1::Dataset {
            role: DataPartition::Sealed,
            ..
        }
    )));
    let facts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_native_tasks),(SELECT count(*) FROM app.run_native_attempts),(SELECT count(*) FROM app.qualifications)",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 0));
    // An exact completed command remains a read even if storage is temporarily
    // unavailable; it never allocates a replacement Run, object or retry key.
    let replay = store
        .start_cycle(
            &actor,
            "native-start",
            &request,
            |_, _| async { panic!("exact command replay must not read native metadata") },
            |_| async { panic!("exact command replay must not publish a second parameter object") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(replay.resource).unwrap(),
        serde_json::to_value(started.resource).unwrap()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_cycle_parameter_io_failure_cannot_leave_budget_run_or_queue_half_state(
    pool: PgPool,
) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze-io", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let missing = store
        .start_cycle(
            &actor,
            "native-io",
            &request,
            |_, _| async { Err(StoreError::Integrity) },
            |_| async { panic!("missing metadata must not reach publication") },
        )
        .await;
    assert!(matches!(missing, Err(StoreError::Integrity)));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));
    let reader = f.objects.clone();
    let write_failed = store
        .start_cycle(
            &actor,
            "native-io",
            &request,
            move |id, size| {
                let objects = reader.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            |_| async { Err(StoreError::Integrity) },
        )
        .await;
    assert!(matches!(write_failed, Err(StoreError::Integrity)));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));
    let native: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_native_tasks),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.native_task')",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(native, (0, 0));
    assert!(
        !f.start(&store, &actor, "native-io", &request)
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn failure_after_queue_enqueue_rolls_back_the_entire_official_start_command(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    sqlx::query("CREATE FUNCTION app.fail_startup_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='injected failure after native queue publication'; END $$")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_startup_fixture BEFORE INSERT ON app.cycle_startups FOR EACH ROW EXECUTE FUNCTION app.fail_startup_fixture()")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        f.start(&store, &actor, "atomic", &request).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));
    sqlx::query("DROP TRIGGER fail_startup_fixture ON app.cycle_startups")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !f.start(&store, &actor, "atomic", &request)
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn frozen_inputs_do_not_retain_permission_after_revocation(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'TEST_REVOKED','test the current license authority')")
        .bind(f.data.grant.as_uuid()).execute(&pool).await.unwrap();
    assert!(f.start(&store, &actor, "revoked", &request).await.is_err());
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 0));
    assert_eq!(
        store.brief(&actor, f.brief.id).await.unwrap().state,
        BriefState::Frozen
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn daily_cycle_quota_and_paused_project_are_checked_in_the_start_transaction(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let mut request = cycle_support::start_request(&store, &actor, &f).await;
    for number in 0..f.brief.content.budget.max_cycles_per_day {
        f.start(&store, &actor, &format!("daily-{number}"), &request)
            .await
            .unwrap();
    }
    assert!(matches!(
        f.start(&store, &actor, "quota", &request).await,
        Err(StoreError::Domain(domain::DomainError::BudgetExhausted(
            "cycles_per_day"
        )))
    ));
    let revision: i64 =
        sqlx::query_scalar("UPDATE app.projects SET state='PAUSED' WHERE id=$1 RETURNING revision")
            .bind(f.data.project.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    request.request.expected_revision = revision.to_string().try_into().unwrap();
    assert!(matches!(
        f.start(&store, &actor, "paused", &request).await,
        Err(StoreError::Domain(domain::DomainError::AdmissionClosed))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn caller_cannot_commit_an_execution_context_without_freezing_the_brief(pool: PgPool) {
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup(&pool, &store, &actor).await;
    let c = &f.freeze.execution_context;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.brief_execution_contexts(brief_id,project_id,runtime_id,runtime_revision,discovery_input_set_id,validation_input_set_id,sealed_input_set_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(f.brief.id.as_uuid()).bind(f.data.project.as_uuid()).bind(c.runtime_id.as_uuid()).bind(c.runtime_revision.get() as i64)
        .bind(c.discovery_input_set_id.as_uuid()).bind(c.validation_input_set_id.as_uuid()).bind(c.sealed_input_set_id.as_uuid())
        .execute(&mut *tx).await.unwrap();
    let error = tx.commit().await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
}

#[test]
fn public_start_and_freeze_contracts_do_not_accept_success_or_runtime_override_fields() {
    let start = json!({"schema_version":1,"brief_id":Id::new(),"expected_revision":"1"});
    for field in [
        "outcome",
        "budget_snapshot",
        "state",
        "runtime_id",
        "run_id",
        "trigger",
        "wake_id",
    ] {
        let mut injected = start.clone();
        injected[field] = json!("injected");
        assert!(serde_json::from_value::<CycleStartV1>(injected).is_err());
    }
    let freeze: Value = json!({"schema_version":1,"expected_revision":"1","execution_context":{"schema_version":1,"runtime_id":Id::new(),"runtime_revision":"1","discovery_input_set_id":Id::new(),"validation_input_set_id":Id::new(),"sealed_input_set_id":Id::new()}});
    assert!(serde_json::from_value::<BriefFreezeV1>(freeze.clone()).is_ok());
    let mut injected = freeze;
    injected["capabilities"] = json!({"status":"AVAILABLE"});
    assert!(serde_json::from_value::<BriefFreezeV1>(injected).is_err());
}
