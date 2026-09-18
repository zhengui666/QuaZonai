//! A bounded local Responses fixture used by the real pinned official App Server.
//! Only the upstream response is synthetic; JSONL, native turns, persistence and
//! process restart remain real. No account, model inference or history-file reads.
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use server::codex_native::{Client, Observation, TokenCounts, TurnStatus};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};

pub const FIRST_PROMPT: &str = "QZ_NATIVE_FIRST_QUESTION: request a bounded research observation.";
pub const SECOND_PROMPT: &str =
    "QZ_NATIVE_SECOND_RESULT: the isolated experiment was rejected; revise the conclusion.";
const FIRST_REPLY: &str = "QZ_NATIVE_FIRST_REPLY: no experiment result is known yet.";
const SECOND_REPLY: &str =
    "QZ_NATIVE_SECOND_REPLY: the supplied experiment rejects the hypothesis.";

#[derive(Default)]
struct Seen {
    count: AtomicUsize,
    prior_context: AtomicBool,
    invalid: AtomicBool,
    slow: AtomicBool,
    fail_continuation: AtomicBool,
    stop_continuation: AtomicBool,
    initial: AtomicBool,
    science: Mutex<Option<SciencePlan>>,
}

struct SciencePlan {
    proposal: contracts::experiments::ExperimentProposalV1,
    experiment: Option<contracts::Id>,
    observation: Option<Value>,
}

