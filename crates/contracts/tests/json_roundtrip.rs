//! Financial floats must not depend on which other workspace package is built.
//! Run this target alone as well as through the native Runtime graph.
use serde_json::{json, Value};

#[test]
fn financial_float_bits_survive_typed_and_value_roundtrips() {
    // The fifteen actual ee7 readback differences (some statistics repeat the
    // same return) plus signs, very small values and the finite range boundary.
    let decimals = [
        "9.999450030218071e-6",
        "9.999300048857407e-6",
        "9.999100081037327e-6",
        "9.998850132308945e-6",
        "9.998650182252433e-6",
        "9.998550209999735e-6",
        "9.998500225094631e-6",
        "9.998450240189527e-6",
        "9.998400255950557e-6",
        "9.998150342305223e-6",
        "-0.000049249147999974596",
        "-0.000049249147999974596",
        "-0.000049249147999974596",
        "-0.000049249147999974596",
        "-0.49998998000046413",
        "0.0",
        "-0.0",
        "1e-300",
        "-1e-300",
        "1.7976931348623157e308",
    ];
    for decimal in decimals {
        let original: f64 = decimal.parse().unwrap();
        let mut value = original;
        for _ in 0..8 {
            let bytes = serde_json::to_vec(&value).unwrap();
            let typed: f64 = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(typed.to_bits(), original.to_bits(), "typed {decimal}");
            let projected: Value = serde_json::from_slice(&bytes).unwrap();
            value = serde_json::from_slice(&serde_json::to_vec(&projected).unwrap()).unwrap();
            assert_eq!(value.to_bits(), original.to_bits(), "projected {decimal}");
        }
    }
}

#[test]
fn native_json_structure_and_decimal_strings_remain_exact() {
    let original = json!({"id":"original-id","clock":"1790985600000000001",
        "count":u64::MAX,"enabled":true,"missing":null,
        "money":"100.00000001 USDT","values":[0.0,-0.5,9.999450030218071e-6_f64]});
    let received: Value = serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(received, original);
    let mut changed = received.clone();
    let bits = received["values"][2].as_f64().unwrap().to_bits();
    changed["values"][2] = json!(f64::from_bits(bits + 1));
    assert_ne!(changed, original, "no numeric tolerance is introduced");
    changed = received.clone();
    changed["clock"] = json!("1790985600000000002");
    assert_ne!(changed, original);
}
