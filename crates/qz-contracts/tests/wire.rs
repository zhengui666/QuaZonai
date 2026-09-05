use qz_contracts::{
    budget::BudgetV1, evidence::MetricValueV1, DbCounter, DecimalValue, Id, Revision, SchemaV1,
};
use serde_json::{from_value, json, to_value};

#[test]
fn precise_database_values_never_cross_json_as_numbers() {
    let boundary = "9007199254740993";
    let counter: DbCounter = from_value(json!(boundary)).unwrap();
    assert_eq!(to_value(counter).unwrap(), json!(boundary));
    let revision: Revision = from_value(json!(i64::MAX.to_string())).unwrap();
    assert!(revision.next().is_none());
    assert!(DbCounter::new(i64::MAX as u64)
        .unwrap()
        .checked_add(1)
        .is_none());
    for invalid in [
        json!(1),
        json!(null),
        json!(true),
        json!("-1"),
        json!("01"),
        json!("+1"),
        json!(" 1"),
        json!("9223372036854775808"),
    ] {
        assert!(
            from_value::<DbCounter>(invalid.clone()).is_err(),
            "{invalid}"
        );
        assert!(
            from_value::<Revision>(invalid.clone()).is_err(),
            "{invalid}"
        );
    }
    assert!(from_value::<Revision>(json!("0")).is_err());
    assert_eq!(
        from_value::<DbCounter>(json!("0")).unwrap(),
        DbCounter::ZERO
    );
}

#[test]
fn native_arbitrary_precision_preserves_the_complete_numeric_38_18_range() {
    for text in [
        "99999999999999999999.999999999999999999",
        "-99999999999999999999.999999999999999999",
        "0.000000000000000001",
        "0",
    ] {
        let value: DecimalValue = from_value(json!(text)).unwrap();
        assert_eq!(to_value(&value).unwrap(), json!(text));
        assert_eq!(
            from_value::<DecimalValue>(to_value(&value).unwrap()).unwrap(),
            value
        );
    }
    assert_eq!(
        to_value("0.2500".parse::<DecimalValue>().unwrap()).unwrap(),
        json!("0.25")
    );
    for text in [
        "100000000000000000000",
        "0.0000000000000000001",
        "1e3",
        "1_000",
        "NaN",
        "Infinity",
        "",
        " 1",
        "1 ",
    ] {
        assert!(from_value::<DecimalValue>(json!(text)).is_err(), "{text}");
    }
    assert!(from_value::<DecimalValue>(json!(0.25)).is_err());
}

#[test]
fn schema_versions_and_local_identity_are_explicit() {
    assert_eq!(to_value(SchemaV1).unwrap(), json!(1));
    for bad in [json!(0), json!(2), json!("1"), json!(1.0), json!(null)] {
        assert!(from_value::<SchemaV1>(bad).is_err());
    }
    let id = Id::new();
    assert_eq!(id.as_uuid().get_version_num(), 7);
    assert_eq!(from_value::<Id>(to_value(id).unwrap()).unwrap(), id);
    for bad in [
        json!("00000000-0000-4000-8000-000000000000"),
        json!("00000000-0000-7000-0000-000000000000"),
        json!("external-job/1"),
        json!(null),
    ] {
        assert!(from_value::<Id>(bad).is_err());
    }
}

#[test]
fn command_objects_reject_unknown_fields_missing_booleans_and_null_integers() {
    let valid = json!({"schema_version":1,"max_experiments":20,"max_parallel_runs":2,
        "max_turns_per_mission":16,"max_repair_turns":2,"max_wall_seconds":3600,
        "max_cpu_seconds":"7200","max_memory_mib":4096,"max_output_bytes":"67108864",
        "max_cycles_per_day":3,"min_cycle_interval_seconds":120,"max_tokens":null,
        "max_cost_decimal":null,"cost_currency":null,"cost_enforcement":"UNAVAILABLE"});
    assert!(from_value::<BudgetV1>(valid.clone()).is_ok());
    let mut unknown = valid.clone();
    unknown["skip_budget_gate"] = json!(true);
    assert!(from_value::<BudgetV1>(unknown).is_err());
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("schema_version");
    assert!(from_value::<BudgetV1>(missing).is_err());
    let mut invalid = valid;
    invalid["max_parallel_runs"] = json!(null);
    assert!(from_value::<BudgetV1>(invalid).is_err());
    for invalid in [
        json!({"schema_version":1,"saved_model":null,"saved_reasoning_effort":null,"saved_fast_mode":false}),
        json!({"schema_version":1,"use_default_model_settings":null,"saved_model":null,"saved_reasoning_effort":null,"saved_fast_mode":false}),
    ] {
        assert!(from_value::<qz_contracts::codex::SavedModelSettingsV1>(invalid).is_err());
    }
}

#[test]
fn nonfinite_native_metrics_cannot_be_serialized_into_innocent_nulls() {
    let value = json!({"schema_version":1,"evaluation_id":Id::new(),"metric_code":"risk","scope":"total",
        "value":0.1,"status":"OK","reason_code":null,"unit":"fraction","period_start":"2026-01-01T00:00:00Z",
        "period_end":"2026-02-01T00:00:00Z","observation_count":"30","frequency":"daily",
        "annualization_factor":252.0,"method_id":"native-risk","method_version":"1","source_artifact_id":Id::new(),"higher_is_better":false});
    let original: MetricValueV1 = from_value(value).unwrap();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut metric = original.clone();
        metric.value = Some(invalid);
        assert!(to_value(&metric).is_err());
        metric.value = None;
        metric.annualization_factor = Some(invalid);
        assert!(to_value(&metric).is_err());
    }
}

#[test]
fn generated_schema_describes_initial_slice_not_nonexistent_http_routes() {
    let first = qz_contracts::openapi_json().unwrap();
    assert_eq!(first, qz_contracts::openapi_json().unwrap());
    let schema: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert!(schema["paths"].as_object().unwrap().is_empty());
    assert_eq!(
        schema["components"]["schemas"]["Revision"]["type"],
        "string"
    );
    assert_eq!(
        schema["components"]["schemas"]["DecimalValue"]["type"],
        "string"
    );
    assert_eq!(schema["components"]["schemas"]["SchemaV1"]["minimum"], 1);
    assert_eq!(
        schema["components"]["schemas"]["BudgetV1"]["additionalProperties"],
        false
    );
}
