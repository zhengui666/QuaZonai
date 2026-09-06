use contracts::research::*;
use domain::{research::evaluation_policy, DomainError};
use serde_json::{json, Value};
fn base() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/contracts/research-policy.json"
    ))
    .unwrap()
}
fn invalid(r: Value, expected: &str) {
    let r: EvaluationPolicyCreate = serde_json::from_value(r).unwrap();
    let Err(DomainError::Fields(fields)) = evaluation_policy(&r) else {
        panic!("expected field failure {expected}")
    };
    assert_eq!(fields[0].code, expected);
}
#[test]
fn walk_and_fixed_horizon_cpcv_require_exact_distinct_field_sets() {
    assert!(evaluation_policy(&serde_json::from_value(base()).unwrap()).is_ok());
    for (p, v, c) in [
        (
            "/split_policy/train_size",
            json!("0"),
            "POSITIVE_COUNT_REQUIRED",
        ),
        (
            "/split_policy/step_size",
            Value::Null,
            "POSITIVE_COUNT_REQUIRED",
        ),
        (
            "/split_policy/group_count",
            json!(5),
            "WALK_FORWARD_HAS_NO_GROUPS",
        ),
        (
            "/split_policy/interval_validation_required",
            json!(false),
            "INTERVAL_VALIDATION_REQUIRED",
        ),
        (
            "/split_policy/train_size",
            json!("9223372036854775807"),
            "COUNT_OVERFLOW",
        ),
    ] {
        let mut r = base();
        *r.pointer_mut(p).unwrap() = v;
        invalid(r, c);
    }
    let mut c = base();
    c["split_policy"]["kind"] = json!("CPCV_FIXED_HORIZON");
    c["split_policy"]["step_size"] = Value::Null;
    c["split_policy"]["group_count"] = json!(5);
    c["split_policy"]["test_group_count"] = json!(2);
    assert!(evaluation_policy(&serde_json::from_value(c.clone()).unwrap()).is_ok());
    let mut r = c.clone();
    r["split_policy"]["test_group_count"] = json!(5);
    invalid(r, "CPCV_GROUP_COUNTS");
    let mut r = c.clone();
    r["split_policy"]["label_horizon_observations"] = Value::Null;
    invalid(r, "FIXED_HORIZON_REQUIRED");
    c["split_policy"]["step_size"] = json!("1");
    invalid(c, "CPCV_HAS_NO_STEP");
}
#[test]
fn policy_selection_and_metric_contracts_never_round_or_skip_required_values() {
    for (p, v, c) in [
        (
            "/selection/candidate_count",
            json!(0),
            "POSITIVE_COUNT_REQUIRED",
        ),
        (
            "/selection/method_id",
            json!("unlisted"),
            "REQUIRED_ALLOWED_METRIC",
        ),
        (
            "/metric_requirements/0/required",
            json!(false),
            "REQUIRED_ALLOWED_METRIC",
        ),
        (
            "/metric_requirements/0/threshold_low",
            Value::Null,
            "EXACT_THRESHOLD_BOUNDS",
        ),
        (
            "/minimum_observations",
            json!(2147483648_u64),
            "POSTGRES_INTEGER_RANGE",
        ),
        (
            "/maximum_missing_fraction",
            json!("1.000000000000000001"),
            "FRACTION_RANGE",
        ),
        (
            "/required_capabilities",
            json!(["one", "one"]),
            "DUPLICATE_CAPABILITY",
        ),
        (
            "/metric_requirements/0/method_allowlist",
            json!([]),
            "METHOD_COUNT",
        ),
    ] {
        let mut r = base();
        *r.pointer_mut(p).unwrap() = v;
        invalid(r, c);
    }
    let mut r = base();
    let metric = r["metric_requirements"][0].clone();
    r["metric_requirements"]
        .as_array_mut()
        .unwrap()
        .push(metric);
    invalid(r, "DUPLICATE_METRIC");
    let mut r = base();
    r["metric_requirements"][0]["comparator"] = json!("BETWEEN");
    r["metric_requirements"][0]["threshold_high"] = json!("0.1");
    invalid(r, "EXACT_THRESHOLD_BOUNDS");
}
