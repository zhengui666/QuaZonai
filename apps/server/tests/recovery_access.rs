//! Disposable initialized instance: restore access cutover, not full backup/recovery proof.
#[path = "support/client.rs"]
#[allow(dead_code)]
mod client;
#[allow(dead_code)]
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use contracts::Id;
use serde_json::{json, Value};
use sqlx::{ConnectOptions, PgPool};

async fn recover(pool: &PgPool, id: Id) -> std::process::Output {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .args(["recover-access", "--recovery-id", &id.to_string()])
        .env_clear()
        .env(
            "DATABASE_URL",
            pool.connect_options().to_url_lossy().as_str(),
        )
        .kill_on_drop(true)
        .output()
        .await
        .unwrap()
}
#[sqlx::test(migrations = "../../migrations")]
async fn offline_cutover_invalidates_retained_authority_and_replays_atomically(pool: PgPool) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &totp, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let project=client::browser(&f,&cookie,"project","/api/v2/projects",json!({"schema_version":1,"name":"Retained project","description":"Recovery test","fork_from_project_id":null})).await;
    let principal=client::browser(&f,&cookie,"principal","/api/v2/machine-principals",json!({"schema_version":1,"name":"Old CLI","kind":"CLI","project_id":project.body["resource"]["id"],"downstream_id":null,"enabled":true})).await;
    let credential=client::browser(&f,&cookie,"credential",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let before = f.store.authentication_snapshot().await.unwrap();
    // A forced receipt failure must roll back both epoch and credential revocation.
    sqlx::raw_sql("CREATE FUNCTION public.reject_recovery() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='AUTH_RESTORE_INVALIDATE' THEN RAISE EXCEPTION 'injected receipt failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_recovery BEFORE INSERT ON app.command_receipts FOR EACH ROW EXECUTE FUNCTION public.reject_recovery()").execute(&pool).await.unwrap();
    let recovery = Id::new();
    let failed = recover(&pool, recovery).await;
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    assert_eq!(
        f.store.authentication_snapshot().await.unwrap().epoch,
        before.epoch
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.machine_credentials c WHERE NOT EXISTS(SELECT 1 FROM app.machine_credential_revocations r WHERE r.credential_id=c.id AND r.effective_at<=clock_timestamp())")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    sqlx::raw_sql("DROP TRIGGER reject_recovery ON app.command_receipts; DROP FUNCTION public.reject_recovery()").execute(&pool).await.unwrap();
    let done = recover(&pool, recovery).await;
    assert!(done.status.success(), "native recovery command failed");
    let report: Value = serde_json::from_slice(&done.stdout).unwrap();
    assert_eq!(report["revoked_machine_credentials"], "1");
    let repeat = recover(&pool, recovery).await;
    assert!(repeat.status.success());
    assert_eq!(done.stdout, repeat.stdout);
    let after = f.store.authentication_snapshot().await.unwrap();
    assert!(after.epoch > before.epoch);
    assert_eq!(after.secret_ref, before.secret_ref);
    let old = support::call(
        &f,
        "GET",
        "/api/v2/auth/session",
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let request = Request::builder()
        .uri("/api/v2/projects")
        .header(header::HOST, "research.example")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let denied = support::exchange(&f.app, request).await;
    assert_eq!(denied.status, StatusCode::UNAUTHORIZED);
    let current = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let relogin=support::call(&f,"POST","/api/v2/auth/login",json!({"schema_version":1,"code":totp.generate((current/30+1)*30),"trust_device":false,"device_label":null}),None).await;
    assert_eq!(
        relogin.status,
        StatusCode::OK,
        "retained verifier must permit a fresh login"
    );
    let projects = support::call(
        &f,
        "GET",
        "/api/v2/projects",
        Value::Null,
        relogin.cookie.as_deref(),
    )
    .await;
    assert_eq!(projects.status, StatusCode::OK);
    assert_eq!(
        projects.body["items"][0]["id"],
        project.body["resource"]["id"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn ordinary_database_identity_cannot_invalidate_restored_access(pool: PgPool) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(login.status, StatusCode::OK);
    let role = format!("recovery_test_{}", Id::new().to_string().replace('-', ""));
    let password = Id::new().to_string();
    let ddl: String =
        sqlx::query_scalar("SELECT format('CREATE ROLE %I LOGIN PASSWORD %L',$1::text,$2::text)")
            .bind(&role)
            .bind(&password)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(sqlx::query(&ddl).execute(&pool).await.is_ok());
    sqlx::query(&format!("GRANT USAGE ON SCHEMA app TO {role}"))
        .execute(&pool)
        .await
        .unwrap();
    let restricted = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            pool.connect_options()
                .as_ref()
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    let before = f.store.authentication_snapshot().await.unwrap().epoch;
    let result = store::Store::from_pool(restricted.clone())
        .invalidate_restored_access(Id::new())
        .await;
    assert!(matches!(result, Err(store::StoreError::Forbidden)));
    assert_eq!(
        f.store.authentication_snapshot().await.unwrap().epoch,
        before
    );
    restricted.close().await;
    sqlx::query(&format!("REVOKE USAGE ON SCHEMA app FROM {role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
}
