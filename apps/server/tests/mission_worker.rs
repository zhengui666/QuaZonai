//! Formal Cycle/PGMQ/issuance + official App Server bootstrap. Only the market
//! preparation and model response are fixtures; no paid-account/T42 claim.
#![cfg(feature = "native-codex")]
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "support/codex_responses.rs"]
mod responses;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{runs::RunState, DbCounter, Id, SchemaV1};
use integrations::secrets::SecretVault;
use server::{
    codex_profiles::{CodexDeployment, CodexDeploymentBinding, CodexDeploymentConfig},
    worker::mission::{MissionLauncher, TurnProgress},
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
    launcher: MissionLauncher,
    vault: Arc<SecretVault>,
    provider: responses::Provider,
    root: tempfile::TempDir,
    http: JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.http.abort();
    }
}

async fn fixture(pool: &PgPool) -> Fixture {
    fixture_with_cost(pool, false).await
}

async fn fixture_with_cost(pool: &PgPool, priced: bool) -> Fixture {
    let (store, actor, data, _, preparation) = mission_support::setup_with_cost(pool, priced).await;
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
    let root = tempfile::tempdir().unwrap();
    for name in ["native", "workspaces", "secrets"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.path().join(name))
            .unwrap();
    }
    let home = root.path().join("native");
    let provider = responses::Provider::start(&home).await;
    let key = root.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let secrets = root.path().join("secrets");
    let vault = Arc::new(SecretVault::open(&secrets, &key).unwrap());
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
        launcher,
        vault,
        provider,
        root,
        http,
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
                    // Real pinned behavior: reconstructed history says completed,
                    // while its buffered real turn/completed says failed. QZ must
                    // never promote this list projection into a success receipt.
                    assert_eq!(actual.status, server::codex_native::TurnStatus::Completed);
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
