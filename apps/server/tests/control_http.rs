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

#[sqlx::test(migrations = "../../migrations")]
async fn grant_replay_precedes_fresh_totp_and_reauth_quota_but_not_current_authority(pool: PgPool) {
    let (f, cookie, native) = authenticated(pool.clone()).await;
    let (_, token, _) = credential(&f, &cookie, None, "CLI").await;
    let bearer = format!("Bearer {token}");
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let payload = json!({"schema_version":1,"name":"already authorized","description":"","fork_from_project_id":null});
    let mut request = json!({"schema_version":1,"command":{"operation":"PROJECT_CREATE","request":payload},"target_id":null,"code":native.generate((now/30+1)*30)});
    let headers = [
        ("authorization", bearer.as_str()),
        ("idempotency-key", "lost-grant-response"),
    ];
    let first = command(
        &f,
        "POST",
        "/api/v2/auth/operator-command-grants",
        request.clone(),
        &headers,
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    // Saturate the real PostgreSQL REAUTH window; retries must not consume it.
    for _ in 1..5 {
        f.store
            .reserve_auth_attempt(store::auth::AuthOperation::Reauth)
            .await
            .unwrap();
    }
    let repeated = command(
        &f,
        "POST",
        "/api/v2/auth/operator-command-grants",
        request.clone(),
        &headers,
    )
    .await;
    assert_eq!(repeated.status, StatusCode::CREATED, "{}", repeated.body);
    assert_eq!(repeated.body["resource"], first.body["resource"]);
    assert_eq!(repeated.body["replayed"], true);
    // Code is explicitly not part of the persisted, nonsecret idempotency body.
    // Use a native authenticator output proven outside the acceptance window.
    let mut stale = native.generate(now - 300);
    if integrations::authentication::accepted_step(&native.secret, &stale, now as i64)
        .unwrap()
        .is_some()
    {
        stale = native.generate(now - 600);
    }
    assert!(
        integrations::authentication::accepted_step(&native.secret, &stale, now as i64)
            .unwrap()
            .is_none()
    );
    request["code"] = json!(stale);
    let reply = command(
        &f,
        "POST",
        "/api/v2/auth/operator-command-grants",
        request.clone(),
        &headers,
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(
        reply.body["resource"], first.body["resource"],
        "replay must not extend expiry"
    );
    let attempts: i32 =
        sqlx::query_scalar("SELECT attempts FROM app.auth_rate_windows WHERE operation='REAUTH'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts, 5);
    let mut changed = request.clone();
    changed["command"]["request"]["name"] = json!("substituted");
    let conflict = command(
        &f,
        "POST",
        "/api/v2/auth/operator-command-grants",
        changed,
        &headers,
    )
    .await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    assert_eq!(conflict.body["code"], "IDEMPOTENCY_CONFLICT");
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    let revoked = command(
        &f,
        "POST",
        "/api/v2/auth/operator-command-grants",
        request,
        &headers,
    )
    .await;
    assert_eq!(revoked.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_issuance_materializes_only_one_verifier_and_database_failure_cleans_it(
    pool: PgPool,
) {
    let (f, cookie, _) = authenticated(pool.clone()).await;
    let p = create_project(&f, &cookie, "verifier-owner").await;
    let created=browser(&f,&cookie,"verifier-principal","POST","/api/v2/machine-principals",json!({"schema_version":1,"name":"CLI","kind":"CLI","project_id":p["id"],"downstream_id":null,"enabled":true})).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let path = format!(
        "/api/v2/machine-principals/{}/credentials",
        created.body["resource"]["id"].as_str().unwrap()
    );
    let request = json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":Utc::now()+Duration::hours(1)});
    let directory = f._state.path().join("secrets");
    let before = std::fs::read_dir(&directory).unwrap().count();
    let (a, b) = tokio::join!(
        browser(
            &f,
            &cookie,
            "concurrent-issue",
            "POST",
            &path,
            request.clone()
        ),
        browser(
            &f,
            &cookie,
            "concurrent-issue",
            "POST",
            &path,
            request.clone()
        )
    );
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    assert_eq!(b.status, StatusCode::CREATED, "{}", b.body);
    assert_eq!(a.body["resource"], b.body["resource"]);
    assert_ne!(a.body["token"].is_null(), b.body["token"].is_null());
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), before + 1);
    // Genuine server-side rejection after encryption, not a mocked Store.
    sqlx::raw_sql("CREATE FUNCTION app.reject_issuance_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='fixture database refusal'; END $$; CREATE TRIGGER reject_fixture BEFORE INSERT ON app.machine_credentials FOR EACH ROW EXECUTE FUNCTION app.reject_issuance_fixture();").execute(&pool).await.unwrap();
    let failed = browser(&f, &cookie, "db-refused", "POST", &path, request).await;
    assert_eq!(
        failed.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        failed.body
    );
    assert_eq!(
        std::fs::read_dir(&directory).unwrap().count(),
        before + 1,
        "a definitely unreferenced verifier must be removed"
    );
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE idempotency_key='db-refused'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 0);
    let vault = std::sync::Arc::new(
        integrations::secrets::SecretVault::open(&directory, &f._state.path().join("master.key"))
            .unwrap(),
    );
    let orphan = vault
        .put("MACHINE_VERIFIER", b"interrupted unpublished fixture")
        .unwrap();
    let totp = vault.put("TOTP", b"must remain").unwrap();
    assert_eq!(
        server::secrets::prune_unpublished_verifiers(&f.store, vault.clone())
            .await
            .unwrap(),
        1
    );
    assert!(!directory.join(orphan.to_string()).exists());
    assert!(directory.join(totp.to_string()).exists());
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), before + 2);
    assert_eq!(
        server::secrets::prune_unpublished_verifiers(&f.store, vault)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_machine_failures_are_bounded_without_consuming_human_crypto_slots(pool: PgPool) {
    let mut f = fixture(pool.clone()).await;
    let state = server::AppState::new(
        f.store.clone(),
        integrations::secrets::SecretVault::open(
            &f._state.path().join("secrets"),
            &f._state.path().join("master.key"),
        )
        .unwrap(),
        server::WebPolicy::new(
            "https://research.example",
            "127.0.0.1:8080".parse().unwrap(),
            false,
        )
        .unwrap(),
    );
    let machine_slots = state.machine_crypto_slots.clone();
    f.app = server::router(state, tower_sessions::cookie::Key::generate());
    let (enrollment, anonymous, native) = start(&f).await;
    let (confirmed, _) = confirm(&f, &enrollment, &anonymous, &native, true).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap();
    let (issued, token, _) = credential(&f, &cookie, None, "CLI").await;
    let bad = format!(
        "Bearer qz2.{}.{}",
        issued["public_token_id"].as_str().unwrap(),
        integrations::authentication::random_capability()
    );
    for _ in 0..5 {
        assert_eq!(
            command(
                &f,
                "GET",
                "/api/v2/auth/machine",
                Value::Null,
                &[("authorization", &bad)]
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED
        );
    }
    let denied = command(
        &f,
        "GET",
        "/api/v2/auth/machine",
        Value::Null,
        &[("authorization", &bad)],
    )
    .await;
    assert_eq!(denied.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(denied.body["code"], "AUTH_RATE_LIMITED");
    assert!(denied.headers.contains_key("retry-after"));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM app.machine_auth_rate_windows")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
    for _ in 0..3 {
        let unknown = format!(
            "Bearer qz2.{}.{}",
            Id::new(),
            integrations::authentication::random_capability()
        );
        assert_eq!(
            command(
                &f,
                "GET",
                "/api/v2/auth/machine",
                Value::Null,
                &[("authorization", &unknown)]
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.machine_auth_rate_windows")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    sqlx::query("UPDATE app.machine_auth_rate_windows SET window_started_at=clock_timestamp()-interval '61 seconds'").execute(&pool).await.unwrap();
    let held = machine_slots.acquire_many_owned(2).await.unwrap();
    let bearer = format!("Bearer {token}");
    let busy = command(
        &f,
        "GET",
        "/api/v2/auth/machine",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(busy.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(busy.body["code"], "CRYPTO_BUSY");
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let verified = call(
        &f,
        "POST",
        "/api/v2/auth/verify",
        json!({"schema_version":1,"code":native.generate((now/30+1)*30)}),
        Some(&cookie),
    )
    .await;
    assert_eq!(verified.status, StatusCode::OK, "{}", verified.body);
    drop(held);
    let ok = command(
        &f,
        "GET",
        "/api/v2/auth/machine",
        Value::Null,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
}
