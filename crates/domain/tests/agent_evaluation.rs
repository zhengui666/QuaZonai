use contracts::{agent_evaluation::AgentEvaluationReportV1, artifacts::*, Id, SchemaV1};
use domain::agent_evaluation::parse;
use serde_json::{json, Value};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/agent-evaluation/unrun-v1.json"
    ))
    .unwrap()
}
fn checked(value: &Value) -> bool {
    parse(&serde_json::to_vec(value).unwrap()).is_ok()
}
fn passing() -> Value {
    let mut value = fixture();
    value["status"] = json!("PASS");
    let requested = value["requested"].clone();
    for case in value["cases"].as_array_mut().unwrap() {
        case["status"] = json!("PASS");
        case["reason"] = json!("Protocol assertion passed, without live model inference.");
        case["observed"] =
            json!({"settings":requested,"invocation_id":format!("fixture-{}",case["id"])});
        case["assertions"] =
            json!([{"id":"expected-result","passed":true,"evidence_sha256":"a".repeat(64)}]);
    }
    value
}
#[test]
fn validates_protocol_fixture_without_implying_execution() {
    assert!(checked(&fixture()));
    assert!(checked(&passing()));
    let report: AgentEvaluationReportV1 = parse(&serde_json::to_vec(&fixture()).unwrap()).unwrap();
    assert!(report
        .cases
        .iter()
        .all(|case| case.measurements.cost.is_none()));
}
#[test]
fn rejects_forged_pass_or_incomplete_observed_identity_and_assertions() {
    for (pointer, replacement) in [
        ("/cases/0/observed", Value::Null),
        ("/cases/0/observed/settings/model", json!("other-model")),
        ("/cases/0/observed/settings/reasoning_effort", json!("low")),
        ("/cases/0/observed/invocation_id", json!("")),
        ("/cases/0/assertions", json!([])),
        ("/cases/0/assertions/0/passed", json!(false)),
        ("/cases/0/assertions/0/evidence_sha256", json!("")),
        ("/cases/0/required_assertions", json!([])),
    ] {
        let mut value = passing();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(!checked(&value), "{pointer}");
    }
    let mut value = fixture();
    value["status"] = json!("PASS");
    assert!(!checked(&value));
}
#[test]
fn rejects_duplicate_ids_cross_split_scenarios_and_dataset_overlap() {
    let mut value = fixture();
    value["cases"][1]["id"] = value["cases"][0]["id"].clone();
    assert!(!checked(&value));
    let mut value = fixture();
    value["held_out"]["case_ids"] = value["tuning"]["case_ids"].clone();
    assert!(!checked(&value));
    let mut value = fixture();
    value["held_out"]["sha256"] = value["tuning"]["sha256"].clone();
    assert!(!checked(&value));
    let mut value = fixture();
    value["cases"][1]["scenario_sha256"] = value["cases"][0]["scenario_sha256"].clone();
    assert!(!checked(&value));
    let mut value = fixture();
    value["cases"][1]["split"] = json!("TUNING");
    assert!(!checked(&value));
    let mut value = passing();
    let assertion = value["cases"][0]["assertions"][0].clone();
    value["cases"][0]["assertions"]
        .as_array_mut()
        .unwrap()
        .push(assertion);
    assert!(!checked(&value));
}
#[test]
fn unknown_usage_is_not_zero_and_precise_measurements_round_trip() {
    let mut value = passing();
    value["cases"][0]["measurements"] = json!({"input_tokens":"9007199254740993","output_tokens":"0","elapsed_ms":null,"tool_calls":"9007199254740993","cost":{"amount":"0.000000000000000001","currency":"EUR"}});
    let report = parse(&serde_json::to_vec(&value).unwrap()).unwrap();
    let roundtrip = serde_json::to_value(report).unwrap();
    assert_eq!(
        roundtrip["cases"][0]["measurements"],
        value["cases"][0]["measurements"]
    );
    assert!(roundtrip["cases"][1]["measurements"]["cost"].is_null());
    value["cases"][0]["measurements"]["input_tokens"] = json!(9007199254740993_u64);
    assert!(!checked(&value));
    let mut value = passing();
    value["cases"][0]["measurements"]["cost"] = json!({"amount":"-1","currency":"EUR"});
    assert!(!checked(&value));
    let mut value = fixture();
    value["cases"][0]["measurements"]["input_tokens"] = json!("0");
    assert!(!checked(&value));
}
#[test]
fn rejects_unknown_fields_invalid_scalars_bounds_and_aggregate_status() {
    for pointer in ["", "/cases/0", "/requested", "/cases/0/measurements"] {
        let mut value = fixture();
        value.pointer_mut(pointer).unwrap()["surprise"] = json!(true);
        assert!(!checked(&value));
    }
    for (pointer, replacement) in [
        ("/schema_version", json!(2)),
        ("/source_sha256", json!("A".repeat(64))),
        ("/runner/name", json!(" ")),
        ("/cases/0/reason", json!("x".repeat(2001))),
        ("/cases/0/required_assertions", json!(["x", "x"])),
    ] {
        let mut value = fixture();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(!checked(&value), "{pointer}");
    }
    let mut value = passing();
    value["cases"][0]["status"] = json!("FAIL");
    value["cases"][0]["assertions"][0]["passed"] = json!(false);
    value["status"] = json!("FAIL");
    assert!(checked(&value));
    value["cases"][1]["status"] = json!("BLOCKED");
    assert!(checked(&value));
    value["status"] = json!("BLOCKED");
    assert!(!checked(&value));
}
#[test]
fn upload_enforces_agent_report_without_rejecting_generic_reports() {
    for (value, expected) in [
        (fixture(), true),
        (json!({"schema_version":1,"text":"ordinary report"}), true),
        (
            json!({"schema_version":1,"report_kind":"AGENT_EVALUATION","status":"PASS"}),
            false,
        ),
    ] {
        let request = ArtifactCreate {
            schema_version: SchemaV1,
            project_id: Id::new(),
            kind: ResearchArtifactKind::Report,
            content: value.to_string(),
        };
        assert_eq!(domain::artifacts::upload(&request).is_ok(), expected);
    }
}

