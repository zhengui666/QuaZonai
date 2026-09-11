//! Actual CLI subprocesses over real TCP listeners. Payloads are controlled fixtures,
//! not production identity, PostgreSQL authority or scientific acceptance evidence.
use axum::{
    body::{to_bytes, Body, Bytes},
    http::{header, Request, Response, StatusCode},
    Router,
};
use contracts::Id;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    convert::Infallible,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::TcpListener, process::Command, task::JoinHandle};

struct Reply {
    status: StatusCode,
    media: &'static str,
    bytes: Vec<u8>,
    headers: Vec<(&'static str, String)>,
    chunk_bytes: Option<usize>,
}
impl Reply {
    fn json(value: Value) -> Self {
        Self {
            status: StatusCode::OK,
            media: "application/json",
            bytes: serde_json::to_vec(&value).unwrap(),
            headers: vec![],
            chunk_bytes: None,
        }
    }
}
struct Seen {
    method: String,
    uri: String,
    key: Option<String>,
    operator: Option<String>,
    after: Option<String>,
    authorized: bool,
    body: Vec<u8>,
}
struct Fixture {
    directory: tempfile::TempDir,
    origin: String,
    token: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    task: JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new(replies: impl FnOnce(&str) -> Vec<Reply>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let token = integrations::authentication::format_machine_token(
            Id::new(),
            &integrations::authentication::random_capability(),
        )
        .unwrap();
        let credential = directory.path().join("cli-credential");
        fs::write(&credential, &token).unwrap();
        fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let queue = Arc::new(Mutex::new(VecDeque::from(replies(&token))));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let records = seen.clone();
        let expected = format!("Bearer {token}");
        let app = Router::new().fallback(move |request: Request<Body>| {
            let queue = queue.clone();
            let records = records.clone();
            let expected = expected.clone();
            async move {
                // The native body is Send but not Sync. Do not retain a closure
                // borrowing the entire Request while asynchronously reading it.
                let (head, body) = request.into_parts();
                let value = |name| {
                    head.headers
                        .get(name)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned)
                };
                let method = head.method.to_string();
                let uri = head.uri.to_string();
                let key = value("idempotency-key");
                let operator = value("x-operator-grant");
                let after = value("last-event-id");
                let authorized = head.headers.get_all(header::AUTHORIZATION).iter().count() == 1
                    && head
                        .headers
                        .get(header::AUTHORIZATION)
                        .is_some_and(|value| value == expected.as_str())
                    && !head.headers.contains_key(header::COOKIE);
                let body = to_bytes(body, 16 * 1024 * 1024).await.unwrap().to_vec();
                records.lock().unwrap().push(Seen {
                    method,
                    uri,
                    key,
                    operator,
                    after,
                    authorized,
                    body,
                });
                let reply = queue
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected native request, including an implicit retry");
                let mut response = Response::builder()
                    .status(reply.status)
                    .header(header::CONTENT_TYPE, reply.media);
                for (name, value) in reply.headers {
                    response = response.header(name, value);
                }
                let body = if let Some(size) = reply.chunk_bytes {
                    let chunks: Vec<Result<Bytes, Infallible>> = reply
                        .bytes
                        .chunks(size)
                        .map(|part| Ok(Bytes::copy_from_slice(part)))
                        .collect();
                    Body::from_stream(futures_util::stream::iter(chunks))
                } else {
                    Body::from(reply.bytes)
                };
                response.body(body).unwrap()
            }
        });
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            directory,
            origin,
            token,
            seen,
            task,
        }
    }

    async fn execute(&self, arguments: &[String], input: &[u8]) -> std::process::Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
            .arg("client")
            .arg("--origin")
            .arg(&self.origin)
            .arg("--credential-file")
            .arg(self.directory.path().join("cli-credential"))
            .arg("--development-http")
            .args(arguments)
            .env_clear()
            .kill_on_drop(true)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input).await.unwrap();
        drop(stdin);
        let result = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
            .await
            .expect("native CLI deadline")
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&result.stdout).contains(&self.token),
            "credential leaked through CLI stdout"
        );
        assert!(
            !String::from_utf8_lossy(&result.stderr).contains(&self.token),
            "credential leaked through CLI stderr"
        );
        assert!(
            self.seen.lock().unwrap().iter().all(|seen| seen.authorized),
            "native machine authorization mismatch"
        );
        result
    }
}
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
fn source(id: Id) -> Value {
    json!({"id":id,"name":"Controlled native catalog","runtime_id":Id::new(),"native_catalog_ref":"registered/catalog","provider_kind":"NAUTILUS_CATALOG","enabled":true,"revision":"9007199254740993","created_at":"2026-09-10T00:00:00Z","updated_at":"2026-09-10T00:00:00Z"})
}
fn problem(status: u16) -> Value {
    json!({"type":"urn:quazonai:problem:revision-conflict","title":"REVISION_CONFLICT","status":status,"code":"REVISION_CONFLICT","detail":"Controlled revision mismatch","request_id":Id::new(),"retryable":false,"current_revision":"9007199254740994","field_errors":[],"safe_next_actions":["RELOAD"]})
}

