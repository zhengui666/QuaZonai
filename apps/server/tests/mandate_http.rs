//! Real browser session/middleware and Store transactions; controlled Runtime metadata.
#[path = "../../../tests/support/mandate.rs"]
mod mandate_support;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::authority::Actor;
use support::*;

async fn send(f: &Fixture, cookie: Option<&str>, method: &str, path: &str, body: Value) -> Reply {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "localhost")
        .header(header::ORIGIN, "https://localhost")
        .header("idempotency-key", "mandate-http");
    if let Some(cookie) = cookie {
        b = b.header(header::COOKIE, cookie);
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        b = b.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    };
    exchange(&f.app, b.body(body).unwrap()).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn actual_mandate_http_preserves_original_version_and_never_updates_it(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let login = local_session(&f).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.browser_logins ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let actor = Actor::Browser {
        login_id: id.to_string().try_into().unwrap(),
    };
    let request = mandate_support::request(&pool, &f.store, &actor).await;
    let body = serde_json::to_value(&request).unwrap();
    let path = "/api/v2/portfolio-mandates";
    assert_eq!(
        invalid_bearer(&f, "POST", path, body.clone()).await.status,
        StatusCode::UNAUTHORIZED
    );
    let created = send(&f, Some(&cookie), "POST", path, body.clone()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.headers[header::CACHE_CONTROL], "no-store");
    let item = format!(
        "{path}/{}",
        created.body["resource"]["id"].as_str().unwrap()
    );
    let read = send(&f, Some(&cookie), "GET", &item, Value::Null).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body, created.body["resource"]);
    let list = send(
        &f,
        Some(&cookie),
        "GET",
        &format!("/api/v2/projects/{}/portfolio-mandates", request.project_id),
        Value::Null,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"][0], read.body);
    let replay = send(&f, Some(&cookie), "POST", path, body.clone()).await;
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], created.body["resource"]);
    assert_eq!(
        send(&f, Some(&cookie), "PATCH", &item, body.clone())
            .await
            .status,
        StatusCode::METHOD_NOT_ALLOWED
    );
    let mut changed = body.clone();
    changed["content"]["exposure_tolerance"] = json!("0.00001");
    assert_eq!(
        send(&f, Some(&cookie), "POST", path, changed).await.status,
        StatusCode::CONFLICT
    );
    let mut extra = body;
    extra["content"]["optimizer"]["parameters"]["guess"] = json!(true);
    assert_eq!(
        send(&f, Some(&cookie), "POST", path, extra).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.portfolio_mandates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
