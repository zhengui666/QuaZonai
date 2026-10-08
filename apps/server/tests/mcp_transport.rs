//! Native SDK protocol and actual HTTP sockets; HTTP replies are fault fixtures.
//! These tests do not claim PostgreSQL authorization or a native Codex E2E run.
#[path = "support/mcp_contract.rs"]
mod mcp_contract;
use axum::{
    body::Body,
    extract::State,
    http::{header, Method, Request, Response, StatusCode},
    Router,
};
use chrono::{Duration as ChronoDuration, Utc};
use contracts::Id;
use integrations::authentication::{format_machine_token, random_capability};
use rmcp::{model::CallToolRequestParams, service::RunningService, RoleClient, ServiceExt};
use serde_json::{json, Value};
use server::mcp::{Failure, MissionBinding, MissionMcp};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinHandle};

struct Responses {
    identity: Value,
    run: Value,
    brief: Value,
    cycle: Value,
    status: StatusCode,
    oversized: bool,
    padding_bytes: usize,
    redirect: bool,
    hits: usize,
    requests: Vec<(Method, String)>,
    raw: Option<(StatusCode, &'static str, String)>,
}
struct TestState {
    credential: String,
    responses: Mutex<Responses>,
    identity_gate: Mutex<Option<Arc<IdentityGate>>>,
}
// Explicit test-side admission observation, not a product concurrency quota.
struct IdentityGate {
    entered: Semaphore,
    release: Semaphore,
}
impl IdentityGate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
        })
    }
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
    {
        let mut values = state.responses.lock().unwrap();
        values.hits += 1;
        values
            .requests
            .push((request.method().clone(), request.uri().path().to_owned()));
    }
    let gate = if request.uri().path() == "/api/v2/auth/machine" {
        state.identity_gate.lock().unwrap().clone()
    } else {
        None
    };
    if let Some(gate) = gate {
        gate.entered.add_permits(1);
        gate.release.acquire().await.unwrap().forget();
    }
    let values = state.responses.lock().unwrap();
    assert_eq!(request.method(), "GET");
    assert_eq!(
        request.headers()[header::AUTHORIZATION],
        format!("Bearer {}", state.credential)
    );
    assert!(!request.headers().contains_key(header::COOKIE));
    assert!(!request.headers().contains_key("x-operator-grant"));
    if let Some((status, media, body)) = &values.raw {
        return Response::builder()
            .status(*status)
            .header(header::CONTENT_TYPE, *media)
            .body(Body::from(body.clone()))
            .unwrap();
    }
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
    let (status, mut body) = if request.uri().path() == "/api/v2/auth/machine" {
        (values.status, values.identity.to_string())
    } else if request.uri().path().starts_with("/api/v2/runs/") {
        (StatusCode::OK, values.run.to_string())
    } else if request.uri().path().starts_with("/api/v2/briefs/") {
        (StatusCode::OK, values.brief.to_string())
    } else if request.uri().path().starts_with("/api/v2/cycles/") {
        (StatusCode::OK, values.cycle.to_string())
    } else {
        (
            StatusCode::NOT_FOUND,
            "upstream diagnostics must not be disclosed".to_owned(),
        )
    };
    body.extend(std::iter::repeat_n(' ', values.padding_bytes));
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
        identity_gate: Mutex::new(None),
        responses: Mutex::new(Responses {
            cycle: json!({"schema_version":1,"id":binding.cycle_id,"project_id":binding.project_id,
                "brief_id":binding.brief_id,"ordinal":1,"revision":"1","trigger":"OPERATOR",
                "state":"RUNNING","outcome":null,"budget":brief_request.content.budget,
                "reserved_experiments":0,"used_experiments":0,"reserved_cpu_seconds":"0",
                "initial_run_id":binding.run_id,"researcher_profile":null,"reviewer_profile":null,
                "next_action":null,"started_at":now,"ended_at":null,"created_at":now,"available_actions":[]}),
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
            padding_bytes: 0,
            redirect: false,
            hits: 0,
            requests: Vec::new(),
            raw: None,
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
    let origin = format!("http://localhost:{}", listener.local_addr().unwrap().port());
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
    connected_with_capacity(api, 64 * 1024).await
}
async fn connected_with_capacity(
    api: &Api,
    capacity: usize,
) -> (
    RunningService<RoleClient, ()>,
    JoinHandle<Result<(), Failure>>,
) {
    let mcp = bridge(api).await.unwrap();
    let (server_io, client_io) = tokio::io::duplex(capacity);
    let (read, write) = tokio::io::split(server_io);
    let task = tokio::spawn(mcp.serve_io(read, write));
    (().serve(client_io).await.unwrap(), task)
}
fn request(name: &str, arguments: Value) -> CallToolRequestParams {
    serde_json::from_value(json!({"name":name,"arguments":arguments})).unwrap()
}
async fn run(client: &RunningService<RoleClient, ()>) -> Value {
    serde_json::to_value(
        client
            .call_tool(request("run.get", json!({})))
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
    assert_eq!(
        names,
        [
            "artifact.submit",
            "experiment.propose",
            "research.get_brief",
            "run.get"
        ]
    );
    let result = run(&client).await;
    assert_ne!(result["isError"], true);
    assert_eq!(body(&result)["id"], json!(api.binding.run_id));
    assert_eq!(body(&result)["state"], "RUNNING");
    assert_eq!(api.state.responses.lock().unwrap().hits, 4);
    for (name, arguments) in [
        ("db.query", json!({"query":"SELECT 1"})),
        ("run.get", json!({"run_id":"../../outside"})),
        ("run.get", json!({"run_id":api.binding.run_id})),
        ("run.get", json!({"run_id":Id::new()})),
        ("run.get", json!({"unknown":true})),
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
    let result = run(&client).await;
    assert_eq!(result["isError"], true);
    assert_eq!(body(&result)["http_status"], 401);
    assert!(!result.to_string().contains(&api.state.credential));
    assert!(!result.to_string().contains("upstream diagnostics"));
    assert_eq!(api.state.responses.lock().unwrap().hits, 3);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn redirects_and_invalid_chunked_json_never_become_tool_data() {
    let api = api().await;
    api.state.responses.lock().unwrap().redirect = true;
    assert!(matches!(bridge(&api).await, Err(Failure::Http(307))));
    assert_eq!(api.state.responses.lock().unwrap().hits, 1);
    {
        let mut values = api.state.responses.lock().unwrap();
        values.redirect = false;
        values.oversized = true;
    }
    assert!(matches!(bridge(&api).await, Err(Failure::Contract)));
    assert_eq!(api.state.responses.lock().unwrap().hits, 2);
}

#[tokio::test]
async fn native_concurrent_requests_preserve_complete_results_under_transport_backpressure() {
    // Both are real finite native pipe buffers. Neither is a tool admission quota.
    for capacity in [64, 64 * 1024] {
        let api = api().await;
        let (client, task) = connected_with_capacity(&api, capacity).await;
        let original = api.state.responses.lock().unwrap().run.clone();
        const REQUESTS: u32 = 32;
        for _ in 0..3 {
            let gate = IdentityGate::new();
            *api.state.identity_gate.lock().unwrap() = Some(gate.clone());
            let before = api.state.responses.lock().unwrap().hits;
            let calls = futures_util::future::join_all((0..REQUESTS).map(|_| run(&client)));
            let release = async {
                // A restored four-call cap or serialized admission cannot reach
                // this barrier: every call must be pending in real HTTP first.
                gate.entered.acquire_many(REQUESTS).await.unwrap().forget();
                assert_eq!(
                    api.state.responses.lock().unwrap().hits,
                    before + REQUESTS as usize
                );
                for arguments in [
                    json!({"unknown":true}),
                    json!({"run_id":api.binding.run_id}),
                ] {
                    let result = client.call_tool(request("run.get", arguments)).await;
                    assert!(
                        result.is_err()
                            || serde_json::to_value(result.unwrap()).unwrap()["isError"] == true
                    );
                }
                // Invalid arguments must not create HTTP work even under load.
                assert_eq!(
                    api.state.responses.lock().unwrap().hits,
                    before + REQUESTS as usize
                );
                *api.state.identity_gate.lock().unwrap() = None;
                gate.release.add_permits(REQUESTS as usize);
            };
            let (results, ()) = tokio::time::timeout(Duration::from_secs(10), async {
                tokio::join!(calls, release)
            })
            .await
            .expect("all native requests must reach and leave the HTTP barrier");
            assert_eq!(results.len(), REQUESTS as usize);
            for result in results {
                assert_ne!(result["isError"], true, "{result}");
                assert_eq!(body(&result), original);
            }
            assert_eq!(
                api.state.responses.lock().unwrap().hits,
                before + 2 * REQUESTS as usize
            );
        }
        client.cancel().await.unwrap();
        assert!(task.await.unwrap().is_ok());
    }
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
    assert_eq!(body(&run(&client).await)["id"], json!(api.binding.run_id));
    client.cancel().await.unwrap();
    let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[tokio::test]
async fn native_problem_exposes_only_safe_metadata_and_rejects_reflections() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    let request_id = Id::new();
    let mut problem = json!({"type":"urn:quazonai:problem:budget_exhausted",
        "title":"private upstream title", "status":429,"code":"BUDGET_EXHAUSTED",
        "detail":"private upstream diagnostics", "request_id":request_id,"retryable":false,
        "field_errors":[],"safe_next_actions":["private upstream action"]});
    api.state.responses.lock().unwrap().raw = Some((
        StatusCode::TOO_MANY_REQUESTS,
        "application/problem+json; charset=utf-8",
        problem.to_string(),
    ));
    let result = run(&client).await;
    assert_eq!(result["isError"], true);
    assert_eq!(
        body(&result),
        json!({"schema_version":1,"code":"MCP_CONTROL_REJECTED",
        "http_status":429,"problem_code":"BUDGET_EXHAUSTED","retryable":false,"request_id":request_id})
    );
    assert!(!result.to_string().contains("private upstream"));
    problem["detail"] = json!(api.state.credential);
    api.state.responses.lock().unwrap().raw = Some((
        StatusCode::TOO_MANY_REQUESTS,
        "application/problem+json",
        problem.to_string(),
    ));
    let result = run(&client).await;
    assert_eq!(
        body(&result),
        json!({"schema_version":1,"code":"MCP_CONTROL_REJECTED","http_status":429})
    );
    assert!(!result.to_string().contains(&api.state.credential));
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn success_rejects_duplicate_json_keys_and_wrong_media() {
    let api = api().await;
    let identity = api.state.responses.lock().unwrap().identity.to_string();
    let duplicate = identity.replacen('{', "{\"schema_version\":1,", 1);
    for (media, body) in [("application/json", duplicate), ("text/plain", identity)] {
        api.state.responses.lock().unwrap().raw = Some((StatusCode::OK, media, body));
        assert!(matches!(bridge(&api).await, Err(Failure::Contract)));
    }
}

#[tokio::test]
async fn complete_http_json_crosses_the_former_limit_and_keeps_native_tool_data() {
    let api = api().await;
    let original = {
        let mut values = api.state.responses.lock().unwrap();
        values.padding_bytes = 1024 * 1024 + 1;
        values.brief.clone()
    };
    let (client, task) = connected(&api).await;
    let result = serde_json::to_value(client.call_tool(request("research.get_brief", json!({}))).await.unwrap()).unwrap();
    assert_ne!(result["isError"], true);
    assert_eq!(body(&result), original);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn native_session_continues_after_the_former_cumulative_input_quota() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    // Invalid tool arguments remain invalid, but consuming them must not close
    // the authenticated session or change its later run identity.
    let padding = "x".repeat(256 * 1024);
    for _ in 0..33 {
        let response = client.call_tool(request("run.get", json!({"unknown":padding}))).await;
        assert!(response.is_err() || serde_json::to_value(response.unwrap()).unwrap()["isError"] == true);
    }
    let result = run(&client).await;
    assert_ne!(result["isError"], true);
    assert_eq!(body(&result)["id"], json!(api.binding.run_id));
    assert_eq!(api.state.responses.lock().unwrap().hits, 4);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}
