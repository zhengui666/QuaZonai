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

// Native clients use only the disposable SQLx connection, never the host's default DB.
fn postgres_tool(pool: &PgPool, tool: &str) -> tokio::process::Command {
    let mut options = pool.connect_options().as_ref().clone();
    let mut command = if let Ok(container) = std::env::var("QZ_TEST_PG_CONTAINER") {
        options = options.host("127.0.0.1").port(5432);
        let mut command = tokio::process::Command::new("docker");
        command.args([
            "exec",
            "-i",
            "--env",
            "PGDATABASE",
            "--env",
            "PGHOST",
            "--env",
            "PGPORT",
            "--env",
            "PGUSER",
            "--env",
            "PGPASSWORD",
            &container,
            tool,
        ]);
        command
    } else {
        tokio::process::Command::new(tool)
    };
    // Libpq's environment default is a database name, not an expanded SQLx URI.
    let url = options.to_url_lossy();
    let encoded = format!(
        "password={}",
        url.password().unwrap_or_default().replace('+', "%2B")
    );
    let password = url::form_urlencoded::parse(encoded.as_bytes())
        .next()
        .unwrap()
        .1
        .into_owned();
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("PGHOST", options.get_host())
        .env("PGPORT", options.get_port().to_string())
        .env("PGUSER", options.get_username())
        .env("PGPASSWORD", password)
        .env(
            "PGDATABASE",
            options.get_database().expect("disposable test database"),
        )
        .kill_on_drop(true);
    command
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_archive_restores_original_receipt_and_retained_totp(pool: PgPool) {
    use integrations::secrets::SecretVault;
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tower_sessions::cookie::Key;

    // Keep the SAME key: rejection must follow the restored epoch, not a changed cookie key.
    let cookie_key = Key::generate();
    let mut f = support::fixture_with_key(pool.clone(), None, None, cookie_key.clone()).await;
    // Persist the same synthetic session key in the native deployment format.
    let reference = SecretVault::open(
        &f._state.path().join("secrets"),
        &f._state.path().join("master.key"),
    )
    .unwrap()
    .put("SESSION_KEY", cookie_key.master())
    .unwrap();
    std::fs::write(
        f._state.path().join("session-key.ref"),
        reference.to_string(),
    )
    .unwrap();
    f.app = server::router(
        server::AppState::new(
            f.store.clone(),
            SecretVault::open(
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
        )
        .with_artifact_store(
            integrations::artifacts::ArtifactStore::open(&f._state.path().join("artifacts"))
                .unwrap(),
        ),
        cookie_key.clone(),
    );
    let (enrollment, initial, totp) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let body = json!({"schema_version":1,"name":"Archived project","description":"Original receipt survives restore","fork_from_project_id":null});
    let original = client::browser(
        &f,
        &cookie,
        "archived-project",
        "/api/v2/projects",
        body.clone(),
    )
    .await;
    assert_eq!(original.status, StatusCode::CREATED);
    let content = "// restored synthetic source, never production evidence\n// 中文\n";
    let artifact = client::browser(&f, &cookie, "archived-artifact", "/api/v2/artifacts", json!({"schema_version":1,"project_id":original.body["resource"]["id"],"kind":"CODE","content":content})).await;
    assert_eq!(artifact.status, StatusCode::CREATED);
    let artifact_id = artifact.body["resource"]["id"].as_str().unwrap();
    let before = f.store.authentication_snapshot().await.unwrap();
    // No workers or requests run during this synthetic filesystem/DB checkpoint.
    let historical =
        integrations::artifacts::ArtifactStore::open(&f._state.path().join("historical-artifacts"))
            .unwrap();
    let historical_id = Id::new();
    let historical_bytes = b"SYNTHETIC retained historical bytes\n";
    historical.put(historical_id, historical_bytes).unwrap();
    // An opaque profile fixture, not a copied user's account or native session.
    let profile = f._state.path().join("codex-profile");
    std::fs::create_dir(&profile).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(
        profile.join("config.toml"),
        "# SYNTHETIC restore fixture; no account\n",
    )
    .unwrap();
    let state = tempfile::tempdir().unwrap();
    let archive_directory = tempfile::tempdir().unwrap();
    let archive = archive_directory.path().join("state.tar");
    // The owner-held master key is a separate recovery input, not part of either
    // the database dump or the encrypted state archive.
    let key_directory = tempfile::tempdir().unwrap();
    let saved_key = key_directory.path().join("master.key");
    std::fs::copy(f._state.path().join("master.key"), &saved_key).unwrap();
    let backup_started_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&pool)
            .await
            .unwrap();
    let saved = tokio::process::Command::new("tar")
        .env_clear()
        .args(["--create", "--file"])
        .arg(&archive)
        .arg("--directory")
        .arg(f._state.path())
        .arg("--exclude=./master.key")
        .arg(".")
        .kill_on_drop(true)
        .output()
        .await
        .expect("native tar must be installed");
    assert!(
        saved.status.success() && saved.stderr.is_empty(),
        "synthetic state archive failed; diagnostics remain private"
    );
    let inventory = tokio::process::Command::new("tar")
        .env_clear()
        .args(["--list", "--file"])
        .arg(&archive)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(inventory.status.success() && inventory.stderr.is_empty());
    let names = std::str::from_utf8(&inventory.stdout).unwrap();
    assert!(!names.lines().any(|name| name == "./master.key"));
    assert!(names.lines().any(|name| name == "./session-key.ref"));
    let dump = postgres_tool(&pool, "pg_dump")
        .args(["--format=custom", "--no-owner", "--no-privileges"])
        .output()
        .await
        .expect("native pg_dump must be installed");
    assert!(
        dump.status.success(),
        "native pg_dump failed (archive and diagnostics remain private)"
    );
    assert!(
        dump.stderr.is_empty(),
        "pg_dump emitted a warning requiring investigation"
    );
    assert!(dump.stdout.starts_with(b"PGDMP"));
    let backup_finished_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&pool)
            .await
            .unwrap();
    // A subsequent source write is deliberately outside the archived point in time.
    let later = client::browser(&f, &cookie, "after-backup-project", "/api/v2/projects",
        json!({"schema_version":1,"name":"After backup","description":"Must remain only in the source instance","fork_from_project_id":null})).await;
    assert_eq!(later.status, StatusCode::CREATED);
    let later_id = later.body["resource"]["id"].as_str().unwrap();
    let later_created_at: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(later.body["resource"]["created_at"].clone()).unwrap();
    assert!(later_created_at >= backup_finished_at);
    let restore_started = std::time::Instant::now();
    // Deliberately omit objects initially so the existing missing-object assertion
    // still proves that a database restore alone cannot make downloads succeed.
    let unpacked = tokio::process::Command::new("tar")
        .env_clear()
        .args(["--extract", "--file"])
        .arg(&archive)
        .arg("--directory")
        .arg(state.path())
        .arg("--exclude=./artifacts")
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        unpacked.status.success() && unpacked.stderr.is_empty(),
        "synthetic state restore failed; diagnostics remain private"
    );
    assert!(!state.path().join("master.key").exists());
    std::fs::copy(&saved_key, state.path().join("master.key")).unwrap();
    let database = format!("restore_test_{}", Id::new().to_string().replace('-', ""));
    sqlx::query(&format!("CREATE DATABASE {database} TEMPLATE template0"))
        .execute(&pool)
        .await
        .unwrap();
    let restored_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect_with(pool.connect_options().as_ref().clone().database(&database))
        .await
        .unwrap();
    let mut child = postgres_tool(&restored_pool, "pg_restore")
        .args([
            "--dbname",
            "",
            "--single-transaction",
            "--exit-on-error",
            "--no-owner",
            "--no-privileges",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("native pg_restore must be installed");
    let mut input = child.stdin.take().unwrap();
    let (written, restored) = tokio::join!(
        async {
            input.write_all(&dump.stdout).await?;
            input.shutdown().await
        },
        child.wait_with_output()
    );
    assert!(written.is_ok());
    let restored = restored.unwrap();
    assert!(
        restored.status.success(),
        "native pg_restore failed (diagnostics remain private)"
    );
    assert!(
        restored.stderr.is_empty(),
        "pg_restore emitted a warning requiring investigation"
    );
    let owner = store::Store::from_pool(restored_pool.clone());
    assert!(owner.verify_runtime_role().await.is_err());
    // Restore intentionally excludes global roles/ACLs. Reapply runtime grants via
    // the deployment migration command, then use a genuinely separate login.
    let role = format!("restored_app_{}", Id::new().to_string().replace('-', ""));
    let password = Id::new().to_string();
    let ddl: String =
        sqlx::query_scalar("SELECT format('CREATE ROLE %I LOGIN PASSWORD %L',$1::text,$2::text)")
            .bind(&role)
            .bind(&password)
            .fetch_one(&restored_pool)
            .await
            .unwrap();
    assert!(sqlx::query(&ddl).execute(&restored_pool).await.is_ok());
    owner
        .migrate_with_application_role(Some(&role))
        .await
        .unwrap();
    let application_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect_with(
            restored_pool
                .connect_options()
                .as_ref()
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    let store = store::Store::from_pool(application_pool.clone());
    store.verify_runtime_role().await.unwrap();
    assert!(matches!(
        store.invalidate_restored_access(Id::new()).await,
        Err(store::StoreError::Forbidden)
    ));
    let forbidden = sqlx::query("TRUNCATE app.projects CASCADE")
        .execute(&application_pool)
        .await
        .unwrap_err();
    assert_eq!(
        forbidden
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("42501")
    );
    assert_eq!(
        store.authentication_snapshot().await.unwrap().epoch,
        before.epoch
    );
    let policy = server::WebPolicy::new(
        "https://research.example",
        "127.0.0.1:8080".parse().unwrap(),
        false,
    )
    .unwrap();
    let restored = support::Fixture {
        app: server::router(
            server::AppState::new(
                store.clone(),
                SecretVault::open(
                    &state.path().join("secrets"),
                    &state.path().join("master.key"),
                )
                .unwrap(),
                policy,
            )
            .with_artifact_store(
                integrations::artifacts::ArtifactStore::open(&state.path().join("artifacts"))
                    .unwrap(),
            ),
            cookie_key,
        ),
        store,
        _state: state,
    };
    // Positive control proves the old session was really restored and decryptable.
    assert_eq!(
        support::call(
            &restored,
            "GET",
            "/api/v2/auth/session",
            Value::Null,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::OK
    );
    assert!(recover(&restored_pool, Id::new()).await.status.success());
    assert_eq!(
        support::call(
            &restored,
            "GET",
            "/api/v2/auth/session",
            Value::Null,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    let now = restored
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let login = support::call(&restored,"POST","/api/v2/auth/login",json!({"schema_version":1,"code":totp.generate((now/30+1)*30),"trust_device":false,"device_label":null}),None).await;
    assert_eq!(login.status, StatusCode::OK);
    let artifact_path = format!("/api/v2/artifacts/{artifact_id}/content");
    // A restored DB alone must not claim that absent object bytes are available.
    assert_eq!(
        support::call(
            &restored,
            "GET",
            &artifact_path,
            Value::Null,
            login.cookie.as_deref()
        )
        .await
        .status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let objects = tokio::process::Command::new("tar")
        .env_clear()
        .args(["--extract", "--file"])
        .arg(&archive)
        .arg("--directory")
        .arg(restored._state.path())
        .arg("./artifacts")
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        objects.status.success() && objects.stderr.is_empty(),
        "synthetic object restore failed; diagnostics remain private"
    );
    // GNU tar compares original bytes, names, modes and timestamps. Compare both
    // trees: restoring the copy must not mutate the original checkpoint files.
    for directory in [restored._state.path(), f._state.path()] {
        let compared = tokio::process::Command::new("tar")
            .env_clear()
            .args(["--compare", "--file"])
            .arg(&archive)
            .arg("--directory")
            .arg(directory)
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        assert!(
            compared.status.success() && compared.stdout.is_empty() && compared.stderr.is_empty(),
            "synthetic state differs from archive; diagnostics remain private"
        );
    }
    let restored_historical = integrations::artifacts::ArtifactStore::open(
        &restored._state.path().join("historical-artifacts"),
    )
    .unwrap();
    assert_eq!(
        restored_historical
            .read(
                historical_id,
                contracts::DbCounter::new(historical_bytes.len() as u64).unwrap()
            )
            .unwrap(),
        historical_bytes
    );
    let metadata = support::call(
        &restored,
        "GET",
        &format!("/api/v2/artifacts/{artifact_id}"),
        Value::Null,
        login.cookie.as_deref(),
    )
    .await;
    assert_eq!(metadata.status, StatusCode::OK);
    assert_eq!(metadata.body, artifact.body["resource"]);
    let request = Request::builder()
        .uri(&artifact_path)
        .header(header::HOST, "research.example")
        .header(header::COOKIE, login.cookie.as_deref().unwrap())
        .body(Body::empty())
        .unwrap();
    use tower::ServiceExt;
    let downloaded = restored.app.clone().oneshot(request).await.unwrap();
    assert_eq!(downloaded.status(), StatusCode::OK);
    assert_eq!(
        axum::body::to_bytes(downloaded.into_body(), 65536)
            .await
            .unwrap()
            .as_ref(),
        content.as_bytes()
    );
    let replay = client::browser(
        &restored,
        login.cookie.as_deref().unwrap(),
        "archived-project",
        "/api/v2/projects",
        body,
    )
    .await;
    assert_eq!(replay.status, original.status);
    assert_eq!(original.body["replayed"], false);
    assert_eq!(replay.body["replayed"], true);
    let mut expected = original.body.clone();
    expected["replayed"] = json!(true);
    assert_eq!(replay.body, expected);
    let projects = support::call(
        &restored,
        "GET",
        "/api/v2/projects",
        Value::Null,
        login.cookie.as_deref(),
    )
    .await;
    assert_eq!(projects.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        projects.body["items"][0]["id"],
        original.body["resource"]["id"]
    );
    let absent = support::call(
        &restored,
        "GET",
        &format!("/api/v2/projects/{later_id}"),
        Value::Null,
        login.cookie.as_deref(),
    )
    .await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
    let retained = support::call(
        &f,
        "GET",
        &format!("/api/v2/projects/{later_id}"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(retained.status, StatusCode::OK);
    assert_eq!(retained.body, later.body["resource"]);
    // Exercise the deployed executable's state loading and role checks over TCP.
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    drop(socket);
    let mut service = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .args([
            "serve",
            "--public-url",
            "https://research.example",
            "--bind",
            &address.to_string(),
        ])
        .arg("--state-dir")
        .arg(restored._state.path())
        .env_clear()
        .env(
            "DATABASE_URL",
            application_pool.connect_options().to_url_lossy().as_str(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            assert!(
                service.try_wait().unwrap().is_none(),
                "restored server exited before readiness"
            );
            if let Ok(response) = http
                .get(format!("http://{address}/health/live"))
                .header(header::HOST, "research.example")
                .send()
                .await
            {
                assert_eq!(response.status(), StatusCode::NO_CONTENT);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("restored server must start within the test deadline");
    let stale = http
        .get(format!("http://{address}/api/v2/auth/session"))
        .header(header::HOST, "research.example")
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::UNAUTHORIZED);
    let current = http
        .get(format!("http://{address}/api/v2/projects"))
        .header(header::HOST, "research.example")
        .header(header::COOKIE, login.cookie.as_deref().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(current.status(), StatusCode::OK);
    assert_eq!(current.json::<Value>().await.unwrap(), projects.body);
    let object = http
        .get(format!("http://{address}{artifact_path}"))
        .header(header::HOST, "research.example")
        .header(header::COOKIE, login.cookie.as_deref().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(object.status(), StatusCode::OK);
    assert_eq!(object.bytes().await.unwrap().as_ref(), content.as_bytes());
    service.kill().await.unwrap();
    assert!(service.try_wait().unwrap().is_some());
    let restore_finished_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&restored_pool)
            .await
            .unwrap();
    println!(
        "{}",
        json!({"scope":"isolated database and state archive, synthetic historical/profile files, access cutover and server process; not production RPO/RTO",
        "backup_started_at":backup_started_at,"backup_finished_at":backup_finished_at,
        "source_write_after_backup_at":later_created_at,"restore_finished_at":restore_finished_at,
        "fixture_restore_elapsed_ms":restore_started.elapsed().as_millis(),
        "restored_project_count":1,"source_only_project_count":1})
    );
    // Recovery of the copy cannot change the live source's authority.
    assert_eq!(
        f.store.authentication_snapshot().await.unwrap().epoch,
        before.epoch
    );
    drop(restored);
    application_pool.close().await;
    restored_pool.close().await;
    sqlx::query(&format!("DROP DATABASE {database}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
}
