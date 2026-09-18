//! Native App Server -> real Job -> MCP/HTTP evidence -> same persisted Thread.
//! Provider decisions and parent research are fixtures; computation is not.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/experiments.rs"]
mod experiment_support;
#[path = "support/mcp_authoring.rs"]
#[allow(dead_code)] // Shared fixture also serves independent SDK/authoring regressions.
mod native;
#[path = "../../../tests/support/research.rs"]
mod research_support;

use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use contracts::{
    portfolio::{AllocationInputV1, AllocationResultV1, SolverStatus},
    Id,
};
use integrations::artifacts::ArtifactStore;
use serde_json::{json, Value};
use server::codex_native::{Client, Launch, Observation, TokenCounts, TurnStatus};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};

const SUMMARY: &str = "QZ_NATIVE_SCIENCE_RESULT:";

struct Seen {
    case: usize,
    phase: u8,
    requests: usize,
    computations: usize,
    publications: usize,
    last_call: String,
    session: Option<u64>,
    requests_by_case: Vec<AllocationInputV1>,
    results: Vec<Value>,
    artifacts: Vec<String>,
    run: Id,
    attempt: Id,
    resumed: bool,
}
struct Provider {
    seen: Arc<Mutex<Seen>>,
    task: JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Provider {
    async fn start(home: &Path, f: &native::Fixture, requests: Vec<AllocationInputV1>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        fs::write(
            home.join("config.toml"),
            format!(
                "model = \"gpt-5.4\"\nmodel_provider = \"local_fixture\"\n\
                 [model_providers.local_fixture]\nname = \"Scientific output consumption fixture\"\n\
                 base_url = \"http://{address}/v1\"\nwire_api = \"responses\"\n\
                 requires_openai_auth = false\nsupports_websockets = false\n\
                 request_max_retries = 0\nstream_max_retries = 0\n"
            ),
        )
        .unwrap();
        let seen = Arc::new(Mutex::new(Seen {
            case: 0,
            phase: 0,
            requests: 0,
            computations: 0,
            publications: 0,
            last_call: String::new(),
            session: None,
            requests_by_case: requests,
            results: Vec::new(),
            artifacts: Vec::new(),
            run: f.binding.run_id,
            attempt: f.binding.attempt_id,
            resumed: false,
        }));
        let app = Router::new()
            .route("/v1/responses", post(respond))
            .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
            .with_state(seen.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { seen, task }
    }
}

fn output<'a>(request: &'a Value, kind: &str, call: &str) -> &'a Value {
    &request["input"]
        .as_array()
        .expect("native request input")
        .iter()
        .rev()
        .find(|item| item["type"] == kind && item["call_id"] == call)
        .expect("the exact native tool result must be consumed")["output"]
}

fn mcp_document(value: &Value) -> Value {
    let document: Value = if let Some(text) = value.as_str() {
        serde_json::from_str(text).expect("native MCP JSON output")
    } else {
        value.clone()
    };
    assert_ne!(document["isError"], true, "the real MCP call must succeed");
    if let Some(content) = document["content"].as_array() {
        assert_eq!(content.len(), 1);
        serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap()
    } else {
        document
    }
}

