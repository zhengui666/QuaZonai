use contracts::recorded_fees::RecordedSourceDecimal;
use serde_json::{json, Value};

#[test]
fn original_source_decimals_roundtrip_before_native_account_rounding() {
    for text in [
        "0.0030000000375025784",
        "0.00007000000000000001",
        "0.00000100000000000002",
    ] {
        let original = json!(text);
        let parsed: RecordedSourceDecimal = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), original);
    }
}

#[test]
fn source_decimals_require_finite_plain_decimal_strings() {
    for invalid in [
        Value::Null,
        json!(0.003),
        json!("NaN"),
        json!("Infinity"),
        json!(""),
        json!("1e-19"),
        json!(" 0.5"),
        json!("0.5\n"),
        json!("0..5"),
    ] {
        assert!(serde_json::from_value::<RecordedSourceDecimal>(invalid).is_err());
    }
}
