//! Real HTTP/session/crypto/database authentication regression tests.
mod support;
use axum::http::{header, StatusCode};
use contracts::Id;
use integrations::{
    authentication::{capability_verifier, random_capability},
    secrets::SecretVault,
};
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::PgPool;
use store::Store;
use support::*;
use totp_rs::TOTP;
use tower_sessions::cookie::Key;

#[sqlx::test(migrations = "../../migrations")]
async fn bootstrap_login_logout_and_replay_are_real_http_database_operations(pool: PgPool) {
    let f = fixture(pool).await;
    let status = call(&f, "GET", "/api/v2/bootstrap/status", Value::Null, None).await;
    assert_eq!(status.body["initialized"], false);
    assert_eq!(
        call(&f, "GET", "/api/v2/auth/session", Value::Null, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let (enrollment, anonymous, native) = start(&f).await;
    let (confirmed, code) = confirm(&f, &enrollment, &anonymous, &native, true).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.body);
    let cookie = confirmed.cookie.unwrap();
    assert_ne!(cookie, anonymous);
    assert_eq!(
        call(
            &f,
            "GET",
            "/api/v2/auth/session",
            Value::Null,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &f,
            "GET",
            "/api/v2/auth/session",
            Value::Null,
            Some(&anonymous)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    let devices = call(
        &f,
        "GET",
        "/api/v2/auth/devices",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(devices.status, StatusCode::OK);
    assert_eq!(devices.body["items"].as_array().unwrap().len(), 1);
    let serialized = devices.body.to_string();
    assert!(!serialized.contains("verifier"));
    assert!(!serialized.contains("secret"));
    let replay = call(
        &f,
        "POST",
        "/api/v2/auth/login",
        json!({"schema_version":1,"code":code,"trust_device":false,"device_label":null}),
        None,
    )
    .await;
    assert_eq!(replay.status, StatusCode::CONFLICT);
    assert_eq!(replay.body["code"], "TOTP_REPLAY");
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/auth/logout",
            Value::Null,
            Some(&cookie)
        )
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
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let new_code = native.generate((now / 30 + 1) * 30);
    let login = call(
        &f,
        "POST",
        "/api/v2/auth/login",
        json!({"schema_version":1,"code":new_code,"trust_device":false,"device_label":null}),
        None,
    )
    .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.body);
    assert!(login.cookie.is_some());
    assert_eq!(
        call(&f, "GET", "/api/v2/bootstrap/status", Value::Null, None)
            .await
            .body["initialized"],
        true
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn csrf_and_host_checks_precede_mutations_and_native_session_loading(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let payload = json!({"schema_version":1,"capability_id":Id::new(),"capability":"A".repeat(43)});
    for origin in [
        None,
        Some("null"),
        Some("https://evil.example"),
        Some("http://research.example"),
        Some("https://research.example.evil.invalid"),
        Some("https://research.example:444"),
    ] {
        let response = request(
            &f.app,
            "POST",
            "/api/v2/bootstrap/start",
            payload.clone(),
            None,
            origin,
            "research.example",
        )
        .await;
        assert_eq!(response.status, StatusCode::FORBIDDEN);
        assert_eq!(response.body["code"], "INVALID_ORIGIN");
    }
    let bad = request(
        &f.app,
        "GET",
        "/api/v2/bootstrap/status",
        Value::Null,
        None,
        None,
        "evil.example",
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM app.auth_rate_windows")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    let response = call(
        &f,
        "POST",
        "/api/v2/auth/login",
        json!({"schema_version":1,"code":"000000","trust_device":false,"unknown":true}),
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.headers[header::CONTENT_TYPE],
        "application/problem+json"
    );
    assert_eq!(
        response.body["request_id"].as_str().unwrap(),
        response.headers["x-request-id"].to_str().unwrap()
    );
    assert!(!response.body.to_string().contains("SQL"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirmation_requires_the_original_private_browser_cookie(pool: PgPool) {
    let f = fixture(pool).await;
    let (enrollment, cookie, native) = start(&f).await;
    let code = native.generate(
        f.store
            .authentication_snapshot()
            .await
            .unwrap()
            .database_now
            .timestamp() as u64,
    );
    let payload = json!({"schema_version":1,"enrollment_id":enrollment["enrollment_id"],"code":code,"trust_device":false,"device_label":null});
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/bootstrap/confirm",
            payload.clone(),
            None
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    let damaged = format!("{}x", cookie);
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/bootstrap/confirm",
            payload.clone(),
            Some(&damaged)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert!(!f.store.authentication_snapshot().await.unwrap().initialized);
    assert_eq!(
        call(
            &f,
            "POST",
            "/api/v2/bootstrap/confirm",
            payload,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_device_revocation_blocks_existing_cookie_immediately(pool: PgPool) {
    let f = fixture(pool).await;
    let (enrollment, cookie, native) = start(&f).await;
    let (confirmed, _) = confirm(&f, &enrollment, &cookie, &native, true).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap();
    let device = confirmed.body["trusted_device_id"].as_str().unwrap();
    let revoked = call(
        &f,
        "DELETE",
        &format!("/api/v2/auth/devices/{device}"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    assert_eq!(
        call(
            &f,
            "GET",
            "/api/v2/auth/devices",
            Value::Null,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_initialization_capabilities_and_rate_limits_do_not_grant_setup(pool: PgPool) {
    let f = fixture(pool).await;
    for _ in 0..5 {
        let rejected = call(
            &f,
            "POST",
            "/api/v2/bootstrap/start",
            json!({"schema_version":1,"capability_id":Id::new(),"capability":"A".repeat(43)}),
            None,
        )
        .await;
        assert_eq!(rejected.status, StatusCode::UNAUTHORIZED);
        assert!(!rejected.body.to_string().contains("provisioning_uri"));
    }
    let limited = call(
        &f,
        "POST",
        "/api/v2/bootstrap/start",
        json!({"schema_version":1,"capability_id":Id::new(),"capability":"A".repeat(43)}),
        None,
    )
    .await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers.contains_key(header::RETRY_AFTER));
    assert!(!f.store.authentication_snapshot().await.unwrap().initialized);
}

#[test]
fn deployment_policy_never_enables_cleartext_public_authentication() {
    let public = "0.0.0.0:8080".parse().unwrap();
    let local = "127.0.0.1:8080".parse().unwrap();
    for (url, bind, dev) in [
        ("http://research.example", public, true),
        ("http://localhost:8080", public, true),
        ("http://localhost:8080", local, false),
        ("https://u:p@research.example", local, false),
        ("https://research.example/subpath", local, false),
        ("https://research.example?secret=x", local, false),
    ] {
        assert!(WebPolicy::new(url, bind, dev).is_err(), "{url}");
    }
    assert!(WebPolicy::new("http://127.0.0.1:8080", local, true).is_ok());
    assert!(WebPolicy::new("https://research.example", public, false).is_ok());
    let schema: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    assert!(schema["paths"]["/api/v2/auth/session"].is_object());
    assert!(schema["paths"]["/api/v2/bootstrap/start"].is_object());
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_tcp_listener_uses_non_owner_database_role_and_native_private_cookie(pool: PgPool) {
    let fixture = fixture(pool.clone()).await;
    assert!(fixture.store.verify_runtime_role().await.is_err());
    let role = format!("api_test_{}", Id::new().to_string().replace('-', ""));
    sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD 'disposable-test-only' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT")).execute(&pool).await.unwrap();
    for sql in [
        format!("GRANT USAGE ON SCHEMA app,tower_sessions TO {role}"),
        format!("GRANT SELECT,INSERT,UPDATE ON ALL TABLES IN SCHEMA app TO {role}"),
        format!(
            "GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA tower_sessions TO {role}"
        ),
    ] {
        sqlx::query(&sql).execute(&pool).await.unwrap();
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
    let capability = random_capability();
    let issued = store
        .issue_bootstrap_capability(&capability_verifier(&capability).unwrap())
        .await
        .unwrap();
    let enrollment = client
        .post(format!("{origin}/api/v2/bootstrap/start"))
        .header(header::ORIGIN, &origin)
        .json(&json!({"schema_version":1,"capability_id":issued.id,"capability":capability}))
        .send()
        .await
        .unwrap();
    assert_eq!(enrollment.status(), StatusCode::CREATED);
    let anonymous = enrollment.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let body: Value = enrollment.json().await.unwrap();
    let native = TOTP::from_url(body["provisioning_uri"].as_str().unwrap()).unwrap();
    let code = native.generate(
        store
            .authentication_snapshot()
            .await
            .unwrap()
            .database_now
            .timestamp() as u64,
    );
    let confirmed=client.post(format!("{origin}/api/v2/bootstrap/confirm")).header(header::ORIGIN,&origin).header(header::COOKIE,&anonymous)
        .json(&json!({"schema_version":1,"enrollment_id":body["enrollment_id"],"code":code,"trust_device":false,"device_label":null})).send().await.unwrap();
    assert_eq!(confirmed.status(), StatusCode::OK);
    let cookie = confirmed.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert_ne!(cookie, anonymous);
    let valid = client
        .get(format!("{origin}/api/v2/auth/session"))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(valid.status(), StatusCode::OK);
    let out = client
        .post(format!("{origin}/api/v2/auth/logout"))
        .header(header::ORIGIN, &origin)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), StatusCode::NO_CONTENT);
    let invalid = client
        .get(format!("{origin}/api/v2/auth/session"))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
    serving.abort();
    let _ = serving.await;
    drop(store);
    app_pool.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
}
