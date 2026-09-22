//! Actual installed-binary and portable-skill checks; these are not LLM rollouts.
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::Path,
    process::{Command, Output, Stdio},
};

const ID: &str = "018fc823-8e40-7000-8000-000000000001";
const MISSING_CREDENTIAL: &str = "/quazonai-not-provisioned/credential";

fn invoke(args: &[&str], input: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
        .env_clear()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Some(input) = input {
        stdin.write_all(input.as_bytes()).unwrap();
    }
    drop(stdin);
    child.wait_with_output().unwrap()
}

fn preview_at(origin: &str, tail: &[&str], input: Option<&str>) -> Output {
    let mut args = vec![
        "client",
        "--origin",
        origin,
        "--credential-file",
        MISSING_CREDENTIAL,
        "--development-http",
        "--preview",
    ];
    args.extend_from_slice(tail);
    invoke(&args, input)
}

fn successful(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn documented_alpha_workflow_distinguishes_entity_version_id_and_version_number() {
    const VERSION_ID: &str = "018fc823-8e40-7000-8000-000000000002";
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/quazonai/references/results.md");
    let instructions = fs::read_to_string(path).unwrap();
    for (command, route) in [
        ("versions", format!("/api/v2/alphas/{ID}/versions")),
        ("show", format!("/api/v2/alphas/{ID}/versions/7")),
        (
            "evaluations",
            format!("/api/v2/alpha-versions/{VERSION_ID}/evaluations"),
        ),
        (
            "qualifications",
            format!("/api/v2/alpha-versions/{VERSION_ID}/qualifications"),
        ),
        (
            "calibration",
            format!("/api/v2/alpha-versions/{VERSION_ID}/calibration"),
        ),
    ] {
        let prefix = format!("alpha {command} ");
        let example = instructions
            .split('`')
            .enumerate()
            .find_map(|(index, text)| (index % 2 == 1 && text.starts_with(&prefix)).then_some(text))
            .unwrap_or_else(|| panic!("missing documented command: {command}"));
        let identity = if matches!(command, "versions" | "show") {
            "ALPHA_ID"
        } else {
            "ALPHA_VERSION_ID"
        };
        assert_eq!(example.split_whitespace().nth(2), Some(identity));
        let args: Vec<_> = example
            .split_whitespace()
            .map(|argument| match argument {
                "ALPHA_ID" => ID,
                "ALPHA_VERSION_ID" => VERSION_ID,
                "VERSION" => "7",
                value => value,
            })
            .collect();
        let output = preview_at("http://localhost:9", &args, None);
        let value = successful(&output);
        assert_eq!(value["route"], route, "{example}");
        assert_eq!(value["method"], "GET");
        assert_eq!(value["request_sent"], false);
        let help = invoke(&["client", "alpha", command, "--help"], None);
        assert!(help.status.success());
        assert!(String::from_utf8(help.stdout)
            .unwrap()
            .contains(&format!("<{identity}>")));
    }
    let help = invoke(&["client", "alpha", "evaluate", "--help"], None);
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout)
        .unwrap()
        .contains("<ALPHA_VERSION_ID>"));
    assert!(instructions.contains("`alpha evaluate ALPHA_VERSION_ID`"));
}

