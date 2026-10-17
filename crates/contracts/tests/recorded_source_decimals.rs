use contracts::recorded_fees::RecordedTradeFeeV1;
use serde_json::{json, Value};

fn record() -> Value {
    json!({
        "id":"synthetic-source-precision-regression", "instrument_id":"123.POLYMARKET",
        "ts_event_ns":"1", "source_quantity":"4", "quantity":"4", "fee_usdc":"0.04",
        "usdc_amount":"2", "price":"0.5", "maker_direction":"BUY", "taker_direction":"SELL"
    })
}

#[test]
fn original_source_price_and_notional_roundtrip_without_numeric_38_18_rounding() {
    for (field, text) in [
        ("price", "0.0030000000375025784"),
        ("usdc_amount", "0.00007000000000000001"),
    ] {
        let mut original = record();
        original[field] = json!(text);
        let parsed: RecordedTradeFeeV1 = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
    }
}

#[test]
fn native_quantities_and_commissions_keep_the_existing_numeric_contract() {
    for field in ["source_quantity", "quantity", "fee_usdc"] {
        let mut original = record();
        original[field] = json!("0.0000000000000000001");
        assert!(serde_json::from_value::<RecordedTradeFeeV1>(original).is_err());
    }
}

#[test]
fn source_audit_decimals_still_require_finite_plain_decimal_strings() {
    for field in ["price", "usdc_amount"] {
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
            let mut original = record();
            original[field] = invalid;
            assert!(serde_json::from_value::<RecordedTradeFeeV1>(original).is_err());
        }
    }
}
