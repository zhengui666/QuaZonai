//! Shared native archive procedure; credentials and archived payloads stay private.
use crate::support::{self, SIGNAL};
use std::{fs, os::unix::fs::OpenOptionsExt, path::Path, time::Duration};

pub(super) async fn tar(stage: &str, mode: &str, archive: &Path, directory: &Path) {
    if mode == "--create" {
        // Keep the archive private and owned by the invoking operator, even
        // though native tar needs privilege for mixed-UID job files.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(archive)
            .unwrap();
    }
    let mut command = tokio::process::Command::new("sudo");
    command
        .env_clear()
        .args(["-n", "--", "tar", mode, "--numeric-owner", "--file"])
        .arg(archive)
        .arg("--directory")
        .arg(directory)
        .kill_on_drop(true);
    if mode == "--create" {
        command.arg(".");
    } else if mode == "--extract" {
        command.args(["--same-owner", "--preserve-permissions"]);
    }
    let result = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("native archive command deadline")
        .expect("native GNU tar and noninteractive sudo must be available");
    // GNU tar reports metadata/operation diagnostics, never archived payloads.
    // Keep test-owned path and credential/source sentinels out of failure logs.
    let root = archive.parent().unwrap().to_string_lossy();
    let diagnostic = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .replace(root.as_ref(), "[fixture]")
            .replace(support::SECRET, "[redacted]")
            .replace(SIGNAL, "[redacted source]")
            .chars()
            .take(2048)
            .collect::<String>()
    };
    assert!(
        result.status.success() && result.stdout.is_empty() && result.stderr.is_empty(),
        "native archive {stage} {mode}: status={}, stdout={:?}, stderr={:?}",
        result.status,
        diagnostic(&result.stdout),
        diagnostic(&result.stderr)
    );
    println!("native archive stage={stage} mode={mode}: passed");
}
