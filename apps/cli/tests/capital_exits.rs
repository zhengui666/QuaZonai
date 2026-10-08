//! Installed portable CLI contract checks; no financial requests are sent.
use serde_json::{json, Value};
use std::{io::Write, process::{Command, Output, Stdio}};
const ID: &str = "018fc823-8e40-7000-8000-000000000001";
fn invoke(args: &[&str], input: Option<Value>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_quazonai")).args(args)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    if let Some(input) = input { child.stdin.take().unwrap().write_all(&serde_json::to_vec(&input).unwrap()).unwrap(); }
    drop(child.stdin.take());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() { return child.wait_with_output().unwrap(); }
        if std::time::Instant::now() >= deadline { child.kill().unwrap(); child.wait().unwrap(); panic!("bounded CLI test timed out"); }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn preview(tail: &[&str], body: Option<Value>) -> Value {
    let mut args = vec!["client", "--origin", "http://localhost:9", "--development-http", "--credential-file", "missing-credential", "--preview", "--idempotency-key", "original-capital-exit-operation", "forward", "accounts", "exits"];
    args.extend(tail);
    let result = invoke(&args, body);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let plan: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["request_sent"], false);
    plan
}
#[test]
fn capital_exit_help_explains_preview_execution_and_withdrawal_boundaries() {
    for (command, expected) in [("preview", "local transport syntax"), ("start", "native reductions"), ("pause", "reserve remains"), ("cancel", "reserve is not reinvested"), ("resume", "never buy back"), ("reconcile", "never performs a transfer")] {
        let result = invoke(&["client", "forward", "accounts", "exits", command, "--help"], None);
        assert!(result.status.success());
        assert!(String::from_utf8(result.stdout).unwrap().contains(expected));
    }
}
#[test]
fn capital_exit_commands_use_native_typed_bodies_and_original_routes() {
    let preview_body = json!({"schema_version":1,"account_source_id":ID,"expected_source_observation_id":ID,
        "scope":{"kind":"AMOUNT","amount":"1234567890.123456789012345678","currency":"USD"},"policy":{"kind":"CASH_ONLY"}});
    let plan = preview(&["preview", "--project-id", ID], Some(preview_body.clone()));
    assert_eq!(plan["route"], format!("/api/v2/projects/{ID}/capital-exit-previews"));
    assert_eq!(plan["expected_http_status"], 201); assert_eq!(plan["requires_idempotency_key"], true);
    let start = json!({"schema_version":1,"preview_id":ID,"expected_account_control_revision":"9007199254740993","acknowledged_plan_artifact_id":ID,"expected_source_observation_id":ID});
    let plan = preview(&["start", "--project-id", ID], Some(start));
    assert_eq!(plan["route"], format!("/api/v2/projects/{ID}/capital-exits")); assert_eq!(plan["expected_http_status"], 202);
    for (command, action, route) in [("pause", "PAUSE", "pause"), ("cancel", "CANCEL", "cancel"), ("resume", "RESUME", "resume"), ("reconcile", "RECONCILE_WITHDRAWAL", "reconcile-withdrawal")] {
        let mut body = json!({"schema_version":1,"action":action,"expected_revision":"9007199254740993"});
        if action == "RESUME" { body["preview_id"] = ID.into(); }
        if action == "RECONCILE_WITHDRAWAL" { body["user_reported_amount"] = "1.000000001".into(); body["currency"] = "USD".into(); body["external_transfer_ref"] = Value::Null; }
        let plan = preview(&[command, ID], Some(body));
        assert_eq!(plan["route"], format!("/api/v2/capital-exits/{ID}/{route}")); assert_eq!(plan["expected_http_status"], 202);
        assert_eq!(plan["requires_idempotency_key"], true); assert_eq!(plan["body_redacted"], true);
    }
    assert_eq!(preview(&["list", "--project-id", ID], None)["route"], format!("/api/v2/projects/{ID}/capital-exits"));
    assert_eq!(preview(&["show", ID], None)["route"], format!("/api/v2/capital-exits/{ID}"));
}
#[test]
fn capital_exit_skill_is_portable_and_distinguishes_simulation_and_unknown_effects() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/quazonai");
    let entry = std::fs::read_to_string(root.join("SKILL.md")).unwrap();
    let procedure = std::fs::read_to_string(root.join("references/capital-exits.md")).unwrap();
    assert!(entry.contains("references/capital-exits.md"));
    for expected in ["original intent ID", "unchanged body", "same key", "SIMULATED", "CANCELLED_RESERVED", "FENCED_PENDING_TARGET", "not a wallet lock", "No withdrawal command", "not insurance", "explicit authorization"] { assert!(procedure.contains(expected), "missing {expected}"); }
}
