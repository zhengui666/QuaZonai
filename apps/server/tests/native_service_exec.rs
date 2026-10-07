//! Fixed exec-bridge component test. No systemd, model, account or external service.
#[cfg(unix)]
#[test]
fn native_service_exec_projects_only_explicit_environment_and_preserves_stdio() {
    use std::{
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("controlled-native");
    fs::write(&binary, b"#!/bin/sh\nread -r qz_input\nprintf '%s|%s|%s|%s' \"$1\" \"$QZ_ALLOWED\" \"${QZ_UNLISTED-unset}\" \"$qz_input\"\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
        .args(["native-codex-exec", "--binary"])
        .arg(&binary)
        .args(["--environment-name", "QZ_ALLOWED"])
        .env_clear()
        .env("QZ_ALLOWED", "controlled-value")
        .env("QZ_UNLISTED", "must-not-reach-native")
        .current_dir(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"controlled-input\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert_eq!(
        result.stdout,
        b"app-server|controlled-value|unset|controlled-input"
    );
    assert!(result.stderr.is_empty());
    let rejected = Command::new(env!("CARGO_BIN_EXE_server"))
        .args(["native-codex-exec", "--binary"])
        .arg(&binary)
        .args(["--environment-name", "QZ_MISSING"])
        .env_clear()
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
}
