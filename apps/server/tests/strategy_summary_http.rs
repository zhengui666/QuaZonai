//! Real authenticated HTTP boundary; scientific replay is covered by native jobs.
#[allow(dead_code)]
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn summary_route_requires_auth_and_reports_invalid_missing_or_unavailable_source(
    pool: PgPool,
) {
    let mut f = support::fixture(pool).await;
    let path = format!(
        "/api/v2/portfolio-candidates/{}/summary",
        contracts::Id::new()
    );
    let anonymous = support::exchange(
        &f.app,
        Request::builder()
            .uri(&path)
            .header(header::HOST, "localhost")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        support::invalid_bearer(&f, "GET", &path, serde_json::Value::Null)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    support::local_session(&f).await;
    let unavailable =
        support::owner_command(&f, "GET", &path, "summary-read", serde_json::Value::Null).await;
    assert_eq!(unavailable.status, StatusCode::SERVICE_UNAVAILABLE);
    let vault = SecretVault::open(
        &f._state.path().join("secrets"),
        &f._state.path().join("master.key"),
    )
    .unwrap();
    let state = server::AppState::new(
        f.store.clone(),
        vault,
        server::WebPolicy::new(
            "https://localhost",
            "127.0.0.1:8080".parse().unwrap(),
            false,
        )
        .unwrap(),
    )
    .with_artifact_store(ArtifactStore::open(&f._state.path().join("objects")).unwrap());
    f.app = server::router(state, tower_sessions::cookie::Key::generate());
    let missing =
        support::owner_command(&f, "GET", &path, "summary-read", serde_json::Value::Null).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND, "{}", missing.body);
    assert_eq!(missing.headers[header::CACHE_CONTROL], "no-store");
    let malformed = support::owner_command(
        &f,
        "GET",
        "/api/v2/portfolio-candidates/not-an-id/summary",
        "summary-read",
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(malformed.status, StatusCode::UNPROCESSABLE_ENTITY);
}
