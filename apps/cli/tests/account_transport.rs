//! Portable subprocess checks for retained account transport and offline preview.
use serde_json::Value;
use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};

const FIXTURE: &[u8] =
    include_bytes!("../../../tests/contracts/native-account-paper-snapshot.json");

fn invoke(arguments: &[&str], input: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_quazonai"))
        .args(arguments)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("account CLI subprocess exceeded its bounded deadline");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn relay_help_and_preview_need_neither_credential_nor_write_key() {
    let help = invoke(&["client", "forward", "accounts", "relay", "--help"], None);
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for flag in ["--input", "--follow", "--max-attempts"] {
        assert!(help.contains(flag));
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    fs::write(&path, FIXTURE).unwrap();
    let result = invoke(
        &[
            "client",
            "--origin",
            "http://localhost:9",
            "--credential-file",
            "missing-credential",
            "--preview",
            "forward",
            "accounts",
            "relay",
            "--input",
            path.to_str().unwrap(),
        ],
        None,
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let plan: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["route"], "/api/v2/forward/account-observations");
    assert_eq!(plan["requires_idempotency_key"], false);
    assert_eq!(plan["request_sent"], false);
    assert_eq!(plan["body_redacted"], true);
    assert_eq!(plan["body_bytes"], FIXTURE.len());
    assert!(!String::from_utf8_lossy(&result.stdout).contains("SIM-001"));
    assert_eq!(fs::read(path).unwrap(), FIXTURE);
}

#[test]
fn account_submit_preview_also_uses_native_identity_without_a_write_key() {
    let result = invoke(
        &[
            "client",
            "--origin",
            "http://localhost:9",
            "--credential-file",
            "missing-credential",
            "--preview",
            "forward",
            "accounts",
            "submit",
        ],
        Some(FIXTURE),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let plan: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["requires_idempotency_key"], false);
    assert_eq!(plan["request_sent"], false);
}

#[test]
fn relay_rejects_partial_records_and_file_wide_keys_without_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    let arguments = [
        "client",
        "--origin",
        "http://localhost:9",
        "--credential-file",
        "missing-credential",
        "--preview",
        "forward",
        "accounts",
        "relay",
        "--input",
        path.to_str().unwrap(),
    ];
    fs::write(&path, &FIXTURE[..FIXTURE.len() - 1]).unwrap();
    let result = invoke(&arguments, None);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let problem: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(
        problem["code"],
        "CLI_ACCOUNT_STREAM_INCOMPLETE_RETAIN_INPUT"
    );
    fs::write(&path, FIXTURE).unwrap();
    for extra in [
        vec!["--idempotency-key", "one-key-for-the-whole-file"],
        vec!["--follow"],
    ] {
        let mut arguments = arguments.to_vec();
        arguments.extend(extra);
        let result = invoke(&arguments, None);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let problem: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(problem["code"], "CLI_INPUT_INVALID");
    }
    assert_eq!(fs::read(path).unwrap(), FIXTURE);
}

#[test]
#[cfg(unix)]
fn permanent_intake_rejection_emits_no_receipt_and_keeps_both_records() {
    use std::{
        io::Read,
        net::TcpListener,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    let retained = [FIXTURE, FIXTURE].concat();
    fs::write(&path, &retained).unwrap();
    let credential = directory.path().join("credential");
    let token = integrations::authentication::format_machine_token(
        contracts::Id::new(),
        &integrations::authentication::random_capability(),
    )
    .unwrap();
    fs::write(&credential, &token).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "relay did not send the account envelope"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("account listener failed: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
            assert!(headers.len() < 16 * 1024);
        }
        let headers = String::from_utf8(headers).unwrap();
        assert!(headers.starts_with("POST /api/v2/forward/account-observations HTTP/1.1\r\n"));
        assert!(!headers.contains("idempotency-key:"));
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(length, FIXTURE.len());
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        assert_eq!(body, FIXTURE);
        let problem = serde_json::to_vec(&serde_json::json!({
            "type": "urn:quazonai:problem:native_identity_conflict",
            "title": "Conflict", "status": 409, "code": "native_identity_conflict",
            "detail": "Changed envelope", "request_id": contracts::Id::new(),
            "retryable": false, "field_errors": [], "safe_next_actions": []
        }))
        .unwrap();
        write!(stream, "HTTP/1.1 409 Conflict\r\nContent-Type: application/problem+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", problem.len()).unwrap();
        stream.write_all(&problem).unwrap();
        listener
    });
    let output = invoke(
        &[
            "client",
            "--origin",
            &origin,
            "--credential-file",
            credential.to_str().unwrap(),
            "forward",
            "accounts",
            "relay",
            "--input",
            path.to_str().unwrap(),
            "--max-attempts",
            "3",
        ],
        None,
    );
    let listener = server.join().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let failure: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure["problem"]["code"], "native_identity_conflict");
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&token));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read(path).unwrap(), retained);
}
