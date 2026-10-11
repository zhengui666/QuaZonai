//! Installed portable CLI contract checks; no financial requests are sent.
use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};
const ID: &str = "018fc823-8e40-7000-8000-000000000001";
fn invoke(args: &[&str], input: Option<Value>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_quazonai"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&input).unwrap())
            .unwrap();
    }
    drop(child.stdin.take());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("bounded CLI test timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn preview(tail: &[&str], body: Option<Value>) -> Value {
    let mut args = vec![
        "client",
        "--origin",
        "http://localhost:9",
        "--development-http",
        "--credential-file",
        "missing-credential",
        "--preview",
        "--idempotency-key",
        "original-capital-exit-operation",
        "forward",
        "accounts",
        "exits",
    ];
    args.extend(tail);
    let result = invoke(&args, body);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let plan: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["request_sent"], false);
    plan
}
#[test]
fn capital_exit_help_explains_preview_execution_and_withdrawal_boundaries() {
    for (command, expected) in [
        ("preview", "local transport syntax"),
        ("start", "native reductions"),
        ("pause", "reserve remains"),
        ("cancel", "reserve is not reinvested"),
        ("resume", "never buy back"),
        ("reconcile", "never performs a transfer"),
    ] {
        let result = invoke(
            &["client", "forward", "accounts", "exits", command, "--help"],
            None,
        );
        assert!(result.status.success());
        let help = String::from_utf8(result.stdout).unwrap();
        assert!(help.contains(expected));
        if command == "preview" {
            assert!(
                help.contains("AMOUNT")
                    && help.contains("PORTFOLIO_SCOPE")
                    && help.contains("unsupported")
            );
        }
        if ["pause", "cancel", "resume"].contains(&command) {
            assert!(help.contains("202 means accepted"));
        }
        if command == "pause" {
            assert!(
                help.contains("even without reasons")
                    && help.contains("does not prove all orders stopped")
            );
        }
    }
}
#[test]
fn capital_exit_commands_use_native_typed_bodies_and_original_routes() {
    let preview_body = json!({"schema_version":1,"account_source_id":ID,"expected_source_observation_id":ID,
        "scope":{"kind":"AMOUNT","amount":"1234567890.123456789012345678","currency":"USD"},"policy":{"kind":"CASH_ONLY"}});
    let plan = preview(&["preview", "--project-id", ID], Some(preview_body.clone()));
    assert_eq!(
        plan["route"],
        format!("/api/v2/projects/{ID}/capital-exit-previews")
    );
    assert_eq!(plan["expected_http_status"], 201);
    assert_eq!(plan["requires_idempotency_key"], true);
    let start = json!({"schema_version":1,"preview_id":ID,"expected_account_control_revision":"9007199254740993","acknowledged_plan_artifact_id":ID,"expected_source_observation_id":ID});
    let plan = preview(&["start", "--project-id", ID], Some(start));
    assert_eq!(
        plan["route"],
        format!("/api/v2/projects/{ID}/capital-exits")
    );
    assert_eq!(plan["expected_http_status"], 202);
    for (command, action, route) in [
        ("pause", "PAUSE", "pause"),
        ("cancel", "CANCEL", "cancel"),
        ("resume", "RESUME", "resume"),
        ("reconcile", "RECONCILE_WITHDRAWAL", "reconcile-withdrawal"),
    ] {
        let mut body =
            json!({"schema_version":1,"action":action,"expected_revision":"9007199254740993"});
        if action == "RESUME" {
            body["preview_id"] = ID.into();
        }
        if action == "RECONCILE_WITHDRAWAL" {
            body["user_reported_amount"] = "1.000000001".into();
            body["currency"] = "USD".into();
            body["external_transfer_ref"] = Value::Null;
        }
        let plan = preview(&[command, ID], Some(body));
        assert_eq!(plan["route"], format!("/api/v2/capital-exits/{ID}/{route}"));
        assert_eq!(plan["expected_http_status"], 202);
        assert_eq!(plan["requires_idempotency_key"], true);
        assert_eq!(plan["body_redacted"], true);
    }
    assert_eq!(
        preview(&["list", "--project-id", ID], None)["route"],
        format!("/api/v2/projects/{ID}/capital-exits")
    );
    assert_eq!(
        preview(&["show", ID], None)["route"],
        format!("/api/v2/capital-exits/{ID}")
    );
}
#[test]
fn capital_exit_skill_is_portable_and_distinguishes_simulation_and_unknown_effects() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/quazonai");
    let entry = std::fs::read_to_string(root.join("SKILL.md")).unwrap();
    let procedure = std::fs::read_to_string(root.join("references/capital-exits.md")).unwrap();
    assert!(entry.contains("references/capital-exits.md"));
    for expected in [
        "original intent ID",
        "unchanged body",
        "same key",
        "SIMULATED",
        "CANCELLED_RESERVED",
        "FENCED_PENDING_TARGET",
        "not a wallet lock",
        "No withdrawal command",
        "not insurance",
        "explicit authorization",
        "supports only `AMOUNT`",
        "read-only inspection",
        "even with an empty `reason_codes`",
        "`FenceApplied`",
        "capital_exit_owner_command_pending",
        "capital_exit_owner_fence_pending",
        "capital_exit_deadline_pause_evidence_pending",
        "CLI performs no extra GET",
    ] {
        assert!(procedure.contains(expected), "missing {expected}");
    }
}

