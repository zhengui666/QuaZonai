//! Actual local Axum/TCP requests, PostgreSQL sessions and browser boundaries.
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use contracts::Id;
use integrations::secrets::SecretVault;
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::PgPool;
use store::Store;
use support::*;
use tower_sessions::cookie::Key;

#[sqlx::test(migrations = "../../migrations")]
async fn first_browser_request_enters_without_enrollment_or_login(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let entered = call(&f, "GET", "/api/v2/projects", Value::Null, None).await;
    assert_eq!(entered.status, StatusCode::OK, "{}", entered.body);
    assert!(entered.cookie.is_some());
    assert_eq!(entered.body["items"], json!([]));
    let snapshot = f.store.authentication_snapshot().await.unwrap();
    assert!(snapshot.initialized);
    let absent: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM app.auth_enrollments) AND NOT EXISTS(SELECT 1 FROM app.bootstrap_capabilities) AND NOT EXISTS(SELECT 1 FROM app.trusted_devices) AND (SELECT totp_secret_ref IS NULL FROM app.operator_auth_state WHERE singleton)")
        .fetch_one(&pool).await.unwrap();
    assert!(absent);
    let cookie = entered.cookie.unwrap();
    let session = call(
        &f,
        "GET",
        "/api/v2/auth/session",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(session.status, StatusCode::OK);
    assert_eq!(session.body.as_object().unwrap().len(), 3);
    let created = exchange(&f.app, Request::builder()
        .method("POST").uri("/api/v2/projects")
        .header(header::HOST, "localhost").header(header::ORIGIN, "https://localhost")
        .header(header::CONTENT_TYPE, "application/json").header("idempotency-key", "direct-local")
        .body(Body::from(json!({"schema_version":1,"name":"Local","description":"","fork_from_project_id":null}).to_string())).unwrap()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert!(
        created.cookie.is_some(),
        "first write also establishes a local session"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn encrypted_local_session_is_reused_and_revocation_creates_a_new_identity(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let first = local_session(&f).await;
    let cookie = first.cookie.unwrap();
    let original: String = sqlx::query_scalar("SELECT id::text FROM app.browser_logins LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let repeated = call(
        &f,
        "GET",
        "/api/v2/auth/session",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(
        repeated.body, first.body,
        "reading does not extend the fixed session deadline"
    );
    f.store
        .logout_browser(original.clone().try_into().unwrap())
        .await
        .unwrap();
    let renewed = call(
        &f,
        "GET",
        "/api/v2/auth/session",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(renewed.status, StatusCode::OK);
    assert_ne!(renewed.cookie.as_deref(), Some(cookie.as_str()));
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NOT NULL FROM app.browser_logins WHERE id=$1::uuid"
    )
    .bind(&original)
    .fetch_one(&pool)
    .await
    .unwrap());
    let (total, active): (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER(WHERE revoked_at IS NULL) FROM app.browser_logins",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((total, active), (2, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn local_access_still_rejects_cross_origin_host_and_bearer_substitution(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    for (method, origin, host, expected) in [
        (
            "GET",
            Some("https://evil.example"),
            "localhost",
            StatusCode::FORBIDDEN,
        ),
        ("POST", None, "localhost", StatusCode::FORBIDDEN),
        ("POST", Some("null"), "localhost", StatusCode::FORBIDDEN),
        ("GET", None, "evil.example", StatusCode::BAD_REQUEST),
    ] {
        let response = request(
            &f.app,
            method,
            "/api/v2/projects",
            Value::Null,
            None,
            origin,
            host,
        )
        .await;
        assert_eq!(response.status, expected, "{}", response.body);
        assert!(response.cookie.is_none());
    }
    let response = exchange(
        &f.app,
        Request::builder()
            .method("GET")
            .uri("/api/v2/projects")
            .header(header::HOST, "localhost")
            .header(header::AUTHORIZATION, "Bearer invalid")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        response.status,
        StatusCode::UNAUTHORIZED,
        "{}",
        response.body
    );
    assert!(response.cookie.is_none());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.browser_logins")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn removed_challenge_and_custom_profile_routes_are_not_exposed(pool: PgPool) {
    let f = fixture(pool).await;
    for path in [
        "/api/v2/auth/login",
        "/api/v2/auth/verify",
        "/api/v2/auth/logout",
        "/api/v2/bootstrap/start",
        "/api/v2/bootstrap/confirm",
    ] {
        let response = call(
            &f,
            "POST",
            path,
            json!({"schema_version":1,"code":"123456"}),
            None,
        )
        .await;
        assert!(
            matches!(
                response.status,
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{path}: {}",
            response.body
        );
    }
    let api: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    for path in [
        "/api/v2/auth/login",
        "/api/v2/auth/verify",
        "/api/v2/auth/devices",
        "/api/v2/bootstrap/status",
        "/api/v2/codex/homes",
    ] {
        assert!(api["paths"].get(path).is_none(), "{path}");
    }
    assert!(api["paths"]["/api/v2/settings/codex"].get("post").is_none());
    assert!(api["paths"]["/api/v2/auth/session"]["get"].is_object());
}

#[test]
fn deployment_policy_requires_loopback_for_http_and_https() {
    let public = "0.0.0.0:8080".parse().unwrap();
    let local = "127.0.0.1:8080".parse().unwrap();
    for (url, bind, dev) in [
        ("https://localhost", public, false),
        ("https://research.example", local, false),
        ("http://localhost:8080", public, true),
        ("http://localhost:8080", local, false),
        ("https://u:p@localhost", local, false),
        ("https://localhost/subpath", local, false),
        ("https://localhost?secret=x", local, false),
    ] {
        assert!(WebPolicy::new(url, bind, dev).is_err(), "{url}");
    }
    assert!(WebPolicy::new("http://127.0.0.1:8080", local, true).is_ok());
    assert!(WebPolicy::new("https://localhost", local, false).is_ok());
    assert!(WebPolicy::new("https://[::1]", "[::1]:8080".parse().unwrap(), false).is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_tcp_listener_uses_non_owner_database_role_and_native_private_cookie(pool: PgPool) {
    let fixture = fixture(pool.clone()).await;
    assert!(fixture.store.verify_runtime_role().await.is_err());
    let role = format!("api_test_{}", Id::new().to_string().replace('-', ""));
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN PASSWORD 'disposable-test-only' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT"))).execute(&pool).await.unwrap();
    for sql in [
        format!("GRANT USAGE ON SCHEMA app,tower_sessions TO {role}"),
        format!("GRANT SELECT,INSERT,UPDATE ON ALL TABLES IN SCHEMA app TO {role}"),
        format!(
            "GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA tower_sessions TO {role}"
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&pool)
            .await
            .unwrap();
    }
    let options = pool
        .connect_options()
        .as_ref()
        .clone()
        .username(&role)
        .password("disposable-test-only");
    let app_pool = PgPool::connect_with(options).await.unwrap();
    let store = Store::from_pool(app_pool.clone());
    store.verify_runtime_role().await.unwrap();
    assert!(
        sqlx::query("ALTER TABLE app.projects ADD COLUMN unsafe_test text")
            .execute(&app_pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("TRUNCATE app.operator_auth_state")
        .execute(&app_pool)
        .await
        .is_err());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let policy = WebPolicy::new(&origin, address, true).unwrap();
    let vault = SecretVault::open(
        &fixture._state.path().join("secrets"),
        &fixture._state.path().join("master.key"),
    )
    .unwrap();
    let router = server::router(AppState::new(store.clone(), vault, policy), Key::generate());
    let serving = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let entered = client
        .get(format!("{origin}/api/v2/projects"))
        .send()
        .await
        .unwrap();
    assert_eq!(entered.status(), StatusCode::OK);
    let attributes = entered.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(attributes.contains("HttpOnly") && attributes.contains("SameSite=Strict"));
    let cookie = attributes.split(';').next().unwrap().to_string();
    let valid = client
        .get(format!("{origin}/api/v2/auth/session"))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(valid.status(), StatusCode::OK);
    let id: String = sqlx::query_scalar("SELECT id::text FROM app.browser_logins LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    store.logout_browser(id.try_into().unwrap()).await.unwrap();
    let renewed = client
        .get(format!("{origin}/api/v2/auth/session"))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(renewed.status(), StatusCode::OK);
    assert_ne!(
        renewed.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap(),
        cookie
    );
    for path in [
        "auth/login",
        "auth/verify",
        "auth/logout",
        "bootstrap/start",
        "bootstrap/confirm",
    ] {
        let removed = client
            .post(format!("{origin}/api/v2/{path}"))
            .header(header::ORIGIN, &origin)
            .json(&json!({"schema_version":1,"code":"123456"}))
            .send()
            .await
            .unwrap();
        assert!(matches!(
            removed.status(),
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ));
    }
    serving.abort();
    let _ = serving.await;
    drop(store);
    app_pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {role}")))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
        .execute(&pool)
        .await
        .unwrap();
}