#[test]
fn wrong_observed_identity_is_blocked_not_a_failure_of_the_requested_model() {
    for field in ["model", "reasoning_effort"] {
        let mut value = passing();
        value["status"] = json!("FAIL");
        value["cases"][0]["status"] = json!("FAIL");
        value["cases"][0]["assertions"][0]["passed"] = json!(false);
        value["cases"][0]["observed"]["settings"][field] = json!("different");
        value["cases"][0]["measurements"]["tool_calls"] = json!("9007199254740993");
        assert!(!checked(&value));
        value["status"] = json!("BLOCKED");
        value["cases"][0]["status"] = json!("BLOCKED");
        assert!(checked(&value));
        let roundtrip =
            serde_json::to_value(parse(&serde_json::to_vec(&value).unwrap()).unwrap()).unwrap();
        assert_eq!(
            roundtrip["cases"][0]["observed"],
            value["cases"][0]["observed"]
        );
        assert_eq!(
            roundtrip["cases"][0]["measurements"]["tool_calls"],
            "9007199254740993"
        );
    }
}
#[test]
fn cost_requires_a_nonnegative_amount_and_native_currency_while_tool_calls_stay_exact() {
    for cost in [
        json!({"amount":"0","currency":"EUR"}),
        json!({"amount":"0.000000000000000001","currency":"JPY"}),
        Value::Null,
    ] {
        let mut value = passing();
        value["cases"][0]["measurements"]["cost"] = cost;
        assert!(checked(&value));
    }
    for cost in [
        json!({"amount":"1"}),
        json!({"currency":"USD"}),
        json!({"amount":"-1","currency":"USD"}),
        json!({"amount":"1","currency":"ZZZ"}),
        json!({"amount":"1","currency":"usd"}),
    ] {
        let mut value = passing();
        value["cases"][0]["measurements"]["cost"] = cost;
        assert!(!checked(&value));
    }
    let mut value = passing();
    value["cases"][0]["measurements"]["tool_calls"] = json!(0);
    assert!(!checked(&value));
    value["cases"][0]["measurements"]["tool_calls"] = json!("0");
    assert!(checked(&value));
    let mut value = fixture();
    value["cases"][0]["measurements"]["tool_calls"] = json!("0");
    assert!(!checked(&value));
}

#[test]
fn recorded_time_serializes_within_the_native_http_datetime_contract() {
    for (input, expected) in [
        ("2026-9-30T00:00:00Z", "2026-09-30T00:00:00Z"),
        ("2026-09-30T01:00:00+01:00", "2026-09-30T00:00:00Z"),
        ("0000-01-01T00:00:00Z", "0000-01-01T00:00:00Z"),
        ("9999-12-31T23:59:59Z", "9999-12-31T23:59:59Z"),
        (
            "2026-09-30T23:59:60.999999999Z",
            "2026-09-30T23:59:60.999999999Z",
        ),
        ("2026-09-30T00:59:60+01:00", "2026-09-29T23:59:60Z"),
    ] {
        let mut value = fixture();
        value["recorded_at"] = json!(input);
        let report = parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(report).unwrap()["recorded_at"],
            expected,
            "{input}"
        );
    }
    for input in [
        "+10000-01-01T00:00:00Z",
        "-0001-01-01T00:00:00Z",
        "2026-09-30T12:00:60Z",
        "2026-09-30T23:59:60+01:00",
    ] {
        let mut value = fixture();
        value["recorded_at"] = json!(input);
        assert!(!checked(&value), "{input}");
    }
}
