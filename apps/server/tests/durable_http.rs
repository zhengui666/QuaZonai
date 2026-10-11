//! Real owner authentication/HTTP proxy; the loopback durable responses are transport fixtures.
#[path = "support/client.rs"]
#[allow(dead_code)]
mod client_support;
mod support;
use axum::{
    body::{to_bytes, Body, Bytes},
    extract::State,
    http::{header, HeaderMap, Method, Request, StatusCode, Uri},
    response::{IntoResponse, Response},
    Router,
};
use serde_json::{json, Value};
use server::durable::ResearchService;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use support::*;
use tokio::{net::TcpListener, task::JoinHandle};
use tower::ServiceExt;

const START: &str = "{\n  \"requestId\": \"unchanged-request\", \"input\": {\"goal\":\"original goal\",\"model\":{\"provider\":\"test\",\"modelId\":\"native\",\"futureModelField\":null},\"data\":{\"asset\":\"A\",\"unknown\":[1,null]},\"policy\":{\"future\":{\"enabled\":true}}},\"future\":7\n}";
const IDS: &str =
    "{ \"conversationId\": 44, \"submissionId\": 71, \"upstreamField\": {\"new\":true} }";
const SNAPSHOT: &str = "{\"id\":44,\"view\":{\"messages\":[{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"native answer\"}]}]},\"tasks\":[{\"id\":9,\"status\":\"running\",\"parentId\":null}],\"futureSnapshotField\":[null,3]}";

