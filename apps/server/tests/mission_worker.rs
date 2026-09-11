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
    worker::mission::MissionLauncher,
    AppState, WebPolicy,
};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, sync::Arc};
use store::{
    authority::Actor,
    lifecycle::{ClaimResult, RunLease, RunMessage},
    turns::{DispatchDecision, TurnOutcome, TurnRequest, UsageReceipt},
    Store,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tower_sessions::cookie::Key;
use tower_sessions_sqlx_store::PostgresStore;

struct Fixture {
    store: Store,
    actor: Actor,
    data: cycle_support::Fixture,
    preparation: Id,
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
    let (store, actor, data, _, preparation) = mission_support::setup(pool).await;
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
        preparation,
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

async fn turn(
    f: &Fixture,
    pool: &PgPool,
    lease: &RunLease,
    connection: &mut server::worker::mission::MissionConnection,
    command: &str,
    prompt: &str,
    baseline: i64,
) {
    let client = &mut connection.client;
    let thread = &connection.session.native.thread_id;
    let parameters: uuid::Uuid = sqlx::query_scalar(
        "SELECT parameters_artifact_id FROM app.run_native_tasks WHERE run_id=$1",
    )
    .bind(f.preparation.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    // Reuse an explicit nonsealed PARAMETERS fixture for this bootstrap test;
    // production turn request publication is a separate Worker integration.
    let reserved = f
        .store
        .reserve_turn(
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
                request_artifact_id: parameters.to_string().try_into().unwrap(),
                deadline_at: lease.run.deadline_at,
            },
        )
        .await
        .unwrap();
    let DispatchDecision::Send { rpc_request_id } = f
        .store
        .claim_turn_dispatch(reserved.id, &lease.fence)
        .await
        .unwrap()
    else {
        panic!("one native send permit required");
    };
    let actual = client
        .start_turn(&rpc_request_id, thread, prompt)
        .await
        .unwrap();
    f.store
        .bind_native_turn(reserved.id, &lease.fence, &actual.id)
        .await
        .unwrap();
    f.store
        .observe_run_running(lease.run.id, &lease.fence, &lease.external_job_id)
        .await
        .unwrap();
    let cumulative = responses::completed(client, thread, &actual.id).await;
    assert_eq!(cumulative.total - baseline, 12);
    f.store
        .settle_turn(
            reserved.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(12).unwrap(),
                actual_cost: reserved.reserved_cost,
                currency: reserved.cost_currency,
                reason_code: "CONTROLLED_NATIVE_TURN".into(),
            },
        )
        .await
        .unwrap();
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
        &pool,
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
        &pool,
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
