//! Actual TOTP/cookie authentication, Axum commands and PostgreSQL/PGMQ startup.
//! Parent data/capability records are explicit fixtures, not production T42 evidence.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
mod support;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::authority::Actor;
use support::{confirm, exchange, fixture, start, Fixture, Reply};

async fn browser(
    f: &Fixture,
    cookie: &str,
    method: &str,
    path: &str,
    key: &str,
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
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(serde_json::to_vec(&body).unwrap())
        })
        .unwrap();
    exchange(&f.app, request).await
}

async fn setup(pool: &PgPool) -> (Fixture, String, cycle_support::Fixture) {
    let f = fixture(pool.clone()).await;
    let (enrollment, anonymous, native) = start(&f).await;
    let (login, _) = confirm(&f, &enrollment, &anonymous, &native, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let login_id: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let actor = Actor::Browser {
        login_id: login_id.try_into().unwrap(),
    };
    let data = cycle_support::setup(pool, &f.store, &actor).await;
    (f, cookie, data)
}

async fn activate(f: &Fixture, cookie: &str, project: contracts::Id) -> Value {
    let path = format!("/api/v2/projects/{project}");
    let current = browser(f, cookie, "GET", &path, "unused", Value::Null).await;
    assert_eq!(current.status, StatusCode::OK, "{}", current.body);
    let updated = browser(
        f,
        cookie,
        "PATCH",
        &path,
        "activate",
        json!({
            "schema_version": 1,
            "expected_revision": current.body["revision"],
            "name": current.body["name"],
            "description": current.body["description"],
            "state": "ACTIVE"
        }),
    )
    .await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
    updated.body["resource"].clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn authenticated_freeze_and_cycle_start_publish_one_real_run_and_original_http_receipt(
    pool: PgPool,
) {
    let (f, cookie, data) = setup(&pool).await;
    let freeze_path = format!("/api/v2/briefs/{}/freeze", data.brief.id);
    let freeze_body = serde_json::to_value(&data.freeze).unwrap();
    let frozen = browser(
        &f,
        &cookie,
        "POST",
        &freeze_path,
        "freeze",
        freeze_body.clone(),
    )
    .await;
    assert_eq!(frozen.status, StatusCode::OK, "{}", frozen.body);
    assert_eq!(frozen.body["resource"]["brief"]["state"], "FROZEN");
    assert_eq!(frozen.headers[header::CACHE_CONTROL], "no-store");
    let context = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/briefs/{}/execution-context", data.brief.id),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(context.status, StatusCode::OK);
    assert_eq!(context.body, frozen.body["resource"]);
    let replay = browser(&f, &cookie, "POST", &freeze_path, "freeze", freeze_body).await;
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], frozen.body["resource"]);

    let project = activate(&f, &cookie, data.data.project).await;
    let path = format!("/api/v2/projects/{}/cycles", data.data.project);
    let body = json!({
        "schema_version": 1,
        "brief_id": data.brief.id,
        "expected_revision": project["revision"]
    });
    let started = browser(&f, &cookie, "POST", &path, "start", body.clone()).await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.body);
    assert_eq!(started.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(started.body["resource"]["run"]["kind"], "DATA_VALIDATE");
    assert_eq!(started.body["resource"]["run"]["state"], "QUEUED");
    let run_id = started.body["resource"]["run"]["id"].as_str().unwrap();
    let cycle_id = started.body["resource"]["cycle"]["id"].as_str().unwrap();
    let run = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/runs/{run_id}"),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(run.status, StatusCode::OK, "{}", run.body);
    assert_eq!(run.body, started.body["resource"]["run"]);
    let cycle = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/cycles/{cycle_id}"),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(cycle.status, StatusCode::OK);
    assert_eq!(cycle.body["initial_run_id"], run_id);
    let replay = browser(&f, &cookie, "POST", &path, "start", body).await;
    assert_eq!(replay.status, StatusCode::ACCEPTED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], started.body["resource"]);
    let page = browser(&f, &cookie, "GET", &path, "unused", Value::Null).await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(page.body["items"].as_array().unwrap().len(), 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn startup_rejects_forged_success_stale_revision_and_unauthenticated_mutation(pool: PgPool) {
    let (f, cookie, data) = setup(&pool).await;
    let path = format!("/api/v2/briefs/{}/freeze", data.brief.id);
    let mut forged = serde_json::to_value(&data.freeze).unwrap();
    forged["capabilities"] = json!({"status": "AVAILABLE"});
    let rejected = browser(&f, &cookie, "POST", &path, "injected", forged).await;
    assert_eq!(rejected.status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut stale = serde_json::to_value(&data.freeze).unwrap();
    stale["expected_revision"] = json!("99");
    let rejected = browser(&f, &cookie, "POST", &path, "stale", stale).await;
    assert_eq!(rejected.status, StatusCode::CONFLICT);
    let rejected = browser(
        &f,
        "",
        "POST",
        &path,
        "anonymous",
        serde_json::to_value(&data.freeze).unwrap(),
    )
    .await;
    assert_eq!(rejected.status, StatusCode::UNAUTHORIZED);
    let contexts: i64 = sqlx::query_scalar("SELECT count(*) FROM app.brief_execution_contexts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(contexts, 0);
    let cycles: i64 = sqlx::query_scalar("SELECT count(*) FROM app.research_cycles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cycles, 0);
}

#[test]
fn startup_routes_are_in_the_actual_native_http_contract() {
    let document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    for (path, method, status, identity) in [
        (
            "/api/v2/briefs/{id}/freeze",
            "post",
            "200",
            "freezeResearchBrief",
        ),
        (
            "/api/v2/briefs/{id}/execution-context",
            "get",
            "200",
            "getResearchBriefExecutionContext",
        ),
        (
            "/api/v2/projects/{id}/cycles",
            "post",
            "202",
            "startResearchCycle",
        ),
        (
            "/api/v2/projects/{id}/cycles",
            "get",
            "200",
            "listProjectResearchCycles",
        ),
        ("/api/v2/cycles/{id}", "get", "200", "getResearchCycle"),
    ] {
        let operation = &document["paths"][path][method];
        assert!(operation["responses"][status].is_object(), "{path}");
        assert!(operation["responses"]["503"].is_object(), "{path}");
        assert_eq!(operation["operationId"], identity, "{path}");
    }
}
