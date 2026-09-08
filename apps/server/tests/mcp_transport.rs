//! Native SDK protocol and actual HTTP sockets; HTTP replies are fault fixtures.
//! These tests do not claim PostgreSQL authorization or a native Codex E2E run.
#[path = "support/mcp_contract.rs"]
mod mcp_contract;
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, Response, StatusCode},
    Router,
};
use chrono::{Duration as ChronoDuration, Utc};
use contracts::Id;
use integrations::authentication::{format_machine_token, random_capability};
use rmcp::{model::CallToolRequestParams, service::RunningService, RoleClient, ServiceExt};
use serde_json::{json, Value};
use server::mcp::{Failure, MissionBinding, MissionMcp};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};

struct Responses {
    identity: Value,
    run: Value,
    brief: Value,
    status: StatusCode,
    oversized: bool,
    redirect: bool,
    hits: usize,
}
struct TestState {
    credential: String,
    responses: Mutex<Responses>,
    delay_ms: AtomicU64,
}
struct Api {
    origin: String,
    binding: MissionBinding,
    state: Arc<TestState>,
    task: JoinHandle<()>,
}
impl Drop for Api {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn reply(State(state): State<Arc<TestState>>, request: Request<Body>) -> Response<Body> {
    let delay = state.delay_ms.load(Ordering::SeqCst);
    if delay != 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    let mut values = state.responses.lock().unwrap();
    values.hits += 1;
    assert_eq!(request.method(), "GET");
    assert_eq!(
        request.headers()[header::AUTHORIZATION],
        format!("Bearer {}", state.credential)
    );
    assert!(!request.headers().contains_key(header::COOKIE));
    assert!(!request.headers().contains_key("x-operator-grant"));
    if values.redirect {
        return Response::builder()
            .status(StatusCode::TEMPORARY_REDIRECT)
            .header(header::LOCATION, "/must-not-follow")
            .body(Body::from("upstream diagnostics must not be disclosed"))
            .unwrap();
    }
    if values.oversized {
        let chunk = "x".repeat(32 * 1024);
        let chunks = futures_util::stream::iter(
            (0..40).map(move |_| Ok::<_, std::io::Error>(chunk.clone())),
        );
        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from_stream(chunks))
            .unwrap();
    }
    let (status, body) = if request.uri().path() == "/api/v2/auth/machine" {
        (values.status, values.identity.to_string())
    } else if request.uri().path().starts_with("/api/v2/runs/") {
        (StatusCode::OK, values.run.to_string())
    } else if request.uri().path().starts_with("/api/v2/briefs/") {
        (StatusCode::OK, values.brief.to_string())
    } else {
        (
            StatusCode::NOT_FOUND,
            "upstream diagnostics must not be disclosed".to_owned(),
        )
    };
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}
async fn api() -> Api {
    let binding = MissionBinding {
        project_id: Id::new(),
        cycle_id: Id::new(),
        run_id: Id::new(),
        attempt_id: Id::new(),
        brief_id: Id::new(),
    };
    let now = Utc::now();
    let expires = now + ChronoDuration::minutes(5);
    let brief_request: contracts::brief::BriefCreate =
        serde_json::from_str(include_str!("../../../tests/contracts/research-brief.json")).unwrap();
    let state = Arc::new(TestState {
        credential: format_machine_token(Id::new(), &random_capability()).unwrap(),
        delay_ms: AtomicU64::new(0),
        responses: Mutex::new(Responses {
            identity: json!({"schema_version":1,"credential_id":Id::new(),"kind":"MISSION",
                "project_id":binding.project_id,"downstream_id":null,"run_id":binding.run_id,
                "scope_codes":["RUN_READ","RESEARCH_READ"],"expires_at":expires}),
            run: json!({"schema_version":1,"id":binding.run_id,"project_id":binding.project_id,
                "cycle_id":binding.cycle_id,"kind":"AGENT_RESEARCH","input_set_id":Id::new(),
                "state":"RUNNING","current_attempt_no":1,"active_attempt_id":binding.attempt_id,
                "last_event_seq":"3","deadline_at":expires,"cancellation_requested_at":null,
                "terminal_reason_code":null,"queued_at":now,"started_at":now,"finished_at":null,"revision":contracts::Revision::INITIAL}),
            brief: json!({"id":binding.brief_id,"project_id":binding.project_id,
                "version":1,"revision":contracts::Revision::INITIAL,"state":"FROZEN","content":brief_request.content,
                "bindings":brief_request.bindings,"supersedes_id":null,
                "frozen_at":now,"created_at":now,"updated_at":now}),
            status: StatusCode::OK,
            oversized: false,
            redirect: false,
            hits: 0,
        }),
    });
    {
        let values = state.responses.lock().unwrap();
        let _: contracts::control::MachineSessionView =
            serde_json::from_value(values.identity.clone()).unwrap();
        let _: contracts::runs::RunSnapshotV1 = serde_json::from_value(values.run.clone()).unwrap();
        let _: contracts::brief::BriefView = serde_json::from_value(values.brief.clone()).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(reply).with_state(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Api {
        origin,
        binding,
        state,
        task,
    }
}
async fn bridge(api: &Api) -> Result<MissionMcp, Failure> {
    MissionMcp::connect(&api.origin, true, &api.state.credential, api.binding).await
}
async fn connected(
    api: &Api,
) -> (
    RunningService<RoleClient, ()>,
    JoinHandle<Result<(), Failure>>,
) {
    let mcp = bridge(api).await.unwrap();
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(server_io);
    let task = tokio::spawn(mcp.serve_io(read, write));
    (().serve(client_io).await.unwrap(), task)
}
fn request(name: &str, arguments: Value) -> CallToolRequestParams {
    serde_json::from_value(json!({"name":name,"arguments":arguments})).unwrap()
}
async fn run(client: &RunningService<RoleClient, ()>, id: Id) -> Value {
    serde_json::to_value(
        client
            .call_tool(request("run.get", json!({"run_id":id})))
            .await
            .unwrap(),
    )
    .unwrap()
}
fn body(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn native_protocol_lists_only_real_tools_and_checks_arguments() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    let tools = client.list_tools(Default::default()).await.unwrap();
    let mut names: Vec<_> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, ["research.get_brief", "run.get"]);
    let result = run(&client, api.binding.run_id).await;
    assert_ne!(result["isError"], true);
    assert_eq!(body(&result)["id"], json!(api.binding.run_id));
    assert_eq!(body(&result)["state"], "RUNNING");
    assert_eq!(api.state.responses.lock().unwrap().hits, 4);
    for (name, arguments) in [
        ("db.query", json!({"query":"SELECT 1"})),
        ("run.get", json!({"run_id":"../../outside"})),
        (
            "run.get",
            json!({"run_id":api.binding.run_id,"role":"OPERATOR"}),
        ),
    ] {
        let response = client.call_tool(request(name, arguments)).await;
        assert!(
            response.is_err()
                || serde_json::to_value(response.unwrap()).unwrap()["isError"] == true
        );
    }
    let denied = run(&client, Id::new()).await;
    assert_eq!(denied["isError"], true);
    assert_eq!(body(&denied)["code"], "MCP_AUTHORITY_REJECTED");
    assert_eq!(api.state.responses.lock().unwrap().hits, 4);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn identity_attempt_and_contract_changes_fail_closed() {
    let api = api().await;
    let original = api.state.responses.lock().unwrap().identity.clone();
    for (field, bad) in [
        ("kind", json!("CLI")),
        ("kind", json!("DOWNSTREAM")),
        ("project_id", json!(Id::new())),
        ("run_id", json!(Id::new())),
        ("downstream_id", json!(Id::new())),
        ("scope_codes", json!(["RUN_READ"])),
        (
            "scope_codes",
            json!(["RUN_READ", "RESEARCH_READ", "DOCTOR_READ"]),
        ),
        ("expires_at", json!(Utc::now() - ChronoDuration::seconds(1))),
    ] {
        {
            let mut values = api.state.responses.lock().unwrap();
            values.identity = original.clone();
            values.identity[field] = bad;
        }
        assert!(matches!(bridge(&api).await, Err(Failure::Authority)));
    }
    api.state.responses.lock().unwrap().identity = original;
    let original = api.state.responses.lock().unwrap().run.clone();
    for (field, bad) in [
        ("project_id", json!(Id::new())),
        ("cycle_id", json!(Id::new())),
        ("active_attempt_id", json!(Id::new())),
        ("kind", json!("EXPORT")),
        ("state", json!("CANCEL_REQUESTED")),
    ] {
        {
            let mut values = api.state.responses.lock().unwrap();
            values.run = original.clone();
            values.run[field] = bad;
        }
        assert!(matches!(bridge(&api).await, Err(Failure::Authority)));
    }
    {
        let mut values = api.state.responses.lock().unwrap();
        values.run = original;
        values.run["unexpected_field"] = json!(true);
    }
    assert!(matches!(bridge(&api).await, Err(Failure::Contract)));
}

#[tokio::test]
async fn next_call_rechecks_revocation_and_never_returns_upstream_diagnostics() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    api.state.responses.lock().unwrap().status = StatusCode::UNAUTHORIZED;
    let result = run(&client, api.binding.run_id).await;
    assert_eq!(result["isError"], true);
    assert_eq!(body(&result)["http_status"], 401);
    assert!(!result.to_string().contains(&api.state.credential));
    assert!(!result.to_string().contains("upstream diagnostics"));
    assert_eq!(api.state.responses.lock().unwrap().hits, 3);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn redirects_and_chunked_oversize_never_become_tool_data() {
    let api = api().await;
    api.state.responses.lock().unwrap().redirect = true;
    assert!(matches!(bridge(&api).await, Err(Failure::Http(307))));
    assert_eq!(api.state.responses.lock().unwrap().hits, 1);
    {
        let mut values = api.state.responses.lock().unwrap();
        values.redirect = false;
        values.oversized = true;
    }
    assert!(matches!(bridge(&api).await, Err(Failure::ResponseLimit)));
    assert_eq!(api.state.responses.lock().unwrap().hits, 2);
}

#[tokio::test]
async fn native_concurrent_requests_are_bounded_without_a_waiting_queue() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    api.state.delay_ms.store(150, Ordering::SeqCst);
    let results =
        futures_util::future::join_all((0..5).map(|_| run(&client, api.binding.run_id))).await;
    assert_eq!(results.iter().filter(|r| r["isError"] != true).count(), 4);
    assert_eq!(
        results
            .iter()
            .filter(|r| body(r)["code"] == "MCP_CONCURRENCY_LIMIT")
            .count(),
        1
    );
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn native_stdio_child_has_no_database_dependency_or_stdout_logs() {
    let api = api().await;
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"));
    command
        .env_clear()
        .env("QUAZONAI_MCP_TOKEN", &api.state.credential)
        .args(["mcp", "--api-origin", &api.origin, "--development-http"])
        .args(["--project-id", &api.binding.project_id.to_string()])
        .args(["--cycle-id", &api.binding.cycle_id.to_string()])
        .args(["--run-id", &api.binding.run_id.to_string()])
        .args(["--attempt-id", &api.binding.attempt_id.to_string()])
        .args(["--brief-id", &api.binding.brief_id.to_string()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    let client =
        ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap()))
            .await
            .unwrap();
    assert_eq!(
        body(&run(&client, api.binding.run_id).await)["id"],
        json!(api.binding.run_id)
    );
    client.cancel().await.unwrap();
    let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
