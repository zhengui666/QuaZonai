//! Actual native CLI/TCP helpers shared by integration tests.
use super::support;
use axum::{
    body::Body,
    http::{header, Request},
};
use integrations::secrets::SecretVault;
use serde_json::Value;
use std::{path::Path, time::Duration};
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
    let state = state.with_downstream_targets(targets).with_artifact_store(
        integrations::artifacts::ArtifactStore::open(&f._state.path().join("artifacts")).unwrap(),
    );
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
pub async fn invoke(
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
