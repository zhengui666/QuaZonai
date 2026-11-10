use contracts::research::*;
use serde_json::{json, Value};
fn request() -> Value {
    serde_json::from_str(r###"{"schema_version":1,"project_id":"01980000-0000-7000-8000-000000000001","question":"Does the proposed effect survive the registered validation?","comparison_input_set_id":"01980000-0000-7000-8000-000000000002","execution_assumptions_id":"01980000-0000-7000-8000-000000000003","selection":{"evaluation_kind":"WALK_FORWARD","metric_code":"RETURN","metric_scope":"total","method_id":"native_method_intent_not_qualified","method_version":"1","unit":"fraction","frequency":"daily","direction":"MAXIMIZE","candidate_count":2},"split_policy":{"schema_version":1,"kind":"WALK_FORWARD","train_size":"100","test_size":"20","step_size":"20","group_count":null,"test_group_count":null,"purge_observations":"1","embargo_observations":"1","label_horizon_observations":"1","interval_validation_required":true,"sealed_revision_id":"01980000-0000-7000-8000-000000000004"},"metric_requirements":[{"schema_version":1,"metric_code":"RETURN","scope":"total","comparator":"GE","threshold_low":"0.10000000000000001","threshold_high":null,"required":true,"minimum_observations":"20","method_allowlist":["native_method_intent_not_qualified"]}],"sealed_metric_requirements":[{"schema_version":1,"metric_code":"RETURN","scope":"total","comparator":"GE","threshold_low":"0.2","threshold_high":null,"required":true,"minimum_observations":"20","method_allowlist":["native_method_intent_not_qualified"]}],"minimum_observations":20,"maximum_missing_fraction":"0.05","require_real_data":true,"required_capabilities":["NATIVE_CAPABILITY_INTENT"],"maximum_sealed_uses_per_lineage":1,"validity_seconds":"3600"}"###)
    .unwrap()
}

#[test]
fn policy_wire_rejects_unknown_identity_fields_and_lossy_counts() {
    let base = request();
    assert!(serde_json::from_value::<EvaluationPolicyCreate>(base.clone()).is_ok());
    for (pointer, value) in [
        (
            "/selection/family_id",
            json!("01980000-0000-7000-8000-000000000008"),
        ),
        ("/split_policy/train_size", json!(100)),
        ("/split_policy/train_size", json!("9223372036854775808")),
        ("/selection/candidate_count", json!(65536)),
        ("/schema_version", json!(2)),
    ] {
        let mut r = base.clone();
        if pointer.ends_with("family_id") {
            r["selection"]["family_id"] = value;
        } else {
            *r.pointer_mut(pointer).unwrap() = value;
        }
        assert!(
            serde_json::from_value::<EvaluationPolicyCreate>(r).is_err(),
            "{pointer}"
        );
    }
    let mut r = base;
    r["split_policy"]["extra_engine_setting"] = json!(true);
    assert!(serde_json::from_value::<EvaluationPolicyCreate>(r).is_err());
}
#[test]
fn input_union_rejects_mixed_references_and_unregistered_roles() {
    let id = json!("01980000-0000-7000-8000-000000000001");
    let mut input = json!({"kind":"DATASET","dataset_revision_id":id,"role":"VALIDATION"});
    assert!(serde_json::from_value::<InputItemV1>(input.clone()).is_ok());
    input["artifact_id"] = id.clone();
    assert!(serde_json::from_value::<InputItemV1>(input).is_err());
    for role in ["SECRET", "*", "LOG", "SEALED"] {
        assert!(serde_json::from_value::<InputItemV1>(
            json!({"kind":"ARTIFACT","artifact_id":id,"role":role})
        )
        .is_err());
    }
}
#[test]
fn generated_policy_schema_exposes_native_scalar_and_array_boundaries() {
    use utoipa::PartialSchema;
    let selection = serde_json::to_value(SelectionParametersV1::schema()).unwrap();
    let policy = serde_json::to_value(EvaluationPolicyCreate::schema()).unwrap();
    let inputs = serde_json::to_value(InputSetCreate::schema()).unwrap();
    assert_eq!(selection["properties"]["candidate_count"]["maximum"], 65535);
    assert_eq!(
        policy["properties"]["minimum_observations"]["maximum"],
        2147483647
    );
    assert_eq!(inputs["properties"]["items"]["maxItems"], 65536);
    assert_eq!(
        inputs["properties"]["decision_cutoff"]["format"],
        "date-time"
    );
    let schema: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let schemas = &schema["components"]["schemas"];
    assert_ne!(
        schemas["NativeBarSelectionV1"]["properties"]["maximum_rows"]["maximum"],
        1000000
    );
    assert_eq!(
        schemas["SplitPolicyV1"]["properties"]["interval_validation_required"]["enum"],
        json!([true])
    );
    assert!(schemas["DbCounter"]["maxLength"].as_u64().is_some());
}

#[test]
fn nested_metric_schema_has_both_array_and_item_boundaries() {
    let schema: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let m = &schema["components"]["schemas"]["MetricRequirementV1"]["properties"];
    for name in ["metric_code", "scope"] {
        assert_eq!(m[name]["minLength"], 1, "{name}");
        assert_eq!(m[name]["maxLength"], 120, "{name}");
    }
    let a = &m["method_allowlist"];
    assert_eq!(a["minItems"], 1);
    assert!(a["maxItems"].is_null());
    assert_eq!(a["uniqueItems"], true);
    assert_eq!(a["items"]["minLength"], 1);
    assert_eq!(a["items"]["maxLength"], 120);
}

#[test]
fn review_policy_fraction_and_capabilities_publish_the_remaining_bounds() {
    use utoipa::PartialSchema;
    let schema = serde_json::to_value(EvaluationPolicyCreate::schema()).unwrap();
    let p = &schema["properties"];
    assert!(
        p["maximum_missing_fraction"]["allOf"].is_array(),
        "exact decimal plus fraction bounds"
    );
    let caps = &p["required_capabilities"];
    assert!(caps["maxItems"].is_null());
    assert_eq!(caps["uniqueItems"], true);
    assert_eq!(caps["items"]["minLength"], 1);
    assert_eq!(caps["items"]["maxLength"], 120);
}

#[test]
fn fraction_wire_corpus_matches_native_exact_decimal_validation() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(r###"[{"input":"0","valid":true},{"input":"-0","valid":true},{"input":"+0","valid":true},{"input":"000","valid":true},{"input":"+000.1","valid":true},{"input":"-0.25","valid":false},{"input":"1.","valid":true},{"input":".1","valid":true},{"input":"+.1","valid":true},{"input":"-.1","valid":false},{"input":"0.000000000000000001","valid":true},{"input":"99999999999999999999.999999999999999999","valid":false},{"input":"-99999999999999999999.999999999999999999","valid":false},{"input":"0000000000000000000000000000000000000000000000000000000000000000","valid":true},{"input":"1.00000000000000000000000000000000000000000000000000000000000000","valid":true},{"input":".000000000000000000000000000000000000000000000000000000000000000","valid":true},{"input":"","valid":false},{"input":"+","valid":false},{"input":"-","valid":false},{"input":".","valid":false},{"input":"+.","valid":false},{"input":"1e3","valid":false},{"input":"1E-3","valid":false},{"input":"1_000","valid":false},{"input":"NaN","valid":false},{"input":"Infinity","valid":false},{"input":" 1","valid":false},{"input":"1 ","valid":false},{"input":"1\n","valid":false},{"input":"1\r\n","valid":false},{"input":"1 ","valid":false},{"input":"1 ","valid":false},{"input":"\n1","valid":false},{"input":"1\u0000","valid":false},{"input":"١","valid":false},{"input":"１","valid":false},{"input":"1..0","valid":false},{"input":"++1","valid":false},{"input":"+-1","valid":false},{"input":"00000000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":"1.000000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":1,"valid":false},{"input":0.25,"valid":false},{"input":null,"valid":false},{"input":true,"valid":false},{"input":"9","valid":false},{"input":"99","valid":false},{"input":"999","valid":false},{"input":"9999","valid":false},{"input":"99999","valid":false},{"input":"999999","valid":false},{"input":"9999999","valid":false},{"input":"99999999","valid":false},{"input":"999999999","valid":false},{"input":"9999999999","valid":false},{"input":"99999999999","valid":false},{"input":"999999999999","valid":false},{"input":"9999999999999","valid":false},{"input":"99999999999999","valid":false},{"input":"999999999999999","valid":false},{"input":"9999999999999999","valid":false},{"input":"99999999999999999","valid":false},{"input":"999999999999999999","valid":false},{"input":"9999999999999999999","valid":false},{"input":"99999999999999999999","valid":false},{"input":"999999999999999999999","valid":false},{"input":"9999999999999999999999","valid":false},{"input":"99999999999999999999999","valid":false},{"input":"0.1","valid":true},{"input":"0.01","valid":true},{"input":"0.001","valid":true},{"input":"0.0001","valid":true},{"input":"0.00001","valid":true},{"input":"0.000001","valid":true},{"input":"0.0000001","valid":true},{"input":"0.00000001","valid":true},{"input":"0.000000001","valid":true},{"input":"0.0000000001","valid":true},{"input":"0.00000000001","valid":true},{"input":"0.000000000001","valid":true},{"input":"0.0000000000001","valid":true},{"input":"0.00000000000001","valid":true},{"input":"0.000000000000001","valid":true},{"input":"0.0000000000000001","valid":true},{"input":"0.00000000000000001","valid":true},{"input":"0.000000000000000001","valid":true},{"input":"0.0000000000000000001","valid":false},{"input":"0.00000000000000000001","valid":false},{"input":"0.000000000000000000001","valid":false},{"input":"0.0000000000000000000001","valid":false},{"input":"0.00000000000000000000001","valid":false},{"input":"0.25","valid":true},{"input":"0.250","valid":true},{"input":"0.2500000000000000000","valid":true},{"input":"0.25000000000000000000","valid":true},{"input":"0.250000000000000000000","valid":true},{"input":"0.2500000000000000000000000000000000","valid":true},{"input":"0.250000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":"-9","valid":false},{"input":"-99","valid":false},{"input":"-999","valid":false},{"input":"-9999","valid":false},{"input":"-99999","valid":false},{"input":"-999999","valid":false},{"input":"-9999999","valid":false},{"input":"-99999999","valid":false},{"input":"-999999999","valid":false},{"input":"-9999999999","valid":false},{"input":"-99999999999","valid":false},{"input":"-999999999999","valid":false},{"input":"-9999999999999","valid":false},{"input":"-99999999999999","valid":false},{"input":"-999999999999999","valid":false},{"input":"-9999999999999999","valid":false},{"input":"-99999999999999999","valid":false},{"input":"-999999999999999999","valid":false},{"input":"-9999999999999999999","valid":false},{"input":"-99999999999999999999","valid":false},{"input":"-999999999999999999999","valid":false},{"input":"-9999999999999999999999","valid":false},{"input":"-99999999999999999999999","valid":false},{"input":"-0.1","valid":false},{"input":"-0.01","valid":false},{"input":"-0.001","valid":false},{"input":"-0.0001","valid":false},{"input":"-0.00001","valid":false},{"input":"-0.000001","valid":false},{"input":"-0.0000001","valid":false},{"input":"-0.00000001","valid":false},{"input":"-0.000000001","valid":false},{"input":"-0.0000000001","valid":false},{"input":"-0.00000000001","valid":false},{"input":"-0.000000000001","valid":false},{"input":"-0.0000000000001","valid":false},{"input":"-0.00000000000001","valid":false},{"input":"-0.000000000000001","valid":false},{"input":"-0.0000000000000001","valid":false},{"input":"-0.00000000000000001","valid":false},{"input":"-0.000000000000000001","valid":false},{"input":"-0.0000000000000000001","valid":false},{"input":"-0.00000000000000000001","valid":false},{"input":"-0.000000000000000000001","valid":false},{"input":"-0.0000000000000000000001","valid":false},{"input":"-0.00000000000000000000001","valid":false},{"input":"-0.25","valid":false},{"input":"-0.250","valid":false},{"input":"-0.2500000000000000000","valid":false},{"input":"-0.25000000000000000000","valid":false},{"input":"-0.250000000000000000000","valid":false},{"input":"-0.2500000000000000000000000000000000","valid":false},{"input":"-0.250000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":"+9","valid":false},{"input":"+99","valid":false},{"input":"+999","valid":false},{"input":"+9999","valid":false},{"input":"+99999","valid":false},{"input":"+999999","valid":false},{"input":"+9999999","valid":false},{"input":"+99999999","valid":false},{"input":"+999999999","valid":false},{"input":"+9999999999","valid":false},{"input":"+99999999999","valid":false},{"input":"+999999999999","valid":false},{"input":"+9999999999999","valid":false},{"input":"+99999999999999","valid":false},{"input":"+999999999999999","valid":false},{"input":"+9999999999999999","valid":false},{"input":"+99999999999999999","valid":false},{"input":"+999999999999999999","valid":false},{"input":"+9999999999999999999","valid":false},{"input":"+99999999999999999999","valid":false},{"input":"+999999999999999999999","valid":false},{"input":"+9999999999999999999999","valid":false},{"input":"+99999999999999999999999","valid":false},{"input":"+0.1","valid":true},{"input":"+0.01","valid":true},{"input":"+0.001","valid":true},{"input":"+0.0001","valid":true},{"input":"+0.00001","valid":true},{"input":"+0.000001","valid":true},{"input":"+0.0000001","valid":true},{"input":"+0.00000001","valid":true},{"input":"+0.000000001","valid":true},{"input":"+0.0000000001","valid":true},{"input":"+0.00000000001","valid":true},{"input":"+0.000000000001","valid":true},{"input":"+0.0000000000001","valid":true},{"input":"+0.00000000000001","valid":true},{"input":"+0.000000000000001","valid":true},{"input":"+0.0000000000000001","valid":true},{"input":"+0.00000000000000001","valid":true},{"input":"+0.000000000000000001","valid":true},{"input":"+0.0000000000000000001","valid":false},{"input":"+0.00000000000000000001","valid":false},{"input":"+0.000000000000000000001","valid":false},{"input":"+0.0000000000000000000001","valid":false},{"input":"+0.00000000000000000000001","valid":false},{"input":"+0.25","valid":true},{"input":"+0.250","valid":true},{"input":"+0.2500000000000000000","valid":true},{"input":"+0.25000000000000000000","valid":true},{"input":"+0.250000000000000000000","valid":true},{"input":"+0.2500000000000000000000000000000000","valid":true},{"input":"+0.250000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":"-000.000","valid":true},{"input":"-.000","valid":true},{"input":"+00001.00000","valid":true},{"input":"000.999999999999999999","valid":true},{"input":"1.000000000000000001","valid":false},{"input":"0.999999999999999999","valid":true},{"input":"+.000000000000000001","valid":true},{"input":"00001.0000000000000000000000000000000000000000000000000000000","valid":true},{"input":"0.000000000000000000000000000000000000000000000000000000000000","valid":true},{"input":"-0.000000000000000000000000000000000000000000000000000000000000","valid":true}]"###).unwrap();
    for row in rows {
        let actual = serde_json::from_value::<contracts::DecimalValue>(row["input"].clone())
            .is_ok_and(|value| value.is_fraction());
        assert_eq!(
            actual,
            row["valid"].as_bool().unwrap(),
            "{:?}",
            row["input"]
        );
    }
}
