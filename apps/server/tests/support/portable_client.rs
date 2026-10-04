//! Execute the separately built portable CLI from the same candidate as the tests.
//! The caller builds it explicitly and supplies QUAZONAI_NATIVE_CLI_BIN; no fallback.
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

pub async fn saved(directory: &Path, arguments: &[&str], body: Value) -> std::process::Output {
    let binary = std::env::var_os("QUAZONAI_NATIVE_CLI_BIN")
        .map(std::path::PathBuf::from)
        .expect("build this candidate with cargo build --locked -p quazonai-cli --bin quazonai, then set QUAZONAI_NATIVE_CLI_BIN to its absolute executable path");
    assert!(
        binary.is_absolute() && binary.is_file(),
        "QUAZONAI_NATIVE_CLI_BIN must be an existing absolute portable CLI path"
    );
    let mut child = Command::new(binary)
        .arg("client")
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .env("XDG_CONFIG_HOME", directory)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("execute the explicitly supplied portable CLI");
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
        .expect("portable CLI HTTP transaction deadline")
        .unwrap()
}
