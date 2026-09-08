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
    let (_, run, attempt, principal) = mission(&f, &pool).await;
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
