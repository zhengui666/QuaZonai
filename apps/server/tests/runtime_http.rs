//! Actual Operator TOTP -> HTTP -> native AEAD -> pinned TCP/TLS -> immutable
//! artifact/PostgreSQL observation. Remote capabilities remain protocol fixtures.
#[path = "support/runtime_native.rs"]
mod native;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use contracts::{DbCounter, Id};
use integrations::artifacts::ArtifactStore;
use native::{native_tls, NativeTls, SECRET};
use serde_json::{json, Value};
use server::runtime_transport::{RuntimeTarget, RuntimeTargets};
use sqlx::PgPool;
use std::sync::atomic::Ordering;
use support::*;

async fn command(
    f: &Fixture,
    cookie: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Value,
) -> Reply {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "research.example")
        .header(header::ORIGIN, "https://research.example")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", key)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    exchange(&f.app, request).await
}
async fn setup(pool: PgPool, allow_endpoint: bool) -> (Fixture, String, NativeTls, Value) {
    let tls = native_tls().await;
    let targets = if allow_endpoint {
        RuntimeTargets::new(
            vec![RuntimeTarget {
                origin: tls.endpoint(),
                addresses: vec![tls.server.address],
            }],
            false,
        )
        .unwrap()
    } else {
        RuntimeTargets::default()
    };
    let f = fixture_with_runtime_targets(pool, Some(targets)).await;
    let (enrollment, anonymous, totp) = start(&f).await;
    let (login, _) = confirm(&f, &enrollment, &anonymous, &totp, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let credential = command(&f, &cookie, "runtime-secret", "POST", "/api/v2/settings/credentials", json!({
        "intent":{"schema_version":1,"purpose":"RUNTIME","label":"Native Runtime credential"},"value":SECRET
    })).await;
    assert_eq!(
        credential.status,
        StatusCode::CREATED,
        "{}",
        credential.body
    );
    let ca = command(&f, &cookie, "runtime-ca", "POST", "/api/v2/settings/credentials", json!({
        "intent":{"schema_version":1,"purpose":"TLS_CA","label":"Native test CA"},"value":String::from_utf8(tls.ca.clone()).unwrap()
    })).await;
    assert_eq!(ca.status, StatusCode::CREATED, "{}", ca.body);
    let runtime = command(&f, &cookie, "runtime-config", "POST", "/api/v2/integrations/runtimes", json!({
        "schema_version":1,"configuration":{"name":"Native Runtime","endpoint":tls.endpoint(),"tls_policy":"PINNED_CA","allowed_capabilities":["DATA_VALIDATE"],"enabled":true,"development_http":false},
        "credential_ref":credential.body["resource"]["id"],"ca_certificate_ref":ca.body["resource"]["id"]
    })).await;
    assert_eq!(runtime.status, StatusCode::CREATED, "{}", runtime.body);
    (f, cookie, tls, runtime.body["resource"].clone())
}

#[sqlx::test(migrations = "../../migrations")]
async fn default_deployment_readiness_requires_auth_and_never_creates_an_observation(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let path = format!("/api/v2/integrations/runtimes/{}/readiness", Id::new());
    let anonymous = call(&f, "GET", &path, Value::Null, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let (enrollment, anonymous, totp) = start(&f).await;
    let (login, _) = confirm(&f, &enrollment, &anonymous, &totp, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let missing = call(&f, "GET", &path, Value::Null, Some(&cookie)).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runtime_probe_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "readiness GET must never run or publish a probe");
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_tls_probe_publishes_actual_bytes_and_a_revision_bound_receipt(pool: PgPool) {
    let (f, cookie, tls, runtime) = setup(pool.clone(), true).await;
    let id = runtime["id"].as_str().unwrap();
    let status = call(
        &f,
        "GET",
        &format!("/api/v2/integrations/runtimes/{id}/readiness"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["state"], "NOT_CHECKED");
    let intent = json!({"schema_version":1,"expected_revision":runtime["revision"]});
    let result = command(
        &f,
        &cookie,
        "probe-once",
        "POST",
        &format!("/api/v2/integrations/runtimes/{id}/probe"),
        intent.clone(),
    )
    .await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.body);
    assert_eq!(result.body["resource"]["outcome"]["status"], "AVAILABLE");
    assert_eq!(result.headers[header::CACHE_CONTROL], "no-store");
    assert!(!result.body.to_string().contains(SECRET));
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 1);
    let artifact: Id = result.body["resource"]["snapshot_artifact_id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(artifact.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let objects = ArtifactStore::open(&f._state.path().join("artifacts")).unwrap();
    let bytes = objects
        .read(artifact, DbCounter::new(count as u64).unwrap())
        .unwrap();
    let stored: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(stored["result"], result.body["resource"]["outcome"]);
    let replay = command(
        &f,
        &cookie,
        "probe-once",
        "POST",
        &format!("/api/v2/integrations/runtimes/{id}/probe"),
        intent,
    )
    .await;
    assert_eq!(replay.body["resource"], result.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 1);
    let readiness = call(
        &f,
        "GET",
        &format!("/api/v2/integrations/runtimes/{id}/readiness"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(readiness.body["state"], "AVAILABLE");
    assert_eq!(
        readiness.body["available_job_kinds"],
        json!(["DATA_VALIDATE"])
    );
    assert_eq!(readiness.body["integration_revision"], runtime["revision"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unlisted_origin_never_receives_a_credential_and_cannot_report_ready(pool: PgPool) {
    let (f, cookie, tls, runtime) = setup(pool, false).await;
    let id = runtime["id"].as_str().unwrap();
    let result = command(
        &f,
        &cookie,
        "probe-denied",
        "POST",
        &format!("/api/v2/integrations/runtimes/{id}/probe"),
        json!({"schema_version":1,"expected_revision":runtime["revision"]}),
    )
    .await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.body);
    assert_eq!(
        result.body["resource"]["outcome"],
        json!({"status":"UNAVAILABLE","reason":"ENDPOINT_DENIED"})
    );
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 0);
    let readiness = call(
        &f,
        "GET",
        &format!("/api/v2/integrations/runtimes/{id}/readiness"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(readiness.body["state"], "UNAVAILABLE");
    assert_eq!(readiness.body["available_job_kinds"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_revision_and_injected_success_never_reach_the_native_transport(pool: PgPool) {
    let (f, cookie, tls, runtime) = setup(pool, true).await;
    let id = runtime["id"].as_str().unwrap();
    let path = format!("/api/v2/integrations/runtimes/{id}/probe");
    let bad = command(
        &f,
        &cookie,
        "injected",
        "POST",
        &path,
        json!({"schema_version":1,"expected_revision":"1","capabilities":{"status":"AVAILABLE"}}),
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let stale = command(
        &f,
        &cookie,
        "stale",
        "POST",
        &path,
        json!({"schema_version":1,"expected_revision":"99"}),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 0);
    let anonymous = call(
        &f,
        "POST",
        &path,
        json!({"schema_version":1,"expected_revision":"1"}),
        None,
    )
    .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 0);
}
