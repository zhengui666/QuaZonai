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
    allowed: DataUse,
) -> (Fixture, String, research_support::ResearchFixture) {
    let f = fixture(pool.clone()).await;
    let r = local_session(&f).await;
    assert_eq!(r.status, StatusCode::OK);
    let login: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let data = research_support::setup_with_use(
        &pool,
        &f.store,
        &Actor::Browser {
            login_id: login.try_into().unwrap(),
        },
        allowed,
    )
    .await;
    (f, r.cookie.unwrap(), data)
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
        .header(header::HOST, "localhost");
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
            ("origin", "https://localhost"),
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
    let (f, cookie, data) = authenticated(pool, DataUse::ResearchAndPaper).await;
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
    let mut request = serde_json::to_value(data.policy(input.header.id)).unwrap();
    let mut portfolio_requirement = request["metric_requirements"][0].clone();
    portfolio_requirement["metric_code"] = json!("PORTFOLIO_DAILY_RETURN_MEAN");
    portfolio_requirement["scope"] = json!("portfolio");
    portfolio_requirement["method_allowlist"] = json!(["nautilus-analysis.ReturnsAverage"]);
    request["portfolio_metric_requirements"] = json!([portfolio_requirement]);
    let study_input = browser(
        &f,
        &cookie,
        "study-input",
        "POST",
        "/api/v2/input-sets",
        serde_json::to_value(data.input(InputPurpose::Portfolio)).unwrap(),
    )
    .await;
    assert_eq!(
        study_input.status,
        StatusCode::CREATED,
        "{}",
        study_input.body
    );
    request["portfolio_study_plan"] = json!({"schema_version":1,
        "input_set_id":study_input.body["resource"]["header"]["id"],
        "evaluation_start":"2019-01-01T00:00:00.000001Z",
        "manual_cutoffs":["2019-01-01T00:00:00.000001Z","2019-01-02T00:00:00.000001Z"]});
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
    assert_eq!(
        policy.body["resource"]["portfolio_study_plan"],
        request["portfolio_study_plan"]
    );
    assert_eq!(
        policy.body["resource"]["portfolio_metric_requirements"],
        request["portfolio_metric_requirements"]
    );
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
            format!("/api/v2/input-sets?project_id={}&limit=2", data.project),
            study_input.body["resource"]["header"]["id"].clone(),
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
        if path.starts_with("/api/v2/input-sets?") {
            assert_eq!(r.body["items"].as_array().unwrap().len(), 2);
            assert_eq!(r.body["items"][1]["id"], json!(input.header.id));
        }
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
async fn research_pages_are_complete_project_scoped_and_preserve_original_receipts(pool: PgPool) {
    let (f, cookie, data) = authenticated(pool.clone(), DataUse::Research).await;
    let (other, other_cookie, other_data) = authenticated(pool.clone(), DataUse::Research).await;
    let projects = [(&f, &cookie, &data), (&other, &other_cookie, &other_data)];
    let mut input_ids = [Vec::new(), Vec::new()];
    let mut policy_ids = [Vec::new(), Vec::new()];
    let mut originals = Vec::new();
    // Interleave projects so a foreign cursor falls inside the other project's history.
    for round in 0..3 {
        for (index, (f, cookie, data)) in projects.iter().enumerate() {
            let key = format!("input-{index}-{round}");
            let request = serde_json::to_value(data.input(InputPurpose::Validation)).unwrap();
            let input = browser(
                f,
                cookie,
                &key,
                "POST",
                "/api/v2/input-sets",
                request.clone(),
            )
            .await;
            assert_eq!(input.status, StatusCode::CREATED, "{}", input.body);
            assert_eq!(input.body["replayed"], false);
            let view: InputSetView =
                serde_json::from_value(input.body["resource"].clone()).unwrap();
            input_ids[index].push(view.header.id.to_string());
            if index == 0 && round == 0 {
                originals.push(("/api/v2/input-sets", key, request, input.body));
            }
            let key = format!("policy-{index}-{round}");
            let mut request = serde_json::to_value(data.policy(view.header.id)).unwrap();
            request["question"] = json!(format!("pagination round {round}"));
            let policy = browser(
                f,
                cookie,
                &key,
                "POST",
                "/api/v2/evaluation-policies",
                request.clone(),
            )
            .await;
            assert_eq!(policy.status, StatusCode::CREATED, "{}", policy.body);
            assert_eq!(policy.body["replayed"], false);
            assert_eq!(policy.body["resource"]["version"], round + 1);
            policy_ids[index].push(policy.body["resource"]["id"].as_str().unwrap().to_owned());
            if index == 0 && round == 0 {
                originals.push(("/api/v2/evaluation-policies", key, request, policy.body));
            }
        }
    }
    let bearer = credential(&f, &cookie, data.project, "AUTOMATION").await;
    for (endpoint, mut ids) in [
        ("/api/v2/input-sets", input_ids),
        ("/api/v2/evaluation-policies", policy_ids),
    ] {
        for (index, (f, cookie, data)) in projects.iter().enumerate() {
            let expected = &mut ids[index];
            expected.sort_unstable_by(|a, b| b.cmp(a));
            let mut path = format!("{endpoint}?project_id={}&limit=1", data.project);
            let mut seen = Vec::new();
            for (page, id) in expected.iter().enumerate() {
                let r = browser(f, cookie, "unused", "GET", &path, Value::Null).await;
                assert_eq!(r.status, StatusCode::OK, "{}", r.body);
                assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
                assert_eq!(r.body["schema_version"], 1);
                let items = r.body["items"].as_array().unwrap();
                assert_eq!(items.len(), 1, "{}", r.body);
                assert_eq!(items[0]["project_id"], json!(data.project));
                let actual = items[0]["id"].as_str().unwrap().to_owned();
                assert_eq!(&actual, id);
                assert!(!seen.contains(&actual), "duplicate page item: {actual}");
                seen.push(actual);
                let next = r.body.get("next_cursor").unwrap();
                if page + 1 < expected.len() {
                    let cursor = next.as_str().unwrap();
                    assert_eq!(cursor, id);
                    path = format!(
                        "{endpoint}?project_id={}&limit=1&cursor={cursor}",
                        data.project
                    );
                } else {
                    assert!(next.is_null(), "{}", r.body);
                }
                for absent in [
                    "storage_object_ref",
                    "verifier_ref",
                    "native_snapshot_ref",
                    "native-fixture",
                    "fixture-not-a-secret",
                ] {
                    assert!(!r.body.to_string().contains(absent));
                }
            }
            assert_eq!(seen, *expected);
            let exhausted = browser(
                f,
                cookie,
                "unused",
                "GET",
                &format!(
                    "{endpoint}?project_id={}&limit=1&cursor={}",
                    data.project,
                    expected.last().unwrap()
                ),
                Value::Null,
            )
            .await;
            assert_eq!(exhausted.status, StatusCode::OK, "{}", exhausted.body);
            assert_eq!(exhausted.body["items"], json!([]));
            assert!(exhausted.body.get("next_cursor").unwrap().is_null());
        }
        // A cursor is a UUID boundary, not authority to read its originating project.
        for index in 0..2 {
            let (f, cookie, data) = projects[index];
            let foreign_cursor = &ids[1 - index][0];
            let path = format!(
                "{endpoint}?project_id={}&limit=100&cursor={foreign_cursor}",
                data.project
            );
            let r = browser(f, cookie, "unused", "GET", &path, Value::Null).await;
            assert_eq!(r.status, StatusCode::OK, "{}", r.body);
            let expected: Vec<_> = ids[index]
                .iter()
                .filter(|id| *id < foreign_cursor)
                .cloned()
                .collect();
            assert!(
                !expected.is_empty(),
                "foreign cursor must exercise populated pages"
            );
            let actual: Vec<_> = r.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    assert_eq!(item["project_id"], json!(data.project));
                    item["id"].as_str().unwrap().to_owned()
                })
                .collect();
            assert_eq!(actual, expected);
            assert!(r.body.get("next_cursor").unwrap().is_null());
            let scoped = send(
                projects[0].0,
                "GET",
                &path,
                Value::Null,
                &[("authorization", &bearer)],
            )
            .await;
            if index == 0 {
                assert_eq!(scoped.status, StatusCode::OK, "{}", scoped.body);
                assert_eq!(scoped.body, r.body);
            } else {
                assert_eq!(scoped.status, StatusCode::NOT_FOUND, "{}", scoped.body);
                assert_eq!(scoped.body["code"], "NOT_FOUND");
                assert_eq!(
                    scoped.headers[header::CONTENT_TYPE],
                    "application/problem+json"
                );
                assert!(scoped.body.get("items").is_none());
                assert!(scoped.body.get("next_cursor").is_none());
            }
        }
    }
    for (endpoint, key, request, mut original) in originals {
        let replay = browser(&f, &cookie, &key, "POST", endpoint, request).await;
        assert_eq!(replay.status, StatusCode::CREATED, "{}", replay.body);
        original["replayed"] = json!(true);
        assert_eq!(replay.body, original);
    }
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.input_sets),(SELECT count(*) FROM app.evaluation_policies)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (6, 6),
        "original retries must not publish extra records"
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn real_bearer_can_read_only_its_metadata_and_not_publish_or_change_sealed_access(
    pool: PgPool,
) {
    let (f, cookie, data) = authenticated(pool.clone(), DataUse::Research).await;
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
    let (f, cookie, data) = authenticated(pool.clone(), DataUse::Research).await;
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
        assert_eq!(r.body["code"], "AUTH_REQUIRED");
        assert!(r.cookie.is_none());
        let authenticated = browser(
            &f,
            &cookie,
            "query",
            "GET",
            &format!("{path}?project_id={}", data.project),
            Value::Null,
        )
        .await;
        assert_eq!(authenticated.status, StatusCode::OK);
        assert_eq!(authenticated.body["items"], json!([]));
        for tail in [
            "&limit=0",
            "&limit=101",
            "&limit=65536",
            "&limit=-1",
            "&limit=1.5",
            "&limit=true",
            "&limit=invalid",
            "&limit=",
            "&limit=1&limit=2",
            "&cursor=invalid",
            "&cursor=123",
            "&cursor=",
            "&cursor=null",
            "&cursor=00000000-0000-4000-8000-000000000000",
            "&unknown=1",
        ] {
            let r = browser(
                &f,
                &cookie,
                "query",
                "GET",
                &format!("{path}?project_id={}{tail}", data.project),
                Value::Null,
            )
            .await;
            assert_eq!(
                r.status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{path}{tail}: {}",
                r.body
            );
            assert_eq!(r.headers[header::CONTENT_TYPE], "application/problem+json");
            assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
            let problem: contracts::http::Problem = serde_json::from_value(r.body).unwrap();
            assert_eq!(problem.status, 422);
            assert_eq!(problem.code, "VALIDATION_ERROR");
            assert_eq!(problem.title, problem.code);
            assert_eq!(problem.kind, "urn:quazonai:problem:validation-error");
            assert!(!problem.retryable);
            assert_eq!(
                r.headers["x-request-id"].to_str().unwrap(),
                problem.request_id.to_string()
            );
            if matches!(tail, "&limit=0" | "&limit=101") {
                assert_eq!(problem.field_errors.len(), 1);
                assert_eq!(problem.field_errors[0].field, "limit");
                assert_eq!(problem.field_errors[0].code, "PAGE_SIZE");
            } else {
                assert!(problem.field_errors.is_empty());
            }
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
async fn native_cli_local_grant_can_publish_only_the_exact_research_request(pool: PgPool) {
    let (f, cookie, data) = authenticated(pool.clone(), DataUse::Research).await;
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
    let grant=send(&f,"POST","/api/v2/auth/operator-command-grants",json!({"schema_version":1,"command":{"operation":"EVALUATION_POLICY_CREATE","request":request},"target_id":null}),&[("authorization",&bearer),("idempotency-key","grant")]).await;
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
    let (f, cookie, data) = authenticated(pool.clone(), DataUse::Research).await;
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

#[sqlx::test(migrations = "../../migrations")]
async fn review_errors_return_the_actual_license_and_threshold_fields(pool: PgPool) {
    let (f, cookie, data) = authenticated(pool.clone(), DataUse::Research).await;
    let denied = browser(
        &f,
        &cookie,
        "denied-use",
        "POST",
        "/api/v2/input-sets",
        serde_json::to_value(data.input(InputPurpose::Portfolio)).unwrap(),
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        denied.body["field_errors"][0]["field"],
        "items.0.dataset_revision_id"
    );
    assert_eq!(
        denied.body["field_errors"][0]["code"],
        "DATA_USE_PURPOSE_NOT_AUTHORIZED"
    );
    let r = browser(
        &f,
        &cookie,
        "comparison",
        "POST",
        "/api/v2/input-sets",
        serde_json::to_value(data.input(InputPurpose::Validation)).unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let input: InputSetView = serde_json::from_value(r.body["resource"].clone()).unwrap();
    let base = serde_json::to_value(data.policy(input.header.id)).unwrap();
    for (cmp, low, high, expected) in [
        (
            "LT",
            Value::Null,
            Value::Null,
            vec!["metric_requirements.0.threshold_high"],
        ),
        (
            "LE",
            Value::Null,
            Value::Null,
            vec!["metric_requirements.0.threshold_high"],
        ),
        (
            "BETWEEN",
            json!("2"),
            json!("1"),
            vec![
                "metric_requirements.0.threshold_low",
                "metric_requirements.0.threshold_high",
            ],
        ),
    ] {
        let mut body = base.clone();
        body["metric_requirements"][0]["comparator"] = json!(cmp);
        body["metric_requirements"][0]["threshold_low"] = low;
        body["metric_requirements"][0]["threshold_high"] = high;
        let r = browser(
            &f,
            &cookie,
            cmp,
            "POST",
            "/api/v2/evaluation-policies",
            body,
        )
        .await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            r.body["field_errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["field"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.command_receipts WHERE operation='EVALUATION_POLICY_CREATE'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}
