//! Exercise the real deserializer and authoring validator, not schema text alone.
use contracts::brief::BriefCreate;
use domain::{admission::validate_budget, brief, DomainError};
use serde_json::{json, Value};

fn example() -> Value {
    serde_json::from_str(include_str!("../../../tests/contracts/research-brief.json")).unwrap()
}

fn decode(value: Value) -> BriefCreate {
    serde_json::from_value(value).expect("representable wire value")
}

fn valid(value: Value) {
    let request = decode(value);
    validate_budget(&request.content.budget, &request.content.stop_rule).unwrap();
    brief::content(&request.content, &request.bindings).unwrap();
}

fn invalid_field(value: Value, field: &str, code: &str) {
    let request = decode(value);
    let Err(DomainError::Fields(issues)) = brief::content(&request.content, &request.bindings)
    else {
        panic!("expected domain field rejection for {field}");
    };
    assert!(
        issues
            .iter()
            .any(|issue| issue.field == field && issue.code == code),
        "missing {field}/{code}: {issues:?}"
    );
}

#[test]
fn every_required_positive_budget_rejects_zero_after_native_deserialization() {
    let mut base = example();
    // Isolate scalar lower bounds from the two cross-field inequalities.
    base["content"]["budget"]["max_repair_turns"] = json!(0);
    base["content"]["stop_rule"]["stop_on_qualified_count"] = json!(1);
    valid(base.clone());
    for (field, zero, one, maximum, overflow) in [
        (
            "max_experiments",
            json!(0),
            json!(1),
            json!(u32::MAX),
            json!(u64::from(u32::MAX) + 1),
        ),
        (
            "max_parallel_runs",
            json!(0),
            json!(1),
            json!(u16::MAX),
            json!(u32::from(u16::MAX) + 1),
        ),
        (
            "max_turns_per_mission",
            json!(0),
            json!(1),
            json!(u16::MAX),
            json!(u32::from(u16::MAX) + 1),
        ),
        (
            "max_wall_seconds",
            json!(0),
            json!(1),
            json!(u32::MAX),
            json!(u64::from(u32::MAX) + 1),
        ),
        (
            "max_memory_mib",
            json!(0),
            json!(1),
            json!(u32::MAX),
            json!(u64::from(u32::MAX) + 1),
        ),
        (
            "max_cycles_per_day",
            json!(0),
            json!(1),
            json!(u16::MAX),
            json!(u32::from(u16::MAX) + 1),
        ),
        (
            "max_cpu_seconds",
            json!("0"),
            json!("1"),
            json!("9223372036854775807"),
            json!("9223372036854775808"),
        ),
        (
            "max_output_bytes",
            json!("0"),
            json!("1"),
            json!("9223372036854775807"),
            json!("9223372036854775808"),
        ),
    ] {
        let mut request = base.clone();
        request["content"]["budget"][field] = zero;
        invalid_field(request, "content.budget", "BUDGET_OR_STOP_RULE");
        for accepted in [one, maximum] {
            let mut request = base.clone();
            request["content"]["budget"][field] = accepted;
            valid(request);
        }
        let mut request = base.clone();
        request["content"]["budget"][field] = overflow;
        assert!(
            serde_json::from_value::<BriefCreate>(request).is_err(),
            "{field} overflow"
        );
        let mut request = base.clone();
        request["content"]["budget"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<BriefCreate>(request).is_err(),
            "{field} missing"
        );
        let mut request = base.clone();
        request["content"]["budget"][field] = Value::Null;
        assert!(
            serde_json::from_value::<BriefCreate>(request).is_err(),
            "{field} null"
        );
    }
}

