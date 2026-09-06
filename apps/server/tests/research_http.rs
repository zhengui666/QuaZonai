//! Real HTTP authentication, PostgreSQL publication and original command receipts.
#[path = "../../../tests/support/research.rs"]
mod research_support;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use contracts::{research::*, Id};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::authority::Actor;
use support::*;

async fn authenticated(
    pool: PgPool,
) -> (
    Fixture,
    String,
    totp_rs::TOTP,
    research_support::ResearchFixture,
) {
    let f = fixture(pool.clone()).await;
    let (e, c, native) = start(&f).await;
    let (r, _) = confirm(&f, &e, &c, &native, true).await;
    assert_eq!(r.status, StatusCode::OK);
    let login: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let data = research_support::setup(
        &pool,
        &f.store,
        &Actor::Browser {
            login_id: login.try_into().unwrap(),
        },
    )
    .await;
    (f, r.cookie.unwrap(), native, data)
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
async fn credential(f: &Fixture, cookie: &str, project: Id, kind: &str) -> String {
    let r=browser(f,cookie,&Id::new().to_string(),"POST","/api/v2/machine-principals",json!({"schema_version":1,"name":"reader","kind":kind,"project_id":project,"downstream_id":null,"enabled":true})).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    let p = r.body["resource"]["id"].as_str().unwrap();
    let c=browser(f,cookie,&Id::new().to_string(),"POST",&format!("/api/v2/machine-principals/{p}/credentials"),json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":Utc::now()+Duration::hours(1)})).await;
    assert_eq!(c.status, StatusCode::CREATED, "{}", c.body);
    format!("Bearer {}", c.body["token"].as_str().unwrap())
}
#[sqlx::test(migrations = "../../migrations")]
async fn real_browser_prepares_input_and_policy_with_exact_public_retries_and_metadata(
    pool: PgPool,
) {
    let (f, cookie, _, data) = authenticated(pool).await;
    let payload = serde_json::to_value(data.input(InputPurpose::Validation)).unwrap();
    let first = browser(
        &f,
        &cookie,
        "input",
        "POST",
        "/api/v2/input-sets",
        payload.clone(),
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(first.headers[header::CACHE_CONTROL], "no-store");
    let again = browser(
        &f,
        &cookie,
        "input",
        "POST",
        "/api/v2/input-sets",
        payload.clone(),
    )
    .await;
    assert_eq!(again.status, StatusCode::CREATED);
    assert_eq!(again.body["replayed"], true);
    assert_eq!(again.body["resource"], first.body["resource"]);
    let input: InputSetView = serde_json::from_value(first.body["resource"].clone()).unwrap();
    assert_eq!(input.items[0].origin, DataOrigin::Fixture);
    assert_eq!(input.items[0].pit_status, Some(PitStatus::Unverified));
    assert_eq!(input.header.revision.get(), 2);
    let get = browser(
        &f,
        &cookie,
        "unused",
        "GET",
        &format!("/api/v2/input-sets/{}", input.header.id),
        Value::Null,
    )
    .await;
    assert_eq!(get.status, StatusCode::OK);
    assert_eq!(get.body, first.body["resource"]);
    let request = serde_json::to_value(data.policy(input.header.id)).unwrap();
    let policy = browser(
        &f,
        &cookie,
        "policy",
        "POST",
        "/api/v2/evaluation-policies",
        request.clone(),
    )
    .await;
    assert_eq!(policy.status, StatusCode::CREATED, "{}", policy.body);
    let replay = browser(
        &f,
        &cookie,
        "policy",
        "POST",
        "/api/v2/evaluation-policies",
        request.clone(),
    )
    .await;
    assert_eq!(replay.body["resource"], policy.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
    for (path, expected) in [
        (
            format!("/api/v2/input-sets?project_id={}&limit=1", data.project),
            json!(input.header.id),
        ),
        (
            format!(
                "/api/v2/evaluation-policies?project_id={}&limit=1",
                data.project
            ),
            policy.body["resource"]["id"].clone(),
        ),
    ] {
        let r = browser(&f, &cookie, "unused", "GET", &path, Value::Null).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        assert_eq!(r.body["items"][0]["id"], expected);
        assert!(r.body["next_cursor"].is_null());
        for absent in [
            "storage_object_ref",
            "verifier_ref",
            "native_snapshot_ref",
            "fixture-not-a-secret",
        ] {
            assert!(!r.body.to_string().contains(absent));
        }
    }
    let mut changed = request;
    changed["question"] = json!("different request");
    let conflict = browser(
        &f,
        &cookie,
        "policy",
        "POST",
        "/api/v2/evaluation-policies",
        changed,
    )
    .await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    assert_eq!(conflict.body["code"], "IDEMPOTENCY_CONFLICT");
}
#[sqlx::test(migrations = "../../migrations")]
async fn real_bearer_can_read_only_its_metadata_and_not_publish_or_change_sealed_access(
    pool: PgPool,
) {
    let (f, cookie, _, data) = authenticated(pool.clone()).await;
    let p = browser(
        &f,
        &cookie,
        "sealed",
        "POST",
        "/api/v2/input-sets",
        serde_json::to_value(data.input(InputPurpose::Sealed)).unwrap(),
    )
    .await;
    assert_eq!(p.status, StatusCode::CREATED, "{}", p.body);
    let id = p.body["resource"]["header"]["id"].as_str().unwrap();
    let bearer = credential(&f, &cookie, data.project, "AUTOMATION").await;
    let metadata = send(
        &f,
        "GET",
        &format!("/api/v2/input-sets/{id}"),
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(metadata.status, StatusCode::OK);
    assert_eq!(metadata.body["items"][0]["item"]["role"], "SEALED");
    assert!(!metadata.body.to_string().contains("native-fixture"));
    let new = send(
        &f,
        "POST",
        "/api/v2/input-sets",
        serde_json::to_value(data.input(InputPurpose::Discovery)).unwrap(),
        &[
            ("authorization", &bearer),
            ("idempotency-key", "no-machine-create"),
        ],
    )
    .await;
    assert_eq!(new.status, StatusCode::FORBIDDEN);
    let other = Id::new();
    for path in [
        format!("/api/v2/input-sets?project_id={other}"),
        format!("/api/v2/evaluation-policies?project_id={other}"),
    ] {
        let r = send(&f, "GET", &path, Value::Null, &[("authorization", &bearer)]).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
    }
    // The selector still refuses Cookie/Bearer combination; it cannot launder a machine into Operator.
    let r = send(
        &f,
        "GET",
        &format!("/api/v2/input-sets/{id}"),
        Value::Null,
        &[("authorization", &bearer), ("cookie", &cookie)],
    )
    .await;
    assert!(matches!(
        r.status,
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.input_sets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
#[sqlx::test(migrations = "../../migrations")]
async fn research_field_errors_are_safe_bounded_and_native_auth_is_not_optional(pool: PgPool) {
    let (f, cookie, _, data) = authenticated(pool.clone()).await;
    let mut request = serde_json::to_value(data.input(InputPurpose::Validation)).unwrap();
    request["items"][1]["role"] = json!("SIGNALS");
    let r = browser(
        &f,
        &cookie,
        "bad-role",
        "POST",
        "/api/v2/input-sets",
        request.clone(),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["field_errors"][0]["field"], "items.1.artifact_id");
    assert_eq!(r.body["field_errors"][0]["code"], "ARTIFACT_BINDING");
    assert!(!r.body.to_string().contains("SELECT"));
    assert!(!r.body.to_string().contains("app.artifacts"));
    request["items"][0]["artifact_id"] = json!(data.artifact);
    let r = browser(&f, &cookie, "mixed", "POST", "/api/v2/input-sets", request).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    for path in ["/api/v2/input-sets", "/api/v2/evaluation-policies"] {
        let r = send(
            &f,
            "GET",
            &format!("{path}?project_id={}", data.project),
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        for tail in ["&limit=0", "&limit=101", "&limit=65536", "&unknown=1"] {
            let r = browser(
                &f,
                &cookie,
                "query",
                "GET",
                &format!("{path}?project_id={}{tail}", data.project),
                Value::Null,
            )
            .await;
            assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    let r = send(
        &f,
        "POST",
        "/api/v2/input-sets",
        serde_json::to_value(data.input(InputPurpose::Validation)).unwrap(),
        &[("cookie", &cookie), ("idempotency-key", "csrf")],
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.input_sets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[sqlx::test(migrations = "../../migrations")]
async fn native_cli_totp_grant_can_publish_only_the_exact_research_request(pool: PgPool) {
    let (f, cookie, native, data) = authenticated(pool.clone()).await;
    let bearer = credential(&f, &cookie, data.project, "CLI").await;
    let input = browser(
        &f,
        &cookie,
        "input",
        "POST",
        "/api/v2/input-sets",
        serde_json::to_value(data.input(InputPurpose::Validation)).unwrap(),
    )
    .await;
    assert_eq!(input.status, StatusCode::CREATED, "{}", input.body);
    let id: Id = input.body["resource"]["header"]["id"]
        .as_str()
        .unwrap()
        .to_string()
        .try_into()
        .unwrap();
    let mut request = serde_json::to_value(data.policy(id)).unwrap();
    // A legitimate policy above the ordinary 16 KiB auth body limit must still fit
    // its human-grant envelope. No secret is stored in that normalized request.
    let metric = request["metric_requirements"][0].clone();
    for index in 1..64 {
        let mut m = metric.clone();
        m["scope"] = json!(format!("fold:{index}:{}", "x".repeat(100)));
        request["metric_requirements"]
            .as_array_mut()
            .unwrap()
            .push(m);
    }
    assert!(serde_json::to_vec(&request).unwrap().len() > 16 * 1024);
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let grant=send(&f,"POST","/api/v2/auth/operator-command-grants",json!({"schema_version":1,"command":{"operation":"EVALUATION_POLICY_CREATE","request":request},"target_id":null,"code":native.generate((now/30+1)*30)}),&[("authorization",&bearer),("idempotency-key","grant")]).await;
    assert_eq!(grant.status, StatusCode::CREATED, "{}", grant.body);
    let g = grant.body["resource"]["id"].as_str().unwrap();
    let mut changed = request.clone();
    changed["question"] = json!("substitute");
    let denied = send(
        &f,
        "POST",
        "/api/v2/evaluation-policies",
        changed,
        &[
            ("authorization", &bearer),
            ("x-operator-grant", g),
            ("idempotency-key", "execute"),
        ],
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.body);
    let done = send(
        &f,
        "POST",
        "/api/v2/evaluation-policies",
        request.clone(),
        &[
            ("authorization", &bearer),
            ("x-operator-grant", g),
            ("idempotency-key", "execute"),
        ],
    )
    .await;
    assert_eq!(done.status, StatusCode::CREATED, "{}", done.body);
    assert_eq!(
        done.body["resource"]["id"],
        grant.body["resource"]["target_id"]
    );
    let replay = send(
        &f,
        "POST",
        "/api/v2/evaluation-policies",
        request.clone(),
        &[
            ("authorization", &bearer),
            ("x-operator-grant", g),
            ("idempotency-key", "execute"),
        ],
    )
    .await;
    assert_eq!(replay.body["resource"], done.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
    let repeated = send(
        &f,
        "POST",
        "/api/v2/evaluation-policies",
        request,
        &[
            ("authorization", &bearer),
            ("x-operator-grant", g),
            ("idempotency-key", "new-key"),
        ],
    )
    .await;
    assert_eq!(repeated.status, StatusCode::CONFLICT);
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.evaluation_policies),(SELECT count(*) FROM app.operator_command_consumptions)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn research_request_size_and_missing_project_fail_without_creating_records(pool: PgPool) {
    let (f, cookie, _, data) = authenticated(pool.clone()).await;
    let mut input = serde_json::to_value(data.input(InputPurpose::Validation)).unwrap();
    input["items"] = serde_json::Value::Array(vec![input["items"][0].clone(); 700]);
    assert!(serde_json::to_vec(&input).unwrap().len() > 64 * 1024);
    let response = browser(
        &f,
        &cookie,
        "too-large",
        "POST",
        "/api/v2/input-sets",
        input,
    )
    .await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.body["code"], "VALIDATION_ERROR");
    for path in ["/api/v2/input-sets", "/api/v2/evaluation-policies"] {
        let r = browser(
            &f,
            &cookie,
            "missing",
            "GET",
            &format!("{path}?project_id={}", Id::new()),
            Value::Null,
        )
        .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.body);
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM app.input_sets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}
