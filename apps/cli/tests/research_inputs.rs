//! Actual portable CLI parsing/preview. No connection or scientific evidence.
use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

const DATASET: &str = "018fc823-8e40-7000-8000-000000000001";
const PROJECT: &str = "018fc823-8e40-7000-8000-000000000002";

fn preview(args: &[&str], body: Option<&Value>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_quazonai"))
        .env_clear()
        .args([
            "client",
            "--origin",
            "http://localhost:9",
            "--credential-file",
            "/not-provisioned",
            "--preview",
        ])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Some(body) = body {
        stdin.write_all(&serde_json::to_vec(body).unwrap()).unwrap();
    }
    drop(stdin);
    child.wait_with_output().unwrap()
}
fn success(result: Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn request() -> Value {
    json!({"schema_version":1,"project_id":PROJECT,"dataset_revision_id":DATASET,
        "feature_part_key":"features-0001","content":" \n{\"schema_version\":1,\"original\":1.00}\n"})
}

#[test]
fn recorded_feature_list_is_bounded_project_scoped_read() {
    let result = success(preview(
        &["data", "features", "list", DATASET, "--project-id", PROJECT],
        None,
    ));
    assert_eq!(result["method"], "GET");
    assert_eq!(
        result["route"],
        format!("/api/v2/data/revisions/{DATASET}/features")
    );
    assert_eq!(result["query"], json!([["project_id", PROJECT]]));
    assert_eq!(result["requires_operator_grant"], false);
    assert_eq!(result["requires_idempotency_key"], false);
    assert_eq!(result["request_sent"], false);
    for args in [
        vec!["data", "features", "list", DATASET],
        vec![
            "data",
            "features",
            "list",
            DATASET,
            "--project-id",
            "invalid",
        ],
    ] {
        assert!(!preview(&args, None).status.success());
    }
}

#[test]
fn recorded_feature_registration_keeps_existing_operator_and_replay_contract() {
    let body = request();
    let args = [
        "--idempotency-key",
        "original-feature",
        "data",
        "features",
        "register",
        DATASET,
    ];
    let result = success(preview(&args, Some(&body)));
    assert_eq!(result["method"], "POST");
    assert_eq!(
        result["route"],
        format!("/api/v2/data/revisions/{DATASET}/features")
    );
    assert_eq!(result["requires_operator_grant"], true);
    assert_eq!(result["requires_idempotency_key"], true);
    assert_eq!(result["expected_http_status"], 201);
    assert_eq!(result["body_redacted"], true);
    assert_eq!(result["request_sent"], false);
    let text = result.to_string();
    assert!(!text.contains("original\\\""));
    let mut wrong = body.clone();
    wrong["dataset_revision_id"] = json!(PROJECT);
    assert!(!preview(&args, Some(&wrong)).status.success());
    wrong = body.clone();
    wrong["origin"] = json!("REAL");
    assert!(!preview(&args, Some(&wrong)).status.success());
    let missing = preview(&["data", "features", "register", DATASET], Some(&body));
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("CLI_IDEMPOTENCY_KEY_REQUIRED"));
}

#[test]
fn frozen_brief_readback_needs_no_idempotency_or_new_credential() {
    let result = success(preview(&["brief", "execution-context", DATASET], None));
    assert_eq!(result["method"], "GET");
    assert_eq!(
        result["route"],
        format!("/api/v2/briefs/{DATASET}/execution-context")
    );
    assert_eq!(result["requires_operator_grant"], false);
    assert_eq!(result["requires_idempotency_key"], false);
}