#[tokio::test]
async fn native_cli_reads_typed_data_pages_with_exact_uuid_cursor_and_bigint_strings() {
    let id = Id::new();
    let row = source(id);
    let expected = row.clone();
    let f = Fixture::new(|_| {
        vec![Reply::json(
            json!({"schema_version":1,"items":[row],"next_cursor":null}),
        )]
    })
    .await;
    let result = f
        .execute(
            &args(&[
                "data",
                "source",
                "list",
                "--limit",
                "1",
                "--cursor",
                &id.to_string(),
            ]),
            b"",
        )
        .await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let body: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(body["items"][0], expected);
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "GET");
    assert_eq!(
        seen[0].uri,
        format!("/api/v2/data/sources?limit=1&cursor={id}")
    );
    assert!(seen[0].body.is_empty() && seen[0].key.is_none() && seen[0].operator.is_none());
}

#[tokio::test]
async fn native_cli_writes_one_exact_operator_intent_without_inventing_a_grant_or_retry() {
    let row = source(Id::new());
    let replay = row.clone();
    let f = Fixture::new(|_| {
        let mut first = Reply::json(json!({"schema_version":1,"resource":row,"replayed":false}));
        first.status = StatusCode::CREATED;
        let mut again = Reply::json(json!({"schema_version":1,"resource":replay,"replayed":true}));
        again.status = StatusCode::CREATED;
        vec![first, again]
    })
    .await;
    let grant = Id::new();
    let command = args(&[
        "--idempotency-key",
        "native fixed intent",
        "--operator-grant",
        &grant.to_string(),
        "data",
        "source",
        "create",
    ]);
    let body = json!({"schema_version":1,"name":"Native source","runtime_id":Id::new(),"native_catalog_ref":"registered/catalog","provider_kind":"NAUTILUS_CATALOG","enabled":true});
    for replayed in [false, true] {
        let result = f
            .execute(&command, &serde_json::to_vec(&body).unwrap())
            .await;
        assert!(result.status.success());
        let reply: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(reply["replayed"], replayed);
    }
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for request in seen.iter() {
        assert_eq!(request.uri, "/api/v2/data/sources");
        assert_eq!(request.method, "POST");
        assert_eq!(request.key.as_deref(), Some("native fixed intent"));
        assert_eq!(
            request.operator.as_deref(),
            Some(grant.to_string().as_str())
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap(),
            body
        );
    }
    assert_eq!(seen[0].body, seen[1].body);
}

#[tokio::test]
async fn native_cli_data_validation_returns_the_typed_queued_run_without_claiming_completion() {
    let run = Id::new();
    let project = Id::new();
    let input = Id::new();
    let body = json!({"schema_version":1,"project_id":project,"input_set_id":input,
        "runtime_id":Id::new(),"expected_runtime_revision":"9007199254740993",
        "limits":{"schema_version":1,"experiments":0,"cpu_seconds":"10","wall_seconds":60,
        "memory_mib":512,"output_bytes":"65536"}});
    let resource = json!({"schema_version":1,"id":run,"project_id":project,"cycle_id":null,
        "kind":"DATA_VALIDATE","input_set_id":input,"state":"QUEUED","current_attempt_no":0,
        "active_attempt_id":null,"last_event_seq":"1","deadline_at":"2026-09-11T00:01:00Z",
        "cancellation_requested_at":null,"terminal_reason_code":null,"queued_at":"2026-09-11T00:00:00Z",
        "started_at":null,"finished_at":null,"revision":"1"});
    let expected = resource.clone();
    let f = Fixture::new(|_| {
        let mut response =
            Reply::json(json!({"schema_version":1,"resource":resource,"replayed":false}));
        response.status = StatusCode::ACCEPTED;
        vec![response]
    })
    .await;
    let grant = Id::new().to_string();
    let command = args(&[
        "--idempotency-key",
        "native-validation",
        "--operator-grant",
        &grant,
        "data",
        "validate",
    ]);
    let result = f
        .execute(&command, &serde_json::to_vec(&body).unwrap())
        .await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let reply: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(reply["resource"], expected);
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].uri, "/api/v2/data/validate");
    assert_eq!(seen[0].key.as_deref(), Some("native-validation"));
    assert_eq!(seen[0].operator.as_deref(), Some(grant.as_str()));
    assert_eq!(
        serde_json::from_slice::<Value>(&seen[0].body).unwrap(),
        body
    );
}

