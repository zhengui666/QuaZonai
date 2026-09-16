//! Real native browser crypto/session middleware and transactional Brief commands.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use contracts::{brief::*, Id};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::authority::Actor;
use support::*;
async fn setup(
    pool: &PgPool,
) -> (
    Fixture,
    String,
    totp_rs::TOTP,
    research_support::ResearchFixture,
    BriefCreateIntent,
) {
    let f = fixture(pool.clone()).await;
    let (e, c, native) = start(&f).await;
    let (r, _) = confirm(&f, &e, &c, &native, true).await;
    assert_eq!(r.status, StatusCode::OK);
    let login: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let actor = Actor::Browser {
        login_id: login.try_into().unwrap(),
    };
    let data = research_support::setup(pool, &f.store, &actor).await;
    let request = brief_support::request(&f.store, &actor, &data).await;
    (f, r.cookie.unwrap(), native, data, request)
}
async fn send(
    f: &Fixture,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> Reply {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "research.example");
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        b = b.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    };
    exchange(&f.app, b.body(body).unwrap()).await
}
async fn browser(
    f: &Fixture,
    cookie: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Value,
) -> Reply {
    send(
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
async fn token(f: &Fixture, cookie: &str, project: Id) -> String {
    let r=browser(f,cookie,"principal","POST","/api/v2/machine-principals",json!({"schema_version":1,"name":"human CLI","kind":"CLI","project_id":project,"downstream_id":null,"enabled":true})).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    let id = r.body["resource"]["id"].as_str().unwrap();
    let c=browser(f,cookie,"token","POST",&format!("/api/v2/machine-principals/{id}/credentials"),json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":Utc::now()+Duration::hours(1)})).await;
    assert_eq!(c.status, StatusCode::CREATED, "{}", c.body);
    format!("Bearer {}", c.body["token"].as_str().unwrap())
}
#[sqlx::test(migrations = "../../migrations")]
async fn browser_saves_edits_and_reads_a_real_draft_with_original_retries(pool: PgPool) {
    let (f, cookie, _, data, request) = setup(&pool).await;
    let path = format!("/api/v2/projects/{}/briefs", data.project);
    let body = serde_json::to_value(&request.request).unwrap();
    let created = browser(&f, &cookie, "create", "POST", &path, body.clone()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.headers[header::CACHE_CONTROL], "no-store");
    let first: BriefView = serde_json::from_value(created.body["resource"].clone()).unwrap();
    assert_eq!(first.state, BriefState::Draft);
    assert!(first.frozen_at.is_none());
    let item = format!("/api/v2/briefs/{}", first.id);
    let mut patch = json!({"schema_version":1,"expected_revision":first.revision,"content":first.content,"bindings":first.bindings});
    patch["content"]["hypothesis"] = json!("An updated bounded hypothesis.");
    let updated = browser(&f, &cookie, "patch", "PATCH", &item, patch.clone()).await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
    assert_eq!(updated.body["resource"]["revision"], "2");
    let read = browser(&f, &cookie, "unused", "GET", &item, Value::Null).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body, updated.body["resource"]);
    let retry = browser(&f, &cookie, "create", "POST", &path, body).await;
    assert_eq!(retry.body["resource"], created.body["resource"]);
    assert_eq!(retry.body["replayed"], true);
    let conflict = browser(&f, &cookie, "another", "PATCH", &item, patch).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    let listing = browser(
        &f,
        &cookie,
        "unused",
        "GET",
        &format!("{path}?limit=1"),
        Value::Null,
    )
    .await;
    assert_eq!(listing.status, StatusCode::OK);
    assert_eq!(listing.body["items"][0], read.body);
    for secret in [
        "secret_ref",
        "native_snapshot_ref",
        "verifier_ref",
        "credential_ref",
    ] {
        assert!(!read.body.to_string().contains(secret));
    }
}
#[sqlx::test(migrations = "../../migrations")]
async fn human_cli_grant_binds_full_intent_and_path_project(pool: PgPool) {
    let (f, cookie, native, data, request) = setup(&pool).await;
    let bearer = token(&f, &cookie, data.project).await;
    let path = format!("/api/v2/projects/{}/briefs", data.project);
    let body = serde_json::to_value(&request.request).unwrap();
    let denied = send(
        &f,
        "POST",
        &path,
        body.clone(),
        &[("authorization", &bearer), ("idempotency-key", "create")],
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let grant=send(&f,"POST","/api/v2/auth/operator-command-grants",json!({"schema_version":1,"command":{"operation":"BRIEF_CREATE","request":request},"target_id":null,"code":native.generate((now/30+1)*30)}),&[("authorization",&bearer),("idempotency-key","grant")]).await;
    assert_eq!(grant.status, StatusCode::CREATED, "{}", grant.body);
    let id = grant.body["resource"]["id"].as_str().unwrap();
    let headers = [
        ("authorization", bearer.as_str()),
        ("x-operator-grant", id),
        ("idempotency-key", "create"),
    ];
    let wrong = send(
        &f,
        "POST",
        &format!("/api/v2/projects/{}/briefs", Id::new()),
        body.clone(),
        &headers,
    )
    .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    let mut substituted = body.clone();
    substituted["content"]["hypothesis"] = json!("not approved");
    assert_eq!(
        send(&f, "POST", &path, substituted, &headers).await.status,
        StatusCode::FORBIDDEN
    );
    let created = send(&f, "POST", &path, body.clone(), &headers).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        created.body["resource"]["id"],
        grant.body["resource"]["target_id"]
    );
    let retry = send(&f, "POST", &path, body.clone(), &headers).await;
    assert_eq!(retry.body["replayed"], true);
    assert_eq!(retry.body["resource"], created.body["resource"]);
    let again = send(
        &f,
        "POST",
        &path,
        body,
        &[
            ("authorization", &bearer),
            ("x-operator-grant", id),
            ("idempotency-key", "duplicate"),
        ],
    )
    .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM app.research_briefs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}
#[sqlx::test(migrations = "../../migrations")]
async fn authoring_rejects_unknown_fields_permission_laundering_and_bad_references(pool: PgPool) {
    let (f, cookie, _, data, request) = setup(&pool).await;
    let path = format!("/api/v2/projects/{}/briefs", data.project);
    let valid = serde_json::to_value(&request.request).unwrap();
    for (pointer, value) in [
        ("/content/base_currency", json!("ZZZ")),
        ("/content/horizon_value", json!("0")),
        ("/content/evaluation_policy_id", json!(Id::new())),
        ("/bindings/1/access_policy", json!("RESEARCH_READ")),
    ] {
        let mut body = valid.clone();
        *body.pointer_mut(pointer).unwrap() = value;
        let r = browser(&f, &cookie, "invalid", "POST", &path, body).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{pointer}:{}",
            r.body
        );
    }
    let mut bad = valid.clone();
    bad["state"] = json!("FROZEN");
    assert_eq!(
        browser(&f, &cookie, "invalid", "POST", &path, bad)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut bad = valid;
    bad["content"]["provider_key"] = json!("not allowed");
    assert_eq!(
        browser(&f, &cookie, "invalid", "POST", &path, bad)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='BRIEF_CREATE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}
#[sqlx::test(migrations = "../../migrations")]
async fn stale_browser_is_read_only_and_cross_origin_cannot_author_brief(pool: PgPool) {
    let (f, cookie, _, data, request) = setup(&pool).await;
    let path = format!("/api/v2/projects/{}/briefs", data.project);
    let body = serde_json::to_value(&request.request).unwrap();
    let csrf = send(
        &f,
        "POST",
        &path,
        body.clone(),
        &[
            ("cookie", &cookie),
            ("origin", "https://untrusted.example"),
            ("idempotency-key", "csrf"),
        ],
    )
    .await;
    assert_eq!(csrf.status, StatusCode::FORBIDDEN);
    // A valid historical authority fixture. Do not disable the monotonic
    // production trigger, forge cookie bytes, or bypass the HTTP extractor.
    let historical = Id::new();
    sqlx::query("INSERT INTO app.browser_logins(id,created_at,auth_epoch,authenticated_at,expires_at,device_id) SELECT $1,clock_timestamp()-interval '301 seconds',auth_epoch,clock_timestamp()-interval '301 seconds',clock_timestamp()+interval '1 hour',device_id FROM app.browser_logins ORDER BY created_at DESC LIMIT 1")
        .bind(historical.as_uuid()).execute(&pool).await.unwrap();
    use tower_sessions::SessionStore;
    let sessions = tower_sessions_sqlx_store::PostgresStore::new(pool.clone());
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM tower_sessions.session")
        .fetch_all(&pool)
        .await
        .unwrap();
    let mut rebound = 0;
    for id in ids {
        let mut record = sessions.load(&id.parse().unwrap()).await.unwrap().unwrap();
        if record.data.contains_key("operator_login") {
            record
                .data
                .insert("operator_login".into(), json!(historical));
            sessions.save(&record).await.unwrap();
            rebound += 1;
        }
    }
    assert_eq!(rebound, 1);
    let read = browser(&f, &cookie, "unused", "GET", &path, Value::Null).await;
    assert_eq!(read.status, StatusCode::OK);
    let stale = browser(&f, &cookie, "stale", "POST", &path, body).await;
    assert!(
        matches!(
            stale.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ),
        "{}",
        stale.body
    );
}