async fn respond(
    State(state): State<Arc<Mutex<Seen>>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    // Check only controlled protocol fields; never print requests or native history.
    assert!(!headers.contains_key(header::AUTHORIZATION));
    assert!(!headers.contains_key(header::COOKIE));
    assert_eq!(request["model"], "gpt-5.4");
    assert_eq!(request["stream"], true);
    assert!(!request.to_string().contains("qz2."));
    let mut seen = state.lock().unwrap();
    assert!(seen.case < 2 && seen.requests < 20);
    let ordinal = seen.requests;
    seen.requests += 1;
    let case = seen.case;
    let call = format!("science-{case}-{ordinal}");
    let item = match seen.phase {
        0 => {
            let input = request["input"].to_string();
            assert!(input.contains(&format!("QZ_SCIENCE_CASE_{case}")));
            if case == 1 {
                seen.resumed = input.contains(SUMMARY) && input.contains(&seen.artifacts[0]);
                assert!(
                    seen.resumed,
                    "the native resumed context lost its first evidence"
                );
            }
            assert!(request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "exec_command"));
            seen.computations += 1;
            seen.phase = 1;
            json!({"type":"function_call","name":"exec_command","call_id":call,
                "arguments":json!({
                    "cmd":format!("./native-job allocate < input-{case}.json > result-{case}.json && /bin/cat result-{case}.json"),
                    "login":false,"yield_time_ms":1000,"max_output_tokens":4000
                }).to_string()})
        }
        1 => {
            let text = output(&request, "function_call_output", &seen.last_call)
                .as_str()
                .expect("native exec output");
            if let Some(suffix) = text.split("Process running with session ID ").nth(1) {
                let session: u64 = suffix.split_whitespace().next().unwrap().parse().unwrap();
                if let Some(original) = seen.session {
                    assert_eq!(session, original);
                }
                seen.session = Some(session);
                // Poll the original native exec session; never execute the job twice.
                json!({"type":"function_call","name":"write_stdin","call_id":call,
                    "arguments":json!({"session_id":session,"chars":"","yield_time_ms":1000,
                        "max_output_tokens":4000}).to_string()})
            } else {
                assert!(text.contains("Process exited with code 0"));
                let raw = text.split_once("Final output:\n").unwrap().1;
                let actual: AllocationResultV1 = serde_json::from_str(raw).unwrap();
                domain::portfolio::allocation_result(&seen.requests_by_case[case], &actual)
                    .unwrap();
                if case == 0 {
                    assert_eq!(actual.solver_status, SolverStatus::Optimal);
                    let targets = actual.targets.as_ref().unwrap();
                    assert_eq!(targets.len(), 2);
                    for (target, expected) in targets.iter().zip([0.8, 0.2]) {
                        let weight: f64 = target.weight.as_decimal().to_string().parse().unwrap();
                        assert!((weight - expected).abs() < 1e-5);
                    }
                } else {
                    assert_eq!(actual.solver_status, SolverStatus::Infeasible);
                    assert!(actual.targets.is_none() && actual.cash_weight.is_none());
                    assert!(actual.reason_code.is_some());
                }
                seen.results.push(serde_json::from_str(raw).unwrap());
                seen.phase = 2;
                json!({"type":"tool_search_call","call_id":call,"execution":"client",
                    "arguments":{"query":"quazonai_mission artifact submit","limit":2}})
            }
        }
        2 => {
            let search = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .rev()
                .find(|item| {
                    item["type"] == "tool_search_output" && item["call_id"] == seen.last_call
                })
                .expect("native tool search must return the publication tool");
            let (namespace, tool) = search["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|namespace| {
                    namespace["tools"]
                        .as_array()?
                        .iter()
                        .find(|tool| {
                            tool["name"]
                                .as_str()
                                .is_some_and(|name| name.ends_with("artifact.submit"))
                        })
                        .map(|tool| (namespace, tool))
                })
                .expect("the real Mission publication tool must be discoverable");
            seen.phase = 3;
            seen.publications += 1;
            json!({"type":"function_call","namespace":namespace["name"],"name":tool["name"],
                "call_id":call,"arguments":json!({"schema_version":1,"kind":"REPORT",
                    "workspace_relative_path":format!("result-{case}.json"),
                    "idempotency_key":format!("native-science-{case}")}).to_string()})
        }
        3 => {
            let receipt = mcp_document(output(&request, "function_call_output", &seen.last_call));
            assert_eq!(receipt["schema_version"], 1);
            assert_eq!(receipt["replayed"], false);
            let artifact = &receipt["resource"];
            assert_eq!(artifact["kind"], "REPORT");
            assert_eq!(artifact["origin"], "SYNTHETIC");
            assert_eq!(artifact["producer_run_id"], json!(seen.run));
            assert_eq!(artifact["producer_attempt_id"], json!(seen.attempt));
            let id = artifact["id"].as_str().unwrap().to_owned();
            let _: Id = id.clone().try_into().unwrap();
            assert!(!seen.artifacts.contains(&id));
            let summary = json!({
                "artifact_id":id,"solver_status":seen.results[case]["solver_status"],
                "targets":seen.results[case]["targets"],
                "prior_artifact_id":seen.artifacts.first(),
                "provider":"CONTROLLED_FIXTURE","qualification_granted":false
            });
            seen.artifacts.push(id);
            seen.case += 1;
            seen.phase = 0;
            seen.session = None;
            json!({"type":"message","role":"assistant","id":format!("science-summary-{case}"),
                "content":[{"type":"output_text","text":format!("{SUMMARY}{summary}")}]})
        }
        _ => unreachable!(),
    };
    seen.last_call = call;
    let id = format!("science-response-{ordinal}");
    let events = [
        json!({"type":"response.created","response":{"id":id}}),
        json!({"type":"response.output_item.done","item":item}),
        json!({"type":"response.completed","response":{"id":id,
            "usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}}}),
    ];
    let body = events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
}

fn launch(home: &Path, work: &Path) -> Launch {
    Launch {
        binary: std::env::var_os("CODEX_NATIVE_BIN")
            .expect("pinned Codex required")
            .into(),
        home: home.into(),
        codex_home: home.into(),
        working_directory: work.into(),
        executable_path: std::env::var_os("PATH").unwrap(),
        native_environment: BTreeMap::new(),
        custom_provider: None,
    }
}

async fn completed(client: &mut Client, thread: &str, turn: &str) -> TokenCounts {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut terminal = false;
        let mut usage = None;
        loop {
            for event in client
                .observations(Duration::from_millis(100))
                .await
                .unwrap()
            {
                match event {
                    Observation::TurnCompleted {
                        thread_id,
                        turn: actual,
                    } => {
                        assert_eq!(thread_id, thread);
                        assert_eq!(actual.id, turn);
                        assert_eq!(actual.status, TurnStatus::Completed);
                        assert!(!actual.has_error);
                        terminal = true;
                    }
                    Observation::Usage {
                        thread_id,
                        turn_id,
                        total,
                    } => {
                        assert_eq!(thread_id, thread);
                        assert_eq!(turn_id, turn);
                        usage = Some(total);
                    }
                    _ => {}
                }
            }
            if let (true, Some(counts)) = (terminal, usage) {
                return counts;
            }
        }
    })
    .await
    .expect("native science turn must finish with actual usage")
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_science_outputs_are_published_and_consumed_in_one_resumable_thread(pool: PgPool) {
    let f = native::fixture(&pool, &["RUN_READ", "RESEARCH_READ", "ARTIFACT_SUBMIT"]).await;
    let binary = PathBuf::from(
        std::env::var_os("QUAZONAI_NATIVE_JOB_BIN").expect("explicit built Job is required"),
    );
    assert!(binary.is_absolute() && binary.is_file());
    let executable = f.work.join("native-job");
    fs::copy(&binary, &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o555)).unwrap();
    let first: AllocationInputV1 = serde_json::from_slice(include_bytes!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut second = first.clone();
    second.constraints.max_asset_weight = "0.1".parse().unwrap();
    let inputs = vec![first, second];
    for (case, input) in inputs.iter().enumerate() {
        fs::write(
            f.work.join(format!("input-{case}.json")),
            serde_json::to_vec(input).unwrap(),
        )
        .unwrap();
        assert!(!f.work.join(format!("result-{case}.json")).exists());
    }
    let home = tempfile::tempdir().unwrap();
    let provider = Provider::start(home.path(), &f, inputs.clone()).await;
    let options = f.mission_options();
    let mut client = Client::start(launch(home.path(), &f.work)).await.unwrap();
    assert!(client.account().await.unwrap().account.is_none());
    let thread = client.start_thread(&options).await.unwrap();
    let initial = client
        .start_turn(
            "native-science-first",
            &thread.thread.id,
            "QZ_SCIENCE_CASE_0: compute and publish original evidence.",
        )
        .await
        .unwrap();
    let initial_usage = completed(&mut client, &thread.thread.id, &initial.id).await;
    let first_summary = client
        .public_summary(&thread.thread.id, &initial.id)
        .await
        .unwrap()
        .unwrap();
    assert!(first_summary.text.starts_with(SUMMARY));
    let first_requests = provider.seen.lock().unwrap().requests;
    assert_eq!(initial_usage.total, first_requests as i64 * 12);
    let original_bytes = fs::read(f.work.join("result-0.json")).unwrap();
    client.close().await.unwrap();

    let mut resumed_client = Client::start(launch(home.path(), &f.work)).await.unwrap();
    let resumed = resumed_client
        .resume_thread(&thread.thread.id, &options)
        .await
        .unwrap();
    assert_eq!(resumed.thread.id, thread.thread.id);
    let next = resumed_client
        .start_turn(
            "native-science-second",
            &thread.thread.id,
            "QZ_SCIENCE_CASE_1: recompute with the new bounds; do not reuse old weights.",
        )
        .await
        .unwrap();
    let cumulative = completed(&mut resumed_client, &thread.thread.id, &next.id).await;
    let second_summary = resumed_client
        .public_summary(&thread.thread.id, &next.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resumed_client
            .public_summary(&thread.thread.id, &initial.id)
            .await
            .unwrap(),
        Some(first_summary.clone())
    );
    let turns = resumed_client.turns(&thread.thread.id).await.unwrap();
    assert_eq!(turns.len(), 2);
    assert!(turns
        .iter()
        .all(|turn| turn.status == TurnStatus::Completed));
    let requests_before_close = provider.seen.lock().unwrap().requests;
    resumed_client.close().await.unwrap();
    assert!(!home.path().join("auth.json").exists());

    let (artifacts, results) = {
        let seen = provider.seen.lock().unwrap();
        assert_eq!(seen.case, 2);
        assert!(seen.resumed);
        assert_eq!(seen.computations, 2);
        assert_eq!(seen.publications, 2);
        assert_eq!(seen.requests, requests_before_close);
        assert_eq!(
            cumulative.since(initial_usage).unwrap().total,
            (seen.requests - first_requests) as i64 * 12
        );
        (seen.artifacts.clone(), seen.results.clone())
    };
    let objects = ArtifactStore::open(&f.private.path().join("artifacts")).unwrap();
    for (case, summary) in [first_summary, second_summary].iter().enumerate() {
        let conclusion: Value =
            serde_json::from_str(summary.text.strip_prefix(SUMMARY).unwrap()).unwrap();
        assert_eq!(conclusion["artifact_id"], artifacts[case]);
        assert_eq!(conclusion["solver_status"], results[case]["solver_status"]);
        assert_eq!(conclusion["targets"], results[case]["targets"]);
        assert_eq!(conclusion["qualification_granted"], false);
        if case == 1 {
            assert_eq!(conclusion["prior_artifact_id"], artifacts[0]);
            assert!(conclusion["targets"].is_null());
        }
        let id: Id = artifacts[case].clone().try_into().unwrap();
        let locator = f.store.artifact_content(&f.operator, id).await.unwrap();
        let bytes = objects.read(id, locator.metadata.byte_count).unwrap();
        assert_eq!(
            bytes,
            fs::read(f.work.join(format!("result-{case}.json"))).unwrap()
        );
        let actual: AllocationResultV1 = serde_json::from_slice(&bytes).unwrap();
        domain::portfolio::allocation_result(&inputs[case], &actual).unwrap();
        assert_eq!(serde_json::to_value(actual).unwrap(), results[case]);
    }
    assert_eq!(
        original_bytes,
        fs::read(f.work.join("result-0.json")).unwrap()
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1 AND producer_attempt_id=$2",
    )
    .bind(f.binding.run_id.as_uuid())
    .bind(f.binding.attempt_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    println!("native science: two real Job results, two immutable HTTP publications, one resumed Thread; model decisions=CONTROLLED_FIXTURE, qualification=false");
}