#[test]
fn preview_sends_no_request_and_does_not_open_missing_credentials_or_echo_body_keys() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let input = json!({"schema_version":1,"project_id":ID,"kind":"REPORT","content":"private-preview-sentinel"}).to_string();
    let output = preview_at(
        &origin,
        &[
            "--idempotency-key",
            "private-key-sentinel",
            "artifact",
            "submit",
        ],
        Some(&input),
    );
    let value = successful(&output);
    assert_eq!(value["route"], "/api/v2/artifacts");
    assert_eq!(value["method"], "POST");
    assert_eq!(value["expected_http_status"], 201);
    assert_eq!(value["requires_idempotency_key"], true);
    assert_eq!(value["requires_operator_grant"], false);
    assert_eq!(value["request_sent"], false);
    assert_eq!(value["authorization_checked"], false);
    assert_eq!(value["server_state_checked"], false);
    assert_eq!(value["body_redacted"], true);
    assert!(value["body_bytes"].as_u64().unwrap() > 0);
    let text = String::from_utf8(output.stdout).unwrap();
    for secret in [
        "private-preview-sentinel",
        "private-key-sentinel",
        MISSING_CREDENTIAL,
        &origin,
    ] {
        assert!(!text.contains(secret));
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn preview_exposes_grant_requirement_without_claiming_authorization() {
    let input = json!({"schema_version":1,"expected_revision":"1"}).to_string();
    let output = preview_at(
        "http://localhost:9",
        &["--idempotency-key", "probe-preview", "runtime", "probe", ID],
        Some(&input),
    );
    let value = successful(&output);
    assert_eq!(value["requires_operator_grant"], true);
    assert_eq!(value["operator_grant_supplied"], false);
    assert_eq!(value["authorization_checked"], false);
}

#[test]
fn malformed_ids_json_unknown_fields_and_missing_write_keys_are_not_previewed_as_valid() {
    let valid = json!({"schema_version":1,"project_id":ID,"kind":"REPORT","content":"hello"});
    let mut extra = valid.clone();
    extra["invented_field"] = json!("not accepted");
    for input in ["{".to_owned(), extra.to_string()] {
        let output = preview_at(
            "http://localhost:9",
            &["--idempotency-key", "invalid-preview", "artifact", "submit"],
            Some(&input),
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("CLI_INPUT_INVALID"));
    }
    let output = preview_at(
        "http://localhost:9",
        &["project", "show", "not-a-uuid"],
        None,
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let output = preview_at(
        "http://localhost:9",
        &["artifact", "submit"],
        Some(&valid.to_string()),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("CLI_IDEMPOTENCY_KEY_REQUIRED"));
}

#[test]
fn documented_read_routes_and_bounded_watch_parse_through_the_native_client() {
    let cases: &[(&[&str], &str)] = &[
        (&["identity"], "/api/v2/auth/machine"),
        (&["project", "list", "--limit", "20"], "/api/v2/projects"),
        (&["brief", "show", ID], "/api/v2/briefs/"),
        (
            &["input-set", "list", "--project-id", ID, "--limit", "20"],
            "/api/v2/input-sets",
        ),
        (&["policy", "show", ID], "/api/v2/evaluation-policies/"),
        (
            &["alpha", "list", "--project-id", ID, "--limit", "20"],
            "/api/v2/alphas",
        ),
        (
            &["portfolio", "candidate", "show", ID],
            "/api/v2/portfolio-candidates/",
        ),
        (
            &[
                "run",
                "watch",
                ID,
                "--max-seconds",
                "30",
                "--max-events",
                "100",
            ],
            "/api/v2/runs/",
        ),
    ];
    for (args, route) in cases {
        let output = preview_at("http://localhost:9", args, None);
        let value = successful(&output);
        assert!(
            value["route"].as_str().unwrap().starts_with(route),
            "{args:?}: {value}"
        );
        assert_eq!(value["method"], "GET");
        assert_eq!(value["requires_idempotency_key"], false);
        assert_eq!(value["request_sent"], false);
    }
    let cursor = format!("{ID}:9007199254740993");
    let output = preview_at(
        "http://localhost:9",
        &[
            "run",
            "watch",
            ID,
            "--after",
            &cursor,
            "--max-seconds",
            "30",
            "--max-events",
            "100",
        ],
        None,
    );
    assert_eq!(successful(&output)["output"], "ndjson");
    let invalid = preview_at(
        "http://localhost:9",
        &["run", "watch", ID, "--after", "invalid"],
        None,
    );
    assert!(!invalid.status.success());
}

#[test]
fn offline_schema_discovery_matches_native_export_and_retains_its_reference_closure() {
    let exported = successful(&invoke(&["openapi"], None));
    let selected = successful(&invoke(&["openapi", "--schema", "ArtifactCreate"], None));
    assert_eq!(
        selected["components"]["schemas"]["ArtifactCreate"],
        exported["components"]["schemas"]["ArtifactCreate"]
    );
    fn check_refs(value: &Value, root: &Value) {
        match value {
            Value::Object(fields) => {
                if let Some(reference) = fields.get("$ref") {
                    let pointer = reference.as_str().unwrap().strip_prefix('#').unwrap();
                    assert!(root.pointer(pointer).is_some(), "unresolved {pointer}");
                }
                for child in fields.values() {
                    check_refs(child, root);
                }
            }
            Value::Array(values) => {
                for child in values {
                    check_refs(child, root);
                }
            }
            _ => {}
        }
    }
    check_refs(&selected, &selected);
    let names = successful(&invoke(&["openapi", "--list-schemas"], None));
    let names = names["schemas"].as_array().unwrap();
    assert!(names.iter().any(|name| name == "ArtifactCreate"));
    assert!(names
        .windows(2)
        .all(|pair| pair[0].as_str() < pair[1].as_str()));
    let missing = invoke(&["openapi", "--schema", "DoesNotExist"], None);
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("CLI_INPUT_INVALID"));
    assert!(!invoke(
        &["openapi", "--schema", "ArtifactCreate", "--list-schemas"],
        None
    )
    .status
    .success());
}

#[test]
fn skill_installs_as_a_self_contained_directory_without_contributor_dependencies() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/quazonai");
    let installed = tempfile::tempdir().unwrap();
    fs::create_dir(installed.path().join("references")).unwrap();
    let entry = fs::read_to_string(source.join("SKILL.md")).unwrap();
    assert!(entry.starts_with("---\nname: quazonai\n"));
    assert!(entry.contains("Operate an existing QuaZonai"));
    assert!(entry.lines().count() < 120);
    let mut files = vec![source.join("SKILL.md")];
    files.extend(
        fs::read_dir(source.join("references"))
            .unwrap()
            .map(|entry| entry.unwrap().path()),
    );
    for file in &files {
        let relative = file.strip_prefix(&source).unwrap();
        fs::copy(file, installed.path().join(relative)).unwrap();
    }
    for file in &files {
        let relative = file.strip_prefix(&source).unwrap();
        let file = installed.path().join(relative);
        let text = fs::read_to_string(&file).unwrap();
        for forbidden in [
            "../../",
            "AGENTS.md",
            "CONTRIBUTING.md",
            ".opensdlc/",
            "cargo test",
            "cargo run",
            "make check",
        ] {
            assert!(
                !text.contains(forbidden),
                "{}: {forbidden}",
                relative.display()
            );
        }
        for part in text.split("](").skip(1) {
            let target = part.split(')').next().unwrap();
            assert!(!target.contains("://") && !target.starts_with('/') && !target.contains(".."));
            assert!(
                file.parent().unwrap().join(target).is_file(),
                "missing skill reference {target}"
            );
        }
    }
}
