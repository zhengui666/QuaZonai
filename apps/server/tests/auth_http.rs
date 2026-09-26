//! Real PostgreSQL sessions, password admission and revocable owner CLI devices.
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use server::WebPolicy;
use sqlx::PgPool;
use support::*;

fn password(value: &str, remember: bool) -> Value {
    json!({"schema_version":1,"password":value,"remember_device":remember})
}

#[sqlx::test(migrations = "../../migrations")]
async fn anonymous_setup_password_login_and_fixed_cookie_deadlines(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let anonymous = call(&f, "GET", "/api/v2/projects", Value::Null, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert!(anonymous.cookie.is_none());
    assert_eq!(
        call(&f, "GET", "/api/v2/auth/status", Value::Null, None)
            .await
            .body["setup_required"],
        true
    );
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/auth/setup",
            password("short", false),
            None
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let first = local_session(&f).await;
    let attributes = first.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(
        attributes.contains("HttpOnly")
            && attributes.contains("SameSite=Strict")
            && attributes.contains("Secure")
    );
    assert!(!attributes.contains("Max-Age") && !attributes.contains("Expires"));
    let first: contracts::auth::BrowserSession = serde_json::from_value(first.body).unwrap();
    assert_eq!(
        first.expires_at - first.authenticated_at,
        chrono::Duration::hours(12)
    );
    assert_eq!(
        call(&f, "GET", "/api/v2/auth/status", Value::Null, None)
            .await
            .body["setup_required"],
        false
    );
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/auth/setup",
            password("replacement-password", false),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/auth/login",
            password("wrong-password", false),
            None
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    let login = call(
        &f,
        "POST",
        "/api/v2/auth/login",
        password("native-test-password", true),
        None,
    )
    .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.body);
    let attributes = login.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(attributes.contains("Max-Age="));
    let session: contracts::auth::BrowserSession =
        serde_json::from_value(login.body.clone()).unwrap();
    assert_eq!(
        session.expires_at - session.authenticated_at,
        chrono::Duration::days(30)
    );
    let cookie = login.cookie.unwrap();
    let repeat = call(
        &f,
        "GET",
        "/api/v2/auth/session",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(
        repeat.body, login.body,
        "session reads must not renew a fixed deadline"
    );
    let hash: String = sqlx::query_scalar("SELECT password_verifier FROM app.operator_auth_state")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert!(!hash.contains("native-test-password"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_and_password_change_never_silently_restore_sessions(pool: PgPool) {
    let f = fixture(pool).await;
    let first = local_session(&f).await.cookie.unwrap();
    let second = local_session(&f).await.cookie.unwrap();
    assert_eq!(
        call(&f, "POST", "/api/v2/auth/logout", Value::Null, Some(&first))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    for route in ["/api/v2/auth/session", "/api/v2/projects"] {
        assert_eq!(
            call(&f, "GET", route, Value::Null, Some(&first))
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }
    let wrong = json!({"schema_version":1,"current_password":"wrong-password","new_password":"new-test-password"});
    assert_eq!(
        call(&f, "POST", "/api/v2/auth/password", wrong, Some(&second))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &f,
            "GET",
            "/api/v2/auth/session",
            Value::Null,
            Some(&second)
        )
        .await
        .status,
        StatusCode::OK
    );
    let body = json!({"schema_version":1,"current_password":"native-test-password","new_password":"new-test-password"});
    assert_eq!(
        call(&f, "POST", "/api/v2/auth/password", body, Some(&second))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(
            &f,
            "GET",
            "/api/v2/auth/session",
            Value::Null,
            Some(&second)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/auth/login",
            password("native-test-password", false),
            None
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/auth/login",
            password("new-test-password", false),
            None
        )
        .await
        .status,
        StatusCode::OK
    );
}

async fn bearer(f: &Fixture, token: &str, method: &str, path: &str, body: Value) -> Reply {
    exchange(
        &f.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotency-key", "device-owner-command")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_cli_needs_one_password_login_and_remains_until_revoked(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let cookie = local_session(&f).await.cookie.unwrap();
    let registered = call(
        &f,
        "POST",
        "/api/v2/auth/cli/login",
        json!({"schema_version":1,"password":"native-test-password","name":"Test laptop"}),
        None,
    )
    .await;
    assert_eq!(
        registered.status,
        StatusCode::CREATED,
        "{}",
        registered.body
    );
    let token = registered.body["token"].as_str().unwrap();
    let id = registered.body["device"]["id"].as_str().unwrap();
    assert!(registered.cookie.is_none());
    assert_eq!(
        bearer(&f, token, "GET", "/api/v2/auth/cli/session", Value::Null)
            .await
            .status,
        StatusCode::OK
    );
    let created=bearer(&f,token,"POST","/api/v2/projects",json!({"schema_version":1,"name":"Device project","description":"","fork_from_project_id":null})).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let change = json!({"schema_version":1,"current_password":"native-test-password","new_password":"new-test-password"});
    assert_eq!(
        call(&f, "POST", "/api/v2/auth/password", change, Some(&cookie))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        bearer(&f, token, "GET", "/api/v2/projects", Value::Null)
            .await
            .status,
        StatusCode::OK
    );
    let cookie = call(
        &f,
        "POST",
        "/api/v2/auth/login",
        password("new-test-password", false),
        None,
    )
    .await
    .cookie
    .unwrap();
    let devices = call(
        &f,
        "GET",
        "/api/v2/auth/cli/devices",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(devices.body.as_array().unwrap().len(), 1);
    assert!(!devices.body.to_string().contains(token));
    assert_eq!(
        call(
            &f,
            "DELETE",
            &format!("/api/v2/auth/cli/devices/{id}"),
            Value::Null,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        bearer(&f, token, "GET", "/api/v2/projects", Value::Null)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &f,
            "GET",
            "/api/v2/auth/cli/devices",
            Value::Null,
            Some(&cookie)
        )
        .await
        .body,
        json!([])
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT revoked_at IS NOT NULL FROM app.cli_devices")
            .fetch_one(&pool)
            .await
            .unwrap()
    );
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

#[test]
fn deployment_policy_requires_loopback_for_http_and_https() {
    let public = "0.0.0.0:8080".parse().unwrap();
    let local = "127.0.0.1:8080".parse().unwrap();
    for (url, bind, dev) in [
        ("https://localhost", public, false),
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
    assert!(WebPolicy::new("https://research.example", local, false).is_ok());
    assert!(WebPolicy::new("http://research.example", local, true).is_err());
    assert!(WebPolicy::new("https://[::1]", "[::1]:8080".parse().unwrap(), false).is_ok());
}