#[cfg(unix)]
fn controlled_credential(root: &std::path::Path) -> (std::path::PathBuf, String) {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("controlled-credential");
    let token = integrations::authentication::format_machine_token(
        contracts::Id::new(),
        &integrations::authentication::random_capability(),
    )
    .unwrap();
    std::fs::write(&path, &token).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    (path, token)
}

#[cfg(unix)]
#[test]
fn new_portfolio_preview_is_rejected_before_any_network_send() {
    let directory = tempfile::tempdir().unwrap();
    let (credential, _) = controlled_credential(directory.path());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = json!({"schema_version":1,"account_source_id":ID,"expected_source_observation_id":ID,
        "scope":{"kind":"PORTFOLIO_SCOPE","stream_ids":[ID],"release_ids":[ID],"amount":"100.123456789012345678","currency":"USD"},"policy":{"kind":"CASH_ONLY"}});
    // Normal execution mode with a valid controlled credential, not --preview:
    // a request reaching this listener would demonstrate an actual send.
    let result = invoke(
        &[
            "client",
            "--origin",
            &origin,
            "--credential-file",
            credential.to_str().unwrap(),
            "--idempotency-key",
            "retained-unsupported-preview",
            "--operator-grant",
            ID,
            "forward",
            "accounts",
            "exits",
            "preview",
            "--project-id",
            ID,
        ],
        Some(body),
    );
    assert!(!result.status.success());
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("CLI_INPUT_INVALID"));
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[cfg(unix)]
#[test]
fn amount_preview_start_and_resume_send_exact_bodies_keys_and_auth_without_extra_get() {
    use std::{
        io::Read,
        net::TcpListener,
        time::{Duration, Instant},
    };
    let directory = tempfile::tempdir().unwrap();
    let (credential, token) = controlled_credential(directory.path());
    let cases = [
        (
            vec!["preview", "--project-id", ID],
            format!("/api/v2/projects/{ID}/capital-exit-previews"),
            json!({"schema_version":1,"account_source_id":ID,"expected_source_observation_id":ID,
                "scope":{"kind":"AMOUNT","amount":"1234567890.123456789012345678","currency":"USD"},
                "policy":{"kind":"BOUNDED_LIMIT","deadline":"2027-10-10T12:00:00Z",
                    "legs":[{"instrument_id":"TEST.SIM","maximum_reduction_quantity":"1.123456789012345678","minimum_sell_price":"100.123456789012345678"}],
                    "max_execution_cost":{"amount":"0.000000000000000001","currency":"USD","reference_evidence_id":ID}}}),
        ),
        (
            vec!["start", "--project-id", ID],
            format!("/api/v2/projects/{ID}/capital-exits"),
            json!({"schema_version":1,"preview_id":ID,"expected_account_control_revision":"9007199254740993","acknowledged_plan_artifact_id":ID,"expected_source_observation_id":ID}),
        ),
        (
            vec!["resume", ID],
            format!("/api/v2/capital-exits/{ID}/resume"),
            json!({"schema_version":1,"action":"RESUME","expected_revision":"9007199254740993","preview_id":ID}),
        ),
    ];
    for (tail, route, expected) in cases {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    error => panic!("bounded controlled listener failed: {error:?}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            let header_end = loop {
                let count = socket.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(index) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                    break index + 4;
                }
                assert!(bytes.len() < 65536);
            };
            let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            let headers = header
                .lines()
                .skip(1)
                .filter_map(|line| line.split_once(':'))
                .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
                .collect::<std::collections::HashMap<_, _>>();
            let length = headers["content-length"].parse::<usize>().unwrap();
            assert!(length < 65536);
            while bytes.len() < header_end + length {
                let count = socket.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
            }
            // The controlled listener returns a failure, never a financial
            // preview/execution success. This test covers CLI transport only.
            let problem = serde_json::to_vec(&json!({"type":"urn:quazonai:problem:controlled-fixture","title":"Controlled fixture","status":503,
                "code":"CONTROLLED_FIXTURE","detail":"Transport only","request_id":ID,"retryable":false,"field_errors":[],"safe_next_actions":[]})).unwrap();
            write!(socket, "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/problem+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", problem.len()).unwrap();
            socket.write_all(&problem).unwrap();
            (
                header.lines().next().unwrap().to_owned(),
                headers,
                bytes[header_end..header_end + length].to_vec(),
                problem,
            )
        });
        let mut args = vec![
            "client",
            "--origin",
            &origin,
            "--credential-file",
            credential.to_str().unwrap(),
            "--idempotency-key",
            "original-capital-exit-operation",
            "--operator-grant",
            ID,
            "forward",
            "accounts",
            "exits",
        ];
        args.extend(tail);
        let result = invoke(&args, Some(expected.clone()));
        let (line, headers, body, problem) = server.join().unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&result.stderr).unwrap(),
            serde_json::from_slice::<Value>(&problem).unwrap()
        );
        assert_eq!(line, format!("POST {route} HTTP/1.1"));
        assert_eq!(
            headers["idempotency-key"],
            "original-capital-exit-operation"
        );
        assert_eq!(headers["authorization"], format!("Bearer {token}"));
        assert_eq!(headers["x-operator-grant"], ID);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), expected);
        eprintln!("controlled capital-exit transport verified: POST {route}");
    }
}
