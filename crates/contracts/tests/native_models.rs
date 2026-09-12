use contracts::portfolio::*;
use utoipa::PartialSchema;

#[test]
fn model_schema_preserves_exact_native_identity_and_closed_parameters() {
    let schema = serde_json::to_value(NativeModelRefV1::schema()).unwrap();
    let variants = schema["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), 3);
    for (variant, class, version) in [
        (&variants[0], CLARABEL_CLASS, CLARABEL_VERSION),
        (&variants[1], FIXED_ENSEMBLE_CLASS, FIXED_ENSEMBLE_VERSION),
        (
            &variants[2],
            SAMPLE_COVARIANCE_CLASS,
            SAMPLE_COVARIANCE_VERSION,
        ),
    ] {
        assert_eq!(variant["additionalProperties"], false);
        assert_eq!(
            variant["properties"]["upstream_class"]["enum"],
            serde_json::json!([class])
        );
        assert_eq!(
            variant["properties"]["upstream_version"]["enum"],
            serde_json::json!([version])
        );
        assert_eq!(
            variant["properties"]["parameters"]["additionalProperties"],
            false
        );
        assert_eq!(variant["required"].as_array().unwrap().len(), 5);
    }
}
