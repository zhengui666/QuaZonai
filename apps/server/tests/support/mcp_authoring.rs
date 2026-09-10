//! Real native MCP and HTTP test infrastructure; only parent research data is a fixture.
use super::{experiment_support, research_support};
use contracts::{
    lifecycle::JobLimitsV1, research::InputPurpose, runs::RunKind, DbCounter, Id, SchemaV1,
};
use integrations::{artifacts::ArtifactStore, authentication, secrets::SecretVault};
use rmcp::{model::CallToolRequestParams, service::RunningService, RoleClient, ServiceExt};
use serde_json::{json, Value};
use server::{
    mcp::{MissionBinding, MissionMcp},
    AppState, WebPolicy,
};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf};
use store::{
    authority::Actor,
    lifecycle::{ClaimResult, RunSubmission},
    Store,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tower_sessions::cookie::Key;
use tower_sessions_sqlx_store::PostgresStore;

pub type McpClient = RunningService<RoleClient, ()>;
pub struct Fixture {
    pub store: Store,
    pub operator: Actor,
    pub research: experiment_support::Fixture,
    pub binding: MissionBinding,
    pub credential: Id,
    pub work: PathBuf,
    pub private: tempfile::TempDir,
    token: String,
    origin: String,
    http: JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.http.abort();
    }
}
pub async fn fixture(pool: &PgPool, scopes: &[&str]) -> Fixture {
    let (store, operator) = research_support::operator(pool).await;
    let research = experiment_support::setup(pool, &store, &operator, 3).await;
    sqlx::query("UPDATE app.runtime_integrations SET allowed_capabilities=ARRAY['AGENT_RESEARCH'] WHERE id=$1")
        .bind(research.data.runtime.as_uuid()).execute(pool).await.unwrap();
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM app.runtime_integrations WHERE id=$1")
            .bind(research.data.runtime.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let input = store
        .create_input_set(
            &operator,
            "mission-input",
            &research.data.input(InputPurpose::Discovery),
        )
        .await
        .unwrap()
        .resource;
    let run = store
        .enqueue_run(
            "mission",
            &RunSubmission {
                cycle_id: research.cycle,
                input_set_id: input.header.id,
                runtime_id: research.data.runtime,
                runtime_revision: revision.to_string().try_into().unwrap(),
                kind: RunKind::AgentResearch,
                limits: JobLimitsV1 {
                    schema_version: SchemaV1,
                    experiments: 0,
                    cpu_seconds: DbCounter::new(10).unwrap(),
                    wall_seconds: 3600,
                    memory_mib: 128,
                    output_bytes: DbCounter::new(4096).unwrap(),
                },
            },
        )
        .await
        .unwrap()
        .resource;
    let message = store
        .read_run_messages(30, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == run.id)
        .unwrap();
    let ClaimResult::Leased(lease) = store.claim_run(&message, "mcp-test", 120).await.unwrap()
    else {
        panic!("native lease expected");
    };
    let binding = MissionBinding {
        project_id: research.data.project,
        cycle_id: research.cycle,
        run_id: run.id,
        attempt_id: lease.fence.attempt_id,
        brief_id: research.brief,
    };
    let private = tempfile::tempdir().unwrap();
    let secrets = private.path().join("secrets");
    fs::DirBuilder::new().mode(0o700).create(&secrets).unwrap();
    let key = private.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let vault = SecretVault::open(&secrets, &key).unwrap();
    let secret = authentication::random_capability();
    let verifier = authentication::capability_verifier(&secret).unwrap();
    let reference = vault.put("MACHINE_VERIFIER", verifier.as_bytes()).unwrap();
    let principal = Id::new();
    let public = Id::new();
    let credential = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,run_id,enabled,credential_epoch) VALUES($1,'SDK author fixture','MISSION',$2,$3,true,1)")
        .bind(principal.as_uuid()).bind(binding.project_id.as_uuid()).bind(run.id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,$3,$4,1,$5,clock_timestamp(),clock_timestamp()+interval '10 minutes','MISSION_SERVICE')")
        .bind(credential.as_uuid()).bind(principal.as_uuid()).bind(public.to_string()).bind(reference.to_string())
        .bind(scopes).execute(pool).await.unwrap();
    let token = authentication::format_machine_token(public, &secret).unwrap();
    let work = private.path().join("work");
    fs::DirBuilder::new().mode(0o700).create(&work).unwrap();
    fs::write(work.join("alpha.rs"), "pub fn forecast() -> f64 { 0.1 }\n").unwrap();
    fs::write(
        work.join("parameters.json"),
        "{\"schema_version\":1,\"period\":5}\n",
    )
    .unwrap();
    fs::write(
        work.join("proposal.json"),
        "{\"schema_version\":1,\"hypothesis\":\"test premise\"}\n",
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    PostgresStore::new(pool.clone()).migrate().await.unwrap();
    let app = server::router(
        AppState::new(
            store.clone(),
            vault,
            WebPolicy::new(&origin, address, true).unwrap(),
        )
        .with_artifact_store(ArtifactStore::open(&private.path().join("artifacts")).unwrap()),
        Key::generate(),
    );
    let http = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Fixture {
        store,
        operator,
        research,
        binding,
        token,
        credential,
        origin,
        work,
        private,
        http,
    }
}
pub async fn client(f: &Fixture) -> (McpClient, JoinHandle<Result<(), server::mcp::Failure>>) {
    let mcp = MissionMcp::connect(&f.origin, true, &f.token, f.binding)
        .await
        .unwrap()
        .with_workspace(&f.work)
        .unwrap();
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(server_io);
    let task = tokio::spawn(mcp.serve_io(read, write));
    (().serve(client_io).await.unwrap(), task)
}
pub async fn call(client: &McpClient, name: &str, arguments: Value) -> Value {
    let request: CallToolRequestParams =
        serde_json::from_value(json!({"name":name,"arguments":arguments})).unwrap();
    serde_json::to_value(client.call_tool(request).await.unwrap()).unwrap()
}
pub fn body(reply: &Value) -> Value {
    serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap()
}
pub async fn upload(client: &McpClient, kind: &str, path: &str, key: &str) -> Value {
    call(client, "artifact.submit", json!({"schema_version":1,"kind":kind,"workspace_relative_path":path,"idempotency_key":key})).await
}
