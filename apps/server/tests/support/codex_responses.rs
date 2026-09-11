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
        Arc,
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
}

async fn respond(
    State(seen): State<Arc<Seen>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let ordinal = seen.count.fetch_add(1, Ordering::SeqCst);
    let fail_continuation = seen.fail_continuation.load(Ordering::SeqCst);
    // Observe only controlled fixture sentinels; don't retain or print requests.
    let input = request.get("input").and_then(Value::as_array);
    let input_text = input
        .map(serde_json::to_string)
        .transpose()
        .unwrap_or_default()
        .unwrap_or_default();
    let valid = !headers.contains_key(header::AUTHORIZATION)
        && !headers.contains_key(header::COOKIE)
        && request["model"] == "gpt-5.4"
        && request["stream"] == true
        && ordinal < 2
        && input.is_some()
        && (!fail_continuation || ordinal == 0 || input_text.contains("QZ_NATIVE_TOOL_DONE"))
        && input_text.contains(if ordinal == 0 || fail_continuation {
            FIRST_PROMPT
        } else {
            SECOND_PROMPT
        });
    if !valid {
        seen.invalid.store(true, Ordering::SeqCst);
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            "{}".into(),
        );
    }
    if ordinal == 1 && !fail_continuation {
        seen.prior_context.store(
            input_text.contains(FIRST_PROMPT) && input_text.contains(FIRST_REPLY),
            Ordering::SeqCst,
        );
    }
    if seen.slow.load(Ordering::SeqCst) {
        // A delayed upstream response must remain pending across real native
        // CPU throttling; a fast five-second reply races the interrupt itself.
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    let id = format!("qz-local-response-{ordinal}");
    if fail_continuation && ordinal == 1 {
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
    let item = if fail_continuation {
        let tool = request["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "exec_command" || tool["name"] == "shell_command")
            .expect("native shell tool required");
        let args = if tool["name"] == "exec_command" {
            json!({"cmd":"printf QZ_NATIVE_TOOL_DONE","login":false,"yield_time_ms":1000,"max_output_tokens":100})
        } else {
            json!({"command":"printf QZ_NATIVE_TOOL_DONE","login":false,"timeout_ms":10000})
        };
        json!({"type":"function_call","name":tool["name"],"call_id":"qz-partial-usage-tool","arguments":args.to_string()})
    } else {
        json!({"type":"message","role":"assistant","id":format!("qz-local-message-{ordinal}"),
            "content":[{"type":"output_text","text":if ordinal==0 {FIRST_REPLY} else {SECOND_REPLY}}]})
    };
    let events = [
        json!({"type":"response.created","response":{"id":id}}),
        json!({"type":"response.output_item.done","item":item}),
        json!({"type":"response.completed","response":{"id":id,"usage":{
            "input_tokens":10,"input_tokens_details":{"cached_tokens":0},
            "output_tokens":2,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":12
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
