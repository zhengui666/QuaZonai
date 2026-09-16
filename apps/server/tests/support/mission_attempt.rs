//! Actual PostgreSQL issuance, Axum authentication and ordinary HTTP reads.
//! Relational Attempt takeover here is not a claim of complete Worker orchestration.
use super::*;

const READ_SCOPES: &[&str] = &["RESEARCH_READ", "RUN_READ"];

async fn status(f: &Fixture, path: &str, bearer: &str) -> StatusCode {
    f.app
        .clone()
        .oneshot(request("GET", path, "", Value::Null, None, Some(bearer)))
        .await
        .unwrap()
        .status()
}

#[sqlx::test(migrations = "../../migrations")]
async fn read_only_mission_cannot_keep_identity_run_or_artifact_access_after_takeover(
    pool: PgPool,
) {
    let (f, _, _, _) = setup(&pool).await;
    let (project, run, first_attempt, principal) = mission(&f, &pool).await;
    let (writer, _, _) = mission_token(&f, &pool, principal, 600).await;
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "read-fence-artifact",
        upload(project, "source"),
        None,
        Some(&writer),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let artifact = created.body["resource"]["id"].as_str().unwrap();
    let paths = [
        "/api/v2/auth/machine".to_owned(),
        format!("/api/v2/runs/{run}"),
        format!("/api/v2/artifacts/{artifact}"),
        format!("/api/v2/artifacts/{artifact}/content"),
    ];
    let (reader, credential, _) =
        mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    let issuance: (uuid::Uuid, Vec<String>) = sqlx::query_as(
        "SELECT issuer_attempt_id,scope_codes FROM app.machine_credentials WHERE id=$1",
    )
    .bind(credential.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(issuance.0, first_attempt.as_uuid());
    assert_eq!(issuance.1, READ_SCOPES);
    for path in &paths {
        assert_eq!(status(&f, path, &reader).await, StatusCode::OK, "{path}");
    }
    // Test-only native relational takeover; never rewrite a credential binding.
    let later = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,2,'read-fence-worker',1,clock_timestamp()+interval '1 hour','NOT_SENT','UNKNOWN')")
        .bind(later.as_uuid()).bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.runs SET active_attempt_id=$2,current_attempt_no=2 WHERE id=$1")
        .bind(run.as_uuid())
        .bind(later.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for path in &paths {
        assert_eq!(
            status(&f, path, &reader).await,
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
    }
    // A new current-Attempt read credential still works. This is not blanket denial.
    let (current, _, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    for path in &paths {
        assert_eq!(status(&f, path, &current).await, StatusCode::OK, "{path}");
    }
    let original_binding: uuid::Uuid =
        sqlx::query_scalar("SELECT issuer_attempt_id FROM app.machine_credentials WHERE id=$1")
            .bind(credential.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(original_binding, first_attempt.as_uuid());
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_attempt_native_owner_takeover_revokes_old_read_credentials(pool: PgPool) {
    use store::lifecycle::{ClaimResult, RunMessage};

    let (f, _, _, _) = setup(&pool).await;
    let (project, run, attempt, principal) = mission(&f, &pool).await;
    let (writer, _, _) = mission_token(&f, &pool, principal, 600).await;
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "same-attempt-output",
        upload(project, "source"),
        None,
        Some(&writer),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let artifact = created.body["resource"]["id"].as_str().unwrap();
    let (old, credential, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    let paths = [
        "/api/v2/auth/machine".to_owned(),
        format!("/api/v2/runs/{run}"),
        format!("/api/v2/artifacts/{artifact}"),
        format!("/api/v2/artifacts/{artifact}/content"),
    ];
    for path in &paths {
        assert_eq!(status(&f, path, &old).await, StatusCode::OK);
    }
    let (message_id, read_count): (i64, i32) =
        sqlx::query_as("SELECT msg_id,read_ct FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(run.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    // Only the clock boundary is injected. The actual native claim method must
    // adopt the existing Attempt and raise its fencing epoch, not create a Run.
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(attempt.as_uuid()).execute(&pool).await.unwrap();
    let message = RunMessage {
        message_id,
        run_id: run,
        read_count,
    };
    let ClaimResult::Leased(lease) = f
        .store
        .claim_run(&message, "replacement-mission-owner", 60)
        .await
        .unwrap()
    else {
        panic!("native lease takeover expected");
    };
    assert_eq!(lease.fence.attempt_id, attempt);
    assert_eq!(lease.attempt_no, 1);
    assert_eq!(lease.fence.owner_epoch.get(), 2);
    for path in &paths {
        assert_eq!(
            status(&f, path, &old).await,
            StatusCode::UNAUTHORIZED,
            "an old process must not borrow the replacement owner's live lease: {path}"
        );
    }
    let (current, _, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    for path in &paths {
        assert_eq!(status(&f, path, &current).await, StatusCode::OK);
    }
    f.store
        .renew_run_lease(run, &lease.fence, 120)
        .await
        .unwrap();
    assert_eq!(
        status(&f, "/api/v2/auth/machine", &current).await,
        StatusCode::OK,
        "renewing the same owner's lease does not revoke its current credential"
    );
    let unchanged: (uuid::Uuid, i64) = sqlx::query_as(
        "SELECT issuer_attempt_id,issuer_owner_epoch FROM app.machine_credentials WHERE id=$1",
    )
    .bind(credential.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unchanged, (attempt.as_uuid(), 1));
    let stale_write = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "stale-owner-output",
        upload(project, "x"),
        None,
        Some(&writer),
    )
    .await;
    assert_eq!(stale_write.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_mission_lease_revokes_ordinary_identity_and_run_reads(pool: PgPool) {
    let (f, _, _, _) = setup(&pool).await;
    let (_, run, attempt, principal) = mission(&f, &pool).await;
    let (token, _, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    assert_eq!(
        status(&f, "/api/v2/auth/machine", &token).await,
        StatusCode::OK
    );
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(attempt.as_uuid()).execute(&pool).await.unwrap();
    for path in [
        "/api/v2/auth/machine".to_owned(),
        format!("/api/v2/runs/{run}"),
    ] {
        assert_eq!(status(&f, &path, &token).await, StatusCode::UNAUTHORIZED);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn issuance_rejects_injected_owner_epoch_and_expired_native_lease(pool: PgPool) {
    let (f, _, _, _) = setup(&pool).await;
    let (_, _, attempt, principal) = mission(&f, &pool).await;
    let (_, original, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    let sql = "INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by,issuer_attempt_id,issuer_owner_epoch) SELECT $2,principal_id,$3,verifier_ref,principal_epoch,scope_codes,clock_timestamp(),expires_at,issued_by,issuer_attempt_id,$4 FROM app.machine_credentials WHERE id=$1";
    let rejected = Id::new();
    let error = sqlx::query(sql)
        .bind(original.as_uuid())
        .bind(rejected.as_uuid())
        .bind(Id::new().to_string())
        .bind(99_i64)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(attempt.as_uuid()).execute(&pool).await.unwrap();
    let expired = Id::new();
    let error = sqlx::query(sql)
        .bind(original.as_uuid())
        .bind(expired.as_uuid())
        .bind(Id::new().to_string())
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.machine_credentials WHERE id=ANY($1)")
            .bind(vec![rejected.as_uuid(), expired.as_uuid()])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = false)]
async fn attempt_bound_legacy_credential_cannot_borrow_the_current_owner_on_upgrade(pool: PgPool) {
    let directory = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        let version: i64 = name.split('_').next().unwrap().parse().unwrap();
        if version < 202609100023 {
            std::fs::copy(entry.path(), directory.path().join(name)).unwrap();
        }
    }
    sqlx::migrate::Migrator::new(directory.path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    let (f, cookie, project, _) = setup(&pool).await;
    let (cli_token, cli_id) = bearer(&f, &cookie, project, READ_SCOPES, "pre-owner-cli").await;
    let mut cli_before: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.machine_credentials c WHERE id=$1")
            .bind(cli_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(cli_before.get("issuer_owner_epoch").is_none());
    let (_, run, attempt, principal) = mission(&f, &pool).await;
    let (legacy, credential, _) =
        mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    let mut expected: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.machine_credentials c WHERE id=$1")
            .bind(credential.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(expected["issuer_attempt_id"], attempt.to_string());
    assert!(expected.get("issuer_owner_epoch").is_none());
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let migrated: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.machine_credentials c WHERE id=$1")
            .bind(credential.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    expected["issuer_owner_epoch"] = Value::Null;
    assert_eq!(
        migrated, expected,
        "migration must preserve every original issuance field"
    );
    let cli_after: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.machine_credentials c WHERE id=$1")
            .bind(cli_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    cli_before["issuer_owner_epoch"] = Value::Null;
    assert_eq!(cli_before, cli_after, "old CLI issuance fields stay exact");
    assert_eq!(
        status(&f, "/api/v2/auth/machine", &cli_token).await,
        StatusCode::OK,
        "a valid pre-023 CLI credential must survive the Mission-only cutover"
    );
    for path in [
        "/api/v2/auth/machine".to_owned(),
        format!("/api/v2/runs/{run}"),
    ] {
        assert_eq!(status(&f, &path, &legacy).await, StatusCode::UNAUTHORIZED);
    }
    let (current, new_id, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    let binding: (uuid::Uuid, i64) = sqlx::query_as(
        "SELECT issuer_attempt_id,issuer_owner_epoch FROM app.machine_credentials WHERE id=$1",
    )
    .bind(new_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(binding, (attempt.as_uuid(), 1));
    assert_eq!(
        status(&f, "/api/v2/auth/machine", &current).await,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = false)]
async fn legacy_unbound_mission_is_preserved_but_requires_new_issuance_after_upgrade(pool: PgPool) {
    // Native SQLx resolves and verifies the actual committed historical SQL.
    let directory = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        let version: i64 = name.split('_').next().unwrap().parse().unwrap();
        if version < 202609080018 {
            std::fs::copy(entry.path(), directory.path().join(name)).unwrap();
        }
    }
    sqlx::migrate::Migrator::new(directory.path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    let (f, _, _, _) = setup(&pool).await;
    // Construct an exact pre-018 relational fixture, not a new service against
    // an intentionally old schema. Current admission correctly requires all
    // current migrations, including native Runtime CA configuration.
    let data = run_support::fixture(&pool, run_support::budget()).await;
    let run = Id::new();
    let attempt = Id::new();
    let principal = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at) VALUES($1,$2,$3,'AGENT_RESEARCH',$4,'DISPATCHING',clock_timestamp()+interval '1 hour',clock_timestamp())")
        .bind(run.as_uuid()).bind(data.project.as_uuid()).bind(data.cycle.as_uuid())
        .bind(data.input_set.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,1,'historical-issuance-fixture',1,clock_timestamp()+interval '1 hour','NOT_SENT','UNKNOWN')")
        .bind(attempt.as_uuid()).bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.runs SET active_attempt_id=$2,current_attempt_no=1 WHERE id=$1")
        .bind(run.as_uuid())
        .bind(attempt.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,run_id,enabled,credential_epoch) VALUES($1,'pre-018 researcher fixture','MISSION',$2,$3,true,1)")
        .bind(principal.as_uuid()).bind(data.project.as_uuid()).bind(run.as_uuid())
        .execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let (legacy, credential, _) =
        mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    let mut expected: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.machine_credentials c WHERE id=$1")
            .bind(credential.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(expected.get("issuer_attempt_id").is_none());
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let migrated: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.machine_credentials c WHERE id=$1")
            .bind(credential.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    expected["issuer_attempt_id"] = Value::Null;
    expected["issuer_owner_epoch"] = Value::Null;
    assert_eq!(
        migrated, expected,
        "all historical issuance fields must remain exact"
    );
    for path in [
        "/api/v2/auth/machine".to_owned(),
        format!("/api/v2/runs/{run}"),
    ] {
        assert_eq!(status(&f, &path, &legacy).await, StatusCode::UNAUTHORIZED);
    }
    let (current, new_id, _) = mission_token_scoped(&f, &pool, principal, 600, READ_SCOPES).await;
    let actual: uuid::Uuid =
        sqlx::query_scalar("SELECT issuer_attempt_id FROM app.machine_credentials WHERE id=$1")
            .bind(new_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(actual, attempt.as_uuid());
    assert_eq!(
        status(&f, "/api/v2/auth/machine", &current).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&f, &format!("/api/v2/runs/{run}"), &current).await,
        StatusCode::OK
    );
}
