//! Actual Axum/native private sessions, TOTP, AEAD and PostgreSQL. No fake auth.
//! Failure diagnostics never print responses, decrypted bytes or tokens.
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use contracts::Id;
use integrations::secrets::SecretVault;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::fs;
use support::*;

async fn authenticated(pool: PgPool) -> (Fixture, String) {
    let f = fixture(pool).await;
    let (enrollment, anonymous, native) = start(&f).await;
    let (reply, _) = confirm(&f, &enrollment, &anonymous, &native, true).await;
    assert_eq!(reply.status, StatusCode::OK);
    let cookie = reply.cookie.unwrap();
    (f, cookie)
}
async fn http(
    f: &Fixture,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> Reply {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "research.example");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let data = if body.is_null() {
        Body::empty()
    } else {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    };
    exchange(&f.app, request.body(data).unwrap()).await
}
async fn browser(
    f: &Fixture,
    cookie: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Value,
) -> Reply {
    http(
        f,
        method,
        path,
        body,
        &[
            ("cookie", cookie),
            ("origin", "https://research.example"),
            ("idempotency-key", key),
        ],
    )
    .await
}
async fn secret(f: &Fixture, cookie: &str, key: &str, purpose: &str, value: &str) -> Reply {
    browser(f,cookie,key,"POST","/api/v2/settings/credentials",json!({"intent":{"schema_version":1,"purpose":purpose,"label":"Private integration credential"},"value":value})).await
}
fn runtime(credential: Value) -> Value {
    json!({"schema_version":1,"configuration":{"name":"Native runtime","endpoint":"https://runtime.example","tls_policy":"SYSTEM_CA","allowed_capabilities":["ALPHA_EVALUATE","PORTFOLIO_SIMULATE"],"enabled":true,"development_http":false},"credential_ref":credential,"ca_certificate_ref":null})
}
fn vault(f: &Fixture) -> SecretVault {
    SecretVault::open(
        &f._state.path().join("secrets"),
        &f._state.path().join("master.key"),
    )
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn plaintext_never_enters_public_response_or_database_receipt(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let value = "RANDOM_INTEGRATION_SECRET_SENTINEL_8t3GvX";
    let response = secret(&f, &cookie, "one", "RUNTIME", value).await;
    assert_eq!(response.status, StatusCode::CREATED);
    assert_eq!(response.headers[header::CACHE_CONTROL], "no-store");
    assert!(!response.body.to_string().contains(value));
    assert!(response.body["resource"].get("value").is_none());
    let id: Id = response.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    assert!(
        vault(&f).read(id, "RUNTIME").unwrap() == value.as_bytes(),
        "native secret value mismatch"
    );
    let encrypted = fs::read(f._state.path().join("secrets").join(id.to_string())).unwrap();
    assert!(!encrypted
        .windows(value.len())
        .any(|bytes| bytes == value.as_bytes()));
    let records:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM app.command_receipts c WHERE operation='INTEGRATION_SECRET_REGISTER'").fetch_all(&pool).await.unwrap();
    assert_eq!(records.len(), 1);
    assert!(!records[0].to_string().contains(value));
    let again = secret(&f, &cookie, "one", "RUNTIME", value).await;
    assert_eq!(again.status, StatusCode::CREATED);
    assert!(
        again.body["replayed"] == true,
        "secret replay flag mismatch"
    );
    assert!(
        again.body["resource"] == response.body["resource"],
        "secret replay resource mismatch"
    );
    let conflict = secret(
        &f,
        &cookie,
        "one",
        "RUNTIME",
        "different-native-secret-fixture-32-byte-minimum",
    )
    .await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    assert!(
        vault(&f).read(id, "RUNTIME").unwrap() == value.as_bytes(),
        "native secret value mismatch"
    );
    let forbidden = http(
        &f,
        "GET",
        &format!("/api/v2/settings/credentials/{id}"),
        Value::Null,
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(forbidden.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_configuration_uses_native_reference_and_never_claims_probe_success(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let credential = secret(
        &f,
        &cookie,
        "runtime-key",
        "RUNTIME",
        "original-capability-fixture-32-byte-minimum",
    )
    .await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let original = runtime(credential.body["resource"]["id"].clone());
    let response = browser(
        &f,
        &cookie,
        "runtime",
        "POST",
        "/api/v2/integrations/runtimes",
        original.clone(),
    )
    .await;
    assert_eq!(response.status, StatusCode::CREATED);
    let resource = &response.body["resource"];
    assert!(
        resource["credential_configured"] == true,
        "credential state mismatch"
    );
    assert!(resource["ca_configured"] == false, "CA state mismatch");
    assert!(resource.get("credential_ref").is_none());
    assert!(resource.get("readiness").is_none());
    assert!(resource["last_capability_snapshot_artifact_id"].is_null());
    let id = resource["id"].as_str().unwrap();
    let mut update = json!({"schema_version":1,"expected_revision":resource["revision"],"configuration":resource["configuration"],"credential_ref":null,"ca_certificate_ref":null});
    update["configuration"]["enabled"] = json!(false);
    let result = browser(
        &f,
        &cookie,
        "disable",
        "PATCH",
        &format!("/api/v2/integrations/runtimes/{id}"),
        update.clone(),
    )
    .await;
    assert_eq!(result.status, StatusCode::OK);
    assert!(
        result.body["resource"]["configuration"]["enabled"] == false,
        "runtime disabled state mismatch"
    );
    let stale = browser(
        &f,
        &cookie,
        "stale",
        "PATCH",
        &format!("/api/v2/integrations/runtimes/{id}"),
        update,
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert!(
        stale.body["current_revision"] == result.body["resource"]["revision"],
        "stale revision response mismatch"
    );
    let replay = browser(
        &f,
        &cookie,
        "runtime",
        "POST",
        "/api/v2/integrations/runtimes",
        original,
    )
    .await;
    assert!(
        replay.body["resource"] == *resource,
        "runtime replay mismatch"
    );
    assert!(
        replay.body["replayed"] == true,
        "runtime replay flag mismatch"
    );
    let page = http(
        &f,
        "GET",
        "/api/v2/integrations/runtimes?limit=1",
        Value::Null,
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(page.body["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_secret_purpose_invalid_ca_and_production_http_do_not_publish(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let downstream = secret(
        &f,
        &cookie,
        "downstream",
        "DOWNSTREAM",
        "downstream-capability",
    )
    .await;
    assert_eq!(downstream.status, StatusCode::CREATED);
    let value = runtime(downstream.body["resource"]["id"].clone());
    let denied = browser(
        &f,
        &cookie,
        "purpose",
        "POST",
        "/api/v2/integrations/runtimes",
        value.clone(),
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNPROCESSABLE_ENTITY);
    let invalid = secret(
        &f,
        &cookie,
        "invalid-ca",
        "TLS_CA",
        "not-a-native-PEM-certificate",
    )
    .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut insecure = value;
    insecure["configuration"]["endpoint"] = json!("http://127.0.0.1:8081");
    insecure["configuration"]["development_http"] = json!(true);
    let denied = browser(
        &f,
        &cookie,
        "http",
        "POST",
        "/api/v2/integrations/runtimes",
        insecure,
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNPROCESSABLE_ENTITY);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runtime_integrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let anonymous = http(
        &f,
        "POST",
        "/api/v2/settings/credentials",
        json!({"intent":{"schema_version":1,"purpose":"RUNTIME","label":"test"},"value":"authorization-boundary-fixture-32-byte-minimum"}),
        &[
            ("origin", "https://research.example"),
            ("idempotency-key", "anonymous"),
        ],
    )
    .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn downstream_and_doctor_authority_follow_the_real_machine_channel(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let credential = secret(
        &f,
        &cookie,
        "recipient-secret",
        "DOWNSTREAM",
        "recipient-capability",
    )
    .await;
    let request = json!({"schema_version":1,"configuration":{"name":"Paper recipient","endpoint":"https://recipient.example","accepted_package_versions":["1"],"environments":"PAPER","enabled":true,"development_http":false},"credential_ref":credential.body["resource"]["id"]});
    let created = browser(
        &f,
        &cookie,
        "recipient",
        "POST",
        "/api/v2/integrations/downstreams",
        request.clone(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let principal=browser(&f,&cookie,"doctor","POST","/api/v2/machine-principals",json!({"schema_version":1,"name":"Read-only doctor","kind":"CLI","project_id":null,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let issued=browser(&f,&cookie,"doctor-credential","POST",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["DOCTOR_READ"],"expires_at":Utc::now()+Duration::hours(1)})).await;
    assert_eq!(issued.status, StatusCode::CREATED);
    let bearer = format!("Bearer {}", issued.body["token"].as_str().unwrap());
    let read = http(
        &f,
        "GET",
        "/api/v2/integrations/downstreams",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    assert!(!read.body.to_string().contains("recipient-capability"));
    assert!(read.body["items"][0].get("credential_ref").is_none());
    let denied = http(
        &f,
        "POST",
        "/api/v2/integrations/downstreams",
        request,
        &[
            ("authorization", &bearer),
            ("idempotency-key", "machine-write"),
        ],
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let denied=http(&f,"POST","/api/v2/settings/credentials",json!({"intent":{"schema_version":1,"purpose":"RUNTIME","label":"forbidden"},"value":"authorization-boundary-fixture-32-byte-minimum"}),&[("authorization",&bearer),("idempotency-key","machine-secret")]).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_identical_secret_writes_keep_one_native_object_and_receipt(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let (a, b) = tokio::join!(
        secret(
            &f,
            &cookie,
            "same",
            "RUNTIME",
            "one-capability-fixture-32-byte-minimum"
        ),
        secret(
            &f,
            &cookie,
            "same",
            "RUNTIME",
            "one-capability-fixture-32-byte-minimum"
        )
    );
    assert_eq!(a.status, StatusCode::CREATED);
    assert_eq!(b.status, StatusCode::CREATED);
    assert!(
        a.body["resource"] == b.body["resource"],
        "concurrent secret resource mismatch"
    );
    assert!(
        a.body["replayed"] != b.body["replayed"],
        "concurrent secret replay flags must differ"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='INTEGRATION_SECRET_REGISTER'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let id: Id = a.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    assert!(
        vault(&f).read(id, "RUNTIME").unwrap() == b"one-capability-fixture-32-byte-minimum",
        "native secret value mismatch"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn disconnected_request_preserves_the_single_admitted_secret_command(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    sqlx::query("CREATE FUNCTION app.test_pause_secret_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='INTEGRATION_SECRET_REGISTER' THEN PERFORM pg_advisory_xact_lock(8263,20); END IF; RETURN NEW; END $$")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER test_pause_secret_receipt BEFORE INSERT ON app.command_receipts FOR EACH ROW EXECUTE FUNCTION app.test_pause_secret_receipt()")
        .execute(&pool).await.unwrap();
    let mut barrier = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(8263,20)")
        .execute(&mut *barrier)
        .await
        .unwrap();
    let app = f.app.clone();
    let cookie_copy = cookie.clone();
    let task = tokio::spawn(async move {
        let request=Request::builder().method("POST").uri("/api/v2/settings/credentials")
            .header(header::HOST,"research.example").header(header::ORIGIN,"https://research.example")
            .header(header::COOKIE,cookie_copy).header(header::CONTENT_TYPE,"application/json")
            .header("idempotency-key","disconnected")
            .body(Body::from(json!({"intent":{"schema_version":1,"purpose":"RUNTIME","label":"Private integration credential"},"value":"survives-disconnection-fixture-32-byte-minimum"}).to_string())).unwrap();
        exchange(&app, request).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10),async {
        loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO app.command_receipts%')")
                .fetch_one(&pool).await.unwrap();
            if blocked {break;}
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    task.abort();
    let _ = task.await;
    barrier.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10),async {
        loop {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.command_receipts WHERE operation='INTEGRATION_SECRET_REGISTER' AND idempotency_key='disconnected')").fetch_one(&pool).await.unwrap();
            if exists {break;}
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    let retry = secret(
        &f,
        &cookie,
        "disconnected",
        "RUNTIME",
        "survives-disconnection-fixture-32-byte-minimum",
    )
    .await;
    assert_eq!(retry.status, StatusCode::CREATED);
    assert!(
        retry.body["replayed"] == true,
        "disconnected secret replay flag mismatch"
    );
    let id: Id = retry.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    assert!(
        vault(&f).read(id, "RUNTIME").unwrap() == b"survives-disconnection-fixture-32-byte-minimum",
        "native secret value mismatch"
    );
}
