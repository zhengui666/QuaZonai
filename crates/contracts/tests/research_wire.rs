use contracts::research::*;
use serde_json::{json, Value};
fn request() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/contracts/research-policy.json"
    ))
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
    let schema: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let schemas = &schema["components"]["schemas"];
    assert_eq!(
        schemas["SelectionParametersV1"]["properties"]["candidate_count"]["maximum"],
        65535
    );
    assert_eq!(
        schemas["EvaluationPolicyCreate"]["properties"]["minimum_observations"]["maximum"],
        2147483647
    );
    assert_eq!(
        schemas["InputSetCreate"]["properties"]["items"]["maxItems"],
        256
    );
    assert_eq!(
        schemas["SplitPolicyV1"]["properties"]["interval_validation_required"]["enum"],
        json!([true])
    );
    assert_eq!(
        schemas["InputSetCreate"]["properties"]["decision_cutoff"]["format"],
        "date-time"
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
    assert_eq!(a["maxItems"], 64);
    assert_eq!(a["uniqueItems"], true);
    assert_eq!(a["items"]["minLength"], 1);
    assert_eq!(a["items"]["maxLength"], 120);
}
