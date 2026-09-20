//! Native exports, sealed startup registration, browser auth and real CLI/TCP/PG.
#[path = "support/client.rs"]
#[allow(dead_code)]
mod client;
#[allow(dead_code)]
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use contracts::{imports::HistoricalRowExportV1, Id};
use integrations::secrets::SecretVault;
use serde_json::{json, Value};
use server::migrations::{ExportRegistration, HistoricalExports};
use sqlx::{ConnectOptions, PgPool};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn registry(directory: &Path, reference: Id) -> HistoricalExports {
    HistoricalExports::load(vec![ExportRegistration {
        export_ref: reference,
        directory: directory.into(),
        artifact_directory: None,
    }])
    .unwrap()
}
async fn export(pool: &PgPool, directory: &Path) -> HistoricalRowExportV1 {
    sqlx::raw_sql("CREATE TABLE public.alembic_version(version_num text NOT NULL); INSERT INTO public.alembic_version VALUES('0029_portfolio_candidate_exposure'); CREATE TABLE public.events(id bigint PRIMARY KEY,kind varchar(100) NOT NULL,aggregate_type varchar(100) NOT NULL,aggregate_id uuid,actor_kind varchar(40) NOT NULL,actor_metadata jsonb NOT NULL,payload jsonb NOT NULL,created_at timestamptz NOT NULL); INSERT INTO public.events VALUES(9007199254740993,'CREATED','JOB',NULL,'OPERATOR','{}','{}','2020-01-01')").execute(pool).await.unwrap();
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .args([
            "export-historical-rows",
            "--source-installation-id",
            &Id::new().to_string(),
            "--output",
        ])
        .arg(directory)
        .env(
            "MIGRATION_SOURCE_DATABASE_URL",
            pool.connect_options().to_url_lossy().as_str(),
        )
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(result.status.success(), "native row export failed");
    serde_json::from_slice(&fs::read(directory.join("report.json")).unwrap()).unwrap()
}
async fn send(
    f: &support::Fixture,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> support::Reply {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "localhost");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    };
    support::exchange(&f.app, request.body(body).unwrap()).await
}
#[sqlx::test(migrations = "../../migrations")]
async fn frozen_export_browser_and_cli_import_preserve_original_history_and_scope(pool: PgPool) {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("export");
    let source = export(&pool, &directory).await;
    let reference = Id::new();
    let exports = registry(&directory, reference);
    let cli_exports = registry(&directory, reference);
    assert!(HistoricalExports::load(vec![
        ExportRegistration {
            export_ref: reference,
            directory: directory.clone(),
            artifact_directory: None
        },
        ExportRegistration {
            export_ref: reference,
            directory: directory.clone(),
            artifact_directory: None
        }
    ])
    .is_err());
    // Both independently started adapters must keep their frozen byte inputs.
    let file = directory.join(format!(
        "{}.csv",
        source.tables.iter().find_map(|t| t.object_ref).unwrap()
    ));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(file, "replaced CSV").unwrap();
    fs::write(directory.join("report.json"), "{}").unwrap();
    let f = support::fixture_with_deployment(pool.clone(), None, Some(exports)).await;
    let login = support::local_session(&f).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap_or(initial);
    let request = json!({"schema_version":1,"export_ref":reference,"dry_run":true});
    let denied = send(
        &f,
        "POST",
        "/api/v2/migrations/import",
        request.clone(),
        &[("idempotency-key", "anon"), ("origin", "https://localhost")],
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNAUTHORIZED);
    let mut injected = request.clone();
    injected["directory"] = json!("/private");
    let denied = client::browser(
        &f,
        &cookie,
        "injected",
        "/api/v2/migrations/import",
        injected,
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNPROCESSABLE_ENTITY);
    let dry = client::browser(
        &f,
        &cookie,
        "dry",
        "/api/v2/migrations/import",
        request.clone(),
    )
    .await;
    assert_eq!(dry.status, StatusCode::ACCEPTED, "{}", dry.body);
    assert_eq!(dry.body["resource"]["projected_rows"], "1");
    assert_eq!(dry.body["resource"]["new_rows"], "0");
    let repeated = client::browser(
        &f,
        &cookie,
        "dry",
        "/api/v2/migrations/import",
        request.clone(),
    )
    .await;
    assert_eq!(repeated.body["resource"], dry.body["resource"]);
    assert_eq!(repeated.body["replayed"], true);
    let project = client::browser(&f,&cookie,"project","/api/v2/projects",json!({"schema_version":1,"name":"Migration CLI","description":"Read-only historical migration","fork_from_project_id":null})).await;
    assert_eq!(project.status, StatusCode::CREATED, "{}", project.body);
    let principal = client::browser(&f,&cookie,"cli","/api/v2/machine-principals",json!({"schema_version":1,"name":"Importer","kind":"CLI","project_id":project.body["resource"]["id"],"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED, "{}", principal.body);
    let credential = client::browser(&f,&cookie,"token",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(
        credential.status,
        StatusCode::CREATED,
        "{}",
        credential.body
    );
    let token = credential.body["token"].as_str().unwrap();
    let bearer = format!("Bearer {token}");
    let actual = json!({"schema_version":1,"export_ref":reference,"dry_run":false});
    let no_grant = send(
        &f,
        "POST",
        "/api/v2/migrations/import",
        actual.clone(),
        &[("authorization", &bearer), ("idempotency-key", "actual")],
    )
    .await;
    assert_eq!(no_grant.status, StatusCode::FORBIDDEN);
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let grant = send(&f,"POST","/api/v2/auth/operator-command-grants",json!({"schema_version":1,"command":{"operation":"MIGRATION_IMPORT","request":actual},"target_id":null}),&[("authorization",&bearer),("idempotency-key","grant")]).await;
    assert_eq!(grant.status, StatusCode::CREATED, "{}", grant.body);
    let grant_id = grant.body["resource"]["id"].as_str().unwrap();
    let wrong = send(
        &f,
        "POST",
        "/api/v2/migrations/import",
        request,
        &[
            ("authorization", &bearer),
            ("idempotency-key", "actual"),
            ("x-operator-grant", grant_id),
        ],
    )
    .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    let credential_file = root.path().join("cli-token");
    fs::write(&credential_file, token).unwrap();
    fs::set_permissions(&credential_file, fs::Permissions::from_mode(0o600)).unwrap();
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let origin = format!("http://{address}");
    let state = server::AppState::new(
        f.store.clone(),
        SecretVault::open(
            &f._state.path().join("secrets"),
            &f._state.path().join("master.key"),
        )
        .unwrap(),
        server::WebPolicy::new(&origin, address, true).unwrap(),
    )
    .with_historical_exports(cli_exports);
    let app = server::router(state, tower_sessions::cookie::Key::generate());
    let task = tokio::spawn(async move {
        axum::serve(socket, app).await.unwrap();
    });
    let reference = reference.to_string();
    let args = [
        "--idempotency-key",
        "actual",
        "--operator-grant",
        grant_id,
        "migrate",
        "import",
        "--export-ref",
        &reference,
    ];
    let mut wrong_args = args.to_vec();
    wrong_args.push("--dry-run");
    let wrong = client::invoke(&origin, &credential_file, &wrong_args, Value::Null).await;
    assert!(!wrong.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&wrong.stderr).unwrap()["status"],
        403
    );
    let output = client::invoke(&origin, &credential_file, &args, Value::Null).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let imported: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(imported["resource"]["new_rows"], "1");
    let retry = client::invoke(&origin, &credential_file, &args, Value::Null).await;
    assert!(retry.status.success());
    let retry: Value = serde_json::from_slice(&retry.stdout).unwrap();
    assert_eq!(retry["resource"], imported["resource"]);
    assert_eq!(retry["replayed"], true);
    let report_id = imported["resource"]["id"].as_str().unwrap();
    let read = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "report", report_id],
        Value::Null,
    )
    .await;
    assert!(read.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&read.stdout).unwrap(),
        imported["resource"]
    );
    let other = client::invoke(
        &origin,
        &credential_file,
        &[
            "migrate",
            "report",
            dry.body["resource"]["id"].as_str().unwrap(),
        ],
        Value::Null,
    )
    .await;
    assert!(!other.status.success());
    let other: Value = serde_json::from_slice(&other.stderr).unwrap();
    assert_eq!(other["status"], 404);
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.historical_records WHERE original_key->>'id'='9007199254740993'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 1);
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
    let listing = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "reports", "--limit", "1"],
        Value::Null,
    )
    .await;
    assert!(listing.status.success());
    let listing: Value = serde_json::from_slice(&listing.stdout).unwrap();
    assert_eq!(listing["items"].as_array().unwrap().len(), 1);
    assert_eq!(listing["items"][0]["id"], report_id);
    assert!(listing["next_cursor"].is_null());
    let mappings = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "mappings", report_id, "--limit", "1"],
        Value::Null,
    )
    .await;
    assert!(mappings.status.success());
    let mappings: Value = serde_json::from_slice(&mappings.stdout).unwrap();
    assert_eq!(
        mappings["items"][0]["key"]["values"]["id"],
        "9007199254740993"
    );
    assert_eq!(mappings["items"][0]["first_import_id"], report_id);
    let source_read = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "source", report_id],
        Value::Null,
    )
    .await;
    assert!(source_read.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&source_read.stdout).unwrap(),
        serde_json::to_value(&source).unwrap()
    );
    for command in ["mappings", "source"] {
        let denied = client::invoke(
            &origin,
            &credential_file,
            &[
                "migrate",
                command,
                dry.body["resource"]["id"].as_str().unwrap(),
            ],
            Value::Null,
        )
        .await;
        assert!(!denied.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
            404
        );
    }
    let record = mappings["items"][0]["id"].as_str().unwrap();
    let field_list = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "fields", report_id, record],
        Value::Null,
    )
    .await;
    assert!(field_list.status.success());
    let field_list: Value = serde_json::from_slice(&field_list.stdout).unwrap();
    assert!(field_list["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "id" && f["character_count"] == "16"));
    assert!(!field_list["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "payload"));
    let content = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "field", report_id, record, "id"],
        Value::Null,
    )
    .await;
    assert!(content.status.success());
    let content: Value = serde_json::from_slice(&content.stdout).unwrap();
    assert_eq!(content["text"], "9007199254740993");
    assert_eq!(content["offset"], "0");
    let no_content = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "field", report_id, record, "payload"],
        Value::Null,
    )
    .await;
    assert!(!no_content.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&no_content.stderr).unwrap()["status"],
        404
    );
    let dry_id = dry.body["resource"]["id"].as_str().unwrap();
    let denied = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "field", dry_id, record, "id"],
        Value::Null,
    )
    .await;
    assert!(!denied.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
        404
    );
    task.abort();
}

