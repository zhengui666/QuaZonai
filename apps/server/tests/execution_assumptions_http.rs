//! Actual Axum/session/PostgreSQL/object store; controlled native catalog/probe observations.
#[path = "../../../tests/support/execution_assumptions.rs"]
mod assumptions;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;

#[sqlx::test(migrations = "../../migrations")]
async fn assumptions_http_missing_and_empty_are_not_fabricated_versions(pool: PgPool) {
    let f = support::fixture(pool).await;
    let missing = format!("/api/v2/execution-assumptions/{}", contracts::Id::new());
    assert_eq!(
        support::call(&f, "GET", &missing, Value::Null, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let (enrollment, initial, totp) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &totp, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap_or(initial);
    assert_eq!(
        send(&f, &cookie, "GET", &missing, Value::Null).await.status,
        StatusCode::NOT_FOUND
    );
    let project = send(&f, &cookie, "POST", "/api/v2/projects", serde_json::json!({"schema_version":1,"name":"Empty assumptions","description":"No source or qualification","fork_from_project_id":null})).await;
    assert_eq!(project.status, StatusCode::CREATED);
    let path = format!(
        "/api/v2/projects/{}/execution-assumptions",
        project.body["resource"]["id"].as_str().unwrap()
    );
    let empty = send(&f, &cookie, "GET", &path, Value::Null).await;
    assert_eq!(empty.status, StatusCode::OK);
    assert_eq!(empty.body["items"], serde_json::json!([]));
    assert_eq!(
        send(&f, &cookie, "GET", &format!("{path}?limit=0"), Value::Null)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

async fn send(
    f: &support::Fixture,
    cookie: &str,
    method: &str,
    path: &str,
    body: Value,
) -> support::Reply {
    support::exchange(
        &f.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "research.example")
            .header(header::ORIGIN, "https://research.example")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", "assumptions-http")
            .body(if body.is_null() {
                Body::empty()
            } else {
                Body::from(serde_json::to_vec(&body).unwrap())
            })
            .unwrap(),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_assumptions_http_creates_reads_replays_and_rejects_changed_intent(pool: PgPool) {
    let targets = server::runtime_transport::RuntimeTargets::new(vec![], false).unwrap();
    let f = support::fixture_with_runtime_targets(pool.clone(), Some(targets)).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &totp, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap_or(initial);
    let login_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.browser_logins ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let actor = store::authority::Actor::Browser {
        login_id: login_id.to_string().try_into().unwrap(),
    };
    let objects = Arc::new(
        integrations::artifacts::ArtifactStore::open(&f._state.path().join("artifacts")).unwrap(),
    );
    let data = assumptions::data::prepare(
        &pool,
        f.store.clone(),
        actor,
        None,
        tempfile::tempdir().unwrap(),
        objects.clone(),
    )
    .await;
    let (_data, request) = assumptions::prepare(&pool, data).await;
    let body = serde_json::to_value(&request).unwrap();
    let path = "/api/v2/execution-assumptions";
    let created = send(&f, &cookie, "POST", path, body.clone()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let item = format!(
        "{path}/{}",
        created.body["resource"]["id"].as_str().unwrap()
    );
    let read = send(&f, &cookie, "GET", &item, Value::Null).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body, created.body["resource"]);
    let list = send(
        &f,
        &cookie,
        "GET",
        &format!(
            "/api/v2/projects/{}/execution-assumptions",
            request.project_id
        ),
        Value::Null,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"][0], read.body);
    let replay = send(&f, &cookie, "POST", path, body.clone()).await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], read.body);
    let mut different = body.clone();
    different["settlement_rule_ref"] = "changed".into();
    assert_eq!(
        send(&f, &cookie, "POST", path, different).await.status,
        StatusCode::CONFLICT
    );
    let mut unsupported = body;
    unsupported["cost_assumption_status"] = "DATA_BACKED".into();
    assert_eq!(
        send(&f, &cookie, "POST", path, unsupported).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(&f, &cookie, "DELETE", &item, Value::Null).await.status,
        StatusCode::METHOD_NOT_ALLOWED
    );
    let artifact: uuid::Uuid = created.body["resource"]["fee_schedule_artifact_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(artifact)
        .fetch_one(&pool)
        .await
        .unwrap();
    let stored: Value = serde_json::from_slice(
        &objects
            .read(
                artifact.to_string().try_into().unwrap(),
                contracts::DbCounter::new(size as u64).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored, serde_json::to_value(&request.settings).unwrap());
}
