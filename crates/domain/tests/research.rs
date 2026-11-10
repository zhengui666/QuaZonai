use contracts::research::*;
use domain::{research::evaluation_policy, DomainError};
use serde_json::{json, Value};
fn base() -> Value {
    serde_json::from_str(r###"{"schema_version":1,"project_id":"01980000-0000-7000-8000-000000000001","question":"Does the proposed effect survive the registered validation?","comparison_input_set_id":"01980000-0000-7000-8000-000000000002","execution_assumptions_id":"01980000-0000-7000-8000-000000000003","selection":{"evaluation_kind":"WALK_FORWARD","metric_code":"RETURN","metric_scope":"total","method_id":"native_method_intent_not_qualified","method_version":"1","unit":"fraction","frequency":"daily","direction":"MAXIMIZE","candidate_count":2},"split_policy":{"schema_version":1,"kind":"WALK_FORWARD","train_size":"100","test_size":"20","step_size":"20","group_count":null,"test_group_count":null,"purge_observations":"1","embargo_observations":"1","label_horizon_observations":"1","interval_validation_required":true,"sealed_revision_id":"01980000-0000-7000-8000-000000000004"},"metric_requirements":[{"schema_version":1,"metric_code":"RETURN","scope":"total","comparator":"GE","threshold_low":"0.10000000000000001","threshold_high":null,"required":true,"minimum_observations":"20","method_allowlist":["native_method_intent_not_qualified"]}],"sealed_metric_requirements":[{"schema_version":1,"metric_code":"RETURN","scope":"total","comparator":"GE","threshold_low":"0.2","threshold_high":null,"required":true,"minimum_observations":"20","method_allowlist":["native_method_intent_not_qualified"]}],"minimum_observations":20,"maximum_missing_fraction":"0.05","require_real_data":true,"required_capabilities":["NATIVE_CAPABILITY_INTENT"],"maximum_sealed_uses_per_lineage":1,"validity_seconds":"3600"}"###)
    .unwrap()
}

#[test]
fn portfolio_requirements_are_independent_optional_and_never_vacuously_pass() {
    let mut request = base();
    let parsed: EvaluationPolicyCreate = serde_json::from_value(request.clone()).unwrap();
    assert!(parsed.portfolio_metric_requirements.is_none());
    let mut requirement = request["metric_requirements"][0].clone();
    requirement["metric_code"] = json!("PORTFOLIO_DAILY_RETURN_MEAN");
    requirement["scope"] = json!("portfolio");
    requirement["method_allowlist"] = json!(["nautilus-analysis.ReturnsAverage"]);
    request["portfolio_metric_requirements"] = json!([requirement.clone()]);
    assert!(evaluation_policy(&serde_json::from_value(request.clone()).unwrap()).is_ok());
    for (criteria, code) in [
        (json!([]), "METRIC_COUNT"),
        (
            json!([requirement.clone(), requirement.clone()]),
            "DUPLICATE_METRIC",
        ),
    ] {
        let mut invalid_request = request.clone();
        invalid_request["portfolio_metric_requirements"] = criteria;
        invalid(invalid_request, code);
    }
    requirement["required"] = json!(false);
    request["portfolio_metric_requirements"] = json!([requirement]);
    invalid(request, "REQUIRED_ALLOWED_METRIC");
}

#[test]
fn portfolio_plan_requires_criteria_and_preserves_precise_ordered_cutoffs() {
    let mut request = base();
    let plan = json!({"schema_version":1,"input_set_id":contracts::Id::new(),
        "evaluation_start":"2019-01-01T00:00:00.000001Z",
        "manual_cutoffs":["2019-01-01T00:00:00.000001Z","2019-01-02T00:00:00.000001Z"]});
    request["portfolio_study_plan"] = plan.clone();
    assert!(evaluation_policy(&serde_json::from_value(request.clone()).unwrap()).is_err());
    request["portfolio_metric_requirements"] = request["metric_requirements"].clone();
    assert!(evaluation_policy(&serde_json::from_value(request.clone()).unwrap()).is_ok());
    for replacement in [
        json!([]),
        json!(["2019-01-01T00:00:00.000001Z"]),
        json!(["2019-01-02T00:00:00.000001Z", "2019-01-01T00:00:00.000001Z"]),
        json!(["2019-01-01T00:00:00.000001Z", "2019-01-01T00:00:00.000001Z"]),
        json!([
            "2019-01-01T00:00:00.000001Z",
            "2019-01-02T00:00:00.000000001Z"
        ]),
        json!(vec!["2019-01-01T00:00:00.000001Z"; 257]),
    ] {
        let mut changed = request.clone();
        changed["portfolio_study_plan"]["manual_cutoffs"] = replacement;
        assert!(evaluation_policy(&serde_json::from_value(changed).unwrap()).is_err());
    }
    for start in [
        "1969-12-31T23:59:59Z",
        "2019-01-01T00:00:00.000000001Z",
        "2400-01-01T00:00:00Z",
    ] {
        let mut changed = request.clone();
        changed["portfolio_study_plan"]["evaluation_start"] = json!(start);
        changed["portfolio_study_plan"]["manual_cutoffs"] = Value::Null;
        assert!(evaluation_policy(&serde_json::from_value(changed).unwrap()).is_err());
    }
    request["portfolio_study_plan"]["manual_cutoffs"] = Value::Null;
    assert!(evaluation_policy(&serde_json::from_value(request.clone()).unwrap()).is_ok());
    request["portfolio_study_plan"]["evaluation_end"] = json!("2020-01-01T00:00:00Z");
    assert!(serde_json::from_value::<EvaluationPolicyCreate>(request).is_err());
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

#[test]
fn comparator_diagnostics_identify_only_the_invalid_endpoints() {
    for (cmp, low, high, expected) in [
        ("GT", Value::Null, Value::Null, vec!["threshold_low"]),
        ("GE", json!("1"), json!("2"), vec!["threshold_high"]),
        ("LT", Value::Null, Value::Null, vec!["threshold_high"]),
        ("LE", json!("1"), json!("2"), vec!["threshold_low"]),
        ("BETWEEN", Value::Null, json!("2"), vec!["threshold_low"]),
        ("BETWEEN", json!("1"), Value::Null, vec!["threshold_high"]),
        (
            "BETWEEN",
            Value::Null,
            Value::Null,
            vec!["threshold_low", "threshold_high"],
        ),
        (
            "BETWEEN",
            json!("2"),
            json!("1"),
            vec!["threshold_low", "threshold_high"],
        ),
    ] {
        let mut r = base();
        r["metric_requirements"][0]["comparator"] = json!(cmp);
        r["metric_requirements"][0]["threshold_low"] = low;
        r["metric_requirements"][0]["threshold_high"] = high;
        let Err(DomainError::Fields(fields)) =
            evaluation_policy(&serde_json::from_value(r).unwrap())
        else {
            panic!("missing diagnostics for {cmp}")
        };
        assert!(fields.iter().all(|f| f.code == "EXACT_THRESHOLD_BOUNDS"));
        assert_eq!(
            fields.iter().map(|f| f.field.as_str()).collect::<Vec<_>>(),
            expected
                .iter()
                .map(|f| format!("metric_requirements.0.{f}"))
                .collect::<Vec<_>>(),
            "{cmp}"
        );
    }
}

#[test]
fn sealed_requirements_are_explicit_and_selection_uses_its_own_evaluation_kind() {
    let mut r = base();
    r.as_object_mut()
        .unwrap()
        .remove("sealed_metric_requirements");
    assert!(serde_json::from_value::<EvaluationPolicyCreate>(r).is_err());
    for (value, code) in [(json!([]), "METRIC_COUNT"), (Value::Null, "")] {
        let mut r = base();
        r["sealed_metric_requirements"] = value;
        if code.is_empty() {
            assert!(serde_json::from_value::<EvaluationPolicyCreate>(r).is_err());
        } else {
            invalid(r, code);
        }
    }
    let mut r = base();
    r["sealed_metric_requirements"][0]["scope"] = json!("asset:0");
    assert!(evaluation_policy(&serde_json::from_value(r.clone()).unwrap()).is_ok());
    r["selection"]["evaluation_kind"] = json!("SEALED");
    invalid(r.clone(), "REQUIRED_ALLOWED_METRIC");
    r["selection"]["metric_scope"] = json!("asset:0");
    assert!(evaluation_policy(&serde_json::from_value(r.clone()).unwrap()).is_ok());
    r["sealed_metric_requirements"][0]["threshold_low"] = Value::Null;
    let Err(DomainError::Fields(fields)) = evaluation_policy(&serde_json::from_value(r).unwrap())
    else {
        panic!("missing Sealed threshold diagnostic")
    };
    assert_eq!(
        fields[0].field,
        "sealed_metric_requirements.0.threshold_low"
    );
    assert_eq!(fields[0].code, "EXACT_THRESHOLD_BOUNDS");
}
