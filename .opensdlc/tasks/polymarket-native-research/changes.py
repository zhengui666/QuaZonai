from pathlib import Path

p = Path('crates/contracts/tests/native_models.rs')
p.write_text('''use contracts::portfolio::*;
use utoipa::PartialSchema;

#[test]
fn model_schema_preserves_exact_native_identity_and_closed_parameters() {
    let schema = serde_json::to_value(NativeModelRefV1::schema()).unwrap();
    let variants = schema["oneOf"].as_array().unwrap();
    let expected = [
        ("NAUTILUS_DEFAULT_FILL", NAUTILUS_FILL_CLASS, NAUTILUS_EXECUTION_VERSION),
        ("NAUTILUS_MAKER_TAKER", NAUTILUS_FEE_CLASS, NAUTILUS_EXECUTION_VERSION),
        ("NAUTILUS_POLYMARKET", NAUTILUS_POLYMARKET_FEE_CLASS, NAUTILUS_EXECUTION_VERSION),
        ("NAUTILUS_STATIC_LATENCY", NAUTILUS_LATENCY_CLASS, NAUTILUS_EXECUTION_VERSION),
        ("CLARABEL_QP", CLARABEL_CLASS, CLARABEL_VERSION),
        ("FIXED_WEIGHTED_FORECAST", FIXED_ENSEMBLE_CLASS, FIXED_ENSEMBLE_VERSION),
        ("SAMPLE_COVARIANCE", SAMPLE_COVARIANCE_CLASS, SAMPLE_COVARIANCE_VERSION),
    ];
    assert_eq!(variants.len(), expected.len());
    for (kind, class, version) in expected {
        let matches = variants.iter().filter(|variant|
            variant["properties"]["adapter_kind"]["enum"] == serde_json::json!([kind])
        ).collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "{kind}");
        let variant = matches[0];
        assert_eq!(variant["additionalProperties"], false);
        assert_eq!(variant["properties"]["upstream_class"]["enum"], serde_json::json!([class]));
        assert_eq!(variant["properties"]["upstream_version"]["enum"], serde_json::json!([version]));
        assert_eq!(variant["properties"]["parameters"]["additionalProperties"], false);
        assert_eq!(variant["required"].as_array().unwrap().len(), 5);
    }
}

#[test]
fn polymarket_fee_wire_does_not_accept_extra_parameters_or_unknown_fields() {
    let original = serde_json::json!({
        "schema_version": 1,
        "adapter_kind": "NAUTILUS_POLYMARKET",
        "upstream_class": NAUTILUS_POLYMARKET_FEE_CLASS,
        "upstream_version": NAUTILUS_EXECUTION_VERSION,
        "parameters": {},
    });
    assert!(matches!(serde_json::from_value::<NativeModelRefV1>(original.clone()).unwrap(),
        NativeModelRefV1::NautilusPolymarket { .. }));
    let mut changed = original.clone();
    changed["parameters"]["fee_rate"] = serde_json::json!(0);
    assert!(serde_json::from_value::<NativeModelRefV1>(changed).is_err());
    let mut changed = original;
    changed["wallet"] = serde_json::json!("not-a-supported-field");
    assert!(serde_json::from_value::<NativeModelRefV1>(changed).is_err());
}
''')

p = Path('apps/runtime/tests/native_oci.rs')
s = p.read_text()
old = '        polymarket::settings(&mut request.execution_settings, "0", expiration);'
assert s.count(old) == 1
p.write_text(s.replace(old, old + '''
        // The same 40% participation limit must remain feasible at the actual
        // sub-dollar prices. Do not relax capacity just to make this test pass.
        request.mandate.capital_assumption = "1000000".parse().unwrap();
        request.execution_settings.starting_capital = request.mandate.capital_assumption.clone();'''))
