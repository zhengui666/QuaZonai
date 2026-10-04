//! Actual native CLI/TCP helpers shared by integration tests.
use super::support;
use axum::{
    body::Body,
    http::{header, Request},
};
use integrations::secrets::SecretVault;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::TcpListener, process::Command, task::JoinHandle};

pub struct Listener(JoinHandle<()>);
impl Drop for Listener {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub async fn listen(f: &support::Fixture) -> (String, Listener) {
    listen_with_downstream_targets(f, server::runtime_transport::RuntimeTargets::default()).await
}

pub async fn listen_with_downstream_targets(
    f: &support::Fixture,
    targets: server::runtime_transport::RuntimeTargets,
) -> (String, Listener) {
    listen_with_resources(
        f.store.clone(),
        SecretVault::open(
            &f._state.path().join("secrets"),
            &f._state.path().join("master.key"),
        )
        .unwrap(),
        integrations::artifacts::ArtifactStore::open(&f._state.path().join("artifacts")).unwrap(),
        targets,
    )
    .await
}

/// Share the same real TCP entry with an existing native fixture's vault and objects.
pub async fn listen_with_resources(
    store: store::Store,
    vault: SecretVault,
    objects: integrations::artifacts::ArtifactStore,
    targets: server::runtime_transport::RuntimeTargets,
) -> (String, Listener) {
    let socket = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let origin = format!("http://localhost:{}", address.port());
    let state = server::AppState::new(
        store,
        vault,
        server::WebPolicy::new(&origin, address, true).unwrap(),
    );
    let state = state
        .with_downstream_targets(targets)
        .with_artifact_store(objects);
    let app = server::router(state, tower_sessions::cookie::Key::generate());
    (
        origin,
        Listener(tokio::spawn(async move {
            axum::serve(socket, app).await.unwrap();
        })),
    )
}

pub async fn browser(
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
            .header(header::HOST, "localhost")
            .header(header::ORIGIN, "https://localhost")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}
pub async fn invoke(
    origin: &str,
    credential: &Path,
    arguments: &[&str],
    body: Value,
) -> std::process::Output {
    let mut command = native_command();
    command
        .arg("--origin")
        .arg(origin)
        .arg("--credential-file")
        .arg(credential)
        .arg("--development-http")
        .args(arguments);
    invoke_command(command, body).await
}

/// The public CLI login issues an owner-device token for the saved profile.
/// Explicit --credential-file remains the separate machine-token entry.
#[allow(dead_code)]
pub async fn owner_profile(f: &support::Fixture, origin: &str) -> PathBuf {
    let directory = f._state.path().join("owner-profile");
    let profile_directory = directory.join("quazonai");
    fs::create_dir_all(&profile_directory).unwrap();
    for path in [&directory, &profile_directory] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = profile_directory.join("client.json");
    let profile = json!({
        "schema_version": 1,
        "origin": origin,
        "token": support::owner_session(f).await,
        "development_http": true,
        "ca_certificate": null
    });
    fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    directory
}

#[allow(dead_code)]
pub async fn invoke_saved(
    directory: &Path,
    arguments: &[&str],
    body: Value,
) -> std::process::Output {
    let mut command = native_command();
    command.env("XDG_CONFIG_HOME", directory).args(arguments);
    invoke_command(command, body).await
}

fn native_command() -> Command {
    let binary = option_env!("CARGO_BIN_EXE_server")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("QUAZONAI_NATIVE_CLI_BIN")
                .expect("native cross-package acceptance requires the built portable CLI")
                .into()
        });
    let mut command = Command::new(binary);
    command
        .arg("client")
        .env_clear()
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    command
}

async fn invoke_command(mut command: Command, body: Value) -> std::process::Output {
    let mut child = command.spawn().unwrap();
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
