use contracts::{
    execution::NativeTaskParametersV1, research::DataPartition, science::*, Id, SchemaV1,
};

#[test]
fn new_feature_abi_cannot_silently_change_old_compile_wire() {
    let code = Id::new();
    let old = NativeTaskParametersV1::CompileModel {
        schema_version: SchemaV1,
        code_artifact_id: code,
    };
    let new = NativeTaskParametersV1::CompileFeatureModel {
        schema_version: SchemaV1,
        code_artifact_id: code,
    };
    let old_json = serde_json::to_value(&old).unwrap();
    assert_eq!(
        old_json,
        serde_json::json!({"schema_version":1,"operation":"COMPILE_MODEL","code_artifact_id":code})
    );
    assert_eq!(
        serde_json::to_value(&new).unwrap()["operation"],
        "COMPILE_FEATURE_MODEL"
    );
    assert!(serde_json::from_value::<NativeTaskParametersV1>(serde_json::json!({"schema_version":1,"operation":"COMPILE_MODEL","code_artifact_id":code,"abi":"FEATURES_V2"})).is_err());
    assert_eq!(
        FEATURE_MODEL_ABI_V2,
        "qz_set_feature_v2(i32,f64,i32,i64,i64)->();qz_predict_v2(i64,i64,i32,i32,i32)->f64"
    );
}

#[test]
fn strict_feature_wire_preserves_null_and_observed_zero() {
    let source = serde_json::json!({"schema_version":1,"partition":"VALIDATION","feature_schema":[{"feature_key":"event-count","source_ref":"snapshot:source-42","source_key":"event-7","availability":{"basis":"MODELED_LAG","lag_ns":"2"},"max_age_ns":null}],"observations":[{"feature_index":0,"event_ns":"9007199254740993","observed_available_ns":null,"sequence":"0","value":0.0,"missing_reason":null}]});
    let parsed: FeatureObservationsV1 = serde_json::from_value(source.clone()).unwrap();
    assert_eq!(parsed.partition, DataPartition::Validation);
    assert_eq!(parsed.observations[0].value, Some(0.0));
    assert_eq!(parsed.observations[0].observed_available_ns, None);
    assert_eq!(serde_json::to_value(&parsed).unwrap(), source);
    let mut extra = source;
    extra["observations"][0]["pit_verified"] = true.into();
    assert!(serde_json::from_value::<FeatureObservationsV1>(extra).is_err());
}
