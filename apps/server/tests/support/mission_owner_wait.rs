//! Actual PostgreSQL lock waits and native HTTP/issuance authority boundaries.
//! Parent research records are fixtures, not complete Worker or T42 acceptance.
use super::*;

const READ_SCOPES: &[&str] = &["RESEARCH_READ", "RUN_READ"];

async fn expire_after_confirmed_wait(pool: &PgPool, blocker: i32, prefix: &str, attempt: Id) {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND $1=ANY(pg_blocking_pids(pid)) AND starts_with(query,$2))")
                .bind(blocker).bind(prefix).fetch_one(pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let live: bool = sqlx::query_scalar(
            "SELECT lease_expires_at>clock_timestamp() FROM app.run_attempts WHERE id=$1",
        )
        .bind(attempt.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(live, "the request must already be waiting before lease expiry");
        // Do not change a lease under the waiting request. Let real database time
        // cross its original deadline while the unrelated principal lock is held.
        sqlx::query("SELECT pg_sleep(GREATEST(0.0,EXTRACT(EPOCH FROM (lease_expires_at-clock_timestamp())))::double precision+0.05) FROM app.run_attempts WHERE id=$1")
            .bind(attempt.as_uuid()).execute(pool).await.unwrap();
        let expired: bool = sqlx::query_scalar(
            "SELECT lease_expires_at<=clock_timestamp() FROM app.run_attempts WHERE id=$1",
        )
        .bind(attempt.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(expired, "release the principal only after native lease expiry");
    })
    .await
    .expect("native lock wait and lease expiry must remain bounded");
}

#[sqlx::test(migrations = "../../migrations")]
async fn identity_read_rechecks_native_lease_after_waiting_for_principal(pool: PgPool) {
    let (f, _, _, _) = setup(&pool).await;
    let (_, _, attempt, principal) = mission(&f, &pool).await;
    let (token, _, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()+interval '10 seconds' WHERE id=$1")
        .bind(attempt.as_uuid()).execute(&pool).await.unwrap();
    let mut lock = pool.begin().await.unwrap();
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM app.machine_principals WHERE id=$1 FOR UPDATE")
        .bind(principal.as_uuid())
        .execute(&mut *lock)
        .await
        .unwrap();
    let reading = f.app.clone().oneshot(request(
        "GET",
        "/api/v2/auth/machine",
        "unused",
        Value::Null,
        None,
        Some(&token),
    ));
    let (response, ()) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        tokio::join!(reading, async {
            expire_after_confirmed_wait(
                &pool,
                blocker,
                "SELECT enabled,credential_epoch,downstream_id FROM app.machine_principals",
                attempt,
            )
            .await;
            lock.commit().await.unwrap();
        })
    })
    .await
    .expect("HTTP authority must finish after the native row lock is released");
    assert_eq!(response.unwrap().status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn issuance_rechecks_native_lease_after_waiting_for_principal(pool: PgPool) {
    let (f, _, _, _) = setup(&pool).await;
    let (_, _, attempt, principal) = mission(&f, &pool).await;
    let (_, original, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()+interval '10 seconds' WHERE id=$1")
        .bind(attempt.as_uuid()).execute(&pool).await.unwrap();
    let mut lock = pool.begin().await.unwrap();
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM app.machine_principals WHERE id=$1 FOR UPDATE")
        .bind(principal.as_uuid())
        .execute(&mut *lock)
        .await
        .unwrap();
    let rejected = Id::new();
    let issuance = sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) SELECT $2,principal_id,$3,verifier_ref,principal_epoch,scope_codes,clock_timestamp(),expires_at,issued_by FROM app.machine_credentials WHERE id=$1")
        .bind(original.as_uuid()).bind(rejected.as_uuid()).bind(Id::new().to_string());
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        tokio::join!(issuance.execute(&pool), async {
            expire_after_confirmed_wait(
                &pool,
                blocker,
                "INSERT INTO app.machine_credentials",
                attempt,
            )
            .await;
            lock.commit().await.unwrap();
        })
    })
    .await
    .expect("native issuance must finish after the principal lock is released");
    let error = result.unwrap_err();
    let error = error.as_database_error().unwrap();
    assert_eq!(error.code().as_deref(), Some("23514"));
    assert_eq!(
        error.message(),
        "Mission credential requires its current live owner lease"
    );
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.machine_credentials WHERE id=$1)")
            .bind(rejected.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !exists,
        "a waited, expired issuance must leave no credential"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn non_mission_credentials_remain_usable_and_cannot_bind_an_attempt_owner(pool: PgPool) {
    let (f, cookie, project, _) = setup(&pool).await;
    let (token, original) = bearer(&f, &cookie, project, READ_SCOPES, "ordinary-cli").await;
    let (_, _, attempt, _) = mission(&f, &pool).await;
    let binding: (Option<uuid::Uuid>, Option<i64>) = sqlx::query_as(
        "SELECT issuer_attempt_id,issuer_owner_epoch FROM app.machine_credentials WHERE id=$1",
    )
    .bind(original.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(binding, (None, None));
    let mut rejected = Vec::new();
    for (attempt, owner) in [
        (Some(attempt.as_uuid()), None),
        (Some(attempt.as_uuid()), Some(1_i64)),
        (None, Some(1_i64)),
    ] {
        let id = Id::new();
        rejected.push(id.as_uuid());
        let error = sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by,issuer_attempt_id,issuer_owner_epoch) SELECT $2,principal_id,$3,verifier_ref,principal_epoch,scope_codes,clock_timestamp(),expires_at,issued_by,$4,$5 FROM app.machine_credentials WHERE id=$1")
            .bind(original.as_uuid()).bind(id.as_uuid()).bind(Id::new().to_string())
            .bind(attempt).bind(owner).execute(&pool).await.unwrap_err();
        let error = error.as_database_error().unwrap();
        assert_eq!(error.code().as_deref(), Some("23514"));
        assert_eq!(
            error.message(),
            "non-Mission credential cannot bind an Attempt owner"
        );
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.machine_credentials WHERE id=ANY($1)")
            .bind(rejected)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    let response = send(
        &f,
        "GET",
        "/api/v2/auth/machine",
        "unused",
        Value::Null,
        None,
        Some(&token),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["kind"], "CLI");
    assert_eq!(response.body["scope_codes"], json!(READ_SCOPES));
}
