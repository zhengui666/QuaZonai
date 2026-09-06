//! Requests traverse the real private-cookie/Bearer selector, native crypto,
//! SecretVault and PostgreSQL business transactions; no authority is injected.
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use contracts::Id;
use serde_json::{json, Value};
use sqlx::PgPool;
use support::*;
use totp_rs::TOTP;

async fn authenticated(pool: PgPool) -> (Fixture, String, TOTP) {
    let f = fixture(pool).await;
    let (enrollment, anonymous, native) = start(&f).await;
    let (reply, _) = confirm(&f, &enrollment, &anonymous, &native, true).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let cookie = reply.cookie.unwrap();
    (f, cookie, native)
}
async fn command(
    f: &Fixture,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "research.example");
    for (k, v) in headers {
        builder = builder.header(*k, *v)
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    };
    exchange(&f.app, builder.body(body).unwrap()).await
}
async fn browser(
    f: &Fixture,
    cookie: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Value,
) -> Reply {
    command(
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
async fn create_project(f: &Fixture, cookie: &str, key: &str) -> Value {
    let response=browser(f,cookie,key,"POST","/api/v2/projects",json!({"schema_version":1,"name":key,"description":"Native HTTP research","fork_from_project_id":null})).await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    response.body["resource"].clone()
}
async fn credential(
    f: &Fixture,
    cookie: &str,
    project: Option<&Value>,
    kind: &str,
) -> (Value, String, Value) {
    let key = Id::new().to_string();
    let response=browser(f,cookie,&key,"POST","/api/v2/machine-principals",json!({"schema_version":1,"name":"Native machine","kind":kind,"project_id":project.map(|p|p["id"].clone()),"downstream_id":null,"enabled":true})).await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let principal = response.body["resource"].clone();
    let request = json!({"schema_version":1,"scope_codes":[if project.is_some(){"RESEARCH_READ"}else{"DOCTOR_READ"}],"expires_at":Utc::now()+Duration::hours(1)});
    let path = format!(
        "/api/v2/machine-principals/{}/credentials",
        principal["id"].as_str().unwrap()
    );
    let response = browser(f, cookie, &key, "POST", &path, request.clone()).await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    assert!(!response.body.to_string().contains("verifier"));
    let token = response.body["token"].as_str().unwrap().to_owned();
    let replay = browser(f, cookie, &key, "POST", &path, request).await;
    assert_eq!(replay.status, StatusCode::CREATED, "{}", replay.body);
    assert_eq!(replay.body["token"], Value::Null);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], response.body["resource"]);
    (response.body["resource"].clone(), token, principal)
}

