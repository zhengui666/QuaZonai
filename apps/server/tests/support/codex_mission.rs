//! Native App Server owns tool dispatch and persistence. Only model responses
//! are a fixture; stdio MCP, HTTP, PostgreSQL and filesystem enforcement are real.
use super::native;
use axum::{extract::State, http::header, routing::post, Json, Router};
use serde_json::{json, Value};
use server::codex_native::{Client, Launch, Observation, TokenCounts, TurnStatus};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};

struct Seen {
    requests: usize,
    brief: String,
    secret: String,
    command: String,
    saw_brief: bool,
    saw_files: bool,
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
    async fn start(home: &Path, f: &native::Fixture, canary: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        std::fs::write(
            home.join("config.toml"),
            format!(
                "model = \"gpt-5.4\"\nmodel_provider = \"local_fixture\"\n\
            [model_providers.local_fixture]\nname = \"Native Mission fixture\"\n\
            base_url = \"http://{address}/v1\"\nwire_api = \"responses\"\n\
            requires_openai_auth = false\nsupports_websockets = false\n\
            request_max_retries = 0\nstream_max_retries = 0\n\
            [mcp_servers.unrelated]\ncommand = \"/path/that/must/not/run\"\nrequired = true\n"
            ),
        )
        .unwrap();
        let secret = format!("TEST_ONLY_CREDENTIAL_{}", contracts::Id::new());
        std::fs::write(home.join("AGENTS.md"), &secret).unwrap();
        let sentinel = f.private.path().join(format!("{}", contracts::Id::new()));
        std::fs::write(&sentinel, &secret).unwrap();
        // Neither the name nor value has a conventional KEY/TOKEN suffix. This
        // checks native filesystem permissions, not a credential-name filter.
        let command = format!(
            "printf QZ_WORKSPACE > native-proof.txt; /bin/cat native-proof.txt; if /bin/cat '{}' 2>/dev/null; then printf QZ_CREDENTIAL_READ; else printf QZ_CREDENTIAL_UNREADABLE; fi; /usr/bin/printenv {canary}; printf QZ_FILE_CHECK_DONE",
            sentinel.display()
        );
        let seen = Arc::new(Mutex::new(Seen {
            requests: 0,
            brief: f.binding.brief_id.to_string(),
            secret,
            command,
            saw_brief: false,
            saw_files: false,
            resumed: false,
        }));
        let app = Router::new()
            .route("/v1/responses", post(respond))
            .with_state(seen.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { seen, task }
    }
}
async fn respond(
    State(state): State<Arc<Mutex<Seen>>>,
    Json(request): Json<Value>,
) -> ([(header::HeaderName, &'static str); 1], String) {
    let mut seen = state.lock().unwrap();
    let input = request["input"].to_string();
    let complete_request = request.to_string();
    assert!(
        !complete_request.contains(&seen.secret),
        "native Mission leaked a synthetic credential"
    );
    assert!(
        !complete_request.contains("TEST_ONLY_NATIVE_ENV_CREDENTIAL"),
        "native shell inherited service environment"
    );
    assert!(
        !complete_request.contains("qz2."),
        "MCP credential entered a model request"
    );
    let ordinal = seen.requests;
    seen.requests += 1;
    let item = match ordinal {
        0 => {
            // The pinned native gpt-5.4 catalog advertises client-side tool
            // search. Codex discovers and dispatches MCP itself; QZ does not.
            assert!(request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["type"] == "tool_search"));
            assert!(!request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "create_goal"));
            json!({"type":"tool_search_call","call_id":"qz-tool-search","execution":"client","arguments":{"query":"quazonai_mission research get_brief","limit":4}})
        }
        1 => {
            let search = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| {
                    item["type"] == "tool_search_output" && item["call_id"] == "qz-tool-search"
                })
                .expect("native tool search output");
            let namespaces = search["tools"]
                .as_array()
                .expect("native namespace catalog");
            let (namespace, tool) = namespaces
                .iter()
                .find_map(|namespace| {
                    namespace["tools"]
                        .as_array()?
                        .iter()
                        .find(|tool| {
                            tool["name"]
                                .as_str()
                                .is_some_and(|name| name.ends_with("get_brief"))
                        })
                        .map(|tool| (namespace, tool))
                })
                .expect("native searched brief tool");
            json!({"type":"function_call","namespace":namespace["name"],"name":tool["name"],"call_id":"qz-brief-call","arguments":"{}"})
        }
        2 => {
            let tools = request["tools"].as_array().expect("native tool catalog");
            seen.saw_brief = input.contains(&seen.brief) && input.contains("hypothesis");
            assert!(seen.saw_brief, "actual MCP brief result missing");
            let tool = tools
                .iter()
                .find(|v| v["name"] == "exec_command" || v["name"] == "shell_command")
                .expect("native shell tool");
            let args = if tool["name"] == "exec_command" {
                json!({"cmd":seen.command,"login":false,"yield_time_ms":1000,"max_output_tokens":1000})
            } else {
                json!({"command":seen.command,"login":false,"timeout_ms":10000})
            };
            json!({"type":"function_call","name":tool["name"],"call_id":"qz-file-call","arguments":args.to_string()})
        }
        3 => {
            let output = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == "qz-file-call"
                })
                .expect("native file result")["output"]
                .to_string();
            seen.saw_files = output.contains("QZ_WORKSPACE")
                && output.contains("QZ_FILE_CHECK_DONE")
                && output.contains("QZ_CREDENTIAL_UNREADABLE")
                && !output.contains("QZ_CREDENTIAL_READ");
            assert!(
                seen.saw_files,
                "native restricted filesystem result missing: {output}"
            );
            message("QZ_MISSION_FIRST_REPLY")
        }
        4 => {
            seen.resumed =
                input.contains("QZ_MISSION_FIRST_REPLY") && input.contains("QZ_MISSION_RESUME");
            assert!(seen.resumed, "native same-Thread history missing");
            message("QZ_MISSION_RESUMED_REPLY")
        }
        _ => panic!("unexpected native model request"),
    };
    let id = format!("qz-mission-{ordinal}");
    let events = [
        json!({"type":"response.created","response":{"id":id}}),
        json!({"type":"response.output_item.done","item":item}),
        json!({"type":"response.completed","response":{"id":id,"usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}}}),
    ];
    let body = events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {}\n\n",
                event["type"].as_str().unwrap(),
                event
            )
        })
        .collect::<String>();
    ([(header::CONTENT_TYPE, "text/event-stream")], body)
}
fn message(text: &str) -> Value {
    json!({"type":"message","role":"assistant","id":text,"content":[{"type":"output_text","text":text}]})
}
fn launch(home: &Path, work: &Path, canary: &str) -> Launch {
    Launch {
        container: None,
        binary: std::env::var_os("CODEX_NATIVE_BIN")
            .expect("pinned native binary required")
            .into(),
        home: home.into(),
        codex_home: home.into(),
        working_directory: work.into(),
        executable_path: std::env::var_os("PATH").unwrap(),
        native_environment: BTreeMap::from([(
            canary.into(),
            "TEST_ONLY_NATIVE_ENV_CREDENTIAL".into(),
        )]),
    }
}
async fn completed(
    client: &mut Client,
    thread: &str,
    turn: &str,
    settled_prior: Option<&str>,
) -> TokenCounts {
    tokio::time::timeout(Duration::from_secs(45), async {
        let mut terminal = false;
        let mut usage = None;
        loop {
            for observation in client
                .observations(Duration::from_millis(200))
                .await
                .unwrap()
            {
                match observation {
                    Observation::TurnCompleted {
                        thread_id,
                        turn: actual,
                    } => {
                        assert_eq!(thread_id, thread);
                        if Some(actual.id.as_str()) == settled_prior {
                            continue;
                        }
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
                        if Some(turn_id.as_str()) == settled_prior {
                            continue;
                        }
                        assert_eq!(turn_id, turn);
                        usage = Some(total);
                    }
                    Observation::TurnStarted {
                        thread_id,
                        turn: started,
                    } => {
                        assert_eq!(thread_id, thread);
                        if Some(started.id.as_str()) == settled_prior {
                            continue;
                        }
                        assert_eq!(started.id, turn);
                    }
                    _ => {}
                }
            }
            if terminal {
                if let Some(usage) = usage {
                    return usage;
                }
            }
        }
    })
    .await
    .expect("native Mission terminal and usage deadline")
}
#[sqlx::test(migrations = "../../migrations")]
async fn native_mission_owns_mcp_dispatch_and_resumes_without_exposing_credentials(pool: PgPool) {
    let f = native::fixture(&pool, &["RUN_READ", "RESEARCH_READ"]).await;
    let home = tempfile::tempdir().unwrap();
    let canary = format!("QZ_{}", contracts::Id::new().to_string().replace('-', ""));
    let provider = Provider::start(home.path(), &f, &canary).await;
    let options = f.mission_options();
    let params = options.start_params().unwrap();
    assert!(params["config"]["mcp_servers"]["quazonai_mission"].is_object());
    let mut first = Client::start(launch(home.path(), &f.work, &canary))
        .await
        .unwrap();
    assert!(matches!(
        first.start_thread(&options).await,
        Err(server::codex_native::NativeFailure::ProfileInstructions)
    ));
    assert_eq!(provider.seen.lock().unwrap().requests, 0);
    // Delete only the synthetic file this test just created in its disposable
    // HOME. The product neither reads, removes nor edits a user's instruction file.
    std::fs::remove_file(home.path().join("AGENTS.md")).unwrap();
    let config = std::fs::read_to_string(home.path().join("config.toml")).unwrap();
    let marker = provider.seen.lock().unwrap().secret.clone();
    let prompt = home.path().join("personal-prompt.txt");
    std::fs::write(&prompt, &marker).unwrap();
    for key in [
        "instructions",
        "developer_instructions",
        "model_instructions_file",
    ] {
        let value = if key == "model_instructions_file" {
            prompt.to_str().unwrap()
        } else {
            &marker
        };
        std::fs::write(
            home.path().join("config.toml"),
            format!(
                "{key} = {}\n{config}",
                serde_json::to_string(value).unwrap()
            ),
        )
        .unwrap();
        assert!(
            matches!(
                first.start_thread(&options).await,
                Err(server::codex_native::NativeFailure::ProfileInstructions)
            ),
            "personal override was not rejected: {key}"
        );
        assert_eq!(provider.seen.lock().unwrap().requests, 0);
    }
    std::fs::write(home.path().join("config.toml"), config).unwrap();
    let thread = first.start_thread(&options).await.unwrap();
    let inventory = first.mission_tool_names(&thread.thread.id).await.unwrap();
    assert!(
        inventory.get("quazonai_mission").is_some_and(|tools| tools
            .iter()
            .any(|tool| tool.ends_with("research.get_brief"))),
        "native MCP inventory: {inventory:?}"
    );
    let turn = first
        .start_turn(
            "mission-first",
            &thread.thread.id,
            "QZ_MISSION_FIRST: inspect the bound Brief through MCP and test the workspace.",
        )
        .await
        .unwrap();
    let initial_usage = completed(&mut first, &thread.thread.id, &turn.id, None).await;
    let settled_prior = turn.id.clone();
    // Four real native model requests within one Turn, not the last request's 12.
    assert_eq!(initial_usage.total, 48);
    assert_eq!(
        std::fs::read_to_string(f.work.join("native-proof.txt")).unwrap(),
        "QZ_WORKSPACE"
    );
    first.close().await.unwrap();
    let mut second = Client::start(launch(home.path(), &f.work, &canary))
        .await
        .unwrap();
    let resumed = second
        .resume_thread(&thread.thread.id, &options)
        .await
        .unwrap();
    assert_eq!(thread.thread.id, resumed.thread.id);
    let turn = second
        .start_turn(
            "mission-resume",
            &thread.thread.id,
            "QZ_MISSION_RESUME: use the existing observed Brief.",
        )
        .await
        .unwrap();
    let cumulative = completed(
        &mut second,
        &thread.thread.id,
        &turn.id,
        Some(&settled_prior),
    )
    .await;
    assert_eq!(cumulative.since(initial_usage).unwrap().total, 12);
    second.close().await.unwrap();
    let seen = provider.seen.lock().unwrap();
    assert!(seen.saw_brief && seen.saw_files && seen.resumed);
    assert_eq!(seen.requests, 5);
}