#[sqlx::test(migrations = "../../migrations")]
async fn registered_public_artifacts_are_report_scoped_native_downloads(pool: PgPool) {
    use integrations::artifacts::ArtifactStore;
    use tower::ServiceExt;
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("rows");
    fs::create_dir(&directory).unwrap();
    sqlx::raw_sql("CREATE TABLE public.alembic_version(version_num text NOT NULL); INSERT INTO public.alembic_version VALUES('0029_portfolio_candidate_exposure'); CREATE TABLE public.mission_artifacts(id uuid PRIMARY KEY,mission_id uuid NOT NULL,turn_id uuid,kind varchar(80) NOT NULL,revision integer NOT NULL,schema_version varchar(40) NOT NULL,state varchar(40) NOT NULL,storage_uri text NOT NULL,metadata jsonb NOT NULL,created_at timestamptz NOT NULL); INSERT INTO public.mission_artifacts SELECT ('00000000-0000-4000-8000-'||lpad(n::text,12,'0'))::uuid,'11111111-1111-4111-8111-111111111111',NULL,'REPORT',1,'1','AVAILABLE','excluded-original-path','{}','2020-01-01' FROM generate_series(1,3) n").execute(&pool).await.unwrap();
    let installation = Id::new();
    let mut files = std::collections::BTreeMap::<Id, Vec<u8>>::new();
    let report = store::Store::from_pool(pool.clone())
        .export_historical_rows(installation, |id, chunk| {
            files.entry(id).or_default().extend_from_slice(chunk);
            Ok(())
        })
        .await
        .unwrap();
    for (id, bytes) in files {
        fs::write(directory.join(format!("{id}.csv")), bytes).unwrap();
    }
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    let old = root.path().join("old");
    fs::create_dir(&old).unwrap();
    let bytes = b"\xff\0PUBLIC HTTP COPY";
    fs::write(old.join("public.bin"), bytes).unwrap();
    let selection = root.path().join("selection.json");
    let identity = |n| json!({"kind":"ARTIFACT","source_table":"mission_artifacts","source_id":format!("00000000-0000-4000-8000-{n:012}")});
    fs::write(&selection,serde_json::to_vec(&json!({"schema_version":1,"source_installation_id":installation,"artifacts":[
        {"identity":identity(1),"relative_path":"public.bin","disposition":"COPY_PUBLIC"},
        {"identity":identity(2),"relative_path":"never-open-sealed.bin","disposition":"SEALED_RETAINED"}
    ]})).unwrap()).unwrap();
    let artifact_directory = root.path().join("artifact-export");
    let status = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .args(["export-historical-artifacts", "--source-root"])
        .arg(&old)
        .arg("--selection")
        .arg(&selection)
        .arg("--output")
        .arg(&artifact_directory)
        .output()
        .await
        .unwrap()
        .status;
    assert!(status.success());
    let reference = Id::new();
    let exports = HistoricalExports::load(vec![ExportRegistration {
        export_ref: reference,
        directory: directory.clone(),
        artifact_directory: Some(artifact_directory.clone()),
    }])
    .unwrap();
    let cli_exports = HistoricalExports::load(vec![ExportRegistration {
        export_ref: reference,
        directory: directory.clone(),
        artifact_directory: Some(artifact_directory.clone()),
    }])
    .unwrap();
    // After registration neither the old file nor the exported object remains trusted.
    let artifact_report: contracts::imports::HistoricalArtifactExportV1 =
        serde_json::from_slice(&fs::read(artifact_directory.join("report.json")).unwrap()).unwrap();
    let native = ArtifactStore::open(&artifact_directory.join("objects")).unwrap();
    let object = artifact_report.artifacts[0].object_ref.unwrap();
    native.discard_unpublished(object).unwrap();
    fs::write(old.join("public.bin"), "replaced").unwrap();
    let f = support::fixture_with_deployment(pool.clone(), None, Some(exports)).await;
    let login = support::local_session(&f).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap_or(initial);
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "https://localhost"),
        ("idempotency-key", "dry-artifacts"),
    ];
    let dry = send(
        &f,
        "POST",
        "/api/v2/migrations/import",
        json!({"schema_version":1,"export_ref":reference,"dry_run":true}),
        &headers,
    )
    .await;
    assert_eq!(dry.status, StatusCode::ACCEPTED, "{}", dry.body);
    let dry_id = dry.body["resource"]["id"].as_str().unwrap();
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "https://localhost"),
        ("idempotency-key", "actual-artifacts"),
    ];
    let actual = send(
        &f,
        "POST",
        "/api/v2/migrations/import",
        json!({"schema_version":1,"export_ref":reference,"dry_run":false}),
        &headers,
    )
    .await;
    assert_eq!(actual.status, StatusCode::ACCEPTED, "{}", actual.body);
    let id = actual.body["resource"]["id"].as_str().unwrap();
    let get_headers = [("cookie", cookie.as_str())];
    let summary = send(
        &f,
        "GET",
        &format!("/api/v2/migrations/reports/{id}/artifacts/summary"),
        Value::Null,
        &get_headers,
    )
    .await;
    assert_eq!(summary.status, StatusCode::OK);
    assert_eq!(summary.body["stored_records"], "1");
    assert_eq!(summary.body["source_records"], "3");
    let list = send(
        &f,
        "GET",
        &format!("/api/v2/migrations/reports/{id}/artifacts?limit=100"),
        Value::Null,
        &get_headers,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK);
    let items = list.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    let record = items.iter().find(|r| r["stored"] == true).unwrap()["record_id"]
        .as_str()
        .unwrap();
    let request = Request::builder()
        .uri(format!(
            "/api/v2/migrations/reports/{id}/artifacts/{record}/content"
        ))
        .header(header::HOST, "localhost")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = f.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    assert!(response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 64 * 1024 * 1024)
            .await
            .unwrap()
            .as_ref(),
        bytes
    );
    for (report, record) in [
        (dry_id, record),
        (
            id,
            items
                .iter()
                .find(|r| r["source_outcome"] == "SEALED_RETAINED")
                .unwrap()["record_id"]
                .as_str()
                .unwrap(),
        ),
    ] {
        let denied = send(
            &f,
            "GET",
            &format!("/api/v2/migrations/reports/{report}/artifacts/{record}/content"),
            Value::Null,
            &get_headers,
        )
        .await;
        assert_eq!(denied.status, StatusCode::NOT_FOUND);
    }

    let project=client::browser(&f,&cookie,"copy-project","/api/v2/projects",json!({"schema_version":1,"name":"Copy CLI","description":"History","fork_from_project_id":null})).await;
    let principal=client::browser(&f,&cookie,"copy-cli","/api/v2/machine-principals",json!({"schema_version":1,"name":"Copy reader","kind":"CLI","project_id":project.body["resource"]["id"],"downstream_id":null,"enabled":true})).await;
    let credential=client::browser(&f,&cookie,"copy-token",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let bearer = format!("Bearer {token}");
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let body = json!({"schema_version":1,"export_ref":reference,"dry_run":false});
    let grant=send(&f,"POST","/api/v2/auth/operator-command-grants",json!({"schema_version":1,"command":{"operation":"MIGRATION_IMPORT","request":body},"target_id":null}),&[("authorization",&bearer),("idempotency-key","copy-grant")]).await;
    assert_eq!(grant.status, StatusCode::CREATED);
    let credential_file = root.path().join("credential");
    fs::write(&credential_file, token).unwrap();
    fs::set_permissions(&credential_file, fs::Permissions::from_mode(0o600)).unwrap();
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let origin = format!("http://{address}");
    let state = server::AppState::new(
        f.store.clone(),
        SecretVault::open(
            &f._state.path().join("secrets"),
            &f._state.path().join("master.key"),
        )
        .unwrap(),
        server::WebPolicy::new(&origin, address, true).unwrap(),
    )
    .with_historical_exports(cli_exports)
    .with_historical_artifact_store(
        ArtifactStore::open(&f._state.path().join("historical-artifacts")).unwrap(),
    );
    let app = server::router(state, tower_sessions::cookie::Key::generate());
    let task = tokio::spawn(async move { axum::serve(socket, app).await.unwrap() });
    let imported = client::invoke(
        &origin,
        &credential_file,
        &[
            "--idempotency-key",
            "cli-copy",
            "--operator-grant",
            grant.body["resource"]["id"].as_str().unwrap(),
            "migrate",
            "import",
            "--export-ref",
            &reference.to_string(),
        ],
        Value::Null,
    )
    .await;
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let imported: Value = serde_json::from_slice(&imported.stdout).unwrap();
    let cli_report = imported["resource"]["id"].as_str().unwrap();
    for command in ["artifact-summary", "artifacts"] {
        let output = client::invoke(
            &origin,
            &credential_file,
            &["migrate", command, cli_report],
            Value::Null,
        )
        .await;
        assert!(output.status.success());
        let denied = client::invoke(
            &origin,
            &credential_file,
            &["migrate", command, id],
            Value::Null,
        )
        .await;
        assert!(!denied.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
            404
        );
    }
    let metadata = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "artifact", cli_report, record],
        Value::Null,
    )
    .await;
    assert!(metadata.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&metadata.stdout).unwrap()["record_id"],
        record
    );
    let download = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "download", cli_report, record],
        Value::Null,
    )
    .await;
    assert!(download.status.success());
    assert_eq!(download.stdout, bytes);
    let denied = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "download", id, record],
        Value::Null,
    )
    .await;
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    let sealed = items
        .iter()
        .find(|r| r["source_outcome"] == "SEALED_RETAINED")
        .unwrap()["record_id"]
        .as_str()
        .unwrap();
    let denied = client::invoke(
        &origin,
        &credential_file,
        &["migrate", "download", cli_report, sealed],
        Value::Null,
    )
    .await;
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    task.abort();
    let _ = task.await;
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM app.artifacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(active, 0);
}