#[sqlx::test(migrations = "../../migrations")]
async fn operator_project_commands_keep_original_receipts_cas_and_fork_lineage(pool: PgPool) {
    let (f, cookie, _) = authenticated(pool.clone()).await;
    let p = create_project(&f, &cookie, "project-first").await;
    let id = p["id"].as_str().unwrap();
    let changed=browser(&f,&cookie,"patch1","PATCH",&format!("/api/v2/projects/{id}"),json!({"schema_version":1,"expected_revision":"1","name":"Edited","description":"Updated","state":"PAUSED"})).await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["resource"]["revision"], "2");
    let replay = create_project(&f, &cookie, "project-first").await;
    assert_eq!(
        replay, p,
        "idempotent retry returns original, not current mutable state"
    );
    let conflict=browser(&f,&cookie,"patch2","PATCH",&format!("/api/v2/projects/{id}"),json!({"schema_version":1,"expected_revision":"1","name":"Stale","description":"","state":"PAUSED"})).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.body);
    assert_eq!(conflict.body["current_revision"], "2");
    let collision=browser(&f,&cookie,"project-first","POST","/api/v2/projects",json!({"schema_version":1,"name":"different","description":"Native HTTP research","fork_from_project_id":null})).await;
    assert_eq!(collision.status, StatusCode::CONFLICT);
    assert_eq!(collision.body["code"], "IDEMPOTENCY_CONFLICT");
    let fork = browser(
        &f,
        &cookie,
        "fork",
        "POST",
        "/api/v2/projects",
        json!({"schema_version":1,"name":"Fork","description":"","fork_from_project_id":id}),
    )
    .await;
    assert_eq!(fork.status, StatusCode::CREATED, "{}", fork.body);
    let parent: String = sqlx::query_scalar(
        "SELECT parent_lineage_id::text FROM app.research_lineages WHERE id=$1::uuid",
    )
    .bind(fork.body["resource"]["root_lineage_id"].as_str().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(parent, p["root_lineage_id"].as_str().unwrap());
    let page = call(
        &f,
        "GET",
        "/api/v2/projects?limit=1",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert_eq!(page.body["items"].as_array().unwrap().len(), 1);
    let next = call(
        &f,
        "GET",
        &format!(
            "/api/v2/projects?limit=1&cursor={}",
            page.body["next_cursor"].as_str().unwrap()
        ),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(next.status, StatusCode::OK);
    assert_eq!(next.body["items"][0]["id"], p["id"]);
    for bad in [
        "/api/v2/projects?limit=0",
        "/api/v2/projects?limit=101",
        "/api/v2/projects?limit=65536",
        "/api/v2/projects?unexpected=1",
        "/api/v2/projects/not-an-id",
    ] {
        let r = call(&f, "GET", bad, Value::Null, Some(&cookie)).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}:{}",
            r.body
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn bearer_is_real_native_possession_scoped_and_never_cookie_fallback(pool: PgPool) {
    let (f, cookie, _) = authenticated(pool.clone()).await;
    let p = create_project(&f, &cookie, "allowed").await;
    let other = create_project(&f, &cookie, "other").await;
    let (_, token, _) = credential(&f, &cookie, Some(&p), "AUTOMATION").await;
    let bearer = format!("Bearer {token}");
    let status = command(
        &f,
        "GET",
        "/api/v2/auth/machine",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(status.status, StatusCode::OK, "{}", status.body);
    assert_eq!(status.body["project_id"], p["id"]);
    assert!(!status.body.to_string().contains("verifier"));
    assert_eq!(status.headers[header::CACHE_CONTROL], "no-store");
    assert!(
        status.cookie.is_none(),
        "machine calls must not mint browser cookies"
    );
    let listed = command(
        &f,
        "GET",
        "/api/v2/projects",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
    let forbidden = command(
        &f,
        "GET",
        &format!("/api/v2/projects/{}", other["id"].as_str().unwrap()),
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(forbidden.status, StatusCode::NOT_FOUND);
    let payload =
        json!({"schema_version":1,"name":"forbidden","description":"","fork_from_project_id":null});
    let write = command(
        &f,
        "POST",
        "/api/v2/projects",
        payload.clone(),
        &[
            ("authorization", &bearer),
            ("idempotency-key", "machine-cannot-create"),
        ],
    )
    .await;
    assert_eq!(write.status, StatusCode::FORBIDDEN, "{}", write.body);
    let mut wrong = token.clone().into_bytes();
    let last = wrong.len() - 2;
    wrong[last] = if wrong[last] == b'A' { b'B' } else { b'A' };
    let wrong = format!("Bearer {}", String::from_utf8(wrong).unwrap());
    for headers in [
        vec![("authorization", wrong.as_str())],
        vec![
            ("authorization", wrong.as_str()),
            ("cookie", cookie.as_str()),
        ],
        vec![
            ("authorization", bearer.as_str()),
            ("cookie", cookie.as_str()),
        ],
        vec![
            ("authorization", bearer.as_str()),
            ("authorization", bearer.as_str()),
        ],
    ] {
        let denied = command(&f, "GET", "/api/v2/projects", Value::Null, &headers).await;
        assert!(
            matches!(
                denied.status,
                StatusCode::UNAUTHORIZED | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "{}",
            denied.body
        );
    }
    let denied = command(
        &f,
        "GET",
        "/api/v2/auth/session",
        Value::Null,
        &[("authorization", &bearer), ("cookie", &cookie)],
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNAUTHORIZED);
    let missing = command(
        &f,
        "POST",
        "/api/v2/projects",
        payload.clone(),
        &[("cookie", &cookie), ("idempotency-key", "missing-origin")],
    )
    .await;
    assert_eq!(missing.status, StatusCode::FORBIDDEN);
    let url = command(
        &f,
        "GET",
        "/api/v2/projects?access_token=secret",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(url.status, StatusCode::UNPROCESSABLE_ENTITY);
    let poisoned = command(
        &f,
        "GET",
        "/api/v2/projects",
        Value::Null,
        &[
            ("authorization", &bearer),
            ("origin", "https://evil.example"),
        ],
    )
    .await;
    assert_eq!(poisoned.status, StatusCode::FORBIDDEN);
    let data: String = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(to_jsonb(r))::text,'[]') FROM app.command_receipts r",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!data.contains(&token));
    assert!(!data.contains("$argon2"));
    assert!(!data.contains("verifier_ref"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn disabling_reenabling_and_revoking_credentials_never_revives_old_tokens(pool: PgPool) {
    let (f, cookie, _) = authenticated(pool).await;
    let p = create_project(&f, &cookie, "owner").await;
    let (_, token, principal) = credential(&f, &cookie, Some(&p), "CLI").await;
    let bearer = format!("Bearer {token}");
    let path = format!(
        "/api/v2/machine-principals/{}",
        principal["id"].as_str().unwrap()
    );
    for (key, revision, enabled, epoch) in
        [("disable", "1", false, "2"), ("reenable", "2", true, "3")]
    {
        let update = browser(
            &f,
            &cookie,
            key,
            "PATCH",
            &path,
            json!({"schema_version":1,"expected_revision":revision,"name":"CLI","enabled":enabled}),
        )
        .await;
        assert_eq!(update.status, StatusCode::OK, "{}", update.body);
        assert_eq!(update.body["resource"]["credential_epoch"], epoch);
        let denied = command(
            &f,
            "GET",
            "/api/v2/auth/machine",
            Value::Null,
            &[("authorization", &bearer)],
        )
        .await;
        assert_eq!(denied.status, StatusCode::UNAUTHORIZED);
    }
    let (c, token, _) = credential(&f, &cookie, Some(&p), "CLI").await;
    let bearer = format!("Bearer {token}");
    let revoke = browser(
        &f,
        &cookie,
        "revoke",
        "POST",
        &format!(
            "/api/v2/machine-credentials/{}/revoke",
            c["id"].as_str().unwrap()
        ),
        json!({"schema_version":1,"reason":"no longer needed"}),
    )
    .await;
    assert_eq!(revoke.status, StatusCode::OK, "{}", revoke.body);
    assert!(!revoke.body["resource"]["revoked_at"].is_null());
    let denied = command(
        &f,
        "GET",
        "/api/v2/auth/machine",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_cli_totp_grant_binds_full_request_once_and_retries_only_original_receipt(
    pool: PgPool,
) {
    let (f, cookie, native) = authenticated(pool.clone()).await;
    let (_, token, _) = credential(&f, &cookie, None, "CLI").await;
    let bearer = format!("Bearer {token}");
    let payload = json!({"schema_version":1,"name":"Human-approved CLI project","description":"exact intent","fork_from_project_id":null});
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let code = native.generate((now / 30 + 1) * 30);
    let request = json!({"schema_version":1,"command":{"operation":"PROJECT_CREATE","request":payload},"target_id":null,"code":code});
    let grant = command(
        &f,
        "POST",
        "/api/v2/auth/operator-command-grants",
        request.clone(),
        &[("authorization", &bearer), ("idempotency-key", "grant1")],
    )
    .await;
    assert_eq!(grant.status, StatusCode::CREATED, "{}", grant.body);
    let id = grant.body["resource"]["id"].as_str().unwrap();
    let target = grant.body["resource"]["target_id"].clone();
    let mut changed = payload.clone();
    changed["name"] = json!("Substituted");
    let bad = command(
        &f,
        "POST",
        "/api/v2/projects",
        changed,
        &[
            ("authorization", &bearer),
            ("x-operator-grant", id),
            ("idempotency-key", "execute"),
        ],
    )
    .await;
    assert_eq!(bad.status, StatusCode::FORBIDDEN, "{}", bad.body);
    let result = command(
        &f,
        "POST",
        "/api/v2/projects",
        payload.clone(),
        &[
            ("authorization", &bearer),
            ("x-operator-grant", id),
            ("idempotency-key", "execute"),
        ],
    )
    .await;
    assert_eq!(result.status, StatusCode::CREATED, "{}", result.body);
    assert_eq!(result.body["resource"]["id"], target);
    let replay = command(
        &f,
        "POST",
        "/api/v2/projects",
        payload.clone(),
        &[
            ("authorization", &bearer),
            ("x-operator-grant", id),
            ("idempotency-key", "execute"),
        ],
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED, "{}", replay.body);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], result.body["resource"]);
    let double = command(
        &f,
        "POST",
        "/api/v2/projects",
        payload,
        &[
            ("authorization", &bearer),
            ("x-operator-grant", id),
            ("idempotency-key", "new-key"),
        ],
    )
    .await;
    assert_eq!(double.status, StatusCode::CONFLICT, "{}", double.body);
    let repeat = command(
        &f,
        "POST",
        "/api/v2/auth/operator-command-grants",
        request,
        &[("authorization", &bearer), ("idempotency-key", "grant1")],
    )
    .await;
    assert_eq!(repeat.status, StatusCode::CREATED, "{}", repeat.body);
    assert_eq!(repeat.body["resource"], grant.body["resource"]);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.operator_command_consumptions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    let old=command(&f,"POST","/api/v2/projects",json!({"schema_version":1,"name":"Human-approved CLI project","description":"exact intent","fork_from_project_id":null}),&[("authorization",&bearer),("x-operator-grant",id),("idempotency-key","execute")]).await;
    assert_eq!(old.status, StatusCode::FORBIDDEN, "{}", old.body);
    let receipt: String = sqlx::query_scalar(
        "SELECT jsonb_agg(normalized_nonsecret_request)::text FROM app.command_receipts",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!receipt.contains("\"code\""));
    assert!(!receipt.contains(&token));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_fields_missing_keys_and_unscoped_doctor_permissions_fail_closed(pool: PgPool) {
    let (f, cookie, _) = authenticated(pool).await;
    let (_, token, _) = credential(&f, &cookie, None, "AUTOMATION").await;
    let bearer = format!("Bearer {token}");
    let no_project = command(
        &f,
        "GET",
        "/api/v2/projects",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(no_project.status, StatusCode::FORBIDDEN);
    let grant=command(&f,"POST","/api/v2/auth/operator-command-grants",json!({"schema_version":1,"command":{"operation":"PROJECT_CREATE","request":{"schema_version":1,"name":"no","description":"","fork_from_project_id":null}},"target_id":null,"code":"000000"}),&[("authorization",&bearer),("idempotency-key","not-human")]).await;
    assert_eq!(grant.status, StatusCode::FORBIDDEN);
    let malformed = browser(
        &f,
        &cookie,
        "malformed",
        "POST",
        "/api/v2/projects",
        json!({"schema_version":1,"name":"no","description":"","root_lineage_id":Id::new()}),
    )
    .await;
    assert_eq!(malformed.status, StatusCode::UNPROCESSABLE_ENTITY);
    let missing = command(
        &f,
        "POST",
        "/api/v2/projects",
        json!({"schema_version":1,"name":"no","description":"","fork_from_project_id":null}),
        &[("cookie", &cookie), ("origin", "https://research.example")],
    )
    .await;
    assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
}
