//! Real authentication, ArtifactStore bytes and PostgreSQL proposal publication.
//! The parent Cycle is an explicit relational fixture, not a freeze/readiness bypass.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/experiments.rs"]
mod experiment_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use contracts::{experiments::ExperimentView, Id};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::PgPool;
use store::authority::Actor;
use support::{confirm, exchange, fixture, start, Fixture, Reply};
use tower_sessions::cookie::Key;

async fn send(
    f: &Fixture,
    method: &str,
    path: &str,
    value: Value,
    headers: &[(&str, &str)],
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "research.example");
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    let body = if value.is_null() {
        Body::empty()
    } else {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&value).unwrap())
    };
    exchange(&f.app, builder.body(body).unwrap()).await
}
async fn browser(
    f: &Fixture,
    cookie: &str,
    method: &str,
    path: &str,
    key: &str,
    value: Value,
) -> Reply {
    send(
        f,
        method,
        path,
        value,
        &[
            ("cookie", cookie),
            ("origin", "https://research.example"),
            ("idempotency-key", key),
        ],
    )
    .await
}

async fn setup(pool: &PgPool) -> (Fixture, String, experiment_support::Fixture) {
    let mut f = fixture(pool.clone()).await;
    let root = f._state.path();
    let state = AppState::new(
        f.store.clone(),
        SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
        WebPolicy::new(
            "https://research.example",
            "127.0.0.1:8080".parse().unwrap(),
            false,
        )
        .unwrap(),
    )
    .with_artifact_store(ArtifactStore::open(&root.join("artifacts")).unwrap());
    f.app = server::router(state, Key::generate());
    let (enrollment, cookie, totp) = start(&f).await;
    let (login, _) = confirm(&f, &enrollment, &cookie, &totp, true).await;
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
    let mut e = experiment_support::setup(pool, &f.store, &actor, 3).await;
    // Replace all relational fixture references with objects uploaded through
    // the real native byte store and authenticated HTTP publication endpoint.
    for kind in ["CODE", "PARAMETERS", "REPORT"] {
        let content = if kind == "CODE" {
            "pub fn research_value() -> f64 { 1.0 }"
        } else {
            "{\"schema_version\":1,\"source\":\"test\"}"
        };
        let uploaded = browser(
            &f,
            &cookie,
            "POST",
            "/api/v2/artifacts",
            kind,
            json!({"schema_version":1,"project_id":e.data.project,"kind":kind,"content":content}),
        )
        .await;
        assert_eq!(uploaded.status, StatusCode::CREATED, "{}", uploaded.body);
        assert_eq!(uploaded.body["resource"]["origin"], "SYNTHETIC");
        let id: Id = uploaded.body["resource"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
            .try_into()
            .unwrap();
        match kind {
            "CODE" => e.request.code_artifact_id = Some(id),
            "PARAMETERS" => e.request.parameter_artifact_id = id,
            _ => e.request.proposal_artifact_id = id,
        }
        let bytes = browser(
            &f,
            &cookie,
            "GET",
            &format!("/api/v2/artifacts/{id}/content"),
            "unused",
            Value::Null,
        )
        .await;
        assert_eq!(bytes.status, StatusCode::OK);
        assert_eq!(
            bytes.headers[header::CONTENT_TYPE],
            "application/octet-stream"
        );
    }
    (f, cookie, e)
}

#[sqlx::test(migrations = "../../migrations")]
async fn browser_proposes_lists_and_replays_real_uploaded_artifact_references(pool: PgPool) {
    let (f, cookie, fixture) = setup(&pool).await;
    let body = serde_json::to_value(&fixture.request).unwrap();
    let created = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "proposal",
        body.clone(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.headers[header::CACHE_CONTROL], "no-store");
    let resource: ExperimentView =
        serde_json::from_value(created.body["resource"].clone()).unwrap();
    assert_eq!(
        resource.proposal_artifact_id,
        fixture.request.proposal_artifact_id
    );
    assert!(resource.run_id.is_none());
    let get = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/experiments/{}", resource.id),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(get.status, StatusCode::OK);
    assert_eq!(get.body, created.body["resource"]);
    let listing = browser(
        &f,
        &cookie,
        "GET",
        &format!(
            "/api/v2/experiments?project_id={}&limit=1",
            fixture.data.project
        ),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(listing.status, StatusCode::OK);
    assert_eq!(listing.body["items"][0], get.body);
    let replay = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "proposal",
        body.clone(),
    )
    .await;
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], created.body["resource"]);
    let mut invalid = body.clone();
    invalid["outcome"] = json!("SUPPORTED");
    let denied = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "fake-pass",
        invalid,
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut different = body.clone();
    different["hypothesis"] = json!("Changed premise");
    assert_eq!(
        browser(
            &f,
            &cookie,
            "POST",
            "/api/v2/experiments",
            "proposal",
            different
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    for key in ["second", "third"] {
        assert_eq!(
            browser(
                &f,
                &cookie,
                "POST",
                "/api/v2/experiments",
                key,
                body.clone()
            )
            .await
            .status,
            StatusCode::CREATED
        );
    }
    let exhausted = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "exhausted",
        body,
    )
    .await;
    assert_eq!(exhausted.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(exhausted.body["code"], "BUDGET_EXHAUSTED");
    assert_eq!(exhausted.body["retryable"], false);
    assert_eq!(exhausted.body["field_errors"][0]["field"], "experiments");
    assert!(!exhausted.headers.contains_key(header::RETRY_AFTER));
}

#[sqlx::test(migrations = "../../migrations")]
async fn machine_requires_scoped_submission_without_acquiring_operator_grant(pool: PgPool) {
    let (f, cookie, fixture) = setup(&pool).await;
    let body = serde_json::to_value(&fixture.request).unwrap();
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/experiments",
            body.clone(),
            &[
                ("origin", "https://research.example"),
                ("idempotency-key", "anon"),
            ]
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/experiments",
            body.clone(),
            &[("cookie", &cookie), ("idempotency-key", "origin-missing")]
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    for (key, scopes, expected) in [
        ("reader", json!(["RESEARCH_READ"]), StatusCode::FORBIDDEN),
        (
            "author",
            json!(["RESEARCH_READ", "EXPERIMENT_SUBMIT"]),
            StatusCode::CREATED,
        ),
    ] {
        let principal = browser(&f, &cookie, "POST", "/api/v2/machine-principals", key,
            json!({"schema_version":1,"name":key,"kind":"CLI","project_id":fixture.data.project,"downstream_id":null,"enabled":true})).await;
        assert_eq!(principal.status, StatusCode::CREATED, "{}", principal.body);
        let id = principal.body["resource"]["id"].as_str().unwrap();
        let credential = browser(&f, &cookie, "POST", &format!("/api/v2/machine-principals/{id}/credentials"), key,
            json!({"schema_version":1,"scope_codes":scopes,"expires_at":Utc::now()+Duration::hours(1)})).await;
        assert_eq!(
            credential.status,
            StatusCode::CREATED,
            "{}",
            credential.body
        );
        let bearer = format!("Bearer {}", credential.body["token"].as_str().unwrap());
        let reply = send(
            &f,
            "POST",
            "/api/v2/experiments",
            body.clone(),
            &[("authorization", &bearer), ("idempotency-key", key)],
        )
        .await;
        assert_eq!(reply.status, expected, "{}", reply.body);
    }
}

#[test]
fn the_actual_http_contract_declares_proposal_capability_without_a_human_grant() {
    let document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let path = &document["paths"]["/api/v2/experiments"]["post"];
    assert_eq!(path["operationId"], "propose_experiment");
    assert_eq!(
        path["security"],
        json!([{"BrowserSession":[]},{"MachineBearer":[]}])
    );
    assert!(path["responses"]["201"]["content"]["application/json"].is_object());
    assert!(path["responses"]["429"]["content"]["application/problem+json"].is_object());
    let schema = &document["components"]["schemas"]["ExperimentProposalV1"];
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("outcome").is_none());
    assert!(schema["properties"].get("author_run_id").is_none());
}