pub struct Provider {
    seen: Arc<Seen>,
    task: JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Provider {
    pub async fn start(root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        // This file belongs solely to the newly created test HOME. SYSTEM must
        // honor native configuration without QZ injecting a provider override.
        let configuration = format!(
            "model = \"gpt-5.4\"\nmodel_provider = \"local_fixture\"\n\
             [model_providers.local_fixture]\nname = \"Bounded local Responses fixture\"\n\
             base_url = \"http://{address}/v1\"\nwire_api = \"responses\"\n\
             requires_openai_auth = false\nsupports_websockets = false\n\
             request_max_retries = 0\nstream_max_retries = 0\n"
        );
        std::fs::write(root.join("config.toml"), configuration).unwrap();
        let seen = Arc::new(Seen::default());
        let app = Router::new()
            .route("/v1/responses", post(respond))
            .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
            .with_state(seen.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { seen, task }
    }

    #[allow(dead_code)] // The real science fixture shares this native provider with protocol tests.
    pub fn submit_experiment(&self, proposal: contracts::experiments::ExperimentProposalV1) {
        assert_eq!(self.seen.count.load(Ordering::SeqCst), 0);
        assert!(self
            .seen
            .science
            .lock()
            .unwrap()
            .replace(SciencePlan {
                proposal,
                experiment: None,
                observation: None,
            })
            .is_none());
    }

    #[allow(dead_code)]
    pub fn proposed_experiment(&self) -> contracts::Id {
        self.seen
            .science
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .experiment
            .unwrap()
    }

    #[allow(dead_code)]
    pub fn scientific_observation(&self) -> Value {
        self.seen
            .science
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .observation
            .clone()
            .expect("the original native Thread must actually receive scientific feedback")
    }

    pub fn request_count(&self) -> usize {
        assert!(
            !self.seen.invalid.load(Ordering::SeqCst),
            "invalid native fixture request"
        );
        self.seen.count.load(Ordering::SeqCst)
    }

    pub fn saw_previous_context(&self) -> bool {
        self.seen.prior_context.load(Ordering::SeqCst)
    }

    #[allow(dead_code)] // Used only by the Mission driver's real interrupt test.
    pub fn slow_response(&self) {
        self.seen.slow.store(true, Ordering::SeqCst);
    }

    #[allow(dead_code)] // Mission fault test; other native fixtures share this module.
    pub fn fail_after_tool(&self) {
        self.seen.fail_continuation.store(true, Ordering::SeqCst);
    }

    #[allow(dead_code)] // Only Mission tests exercise the real token-limit interrupt.
    pub fn exceed_tokens_before_tool(&self) {
        self.seen.stop_continuation.store(true, Ordering::SeqCst);
    }

    #[allow(dead_code)] // Only the daemon tests prepare a real initial request.
    pub fn initial_request(&self) {
        self.seen.initial.store(true, Ordering::SeqCst);
    }
}

async fn respond(
    State(seen): State<Arc<Seen>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let ordinal = seen.count.fetch_add(1, Ordering::SeqCst);
    let fail_continuation = seen.fail_continuation.load(Ordering::SeqCst);
    let stop_continuation = seen.stop_continuation.load(Ordering::SeqCst);
    let tool_continuation = fail_continuation || stop_continuation;
    // Observe only controlled fixture sentinels; don't retain or print requests.
    let input = request.get("input").and_then(Value::as_array);
    let input_text = input
        .map(serde_json::to_string)
        .transpose()
        .unwrap_or_default()
        .unwrap_or_default();
    {
        let mut plan = seen.science.lock().unwrap();
        if let Some(plan) = plan.as_mut() {
            assert!(
                !headers.contains_key(header::AUTHORIZATION)
                    && !headers.contains_key(header::COOKIE)
            );
            assert!(request["model"] == "gpt-5.4" && request["stream"] == true);
            assert!(
                !input_text.contains("qz2."),
                "MCP authority must not enter native model input"
            );
            let item = science_item(&seen, plan, ordinal, &request, &input_text);
            return stream_response(ordinal, item, false);
        }
    }
    let review = input_text.contains("QZ_MISSION_REVIEW_V1");
    let review_input_read = input.is_some_and(|items| {
        items.iter().any(|item| {
            item["type"] == "function_call_output"
                && item["output"].to_string().contains("NOT_GRANTED")
        })
    });
    // Native exec may yield a live process instead of its file output. Continue
    // that exact session, never rerun the command or pretend the files were read.
    let review_session = input
        .and_then(|items| {
            items
                .iter()
                .rev()
                .find(|item| item["type"] == "function_call_output")
        })
        .and_then(|item| item["output"].as_str())
        .and_then(|output| output.split("Process running with session ID ").nth(1))
        .and_then(|suffix| suffix.split_whitespace().next())
        .and_then(|id| id.parse::<u64>().ok());
    let valid = !headers.contains_key(header::AUTHORIZATION)
        && !headers.contains_key(header::COOKIE)
        && request["model"] == "gpt-5.4"
        && request["stream"] == true
        && (ordinal < 2 || (review && ordinal < 8))
        && input.is_some()
        && (!tool_continuation || ordinal == 0 || input_text.contains("QZ_NATIVE_TOOL_DONE"))
        && if review {
            (ordinal == 2 || (ordinal > 2 && (review_input_read || review_session.is_some())))
                && !input_text.contains("QZ_MISSION_INITIAL_V1")
                && !input_text.contains(FIRST_REPLY)
        } else {
            input_text.contains(if seen.initial.load(Ordering::SeqCst) {
                if ordinal == 0 {
                    "QZ_MISSION_INITIAL_V1"
                } else {
                    "QZ_MISSION_RESULT_V1"
                }
            } else if ordinal == 0 || tool_continuation {
                FIRST_PROMPT
            } else {
                SECOND_PROMPT
            })
        };
    if !valid {
        // Shape-only fixture diagnostics. Never retain or print native input,
        // headers, tool output contents, credential values or hidden reasoning.
        let outputs: Vec<_> = input
            .into_iter()
            .flatten()
            .filter(|item| item["type"] == "function_call_output")
            .take(4)
            .map(|item| {
                (
                    item["output"].is_string(),
                    item["output"].is_array(),
                    item["output"].to_string().contains("NOT_GRANTED"),
                    [
                        "No such file",
                        "Permission denied",
                        "Operation not permitted",
                        "bwrap",
                        "not found",
                        "Process exited with code 0",
                        "Process running",
                        "failed",
                        "denied",
                    ]
                    .into_iter()
                    .filter(|marker| item["output"].to_string().contains(marker))
                    .collect::<Vec<_>>(),
                )
            })
            .collect();
        eprintln!("native fixture rejected ordinal={ordinal} review={review} review_input_read={review_input_read} output_shapes={outputs:?} prior_research={}", input_text.contains("QZ_MISSION_INITIAL_V1") || input_text.contains(FIRST_REPLY));
        seen.invalid.store(true, Ordering::SeqCst);
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            "{}".into(),
        );
    }
    if ordinal == 1 && !tool_continuation {
        seen.prior_context.store(
            input_text.contains(if seen.initial.load(Ordering::SeqCst) {
                "QZ_MISSION_INITIAL_V1"
            } else {
                FIRST_PROMPT
            }) && input_text.contains(FIRST_REPLY),
            Ordering::SeqCst,
        );
    }
    if seen.slow.load(Ordering::SeqCst) || (stop_continuation && !fail_continuation && ordinal == 1)
    {
        // A delayed upstream response must remain pending across real native
        // CPU throttling; a fast five-second reply races the interrupt itself.
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    let id = format!("qz-local-response-{ordinal}");
    if tool_continuation && ordinal == 1 {
        // A real second native model request receives a broken stream with no
        // usage receipt. The first response's 12 tokens cannot price this request.
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/event-stream")],
            format!(
                "event: response.created\ndata: {}\n\n",
                json!({"type":"response.created","response":{"id":id}})
            ),
        );
    }
    let item = if review && ordinal > 2 && !review_input_read {
        assert!(request["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "write_stdin"));
        json!({"type":"function_call","name":"write_stdin","call_id":format!("qz-review-read-{ordinal}"),
            "arguments":json!({"session_id":review_session.unwrap(),"chars":"","yield_time_ms":1000,"max_output_tokens":4000}).to_string()})
    } else if tool_continuation || (review && ordinal == 2) {
        let tool = request["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "exec_command" || tool["name"] == "shell_command")
            .expect("native shell tool required");
        let command = if review {
            let experiment = input_text
                .split("review-")
                .nth(1)
                .unwrap()
                .chars()
                .take(36)
                .collect::<String>();
            let _: contracts::Id = experiment.clone().try_into().unwrap();
            format!("find review-{experiment} -maxdepth 1 -type f -exec cat {{}} +")
        } else {
            "printf QZ_NATIVE_TOOL_DONE".into()
        };
        let args = if tool["name"] == "exec_command" {
            json!({"cmd":command,"login":false,"yield_time_ms":1000,"max_output_tokens":4000})
        } else {
            json!({"command":command,"login":false,"timeout_ms":10000})
        };
        json!({"type":"function_call","name":tool["name"],"call_id":"qz-partial-usage-tool","arguments":args.to_string()})
    } else {
        let reply = if review {
            let target = input_text
                .split("Independent Reviewer for frozen Alpha ")
                .nth(1)
                .unwrap()
                .chars()
                .take(36)
                .collect::<String>();
            let target: contracts::Id = target.try_into().unwrap();
            json!({"schema_version":1,"alpha_version_id":target,"decision":"PASS","reasons":["Controlled native file-tool review; not market qualification."]}).to_string()
        } else if ordinal == 0 {
            FIRST_REPLY.into()
        } else {
            SECOND_REPLY.into()
        };
        json!({"type":"message","role":"assistant","id":format!("qz-local-message-{ordinal}"),
            "content":[{"type":"output_text","text":reply}]})
    };
    stream_response(ordinal, item, stop_continuation)
}

fn stream_response(
    ordinal: usize,
    item: Value,
    stop_continuation: bool,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let id = format!("qz-local-response-{ordinal}");
    let events = [
        json!({"type":"response.created","response":{"id":id}}),
        json!({"type":"response.output_item.done","item":item}),
        json!({"type":"response.completed","response":{"id":id,"usage":{
            "input_tokens":if stop_continuation {118} else {10},"input_tokens_details":{"cached_tokens":0},
            "output_tokens":2,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":if stop_continuation {120} else {12}
        }}}),
    ];
    // Event shapes follow the pinned upstream core/tests/common/responses.rs.
    let mut body = String::new();
    for event in events {
        body.push_str("event: ");
        body.push_str(event["type"].as_str().unwrap());
        body.push_str("\ndata: ");
        body.push_str(&serde_json::to_string(&event).unwrap());
        body.push_str("\n\n");
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
}

// Recursively inspect already-parsed native tool output. Retain only the specific
// actual Experiment ID, never canonical history, credentials or hidden reasoning.
fn proposed_id(value: &Value, cycle: contracts::Id) -> Option<contracts::Id> {
    if value["trial_source"] == "CODEX" && value["cycle_id"] == cycle.to_string() {
        return value["id"]
            .as_str()
            .and_then(|id| id.to_owned().try_into().ok());
    }
    match value {
        Value::Object(values) => values.values().find_map(|child| proposed_id(child, cycle)),
        Value::Array(values) => values.iter().find_map(|child| proposed_id(child, cycle)),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|child| proposed_id(&child, cycle)),
        _ => None,
    }
}

fn result_observation(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => text
            .split("QZ_MISSION_RESULT_V1\n")
            .nth(1)
            .and_then(|suffix| suffix.lines().next())
            .and_then(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|body| {
                body["stage"] == "VALIDATION" && body["formal_evaluation"] == "PUBLISHED"
            }),
        Value::Object(values) => values.values().find_map(result_observation),
        Value::Array(values) => values.iter().find_map(result_observation),
        _ => None,
    }
}

fn science_item(
    seen: &Seen,
    plan: &mut SciencePlan,
    ordinal: usize,
    request: &Value,
    input: &str,
) -> Value {
    match ordinal {
        0 => {
            assert!(input.contains("QZ_MISSION_INITIAL_V1"));
            assert!(request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["type"] == "tool_search"));
            json!({"type":"tool_search_call","call_id":"native-science-search","execution":"client",
                "arguments":{"query":"quazonai_mission experiment propose","limit":4}})
        }
        1 => {
            let search = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| {
                    item["type"] == "tool_search_output"
                        && item["call_id"] == "native-science-search"
                })
                .expect("the official App Server must return actual MCP discovery");
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
                                .is_some_and(|name| name.ends_with("experiment.propose"))
                        })
                        .map(|tool| (namespace, tool))
                })
                .expect("the actual experiment.propose tool must be discovered");
            json!({"type":"function_call","namespace":namespace["name"],"name":tool["name"],
                "call_id":"native-science-proposal",
                "arguments":json!({"idempotency_key":"native-science-proposal","proposal":plan.proposal}).to_string()})
        }
        2 => {
            let output = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| {
                    item["type"] == "function_call_output"
                        && item["call_id"] == "native-science-proposal"
                })
                .expect("the official App Server must consume the actual MCP reply");
            plan.experiment = Some(
                proposed_id(&output["output"], plan.proposal.cycle_id).expect(
                    "MCP must publish the exact proposed experiment under CODEX authorship",
                ),
            );
            json!({"type":"message","role":"assistant","id":"native-science-initial",
                "content":[{"type":"output_text","text":FIRST_REPLY}]})
        }
        3 => {
            let observed = result_observation(&request["input"])
                .expect("the native resumed Thread must actually consume published science");
            assert_eq!(
                observed["experiment_id"],
                plan.experiment.unwrap().to_string()
            );
            assert!(observed["origin"] == "FIXTURE" && observed["execution_state"] == "SUCCEEDED");
            assert!(
                input.contains("QZ_MISSION_INITIAL_V1") && input.contains(FIRST_REPLY),
                "the result must return to the original persistent Thread"
            );
            for field in ["id", "report_artifact_id", "alpha_version_id"] {
                let _: contracts::Id = observed["evaluation"][field]
                    .as_str()
                    .unwrap()
                    .to_owned()
                    .try_into()
                    .unwrap();
            }
            for field in ["forecast", "folds", "points", "calibration"] {
                assert!(
                    observed.get(field).is_none() && observed["evaluation"].get(field).is_none()
                );
            }
            seen.prior_context.store(true, Ordering::SeqCst);
            let reply = format!(
                "QZ_NATIVE_SCIENTIFIC_CONCLUSION: evaluation={} decision={} report={}; this controlled response acknowledges actual FIXTURE science, not qualification or market evidence.",
                observed["evaluation"]["id"].as_str().unwrap(),
                observed["evaluation"]["decision"].as_str().unwrap(),
                observed["evaluation"]["report_artifact_id"].as_str().unwrap(),
            );
            plan.observation = Some(observed);
            json!({"type":"message","role":"assistant","id":"native-science-conclusion",
                "content":[{"type":"output_text","text":reply}]})
        }
        _ => panic!("unexpected model call in the bounded native science fixture"),
    }
}

pub async fn completed(client: &mut Client, thread: &str, turn: &str) -> TokenCounts {
    tokio::time::timeout(Duration::from_secs(30), async {
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
                        turn: completed,
                    } => {
                        assert_eq!(thread_id, thread);
                        assert_eq!(completed.id, turn);
                        assert_eq!(completed.status, TurnStatus::Completed);
                        assert!(!completed.has_error);
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
                    Observation::TurnStarted {
                        thread_id,
                        turn: started,
                    } => {
                        assert_eq!(thread_id, thread);
                        assert_eq!(started.id, turn);
                    }
                    _ => panic!("unexpected native fixture observation"),
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
    .expect("native turn terminal and usage deadline")
}
