//! Real local Axum entry, opaque encrypted cookies and PostgreSQL sessions.
//! No fake authority extractor, memory session store, or skip-auth switch.
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    Router,
};
use integrations::secrets::SecretVault;
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};
use store::Store;
use tower::ServiceExt;
use tower_sessions::cookie::Key;
use tower_sessions_sqlx_store::PostgresStore;

pub struct Fixture {
    pub app: Router,
    // Each integration target compiles this shared fixture independently.
    #[allow(dead_code)]
    pub store: Store,
    pub _state: tempfile::TempDir,
}
pub async fn fixture(pool: PgPool) -> Fixture {
    fixture_with_runtime_targets(pool, None).await
}
pub async fn fixture_with_runtime_targets(
    pool: PgPool,
    targets: Option<server::runtime_transport::RuntimeTargets>,
) -> Fixture {
    fixture_with_deployment(pool, targets, None).await
}
pub async fn fixture_with_deployment(
    pool: PgPool,
    targets: Option<server::runtime_transport::RuntimeTargets>,
    exports: Option<server::migrations::HistoricalExports>,
) -> Fixture {
    fixture_with_key(pool, targets, exports, Key::generate()).await
}
pub async fn fixture_with_key(
    pool: PgPool,
    targets: Option<server::runtime_transport::RuntimeTargets>,
    exports: Option<server::migrations::HistoricalExports>,
    cookie_key: Key,
) -> Fixture {
    PostgresStore::new(pool.clone()).migrate().await.unwrap();
    let root = tempfile::tempdir().unwrap();
    let secrets = root.path().join("secrets");
    fs::create_dir(&secrets).unwrap();
    fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700)).unwrap();
    let key = root.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let store = Store::from_pool(pool);
    let policy = WebPolicy::new(
        "https://localhost",
        "127.0.0.1:8080".parse().unwrap(),
        false,
    )
    .unwrap();
    let mut state = AppState::new(
        store.clone(),
        SecretVault::open(&secrets, &key).unwrap(),
        policy,
    );
    if let Some(targets) = targets {
        state = state.with_runtime_targets(targets).with_artifact_store(
            integrations::artifacts::ArtifactStore::open(&root.path().join("artifacts")).unwrap(),
        );
    }
    if let Some(exports) = exports {
        state = state
            .with_historical_exports(exports)
            .with_historical_artifact_store(
                integrations::artifacts::ArtifactStore::open(
                    &root.path().join("historical-artifacts"),
                )
                .unwrap(),
            );
    }
    let app = server::router(state, cookie_key);
    Fixture {
        app,
        store,
        _state: root,
    }
}
pub struct Reply {
    pub status: StatusCode,
    pub body: Value,
    pub cookie: Option<String>,
    pub headers: axum::http::HeaderMap,
}
pub async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    cookie: Option<&str>,
    origin: Option<&str>,
    host: &str,
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, host);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    };
    exchange(app, builder.body(body).unwrap()).await
}
pub async fn exchange(app: &Router, request: Request<Body>) -> Reply {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let cookie = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("__Host-quazonai=") && !value.contains("Max-Age=0"))
        .map(|value| value.split(';').next().unwrap().to_string());
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"unparsed":String::from_utf8_lossy(&bytes)}))
    };
    Reply {
        status,
        body,
        cookie,
        headers,
    }
}
pub async fn call(
    f: &Fixture,
    method: &str,
    path: &str,
    body: Value,
    cookie: Option<&str>,
) -> Reply {
    request(
        &f.app,
        method,
        path,
        body,
        cookie,
        Some("https://localhost"),
        "localhost",
    )
    .await
}
pub async fn local_session(f: &Fixture) -> Reply {
    let response = call(f, "GET", "/api/v2/auth/session", Value::Null, None).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.headers[header::CACHE_CONTROL], "no-store");
    let attributes = response.headers[header::SET_COOKIE].to_str().unwrap();
    for expected in ["HttpOnly", "SameSite=Strict", "Secure", "Path=/"] {
        assert!(attributes.contains(expected), "{attributes}");
    }
    assert!(!attributes.contains("Domain="));
    assert!(response.cookie.is_some());
    assert_eq!(response.body["schema_version"], 1);
    assert!(response.body.get("provisioning_uri").is_none());
    response
}

// Only machine-boundary targets need this shared request helper.
#[allow(dead_code)]
pub async fn invalid_bearer(f: &Fixture, method: &str, path: &str, body: Value) -> Reply {
    exchange(
        &f.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::AUTHORIZATION, "Bearer invalid")
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotency-key", "invalid-machine")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}
