//! Formal Cycle -> native result -> Mission admission. Model/OCI execution is
//! not claimed by the explicit native result fixture used in these PG tests.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{
    control::ProjectUpdate,
    runs::{ProjectState, RunKind, RunState},
    DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use store::{authority::Actor, lifecycle::ClaimResult, Store, StoreError};

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

use mission_support::{complete, setup};

async fn prepare_prompt(
    store: &Store,
    lease: &store::lifecycle::RunLease,
    f: &cycle_support::Fixture,
    request: &store::turns::TurnRequest,
    prompt: &str,
) -> Result<store::turns::Reservation, StoreError> {
    let reading = f.objects.clone();
    let publishing = f.objects.clone();
    store.prepare_mission_turn(lease.run.id,&lease.fence,request,prompt,
        move |id,size| async move {reading.read(id,size).map_err(|_|StoreError::Integrity)},
        move |object| async move {publishing.put(object.id,&object.bytes).map_err(|_|StoreError::Integrity)}).await
}

fn prompt_request(
    f: &cycle_support::Fixture,
    lease: &store::lifecycle::RunLease,
) -> store::turns::TurnRequest {
    store::turns::TurnRequest {
        command_key: "native-request".into(),
        turn_kind: domain::admission::TurnKind::Research,
        tokens: DbCounter::new(100).unwrap(),
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
        request_artifact_id: Id::new(),
        deadline_at: lease.run.deadline_at,
    }
}

