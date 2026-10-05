//! Actual authenticated HTTP and PostgreSQL; runtime/catalog observations are controlled.
#[path = "../../../tests/support/strategy_mandate.rs"]
mod strategy;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

async fn get(f: &support::Fixture, cookie: &str, path: &str) -> support::Reply {
    support::exchange(
        &f.app,
        Request::builder()
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn strategy_mandate_http_preserves_original_receipt_and_read_discriminator(pool: PgPool) {
    let f = support::fixture(pool.clone()).await;
    let login = support::local_session(&f).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let login_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.browser_logins ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let actor = store::authority::Actor::Browser {
        login_id: login_id.to_string().try_into().unwrap(),
    };
    let directory = tempfile::tempdir().unwrap();
    let objects = Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let data =
        strategy::data::prepare(&pool, f.store.clone(), actor, None, directory, objects).await;
    let (_data, request) = strategy::prepare(&pool, data).await;
    let path = "/api/v2/portfolio-mandates";
    let body = serde_json::to_value(&request).unwrap();
    assert_eq!(
        support::invalid_bearer(&f, "POST", path, body.clone())
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let created =
        support::owner_command(&f, "POST", path, "strategy-mandate-http", body.clone()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        created.body["resource"]["content"]["allocation_method"],
        "FIXED_TARGET_WEIGHTS"
    );
    for field in [
        "optimizer",
        "covariance_estimator",
        "required_evaluation_policy_id",
        "objective",
        "risk_measure",
    ] {
        assert!(
            created.body["resource"]["content"].get(field).is_none(),
            "{field}"
        );
    }
    let read = get(
        &f,
        &cookie,
        &format!(
            "{path}/{}",
            created.body["resource"]["id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body, created.body["resource"]);
    let list = get(
        &f,
        &cookie,
        &format!("/api/v2/projects/{}/portfolio-mandates", request.project_id),
    )
    .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"], json!([read.body]));
    let replay =
        support::owner_command(&f, "POST", path, "strategy-mandate-http", body.clone()).await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], created.body["resource"]);
    let mut changed = body.clone();
    changed["content"]["target_ttl_seconds"] = json!(301);
    assert_eq!(
        support::owner_command(&f, "POST", path, "strategy-mandate-http", changed)
            .await
            .status,
        StatusCode::CONFLICT
    );
    let mut risk = body;
    risk["content"]["constraints"]["max_ex_ante_risk"] = json!("0.01");
    let denied = support::owner_command(&f, "POST", path, "unsupported-risk", risk).await;
    assert_eq!(
        denied.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        denied.body
    );
    let count:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_mandates),(SELECT count(*) FROM app.command_receipts WHERE operation='MANDATE_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(count, (1, 1));
    assert_eq!(denied.body["code"], "STRATEGY_CONSTRAINT_UNSUPPORTED");
}