struct Seen {
    method: Method,
    uri: String,
    headers: HeaderMap,
    bytes: Vec<u8>,
}
struct Upstream {
    origin: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    task: JoinHandle<()>,
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Upstream {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new().fallback(upstream).with_state(seen.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { origin, seen, task }
    }
    async fn fixture(&self, pool: PgPool) -> Fixture {
        fixture_with_research_service(pool, ResearchService::new(&self.origin).unwrap()).await
    }
}
async fn upstream(
    State(seen): State<Arc<Mutex<Vec<Seen>>>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    let duplicate = {
        let mut seen = seen.lock().unwrap();
        let duplicate = seen
            .iter()
            .any(|call| call.method == Method::POST && call.uri == "/research");
        seen.push(Seen {
            method: method.clone(),
            uri: uri.to_string(),
            headers,
            bytes: bytes.to_vec(),
        });
        duplicate
    };
    let (status, body) = match (method.as_str(), uri.path()) {
        ("POST", "/research") => (
            if duplicate {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            },
            IDS,
        ),
        ("GET", "/research") => (
            StatusCode::OK,
            "[{\"id\":44,\"nativeView\":{\"unknown\":true}}]",
        ),
        ("GET", "/research/44") => (StatusCode::OK, SNAPSHOT),
        ("GET", "/models") => (
            StatusCode::OK,
            "[{\"provider\":\"test\",\"modelId\":\"native\",\"cost\":null}]",
        ),
        ("GET", "/tasks") => (
            StatusCode::OK,
            "[{\"id\":9,\"parentId\":null,\"state\":\"running\"}]",
        ),
        ("GET", "/usage") => (
            StatusCode::OK,
            "{\"usage\":{\"tokens\":17,\"cost\":null},\"source\":\"native\"}",
        ),
        ("POST", "/research/44/stop") => (
            StatusCode::CONFLICT,
            "{\"error\":\"cannot stop\",\"nativeState\":\"terminal\"}",
        ),
        ("POST", "/research/45/stop") => (
            StatusCode::ACCEPTED,
            "{\"state\":\"stopping\",\"task\":{\"id\":9,\"state\":\"running\"}}",
        ),
        ("POST", "/research/46/stop") => (
            StatusCode::BAD_GATEWAY,
            "{\"error\":\"remote stop failed\",\"cancelled\":false}",
        ),
        ("GET", "/research/307") => (StatusCode::TEMPORARY_REDIRECT, "{\"redirect\":true}"),
        _ => (
            StatusCode::NOT_FOUND,
            "{\"error\":\"missing native conversation\",\"extra\":null}",
        ),
    };
    let mut response = (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        body,
    )
        .into_response();
    if status == StatusCode::TEMPORARY_REDIRECT {
        response
            .headers_mut()
            .insert(header::LOCATION, "/research/44".parse().unwrap());
    }
    response
}
fn raw_request(method: &str, path: &str, token: Option<&str>, bytes: &[u8]) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "localhost");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if token.is_none() {
        request = request.header(header::ORIGIN, "https://localhost");
    }
    if !bytes.is_empty() {
        request = request.header(header::CONTENT_TYPE, "application/json");
    }
    request.body(Body::from(bytes.to_vec())).unwrap()
}
async fn raw_reply(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_browser_and_device_required_before_any_upstream_call(pool: PgPool) {
    let upstream = Upstream::start().await;
    let f = upstream.fixture(pool).await;
    for (method, path, bytes) in [
        ("GET", "/api/v2/research", ""),
        ("POST", "/api/v2/research", START),
        ("GET", "/api/v2/research/44", ""),
        ("POST", "/api/v2/research/44/stop", ""),
        ("GET", "/api/v2/research/models", ""),
        ("GET", "/api/v2/research/tasks", ""),
        ("GET", "/api/v2/research/usage", ""),
    ] {
        let anonymous = raw_reply(&f.app, raw_request(method, path, None, bytes.as_bytes())).await;
        assert_eq!(anonymous.0, StatusCode::UNAUTHORIZED);
    }
    assert!(upstream.seen.lock().unwrap().is_empty());
    let cookie = local_session(&f).await.cookie.unwrap();
    let browser = call(&f, "GET", "/api/v2/research/44", Value::Null, Some(&cookie)).await;
    assert_eq!(browser.status, StatusCode::OK);
    assert_eq!(
        browser.body,
        serde_json::from_str::<Value>(SNAPSHOT).unwrap()
    );
    let mut invalid = raw_request("GET", "/api/v2/research", Some("invalid"), b"");
    invalid
        .headers_mut()
        .insert(header::COOKIE, cookie.parse().unwrap());
    assert_eq!(raw_reply(&f.app, invalid).await.0, StatusCode::UNAUTHORIZED);
    // A real scoped machine verifier is authenticated, but is not owner authority.
    let principal = owner_command(&f, "POST", "/api/v2/machine-principals", "durable-doctor", json!({
        "schema_version":1,"name":"Scoped CLI","kind":"CLI","project_id":null,"downstream_id":null,"enabled":true
    })).await;
    assert_eq!(principal.status, StatusCode::CREATED, "{}", principal.body);
    let credentials = owner_command(&f, "POST", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), "durable-doctor-token", json!({
        "schema_version":1,"scope_codes":["DOCTOR_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)
    })).await;
    assert_eq!(
        credentials.status,
        StatusCode::CREATED,
        "{}",
        credentials.body
    );
    assert_eq!(
        raw_reply(
            &f.app,
            raw_request(
                "GET",
                "/api/v2/research",
                credentials.body["token"].as_str(),
                b""
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(upstream.seen.lock().unwrap().len(), 1);
    let token = owner_session(&f).await;
    let authorized = raw_reply(
        &f.app,
        raw_request("GET", "/api/v2/research", Some(token), b""),
    )
    .await;
    assert_eq!(authorized.0, StatusCode::OK);
    // Device revocation is checked by the existing extractor on every request.
    let identity =
        owner_command(&f, "GET", "/api/v2/auth/cli/session", "unused", Value::Null).await;
    let revoked = call(
        &f,
        "DELETE",
        &format!(
            "/api/v2/auth/cli/devices/{}",
            identity.body["id"].as_str().unwrap()
        ),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    assert_eq!(
        raw_reply(
            &f.app,
            raw_request("GET", "/api/v2/research", Some(token), b"")
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(upstream.seen.lock().unwrap().len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_duplicates_and_native_snapshots_are_relayed_without_local_state(pool: PgPool) {
    let upstream = Upstream::start().await;
    let f = upstream.fixture(pool.clone()).await;
    local_session(&f).await;
    let token = owner_session(&f).await;
    for status in [StatusCode::CREATED, StatusCode::OK] {
        let reply = raw_reply(
            &f.app,
            raw_request("POST", "/api/v2/research", Some(token), START.as_bytes()),
        )
        .await;
        assert_eq!(reply.0, status);
        assert_eq!(reply.2, IDS.as_bytes());
        assert_eq!(
            reply.1[header::CONTENT_TYPE],
            "application/json; charset=utf-8"
        );
    }
    let snapshot = raw_reply(
        &f.app,
        raw_request("GET", "/api/v2/research/44", Some(token), b""),
    )
    .await;
    assert_eq!(snapshot.2, SNAPSHOT.as_bytes());
    for suffix in ["models", "tasks", "usage"] {
        assert_eq!(
            raw_reply(
                &f.app,
                raw_request(
                    "GET",
                    &format!("/api/v2/research/{suffix}"),
                    Some(token),
                    b""
                )
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let query = raw_reply(
        &f.app,
        raw_request(
            "GET",
            "/api/v2/research/tasks?cursor=a%2Fb&state=running",
            Some(token),
            b"",
        ),
    )
    .await;
    assert_eq!(query.0, StatusCode::OK);
    let seen = upstream.seen.lock().unwrap();
    assert_eq!(seen.len(), 7);
    assert_eq!(seen[0].bytes, START.as_bytes());
    assert_eq!(seen[1].bytes, START.as_bytes());
    assert_eq!(seen[6].uri, "/tasks?cursor=a%2Fb&state=running");
    for call in seen.iter() {
        for name in [
            "authorization",
            "cookie",
            "x-operator-grant",
            "idempotency-key",
        ] {
            assert!(
                !call.headers.contains_key(name),
                "owner authority leaked upstream: {name}"
            );
        }
    }
    drop(seen);
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.run_missions)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (0, 0),
        "proxy must not duplicate durable execution state"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_failure_cancel_and_redirect_status_remain_honest(pool: PgPool) {
    let upstream = Upstream::start().await;
    let f = upstream.fixture(pool).await;
    local_session(&f).await;
    let token = owner_session(&f).await;
    for (method, path, status, expected) in [
        (
            "GET",
            "/api/v2/research/404",
            StatusCode::NOT_FOUND,
            json!({"error":"missing native conversation","extra":null}),
        ),
        (
            "POST",
            "/api/v2/research/44/stop",
            StatusCode::CONFLICT,
            json!({"error":"cannot stop","nativeState":"terminal"}),
        ),
        (
            "POST",
            "/api/v2/research/45/stop",
            StatusCode::ACCEPTED,
            json!({"state":"stopping","task":{"id":9,"state":"running"}}),
        ),
        (
            "POST",
            "/api/v2/research/46/stop",
            StatusCode::BAD_GATEWAY,
            json!({"error":"remote stop failed","cancelled":false}),
        ),
        (
            "GET",
            "/api/v2/research/307",
            StatusCode::TEMPORARY_REDIRECT,
            json!({"redirect":true}),
        ),
    ] {
        let reply = raw_reply(&f.app, raw_request(method, path, Some(token), b"")).await;
        assert_eq!(reply.0, status);
        assert_eq!(serde_json::from_slice::<Value>(&reply.2).unwrap(), expected);
    }
    assert_eq!(
        upstream.seen.lock().unwrap().len(),
        5,
        "no retries or redirect-following"
    );
    let invalid = raw_reply(
        &f.app,
        raw_request("GET", "/api/v2/research/not-numeric", Some(token), b""),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unavailable_transport_is_opaque_and_does_not_claim_remote_cancel(pool: PgPool) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let f = fixture_with_research_service(pool, ResearchService::new(&origin).unwrap()).await;
    local_session(&f).await;
    let token = owner_session(&f).await;
    let reply = raw_reply(
        &f.app,
        raw_request("POST", "/api/v2/research/44/stop", Some(token), b""),
    )
    .await;
    assert_eq!(reply.0, StatusCode::SERVICE_UNAVAILABLE);
    let problem: Value = serde_json::from_slice(&reply.2).unwrap();
    assert_eq!(problem["code"], "RESEARCH_UNAVAILABLE");
    let text = String::from_utf8(reply.2).unwrap();
    assert!(!text.contains(&origin) && !text.contains(token));
    assert!(problem.get("cancelled").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn saved_owner_cli_uses_the_same_proxy_and_retains_file_request_id(pool: PgPool) {
    let upstream = Upstream::start().await;
    let f = upstream.fixture(pool).await;
    local_session(&f).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let state = server::AppState::new(
        f.store.clone(),
        integrations::secrets::SecretVault::open(
            &f._state.path().join("secrets"),
            &f._state.path().join("master.key"),
        )
        .unwrap(),
        server::WebPolicy::new(&origin, address, true).unwrap(),
    )
    .with_research_service(ResearchService::new(&upstream.origin).unwrap());
    let app = server::router(state, tower_sessions::cookie::Key::generate());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let profile = client_support::owner_profile(&f, &origin).await;
    let file = f._state.path().join("research.json");
    std::fs::write(&file, START).unwrap();
    for _ in 0..2 {
        let output = client_support::invoke_saved(
            &profile,
            &["research", "start", "--file", file.to_str().unwrap()],
            Value::Null,
        )
        .await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            serde_json::from_str::<Value>(IDS).unwrap()
        );
    }
    for command in [
        vec!["research", "list"],
        vec!["research", "get", "44"],
        vec!["research", "models"],
        vec!["research", "tasks"],
        vec!["research", "usage"],
        vec!["research", "stop", "45"],
    ] {
        let output = client_support::invoke_saved(&profile, &command, Value::Null).await;
        assert!(
            output.status.success(),
            "{command:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        if command == ["research", "get", "44"] {
            assert_eq!(value, serde_json::from_str::<Value>(SNAPSHOT).unwrap());
        }
        if command == ["research", "stop", "45"] {
            assert_eq!(value["state"], "stopping");
        }
    }
    let failed =
        client_support::invoke_saved(&profile, &["research", "stop", "46"], Value::Null).await;
    assert!(!failed.status.success());
    let failure: Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(failure["status"], 502);
    assert_eq!(
        failure["payload"],
        json!({"error":"remote stop failed","cancelled":false})
    );
    assert_eq!(failure["request_sent_again"], false);
    let seen = upstream.seen.lock().unwrap();
    assert_eq!(seen.len(), 9);
    assert_eq!(seen[0].bytes, START.as_bytes());
    assert_eq!(seen[1].bytes, START.as_bytes());
    task.abort();
}

#[test]
fn durable_openapi_is_native_json_owner_only_and_old_execution_routes_are_absent() {
    let document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    for (path, method) in [
        ("/api/v2/research", "get"),
        ("/api/v2/research", "post"),
        ("/api/v2/research/{id}", "get"),
        ("/api/v2/research/{id}/stop", "post"),
        ("/api/v2/research/models", "get"),
        ("/api/v2/research/tasks", "get"),
        ("/api/v2/research/usage", "get"),
    ] {
        let operation = &document["paths"][path][method];
        assert!(operation.is_object(), "{path}");
        assert_eq!(
            operation["security"],
            json!([{"BrowserSession":[]},{"OwnerDeviceBearer":[]}])
        );
    }
    assert!(
        document["paths"]["/api/v2/research/{id}/stop"]["post"]["responses"]["409"]["content"]
            ["application/json"]
            .is_object()
    );
    assert!(document["paths"]["/api/v2/projects/{id}/cycles"]["post"].is_null());
    assert!(document["paths"]
        .as_object()
        .unwrap()
        .keys()
        .all(|path| !path.contains("codex")));
}