async fn prepare_initial(
    store: &Store,
    lease: &store::lifecycle::RunLease,
    f: &cycle_support::Fixture,
) -> Result<(), StoreError> {
    let reading = f.objects.clone();
    let publishing = f.objects.clone();
    store.prepare_initial_mission_turn(lease.run.id, &lease.fence,
        move |id, size| async move { reading.read(id, size).map_err(|_| StoreError::Integrity) },
        move |object| async move { publishing.put(object.id, &object.bytes).map_err(|_| StoreError::Integrity) }).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn initial_request_uses_remaining_budget_once_and_never_replaces_unknown_sent_work(
    pool: PgPool,
) {
    let (store, _, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    store.advance_initial_cycle(preparation).await.unwrap();
    let lease = mission_lease(&store).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    assert!(matches!(
        store
            .prepare_initial_mission_turn(
                lease.run.id,
                &lease.fence,
                |_, _| async { Err(StoreError::Integrity) },
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.model_turn_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, 0,
        "failed publication cannot reserve model spending"
    );
    prepare_initial(&store, &lease, &f).await.unwrap();
    let original = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap();
    assert_eq!(
        original.reservation.tokens,
        f.brief.content.budget.max_tokens.unwrap()
    );
    assert_eq!(original.reservation.ordinal, 1);
    assert!(!original.sent && original.receipt.is_none());
    let reading = f.objects.clone();
    let prompt = store.mission_turn_prompt(lease.run.id, &lease.fence, original.reservation.id,
        move |id, size| async move {reading.read(id, size).map_err(|_| StoreError::Integrity)}).await.unwrap();
    assert!(prompt.starts_with("QZ_MISSION_INITIAL_V1\n"));
    assert!(prompt.contains(&f.brief.id.to_string()) && prompt.contains(&lease.run.id.to_string()));
    store
        .claim_turn_dispatch(original.reservation.id, &lease.fence)
        .await
        .unwrap();
    sqlx::query("UPDATE app.codex_profiles SET name='changed after first intent' WHERE id=$1")
        .bind(f.researcher_profile.profile_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    prepare_initial(&store, &lease, &f).await.unwrap();
    let checkpoint = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(checkpoint.accounted_tokens, DbCounter::ZERO);
    let latest = checkpoint.latest.unwrap();
    assert_eq!(latest.reservation, original.reservation);
    assert!(latest.sent && latest.native_turn_id.is_none() && latest.receipt.is_none());
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM pgmq.q_model_turns),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.mission_turn')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn initial_request_refuses_unavailable_native_pricing_without_a_reservation(pool: PgPool) {
    let (store, _, f, _, preparation) = mission_support::setup_with_cost(&pool, true).await;
    complete(&pool, &store, &f, preparation, false).await;
    store.advance_initial_cycle(preparation).await.unwrap();
    let lease = mission_lease(&store).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    assert!(matches!(
        prepare_initial(&store, &lease, &f).await,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("native_cost_usage")
        ))
    ));
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM pgmq.q_model_turns),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.mission_turn')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_token_stop_is_fenced_idempotent_and_does_not_settle_usage(pool: PgPool) {
    let (store, actor, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    store.advance_initial_cycle(preparation).await.unwrap();
    let lease = mission_lease(&store).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    let request = prompt_request(&f, &lease);
    let reserved = prepare_prompt(&store, &lease, &f, &request, "Bounded request fixture")
        .await
        .unwrap();
    // An unsent or unrelated reservation is not a native observation.
    assert!(matches!(
        store
            .observe_mission_token_limit(lease.run.id, &lease.fence, reserved.id, reserved.tokens)
            .await,
        Err(StoreError::Conflict)
    ));
    store
        .claim_turn_dispatch(reserved.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(reserved.id, &lease.fence, "token-limit-native")
        .await
        .unwrap();
    assert!(matches!(
        store
            .observe_mission_token_limit(lease.run.id, &lease.fence, Id::new(), reserved.tokens)
            .await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store
            .observe_mission_token_limit(
                lease.run.id,
                &lease.fence,
                reserved.id,
                DbCounter::new(99).unwrap()
            )
            .await,
        Err(StoreError::Invalid("native_token_limit_not_reached"))
    ));
    let (a, b) = tokio::join!(
        store.observe_mission_token_limit(lease.run.id, &lease.fence, reserved.id, reserved.tokens),
        store.observe_mission_token_limit(lease.run.id, &lease.fence, reserved.id, reserved.tokens)
    );
    a.unwrap();
    b.unwrap();
    let run = store.get_run(&actor, lease.run.id).await.unwrap();
    assert_eq!(run.state, RunState::CancelRequested);
    let events = store
        .run_events(&actor, run.id, DbCounter::ZERO, 100)
        .await
        .unwrap();
    let limits: Vec<_> = events
        .events
        .iter()
        .filter(|event| event.event_type == "mission.token_limit")
        .collect();
    assert_eq!(limits.len(), 1);
    assert_eq!(
        limits[0].payload,
        serde_json::json!({"schema_version":1,"reservation_id":reserved.id,"observed_tokens":"100","reserved_tokens":"100"})
    );
    let checkpoint = store
        .mission_turn_checkpoint(run.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(checkpoint.accounted_tokens, DbCounter::ZERO);
    assert!(checkpoint.latest.unwrap().receipt.is_none());
    sqlx::query("UPDATE app.run_attempts SET worker_owner_id='takeover',owner_epoch=owner_epoch+1 WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store
            .observe_mission_token_limit(run.id, &lease.fence, reserved.id, reserved.tokens)
            .await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    assert_eq!(store.get_run(&actor, run.id).await.unwrap(), run);
}

#[sqlx::test(migrations = "../../migrations")]
async fn public_turn_request_and_reservation_commit_once_and_unknown_send_keeps_the_original(
    pool: PgPool,
) {
    let (store, _, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    store.advance_initial_cycle(preparation).await.unwrap();
    let lease = mission_lease(&store).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    let request = prompt_request(&f, &lease);
    let text = "Read the frozen Brief through scoped MCP; never infer a scientific result.";
    let (first, second) = tokio::join!(
        prepare_prompt(&store, &lease, &f, &request, text),
        prepare_prompt(&store, &lease, &f, &request, text)
    );
    let reserved = first.unwrap();
    assert_eq!(second.unwrap(), reserved);
    prepare_initial(&store, &lease, &f).await.unwrap();
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.artifacts WHERE schema_name='qz.mission_turn'),(SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM pgmq.q_model_turns)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 1));
    assert!(matches!(
        prepare_prompt(&store, &lease, &f, &request, "changed prompt").await,
        Err(StoreError::Conflict)
    ));
    let mut changed = request.clone();
    changed.command_key = "different-turn".into();
    assert!(matches!(
        prepare_prompt(&store, &lease, &f, &changed, text).await,
        Err(StoreError::Conflict)
    ));
    changed.request_artifact_id = Id::new();
    assert!(matches!(
        prepare_prompt(&store, &lease, &f, &changed, text).await,
        Err(StoreError::TurnPending)
    ));
    assert!(matches!(
        store
            .claim_turn_dispatch(reserved.id, &lease.fence)
            .await
            .unwrap(),
        store::turns::DispatchDecision::Send { .. }
    ));
    // A changed profile prevents new paid calls, not exact request recovery.
    sqlx::query("UPDATE app.codex_profiles SET name='changed after original intent' WHERE id=$1")
        .bind(f.researcher_profile.profile_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        prepare_prompt(&store, &lease, &f, &request, text)
            .await
            .unwrap(),
        reserved
    );
    let reading = f.objects.clone();
    assert_eq!(store.mission_turn_prompt(lease.run.id,&lease.fence,reserved.id,
        move |id,size|async move{reading.read(id,size).map_err(|_|StoreError::Integrity)}).await.unwrap(),text);
    let checkpoint = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(checkpoint.accounted_tokens, DbCounter::ZERO);
    let pending = checkpoint.latest.unwrap();
    assert!(
        pending.sent
            && pending.native_turn_id.is_none()
            && pending.terminal.is_none()
            && pending.receipt.is_none()
    );
    assert_eq!(pending.reservation, reserved);
    assert!(matches!(
        store
            .claim_turn_dispatch(reserved.id, &lease.fence)
            .await
            .unwrap(),
        store::turns::DispatchDecision::Reconcile {
            native_turn_id: None
        }
    ));
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store
            .mission_turn_checkpoint(lease.run.id, &lease.fence)
            .await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_or_lease_expired_request_publication_leaves_no_reservation_or_queue_half_state(
    pool: PgPool,
) {
    let (store, _, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    store.advance_initial_cycle(preparation).await.unwrap();
    let lease = mission_lease(&store).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    let request = prompt_request(&f, &lease);
    assert!(matches!(
        store
            .prepare_mission_turn(
                lease.run.id,
                &lease.fence,
                &request,
                "controlled request",
                |_, _| async { panic!("new request must not read a nonexistent object") },
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let publishing = f.objects.clone();
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()+interval '1 second' WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    let result = store
        .prepare_mission_turn(
            lease.run.id,
            &lease.fence,
            &request,
            "controlled request",
            |_, _| async { panic!("rolled-back metadata must not look published") },
            move |object| async move {
                publishing
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)?;
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                Ok(())
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.artifacts WHERE schema_name='qz.mission_turn'),(SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM pgmq.q_model_turns)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0, 0));
    let objects = f.objects.clone();
    assert!(store
        .discard_unpublished_native_object(
            lease.run.id,
            request.request_artifact_id,
            move |id| async move {
                objects
                    .discard_unpublished(id)
                    .map_err(|_| StoreError::Integrity)
            }
        )
        .await
        .unwrap());
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

#[sqlx::test(migrations = "../../migrations")]
async fn native_mission_issuance_is_once_per_owner_and_old_tokens_do_not_borrow_takeover(
    pool: PgPool,
) {
    use integrations::{
        authentication::{capability_verifier, random_capability, verify_capability},
        secrets::SecretVault,
    };
    use std::os::unix::fs::DirBuilderExt;
    let (store, actor, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let lease = mission_lease(&store).await;
    let directory = tempfile::tempdir().unwrap();
    let key = directory.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let secrets = directory.path().join("secrets");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&secrets)
        .unwrap();
    let vault = SecretVault::open(&secrets, &key).unwrap();
    let secret = random_capability();
    let verifier = capability_verifier(&secret).unwrap();
    let reference = vault.put("MACHINE_VERIFIER", verifier.as_bytes()).unwrap();
    let token = Id::new();
    let (one, two) = tokio::join!(
        store.issue_mission_credential(lease.run.id, &lease.fence, token, reference),
        store.issue_mission_credential(lease.run.id, &lease.fence, token, reference)
    );
    let credential = one.unwrap();
    assert_eq!(credential, two.unwrap());
    let challenge = store.machine_challenge(token).await.unwrap();
    assert_eq!(challenge.credential_id, credential);
    let bytes = vault
        .read(challenge.verifier_ref, "MACHINE_VERIFIER")
        .unwrap();
    assert!(verify_capability(
        &secret,
        std::str::from_utf8(&bytes).unwrap()
    ));
    let machine = challenge.verified_actor(None);
    let view = store.machine_session(&machine).await.unwrap();
    assert_eq!(view.kind, contracts::control::PrincipalKind::Mission);
    assert_eq!(view.run_id, Some(lease.run.id));
    assert_eq!(view.project_id, Some(lease.run.project_id));
    assert_eq!(view.expires_at, lease.run.deadline_at);
    assert_eq!(view.downstream_id, None);
    let scopes: std::collections::BTreeSet<_> =
        view.scope_codes.into_iter().map(|s| s.code()).collect();
    assert_eq!(
        scopes,
        [
            "RESEARCH_READ",
            "EXPERIMENT_SUBMIT",
            "ARTIFACT_SUBMIT",
            "EVIDENCE_READ",
            "RUN_READ"
        ]
        .into_iter()
        .collect()
    );
    assert!(matches!(
        store
            .issue_mission_credential(lease.run.id, &lease.fence, token, Id::new())
            .await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store
            .issue_mission_credential(lease.run.id, &lease.fence, Id::new(), reference)
            .await,
        Err(StoreError::Conflict)
    ));
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(store.machine_session(&machine).await.is_err());
    let (message_id, read_count): (i64, i32) =
        sqlx::query_as("SELECT msg_id,read_ct FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(lease.run.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    let message = store::lifecycle::RunMessage {
        message_id,
        read_count,
        run_id: lease.run.id,
    };
    let Some(ClaimResult::Leased(next)) = store
        .claim_mission(&message, "replacement-native-owner", 60)
        .await
        .unwrap()
    else {
        panic!("takeover required");
    };
    assert!(store.machine_session(&machine).await.is_err());
    let next_secret = random_capability();
    let next_verifier = capability_verifier(&next_secret).unwrap();
    let next_ref = vault
        .put("MACHINE_VERIFIER", next_verifier.as_bytes())
        .unwrap();
    let next_token = Id::new();
    store
        .issue_mission_credential(lease.run.id, &next.fence, next_token, next_ref)
        .await
        .unwrap();
    assert!(store.machine_challenge(token).await.is_err());
    assert!(
        vault.read(reference, "MACHINE_VERIFIER").is_ok(),
        "historical verifier is not deleted during rotation"
    );
    let next_challenge = store.machine_challenge(next_token).await.unwrap();
    let bytes = vault
        .read(next_challenge.verifier_ref, "MACHINE_VERIFIER")
        .unwrap();
    assert!(verify_capability(
        &next_secret,
        std::str::from_utf8(&bytes).unwrap()
    ));
    let next_actor = next_challenge.verified_actor(None);
    assert!(store.machine_session(&next_actor).await.is_ok());
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.machine_principals),(SELECT count(*) FROM app.machine_credentials),(SELECT max(credential_epoch)::bigint FROM app.machine_principals)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 2, 2));
    sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) VALUES($1,clock_timestamp(),'controlled native issuance revocation')")
        .bind(next_challenge.credential_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store
            .issue_mission_credential(lease.run.id, &next.fence, next_token, next_ref)
            .await,
        Err(StoreError::Conflict)
    ));
    sqlx::query("UPDATE app.machine_principals SET enabled=false,credential_epoch=credential_epoch+1 WHERE run_id=$1")
        .bind(lease.run.id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store
            .issue_mission_credential(lease.run.id, &next.fence, Id::new(), Id::new())
            .await,
        Err(StoreError::Forbidden)
    ));
    let current = store.get_run(&actor, lease.run.id).await.unwrap();
    store
        .cancel_run(
            &actor,
            "stop-mission",
            lease.run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    assert!(store.machine_session(&next_actor).await.is_err());
    assert!(matches!(
        store
            .issue_mission_credential(lease.run.id, &next.fence, Id::new(), Id::new())
            .await,
        Err(StoreError::Domain(domain::DomainError::AdmissionClosed))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_science_attempt_cannot_obtain_mission_credentials(pool: PgPool) {
    let (store, _, _f, _, run) = setup(&pool).await;
    let message = store
        .read_native_run_messages(30, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == run)
        .unwrap();
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "science-only", 60)
        .await
        .unwrap()
    else {
        panic!("science lease required");
    };
    assert!(matches!(
        store
            .issue_mission_credential(run, &lease.fence, Id::new(), Id::new())
            .await,
        Err(StoreError::Invalid("mission_not_defined"))
    ));
    let facts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.machine_principals),(SELECT count(*) FROM app.machine_credentials)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0));
}