#[test]
fn optional_token_cap_is_absent_or_positive_and_never_an_imprecise_number() {
    let base = example();
    for accepted in [Value::Null, json!("1"), json!("9223372036854775807")] {
        let mut request = base.clone();
        request["content"]["budget"]["max_tokens"] = accepted;
        valid(request);
    }
    let mut request = base.clone();
    request["content"]["budget"]
        .as_object_mut()
        .unwrap()
        .remove("max_tokens");
    valid(request);
    let mut request = base.clone();
    request["content"]["budget"]["max_tokens"] = json!("0");
    invalid_field(request, "content.budget", "BUDGET_OR_STOP_RULE");
    for rejected in [
        json!(1),
        json!(-1),
        json!(1.5),
        json!(true),
        json!("-1"),
        json!("01"),
        json!("1.0"),
        json!("1e3"),
        json!("9223372036854775808"),
    ] {
        let mut request = base.clone();
        request["content"]["budget"]["max_tokens"] = rejected;
        assert!(serde_json::from_value::<BriefCreate>(request).is_err());
    }
}

#[test]
fn intentional_zero_limits_and_optional_stop_rule_remain_usable() {
    let mut request = example();
    request["content"]["budget"]["max_repair_turns"] = json!(0);
    request["content"]["budget"]["min_cycle_interval_seconds"] = json!(0);
    request["content"]["stop_rule"]["stop_on_no_improvement_trials"] = Value::Null;
    valid(request.clone());
    request["content"]["stop_rule"]
        .as_object_mut()
        .unwrap()
        .remove("stop_on_no_improvement_trials");
    valid(request.clone());
    for accepted in [1, u16::MAX] {
        request["content"]["stop_rule"]["stop_on_no_improvement_trials"] = json!(accepted);
        valid(request.clone());
    }
    request["content"]["stop_rule"]["stop_on_no_improvement_trials"] = json!(0);
    invalid_field(request, "content.budget", "BUDGET_OR_STOP_RULE");
    let mut request = example();
    request["content"]["stop_rule"]["stop_on_qualified_count"] = json!(0);
    invalid_field(request, "content.budget", "BUDGET_OR_STOP_RULE");
}

#[test]
fn scalar_bounds_do_not_replace_budget_cross_field_validation() {
    let mut request = example();
    request["content"]["budget"]["max_turns_per_mission"] = json!(1);
    request["content"]["budget"]["max_repair_turns"] = json!(2);
    invalid_field(request, "content.budget", "BUDGET_OR_STOP_RULE");
    let mut request = example();
    request["content"]["budget"]["max_experiments"] = json!(1);
    request["content"]["stop_rule"]["stop_on_qualified_count"] = json!(2);
    invalid_field(request, "content.budget", "BUDGET_OR_STOP_RULE");
}

#[test]
fn fixed_and_variable_horizons_cover_missing_null_zero_and_exact_bigint_limits() {
    for kind in ["FIXED_BARS", "FIXED_DURATION", "VARIABLE_INTERVAL"] {
        let fixed = kind != "VARIABLE_INTERVAL";
        for value in [
            None,
            Some(Value::Null),
            Some(json!("0")),
            Some(json!("1")),
            Some(json!("9223372036854775807")),
        ] {
            let accepted = match value.as_ref() {
                None | Some(Value::Null) => !fixed,
                Some(value) => fixed && value != &json!("0"),
            };
            let mut request = example();
            request["content"]["horizon_kind"] = json!(kind);
            match value {
                Some(value) => request["content"]["horizon_value"] = value,
                None => {
                    request["content"]
                        .as_object_mut()
                        .unwrap()
                        .remove("horizon_value");
                }
            }
            if accepted {
                valid(request);
            } else {
                invalid_field(request, "content.horizon_value", "HORIZON_SHAPE");
            }
        }
        for rejected in [
            json!(1),
            json!("-1"),
            json!("01"),
            json!("1.0"),
            json!("9223372036854775808"),
        ] {
            let mut request = example();
            request["content"]["horizon_kind"] = json!(kind);
            request["content"]["horizon_value"] = rejected;
            assert!(serde_json::from_value::<BriefCreate>(request).is_err());
        }
    }
}
