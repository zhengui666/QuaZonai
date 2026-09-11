//! Actual CLI -> TCP -> native Axum/Bearer/TOTP/PostgreSQL data administration.
//! Every credential and service here is disposable; no authority is injected into a route.
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use contracts::Id;
use integrations::secrets::SecretVault;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};
use tokio::{io::AsyncWriteExt, net::TcpListener, process::Command, task::JoinHandle};

struct Listener(JoinHandle<()>);
impl Drop for Listener {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn browser(
    f: &support::Fixture,
    cookie: &str,
    key: &str,
    path: &str,
    body: Value,
) -> support::Reply {
    support::exchange(
        &f.app,
        Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, "research.example")
            .header(header::ORIGIN, "https://research.example")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}
async fn invoke(
    origin: &str,
    credential: &Path,
    arguments: &[&str],
    body: Value,
) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("client")
        .arg("--origin")
        .arg(origin)
        .arg("--credential-file")
        .arg(credential)
        .arg("--development-http")
        .args(arguments)
        .env_clear()
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    if !body.is_null() {
        input
            .write_all(&serde_json::to_vec(&body).unwrap())
            .await
            .unwrap();
    }
    drop(input);
    tokio::time::timeout(Duration::from_secs(25), child.wait_with_output())
        .await
        .expect("native CLI transaction deadline")
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_cli_human_grant_source_creation_replay_and_intent_binding_are_real_transactions(
    pool: PgPool,
) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (confirmed, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap_or(initial);
    let principal = browser(&f, &cookie, "cli-principal", "/api/v2/machine-principals", json!({
        "schema_version":1,"name":"Native CLI acceptance","kind":"CLI","project_id":null,"downstream_id":null,"enabled":true
    })).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "cli-credential", &format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()), json!({
        "schema_version":1,"scope_codes":["DOCTOR_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)
    })).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let credential_file = f._state.path().join("native-cli-token");
    fs::write(&credential_file, token).unwrap();
    fs::set_permissions(&credential_file, fs::Permissions::from_mode(0o600)).unwrap();
    let runtime_secret = browser(
        &f,
        &cookie,
        "runtime-credential",
        "/api/v2/settings/credentials",
        json!({
            "intent":{"schema_version":1,"purpose":"RUNTIME","label":"Disposable native Runtime"},
            "value":integrations::authentication::random_capability()
        }),
    )
    .await;
    assert_eq!(runtime_secret.status, StatusCode::CREATED);
    let runtime = browser(&f, &cookie, "runtime", "/api/v2/integrations/runtimes", json!({
        "schema_version":1,"configuration":{"name":"Unprobed native registry","endpoint":"https://runtime.invalid","tls_policy":"SYSTEM_CA","allowed_capabilities":["DATA_VALIDATE"],"enabled":true,"development_http":false},
        "credential_ref":runtime_secret.body["resource"]["id"],"ca_certificate_ref":null
    })).await;
    assert_eq!(runtime.status, StatusCode::CREATED);

    // The same native Store and vault, with an explicitly allowed test-loopback
    // ingress. Browser enrollment above is real; this second listener uses only Bearer.
    let socket = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
    );
    let app = server::router(state, tower_sessions::cookie::Key::generate());
    let _listener = Listener(tokio::spawn(async move {
        axum::serve(socket, app).await.unwrap();
    }));
    let body = json!({"schema_version":1,"name":"Native CLI source","runtime_id":runtime.body["resource"]["id"],"native_catalog_ref":"registered/cli-fixture","provider_kind":"NAUTILUS_CATALOG","enabled":true});
    let denied = invoke(
        &origin,
        &credential_file,
        &[
            "--idempotency-key",
            "source-create",
            "data",
            "source",
            "create",
        ],
        body.clone(),
    )
    .await;
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let human = invoke(&origin, &credential_file, &["--idempotency-key","source-human-grant","operator-grant"], json!({
        "schema_version":1,"command":{"operation":"DATA_SOURCE_CREATE","request":body},"target_id":null,"code":totp.generate((now/30+1)*30)
    })).await;
    assert!(human.status.success(), "native human grant failed");
    assert!(human.stderr.is_empty());
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let grant_id = grant["resource"]["id"].as_str().unwrap();
    let expected_id = grant["resource"]["target_id"].as_str().unwrap();
    let command = &[
        "--idempotency-key",
        "source-create",
        "--operator-grant",
        grant_id,
        "data",
        "source",
        "create",
    ];
    let mut receipt = Value::Null;
    for replay in [false, true] {
        let output = invoke(&origin, &credential_file, command, body.clone()).await;
        assert!(output.status.success(), "native data source command failed");
        assert!(output.stderr.is_empty());
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains(token),
            "native CLI credential leaked"
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["replayed"], replay);
        assert_eq!(value["resource"]["id"], expected_id);
        if replay {
            assert_eq!(value["resource"], receipt);
        } else {
            receipt = value["resource"].clone();
        }
    }
    let listed = invoke(
        &origin,
        &credential_file,
        &["data", "source", "list", "--limit", "1"],
        Value::Null,
    )
    .await;
    assert!(listed.status.success());
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed["items"][0], receipt);
    let mut conflict = body.clone();
    conflict["name"] = json!("Different immutable intent");
    let conflict = invoke(&origin, &credential_file, command, conflict).await;
    assert!(!conflict.status.success());
    assert!(conflict.stdout.is_empty());
    let error: Value = serde_json::from_slice(&conflict.stderr).unwrap();
    // The original one-time grant authorizes only the exact original intent.
    // The native authority gate rejects this different body before receipt lookup.
    assert_eq!(error["status"], 403);
    assert_eq!(error["code"], "FORBIDDEN");
    let runtime_id: Id = runtime.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.data_sources WHERE runtime_id=$1 AND native_catalog_ref='registered/cli-fixture'")
        .bind(runtime_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    // Read-only Doctor identity did not acquire arbitrary project or research access.
    let private = invoke(&origin, &credential_file, &["project", "list"], Value::Null).await;
    assert!(!private.status.success());
    assert!(private.stdout.is_empty());
    let error: Value = serde_json::from_slice(&private.stderr).unwrap();
    assert_eq!(error["status"], 403);
}
