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
        .header(header::HOST, "research.example");
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
            directory: directory.clone()
        },
        ExportRegistration {
            export_ref: reference,
            directory: directory.clone()
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
    let (enrollment, initial, native) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &native, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap_or(initial);
    let request = json!({"schema_version":1,"export_ref":reference,"dry_run":true});
    let denied = send(
        &f,
        "POST",
        "/api/v2/migrations/import",
        request.clone(),
        &[
            ("idempotency-key", "anon"),
            ("origin", "https://research.example"),
        ],
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
    let grant = send(&f,"POST","/api/v2/auth/operator-command-grants",json!({"schema_version":1,"command":{"operation":"MIGRATION_IMPORT","request":actual},"target_id":null,"code":native.generate((now/30+1)*30)}),&[("authorization",&bearer),("idempotency-key","grant")]).await;
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
    task.abort();
}
