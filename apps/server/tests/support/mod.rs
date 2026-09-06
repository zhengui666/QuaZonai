//! Real Axum middleware, native Argon2/TOTP/AEAD and PostgreSQL session storage.
//! No fake authentication extractor, memory session store, or skip-auth switch.
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    Router,
};
use integrations::{
    authentication::{capability_verifier, random_capability},
    secrets::SecretVault,
};
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};
use store::Store;
use totp_rs::TOTP;
use tower::ServiceExt;
use tower_sessions::cookie::Key;
use tower_sessions_sqlx_store::PostgresStore;

pub struct Fixture {
    pub app: Router,
    pub store: Store,
    pub _state: tempfile::TempDir,
}
pub async fn fixture(pool: PgPool) -> Fixture {
    PostgresStore::new(pool.clone()).migrate().await.unwrap();
    let root = tempfile::tempdir().unwrap();
    let secrets = root.path().join("secrets");
    fs::create_dir(&secrets).unwrap();
    fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700)).unwrap();
    let key = root.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let store = Store::from_pool(pool);
    let policy = WebPolicy::new(
        "https://research.example",
        "127.0.0.1:8080".parse().unwrap(),
        false,
    )
    .unwrap();
    let app = server::router(
        AppState::new(
            store.clone(),
            SecretVault::open(&secrets, &key).unwrap(),
            policy,
        ),
        Key::generate(),
    );
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
        Some("https://research.example"),
        "research.example",
    )
    .await
}
pub async fn start(f: &Fixture) -> (Value, String, TOTP) {
    let capability = random_capability();
    let verifier = capability_verifier(&capability).unwrap();
    let issued = f.store.issue_bootstrap_capability(&verifier).await.unwrap();
    let response = call(
        f,
        "POST",
        "/api/v2/bootstrap/start",
        json!({"schema_version":1,"capability_id":issued.id,"capability":capability}),
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    assert_eq!(response.headers[header::CACHE_CONTROL], "no-store");
    let cookie_attributes = response.headers[header::SET_COOKIE].to_str().unwrap();
    for expected in ["HttpOnly", "SameSite=Strict", "Secure", "Path=/"] {
        assert!(cookie_attributes.contains(expected));
    }
    assert!(!cookie_attributes.contains("Domain="));
    let native = TOTP::from_url(response.body["provisioning_uri"].as_str().unwrap()).unwrap();
    (response.body, response.cookie.unwrap(), native)
}
pub async fn confirm(
    f: &Fixture,
    enrollment: &Value,
    cookie: &str,
    native: &TOTP,
    trust: bool,
) -> (Reply, String) {
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let code = native.generate(now);
    let response=call(f,"POST","/api/v2/bootstrap/confirm",json!({"schema_version":1,"enrollment_id":enrollment["enrollment_id"],"code":code,"trust_device":trust,"device_label":if trust{json!("Browser fixture")}else{Value::Null}}),Some(cookie)).await;
    (response, code)
}