#[tokio::test]
async fn native_cli_refuses_missing_human_grant_and_unknown_fields_before_the_network() {
    let f = Fixture::new(|_| vec![]).await;
    let valid = json!({"schema_version":1,"name":"Controlled source","runtime_id":Id::new(),"native_catalog_ref":"registered/catalog","provider_kind":"NAUTILUS_CATALOG","enabled":true});
    let mut invalid = valid.clone();
    invalid["origin"] = json!("REAL");
    for body in [valid, invalid] {
        let output = f
            .execute(
                &args(&["--idempotency-key", "bound", "data", "source", "create"]),
                &serde_json::to_vec(&body).unwrap(),
            )
            .await;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    let mismatched = json!({"schema_version":1,"source_id":Id::new(),"license_reference":"Controlled license","evidence_artifact_id":Id::new(),"allowed_uses":"RESEARCH","valid_from":"2026-01-01T00:00:00Z","valid_until":null});
    let output = f
        .execute(
            &args(&[
                "--idempotency-key",
                "bound",
                "data",
                "grant",
                "create",
                &Id::new().to_string(),
            ]),
            &serde_json::to_vec(&mismatched).unwrap(),
        )
        .await;
    assert!(!output.status.success());
    assert!(f.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn native_cli_returns_only_a_validated_problem_and_does_not_follow_redirects() {
    let expected = problem(409);
    let reply = expected.clone();
    let f = Fixture::new(|_| {
        let mut response = Reply::json(reply);
        response.status = StatusCode::CONFLICT;
        response.media = "application/problem+json";
        vec![response]
    })
    .await;
    let output = f
        .execute(
            &args(&["data", "source", "show", &Id::new().to_string()]),
            b"",
        )
        .await;
    assert!(!output.status.success() && output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap(),
        expected
    );
    assert_eq!(f.seen.lock().unwrap().len(), 1);
    let redirect = Fixture::new(|_| {
        vec![Reply {
            status: StatusCode::TEMPORARY_REDIRECT,
            media: "application/json",
            bytes: vec![],
            headers: vec![("location", format!("{}/api/v2/data/sources", f.origin))],
            chunk_bytes: None,
        }]
    })
    .await;
    let output = redirect
        .execute(&args(&["data", "source", "list"]), b"")
        .await;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(f.seen.lock().unwrap().len(), 1);
    assert_eq!(redirect.seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn native_cli_rejects_wrong_status_duplicate_json_and_escaped_reflection_without_echo() {
    for mode in 0..4 {
        let f = Fixture::new(|credential| {
            let mut response =
                Reply::json(json!({"schema_version":1,"items":[],"next_cursor":null}));
            match mode {
                0 => response.status = StatusCode::CREATED,
                1 => {
                    response.bytes =
                        br#"{"schema_version":1,"items":[],"items":[],"next_cursor":null}"#.to_vec()
                }
                2 => {
                    let escaped = credential
                        .chars()
                        .map(|character| format!("\\u{:04x}", character as u32))
                        .collect::<String>();
                    response.bytes = format!(
                        r#"{{"schema_version":1,"items":[],"next_cursor":null,"{escaped}":false}}"#
                    )
                    .into_bytes();
                }
                _ => {
                    response.bytes = serde_json::to_vec(&problem(409)).unwrap();
                    response.status = StatusCode::FORBIDDEN;
                    response.media = "application/problem+json";
                }
            }
            vec![response]
        })
        .await;
        let output = f.execute(&args(&["data", "source", "list"]), b"").await;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "CLI_RESPONSE_CONTRACT_INVALID");
        assert_eq!(f.seen.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn native_cli_artifact_export_uses_declared_media_and_exact_immutable_bytes() {
    let id = Id::new();
    let content = b"fn predict() {}\n".to_vec();
    let expected = content.clone();
    let f = Fixture::new(|_| vec![
        Reply::json(json!({"id":id,"project_id":Id::new(),"producer_run_id":null,"producer_attempt_id":null,
            "kind":"CODE","media_type":"text/x-rust","schema_name":"qz.rust_source","schema_version":"1","byte_count":content.len().to_string(),
            "access_class":"RESEARCH","origin":"SYNTHETIC","created_by":"OPERATOR","created_at":"2026-09-10T00:00:00Z"})),
        Reply {status:StatusCode::OK,media:"text/x-rust",bytes:content,headers:vec![],chunk_bytes:Some(3)},
    ]).await;
    let output = f
        .execute(&args(&["artifact", "export", &id.to_string()]), b"")
        .await;
    assert!(output.status.success());
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].uri, format!("/api/v2/artifacts/{id}"));
    assert_eq!(seen[1].uri, format!("/api/v2/artifacts/{id}/content"));
}

fn event(run: Id, seq: u64) -> Value {
    json!({"schema_version":1,"run_id":run,"seq":seq.to_string(),"attempt_id":null,"event_type":"future.public_observation","occurred_at":"2026-09-10T00:00:00Z","payload":{"schema_version":1,"observed":true}})
}
#[tokio::test]
async fn native_cli_uses_native_sse_framing_and_preserves_compatible_unknown_events_without_cancelling(
) {
    let run = Id::new();
    let first = event(run, 9007199254740993);
    let second = event(run, 9007199254740994);
    let frames = format!(": keepalive\r\n\r\nevent: future.public_observation\r\nid: {run}:9007199254740993\r\ndata: {first}\r\n\r\nevent: future.public_observation\nid: {run}:9007199254740994\ndata: {second}\n\n").into_bytes();
    let f = Fixture::new(|_| {
        vec![Reply {
            status: StatusCode::OK,
            media: "text/event-stream",
            bytes: frames,
            headers: vec![],
            chunk_bytes: Some(1),
        }]
    })
    .await;
    let after = format!("{run}:9007199254740992");
    let output = f
        .execute(
            &args(&[
                "run",
                "watch",
                &run.to_string(),
                "--after",
                &after,
                "--max-events",
                "2",
                "--max-seconds",
                "2",
            ]),
            b"",
        )
        .await;
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let values: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0]["event"], first);
    assert_eq!(values[1]["event"], second);
    assert_eq!(values[0]["event_id"], format!("{run}:9007199254740993"));
    assert_eq!(values[1]["event_id"], format!("{run}:9007199254740994"));
    assert_eq!(
        values[2]["last_event_id"],
        format!("{run}:9007199254740994")
    );
    assert_eq!(values[2]["cancellation_requested"], false);
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].after.as_deref(), Some(after.as_str()));
}

#[tokio::test]
async fn native_cli_sse_bad_cursor_or_event_identity_does_not_invent_a_resume_or_cancel() {
    let run = Id::new();
    let other = Id::new();
    let body = event(other, 1);
    let f = Fixture::new(|_| {
        vec![Reply {
            status: StatusCode::OK,
            media: "text/event-stream",
            bytes: format!("id: {other}:1\nevent: future.public_observation\ndata: {body}\n\n")
                .into_bytes(),
            headers: vec![],
            chunk_bytes: None,
        }]
    })
    .await;
    let output = f
        .execute(
            &args(&[
                "run",
                "watch",
                &run.to_string(),
                "--after",
                &format!("{other}:0"),
            ]),
            b"",
        )
        .await;
    assert!(!output.status.success());
    assert!(f.seen.lock().unwrap().is_empty());
    let output = f
        .execute(
            &args(&["run", "watch", &run.to_string(), "--max-seconds", "2"]),
            b"",
        )
        .await;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(f.seen.lock().unwrap().len(), 1);
}
