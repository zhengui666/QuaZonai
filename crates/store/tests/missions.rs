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

async fn summary(
    store: &Store,
    lease: &store::lifecycle::RunLease,
    f: &cycle_support::Fixture,
    reservation: Id,
    text: &store::turns::NativePublicSummary,
) -> Result<Id, StoreError> {
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    store.record_mission_summary(reservation,&lease.fence,text,
        move |id,size|async move {reading.read(id,size).map_err(|_|StoreError::Integrity)},
        move |object|async move {writing.put(object.id,&object.bytes).map_err(|_|StoreError::Integrity)}).await
}

async fn cancel(store: &Store, actor: &Actor, run: Id) {
    let run = store.get_run(actor, run).await.unwrap();
    let result = store
        .cancel_run(
            actor,
            "cancel-mission",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(result.state, RunState::CancelRequested);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reviewer_turn_and_summary_keep_original_role_without_general_sealed_access(pool: PgPool) {
    use store::turns::{NativePublicSummary, TurnOutcome, UsageReceipt};
    let (store, actor, f, cycle, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    let context = &f.freeze.execution_context;
    let admitted = store
        .enqueue_run(
            "controlled-reviewer",
            &store::lifecycle::RunSubmission {
                cycle_id: cycle,
                input_set_id: context.discovery_input_set_id,
                runtime_id: context.runtime_id,
                runtime_revision: context.runtime_revision,
                kind: RunKind::AgentResearch,
                limits: contracts::lifecycle::JobLimitsV1 {
                    schema_version: SchemaV1,
                    experiments: 0,
                    cpu_seconds: DbCounter::new(10).unwrap(),
                    wall_seconds: 60,
                    memory_mib: 1024,
                    output_bytes: DbCounter::new(1048576).unwrap(),
                },
            },
        )
        .await
        .unwrap()
        .resource;
    let profile = store
        .codex_profile(&actor, f.reviewer_profile.profile_id)
        .await
        .unwrap();
    // Controlled role association only: this test does not claim automatic
    // Reviewer admission, native inference or a scientific qualification.
    sqlx::query("INSERT INTO app.run_missions(run_id,project_id,cycle_id,role,profile_id,profile_revision,profile_snapshot) VALUES($1,$2,$3,'INDEPENDENT_REVIEWER',$4,$5,$6)")
        .bind(admitted.id.as_uuid()).bind(admitted.project_id.as_uuid()).bind(cycle.as_uuid())
        .bind(profile.id.as_uuid()).bind(profile.revision.get() as i64)
        .bind(serde_json::json!({"schema_version":1,"profile":profile})).execute(&pool).await.unwrap();
    let lease = mission_lease(&store).await;
    assert_eq!(lease.run.id, admitted.id);
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(lease.run.id, &lease.fence, &native_thread())
        .await
        .unwrap();
    let request = prompt_request(&f, &lease);
    let text = "Controlled independent review input; no raw Sealed rows or research conversation.";
    let reserved = prepare_prompt(&store, &lease, &f, &request, text)
        .await
        .unwrap();
    assert_eq!(
        prepare_prompt(&store, &lease, &f, &request, text)
            .await
            .unwrap(),
        reserved
    );
    let reading = f.objects.clone();
    assert_eq!(store.mission_turn_prompt(lease.run.id, &lease.fence, reserved.id,
        move |id, size| async move { reading.read(id, size).map_err(|_| StoreError::Integrity) }).await.unwrap(), text);
    let access: String = sqlx::query_scalar("SELECT access_class FROM app.artifacts WHERE id=$1")
        .bind(request.request_artifact_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(access, "EVALUATOR_ONLY");
    assert!(matches!(
        store
            .artifact_content(&actor, request.request_artifact_id)
            .await,
        Err(StoreError::NotFound)
    ));
    store
        .claim_turn_dispatch(reserved.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(reserved.id, &lease.fence, "controlled-review-turn")
        .await
        .unwrap();
    store
        .observe_mission_turn_terminal(
            reserved.id,
            &lease.fence,
            TurnOutcome::Succeeded,
            "NATIVE_TURN_COMPLETED",
        )
        .await
        .unwrap();
    store
        .settle_turn(
            reserved.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(12).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "NATIVE_TURN_COMPLETED".into(),
            },
        )
        .await
        .unwrap();
    let message = NativePublicSummary {
        schema_version: SchemaV1,
        native_turn_id: "controlled-review-turn".into(),
        native_item_id: "controlled-review-item".into(),
        phase: Some("final_answer".into()),
        text: "Independent limitation; not a qualification.".into(),
    };
    let artifact = summary(&store, &lease, &f, reserved.id, &message)
        .await
        .unwrap();
    assert_eq!(
        summary(&store, &lease, &f, reserved.id, &message)
            .await
            .unwrap(),
        artifact
    );
    let access: String = sqlx::query_scalar("SELECT access_class FROM app.artifacts WHERE id=$1")
        .bind(artifact.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(access, "EVALUATOR_ONLY");
    assert!(matches!(
        store.artifact_content(&actor, artifact).await,
        Err(StoreError::NotFound)
    ));
    let arbitrary = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) SELECT $1,project_id,producer_run_id,producer_attempt_id,kind,media_type,'qz.native_task',schema_version,storage_backend,$2,storage_version,byte_count,access_class,origin,created_by,retention_class FROM app.artifacts WHERE id=$3")
        .bind(arbitrary.as_uuid()).bind(arbitrary.to_string()).bind(request.request_artifact_id.as_uuid()).execute(&pool).await.unwrap();
    let mut invalid = request;
    invalid.command_key = "not-an-arbitrary-sealed-envelope".into();
    invalid.request_artifact_id = arbitrary;
    assert!(matches!(
        store
            .reserve_turn(lease.run.id, &lease.fence, &invalid)
            .await,
        Err(StoreError::Invalid("request_artifact"))
    ));
    // Reviewer refresh uses the same frozen Runtime revision as Researcher.
    // A changed Runtime must be rejected, not silently skipped by role.
    sqlx::query("UPDATE app.runtime_integrations SET enabled=false WHERE id=$1")
        .bind(context.runtime_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store
            .prepare_run_runtime_probe(lease.run.id, &lease.fence)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_zero_turn_mission_does_not_require_a_thread_or_fabricate_usage(pool: PgPool) {
    let (store, actor, f, _, preparation) = setup(&pool).await;
    complete(&pool, &store, &f, preparation, false).await;
    store.advance_initial_cycle(preparation).await.unwrap();
    let lease = mission_lease(&store).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    cancel(&store, &actor, lease.run.id).await;
    assert!(store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let observation: serde_json::Value =
        sqlx::query_scalar("SELECT observation FROM app.run_terminal_receipts WHERE run_id=$1")
            .bind(lease.run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(observation["session_id"].is_null());
    assert!(observation["summary_artifact_id"].is_null());
    assert!(observation["concluding_reservation_id"].is_null());
    let facts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_sessions),(SELECT count(*) FROM app.model_turn_reservations),(SELECT count(*) FROM app.model_turn_receipts)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (0, 0, 0));
    assert_eq!(
        store.get_run(&actor, lease.run.id).await.unwrap().state,
        RunState::Cancelled
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_unsent_turn_settlement_and_run_finish_share_one_transaction(pool: PgPool) {
    let (store, actor, f, _, preparation) = mission_support::setup_with_cost(&pool, true).await;
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
    let reserved = prepare_prompt(
        &store,
        &lease,
        &f,
        &prompt_request(&f, &lease),
        "Original unsent request",
    )
    .await
    .unwrap();
    cancel(&store, &actor, lease.run.id).await;
    sqlx::raw_sql("CREATE FUNCTION public.reject_mission_finish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.observation->>'source'='NATIVE_MISSION' THEN RAISE EXCEPTION 'injected terminal publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_finish BEFORE INSERT ON app.run_terminal_receipts FOR EACH ROW EXECUTE FUNCTION public.reject_mission_finish();").execute(&pool).await.unwrap();
    assert!(store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .is_err());
    let rolled_back: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_terminals),(SELECT count(*) FROM app.model_turn_receipts),(SELECT reserved_tokens FROM app.model_turn_accounting WHERE run_id=$1)")
        .bind(lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(rolled_back, (0, 0, 100));
    assert_eq!(
        store.get_run(&actor, lease.run.id).await.unwrap().state,
        RunState::CancelRequested
    );
    sqlx::raw_sql("DROP TRIGGER reject_finish ON app.run_terminal_receipts; DROP FUNCTION public.reject_mission_finish();").execute(&pool).await.unwrap();
    let (one, two) = tokio::join!(
        store.complete_research_mission(lease.run.id, &lease.fence),
        store.complete_research_mission(lease.run.id, &lease.fence)
    );
    assert!(one.unwrap() && two.unwrap());
    let receipt: (String,i64,String,String,String) = sqlx::query_as("SELECT outcome,actual_tokens,actual_cost::text,cost_currency,usage_source FROM app.model_turn_receipts WHERE reservation_id=$1")
        .bind(reserved.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        receipt,
        (
            "NOT_SENT".into(),
            0,
            "0".into(),
            "USD".into(),
            "CONFIRMED_NOT_SENT".into()
        )
    );
    let usage: (i64,i64,bool,bool) = sqlx::query_as("SELECT reserved_tokens,used_tokens,reserved_cost=0,used_cost=0 FROM app.model_turn_accounting WHERE run_id=$1")
        .bind(lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(usage, (0, 0, true, true));
    let native: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_dispatches),(SELECT count(*) FROM app.model_turn_bindings),(SELECT count(*) FROM app.model_turn_summaries)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(native, (0, 0, 0));
    let message_id: i64 = sqlx::query_scalar(
        "SELECT initial_queue_message_id FROM app.run_admissions WHERE run_id=$1",
    )
    .bind(lease.run.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let message = store::lifecycle::RunMessage {
        message_id,
        run_id: lease.run.id,
        read_count: 1,
    };
    store.acknowledge_run(&message).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_unknown_send_keeps_its_reservation_until_real_failed_usage_arrives(
    pool: PgPool,
) {
    use store::turns::{TurnOutcome, UsageReceipt};
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
    let reserved = prepare_prompt(
        &store,
        &lease,
        &f,
        &prompt_request(&f, &lease),
        "Unknown send acknowledgement",
    )
    .await
    .unwrap();
    store
        .claim_turn_dispatch(reserved.id, &lease.fence)
        .await
        .unwrap();
    cancel(&store, &actor, lease.run.id).await;
    assert!(!store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let unknown: (i64,i64) = sqlx::query_as("SELECT reserved_tokens,(SELECT count(*) FROM app.model_turn_receipts) FROM app.model_turn_accounting WHERE run_id=$1")
        .bind(lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(unknown, (100, 0));
    store
        .bind_native_turn(reserved.id, &lease.fence, "original-failed-turn")
        .await
        .unwrap();
    store
        .observe_mission_turn_terminal(
            reserved.id,
            &lease.fence,
            TurnOutcome::Failed,
            "NATIVE_TURN_FAILED",
        )
        .await
        .unwrap();
    assert!(!store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    store
        .settle_turn(
            reserved.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Failed,
                actual_tokens: DbCounter::new(7).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "NATIVE_TURN_FAILED".into(),
            },
        )
        .await
        .unwrap();
    assert!(store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let known: (i64,i64,i64) = sqlx::query_as("SELECT reserved_tokens,used_tokens,(SELECT count(*) FROM app.model_turn_summaries) FROM app.model_turn_accounting WHERE run_id=$1")
        .bind(lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(known, (0, 7, 0));
    let run = store.get_run(&actor, lease.run.id).await.unwrap();
    assert_eq!(run.state, RunState::Cancelled);
    assert_eq!(
        run.terminal_reason_code.as_deref(),
        Some("RUNTIME_CANCELLED")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_turn_deadline_closes_proven_unsent_work_without_fabricated_native_stop(pool: PgPool) {
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
    let mut request = prompt_request(&f, &lease);
    request.deadline_at = sqlx::query_scalar("SELECT clock_timestamp()+interval '2 seconds'")
        .fetch_one(&pool)
        .await
        .unwrap();
    prepare_prompt(&store, &lease, &f, &request, "Expire before any wire write")
        .await
        .unwrap();
    assert!(!store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let run = store.get_run(&actor, lease.run.id).await.unwrap();
    assert_eq!(run.state, RunState::Cancelled);
    assert!(run.cancellation_requested_at.unwrap() >= request.deadline_at);
    let facts: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_receipts WHERE outcome='NOT_SENT' AND actual_tokens=0),(SELECT count(*) FROM app.model_turn_bindings)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_public_summary_requires_original_success_and_is_immutable_budgeted_and_replayable(
    pool: PgPool,
) {
    use store::turns::{NativePublicSummary, TurnOutcome, UsageReceipt};
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
    assert!(!store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    prepare_initial(&store, &lease, &f).await.unwrap();
    let reserved = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap()
        .reservation;
    store
        .claim_turn_dispatch(reserved.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(reserved.id, &lease.fence, "observed-public-turn")
        .await
        .unwrap();
    let mut message = NativePublicSummary {
        schema_version: SchemaV1,
        native_turn_id: "observed-public-turn".into(),
        native_item_id: "observed-public-item".into(),
        phase: None,
        text: "A public limitation, not qualified evidence.\nOriginal exact text.".into(),
    };
    assert!(matches!(
        summary(&store, &lease, &f, reserved.id, &message).await,
        Err(StoreError::Integrity)
    ));
    store
        .observe_mission_turn_terminal(
            reserved.id,
            &lease.fence,
            TurnOutcome::Succeeded,
            "NATIVE_TURN_COMPLETED",
        )
        .await
        .unwrap();
    assert!(matches!(
        summary(&store, &lease, &f, reserved.id, &message).await,
        Err(StoreError::TurnPending)
    ));
    store
        .settle_turn(
            reserved.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(12).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "NATIVE_TURN_COMPLETED".into(),
            },
        )
        .await
        .unwrap();
    message.native_turn_id = "different-turn".into();
    assert!(!store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .unwrap());
    assert!(matches!(
        summary(&store, &lease, &f, reserved.id, &message).await,
        Err(StoreError::Conflict)
    ));
    message.native_turn_id = "observed-public-turn".into();
    assert!(matches!(
        store
            .record_mission_summary(
                reserved.id,
                &lease.fence,
                &message,
                |_, _| async { panic!("new summary has no prior bytes") },
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.model_turn_summaries),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.mission_summary'),(SELECT count(*) FROM app.model_turn_receipts)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts,
        (0, 0, 1),
        "summary failure never erases already observed usage"
    );
    let (a, b) = tokio::join!(
        summary(&store, &lease, &f, reserved.id, &message),
        summary(&store, &lease, &f, reserved.id, &message)
    );
    let artifact = a.unwrap();
    assert_eq!(b.unwrap(), artifact);
    let checkpoint = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(
        checkpoint.latest.unwrap().summary_artifact_id,
        Some(artifact)
    );
    let size:i64=sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND origin='SYNTHETIC' AND access_class='RESEARCH' AND producer_run_id=$2 AND producer_attempt_id=$3")
        .bind(artifact.as_uuid()).bind(lease.run.id.as_uuid()).bind(lease.fence.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(
        &f.objects
            .read(artifact, DbCounter::new(size as u64).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["text"], message.text);
    assert!(value["phase"].is_null());
    message.text.push_str(" changed");
    assert!(matches!(
        summary(&store, &lease, &f, reserved.id, &message).await,
        Err(StoreError::Conflict)
    ));
    assert!(
        sqlx::query("DELETE FROM app.model_turn_summaries WHERE reservation_id=$1")
            .bind(reserved.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    message.text = "x".repeat(64 * 1024 + 1);
    assert!(matches!(
        summary(&store, &lease, &f, reserved.id, &message).await,
        Err(StoreError::Invalid("native_public_summary"))
    ));
    message.text = "within bounds".into();
    let mut stale = lease.clone();
    stale.fence.worker_owner_id = "expired-owner".into();
    assert!(matches!(
        summary(&store, &stale, &f, reserved.id, &message).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    assert!(matches!(
        store
            .complete_research_mission(lease.run.id, &stale.fence)
            .await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    sqlx::raw_sql("CREATE FUNCTION public.reject_mission_finish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.observation->>'source'='NATIVE_MISSION' THEN RAISE EXCEPTION 'injected terminal publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_finish BEFORE INSERT ON app.run_terminal_receipts FOR EACH ROW EXECUTE FUNCTION public.reject_mission_finish();").execute(&pool).await.unwrap();
    assert!(store
        .complete_research_mission(lease.run.id, &lease.fence)
        .await
        .is_err());
    let rolled_back:(String,Option<uuid::Uuid>,i64)=sqlx::query_as("SELECT a.dispatch_state,a.result_manifest_artifact_id,(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=a.run_id) FROM app.run_attempts a WHERE a.id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_ne!(rolled_back.0, "TERMINAL");
    assert_eq!((rolled_back.1, rolled_back.2), (None, 0));
    sqlx::raw_sql("DROP TRIGGER reject_finish ON app.run_terminal_receipts; DROP FUNCTION public.reject_mission_finish();").execute(&pool).await.unwrap();
    let (one, two) = tokio::join!(
        store.complete_research_mission(lease.run.id, &lease.fence),
        store.complete_research_mission(lease.run.id, &lease.fence)
    );
    assert!(one.unwrap() && two.unwrap());
    let completed:(String,uuid::Uuid,serde_json::Value,String,i64)=sqlx::query_as("SELECT r.state,a.result_manifest_artifact_id,t.observation,c.state,(SELECT count(*) FROM app.qualifications) FROM app.runs r JOIN app.run_attempts a ON a.id=r.active_attempt_id JOIN app.run_terminal_receipts t ON t.run_id=r.id JOIN app.research_cycles c ON c.id=r.cycle_id WHERE r.id=$1")
        .bind(lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(completed.0, "SUCCEEDED");
    assert_eq!(completed.1, artifact.as_uuid());
    assert_eq!(completed.2["summary_artifact_id"], artifact.to_string());
    assert_eq!(completed.2["formal_evaluation"], "NOT_PERFORMED");
    assert_eq!((completed.3.as_str(), completed.4), ("RUNNING", 0));
    let message_id:i64=sqlx::query_scalar("SELECT q.msg_id FROM pgmq.q_runs q JOIN app.run_admissions a ON a.initial_queue_message_id=q.msg_id WHERE a.run_id=$1")
        .bind(lease.run.id.as_uuid()).fetch_one(&pool).await.expect("terminal commit precedes native PGMQ archive");
    let message = store::lifecycle::RunMessage {
        message_id,
        run_id: lease.run.id,
        read_count: 1,
    };
    store.acknowledge_run(&message).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn mission_cancel_requires_full_native_receipts_and_wins_before_report_adoption(
    pool: PgPool,
) {
    use store::turns::{NativePublicSummary, TurnOutcome, UsageReceipt};
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
    prepare_initial(&store, &lease, &f).await.unwrap();
    let reserved = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap()
        .reservation;
    store
        .claim_turn_dispatch(reserved.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(reserved.id, &lease.fence, "cancel-race-turn")
        .await
        .unwrap();
    let run = store.get_run(&actor, lease.run.id).await.unwrap();
    let cancelled = store
        .cancel_run(
            &actor,
            "cancel-before-report",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(cancelled.state, RunState::CancelRequested);
    assert!(!store
        .complete_research_mission(run.id, &lease.fence)
        .await
        .unwrap());
    store
        .observe_mission_turn_terminal(
            reserved.id,
            &lease.fence,
            TurnOutcome::Succeeded,
            "NATIVE_TURN_COMPLETED",
        )
        .await
        .unwrap();
    assert!(!store
        .complete_research_mission(run.id, &lease.fence)
        .await
        .unwrap());
    store
        .settle_turn(
            reserved.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(12).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "NATIVE_TURN_COMPLETED".into(),
            },
        )
        .await
        .unwrap();
    summary(
        &store,
        &lease,
        &f,
        reserved.id,
        &NativePublicSummary {
            schema_version: SchemaV1,
            native_turn_id: "cancel-race-turn".into(),
            native_item_id: "cancel-race-item".into(),
            phase: None,
            text: "Public answer arrived after cancellation; not a qualification.".into(),
        },
    )
    .await
    .unwrap();
    assert!(store
        .complete_research_mission(run.id, &lease.fence)
        .await
        .unwrap());
    let finished = store.get_run(&actor, run.id).await.unwrap();
    assert_eq!(finished.state, RunState::Cancelled);
    assert_eq!(
        finished.terminal_reason_code.as_deref(),
        Some("RESULT_DISCARDED_AFTER_CANCEL")
    );
    let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM app.model_turn_receipts WHERE reservation_id=$2),(SELECT count(*) FROM app.qualifications)").bind(run.id.as_uuid()).bind(reserved.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 0));
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
    let family: uuid::Uuid =
        sqlx::query_scalar("SELECT family_id FROM app.evaluation_policies WHERE id=$1")
            .bind(f.brief.content.evaluation_policy_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(prompt.contains(&format!("Cycle: {}", lease.run.cycle_id.unwrap())));
    assert!(prompt.contains(&format!("experiment family: {family}")));
    assert!(prompt.contains("wasm32-unknown-unknown") && prompt.contains("dataset_revision_id"));
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
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
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
