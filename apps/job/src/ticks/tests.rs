//! Controlled native TradeTick/Wasmi counterexamples; not market-quality evidence.
use super::*;
use domain::ticks::evaluation_request;
use nautilus_model::{
    enums::AggressorSide,
    identifiers::{InstrumentId, TradeId},
    types::{Price, Quantity},
};
use serde_json::json;

pub(super) fn request() -> NativeTradeTickFactorEvaluationRequestV1 {
    serde_json::from_value(json!({"schema_version":1,
        "input":{"modality":"TRADE_TICK","instrument_ids":["AUD/USD.SIM"],"event_start_ns":"0","event_end_ns":"80","decision_cutoff_ns":"80","source_revision":"fixed-source","availability":"EVENT_TIME_PROXY_UNVERIFIED"},
        "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
        "target":{"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1},
        "parameters":{"fast_period_cohorts":1,"slow_period_cohorts":2},
        "train_end_ns":"30","validation_end_ns":"50"})).unwrap()
}

fn trade(time: u64, id: &str, price: &str, quantity: &str) -> TradeTick {
    TradeTick::new(
        InstrumentId::from("AUD/USD.SIM"),
        Price::from(price),
        Quantity::from(quantity),
        AggressorSide::Buy,
        TradeId::from(id),
        time.into(),
        time.into(),
    )
}

pub(super) fn series() -> BTreeMap<String, Vec<TradeTick>> {
    BTreeMap::from([(
        "AUD/USD.SIM".to_owned(),
        (1..=7)
            .map(|i| trade(i * 10, &i.to_string(), &(10 + i).to_string(), "1"))
            .collect(),
    )])
}

pub(super) fn model(slot: i32) -> Vec<u8> {
    wat::parse_str(format!(r#"(module
        (global $value (mut f64) (f64.const 0))
        (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
            local.get 0 i32.const {slot} i32.eq
            if local.get 1 global.set $value end)
        (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) global.get $value))"#)).unwrap()
}

#[test]
fn flow_reduction_preserves_marks_and_distinguishes_balanced_zero_from_unknown_side() {
    let mut rows = vec![
        trade(10, "a", "10", "0.1"),
        trade(10, "b", "20", "0.2"),
        trade(10, "c", "30", "0.3"),
    ];
    rows[2].aggressor_side = AggressorSide::Sell;
    let original = cohorts(&rows, 100).unwrap();
    let balanced = cohorts_with_flow(&rows, 100, true).unwrap();
    assert_eq!(balanced[0].signed_quantity, Some(0.0));
    assert_eq!(balanced[0].vwap.to_bits(), original[0].vwap.to_bits());
    assert_eq!(
        balanced[0].quantity.to_bits(),
        original[0].quantity.to_bits()
    );
    rows.reverse();
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap()[0].signed_quantity,
        Some(0.0)
    );
    rows.pop();
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap()[0].signed_quantity,
        Some(-0.1)
    );
    rows[0].aggressor_side = AggressorSide::Buy;
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap()[0].signed_quantity,
        Some(0.5)
    );
    rows[0].aggressor_side = AggressorSide::NoAggressor;
    assert!(cohorts(&rows, 100).unwrap()[0].signed_quantity.is_none());
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap_err().to_string(),
        "TICK_FLOW_AGGRESSOR_UNKNOWN"
    );
}

#[test]
fn exact_cohort_reduction_is_tie_order_invariant_and_has_no_last_trade_mark() {
    let rows = vec![trade(10, "a", "10", "1"), trade(10, "b", "20", "3")];
    let mut reversed = rows.clone();
    reversed.reverse();
    let a = cohorts(&rows, 100).unwrap();
    let b = cohorts(&reversed, 100).unwrap();
    assert_eq!(a[0].vwap, 17.5);
    assert_eq!(a[0].quantity, 4.0);
    assert_eq!(a[0].count, 2);
    assert_eq!(a[0].vwap, b[0].vwap);
    assert_eq!(rows[0].price, Price::from("10"));
}

#[test]
fn explicit_tick_universe_does_not_inherit_the_bar_256_instrument_limit() {
    let mut request = request();
    request.input.instrument_ids = (0..257).map(|i| format!("ASSET{i}.SIM")).collect();
    assert!(evaluation_request(&request).is_ok());
    request
        .input
        .instrument_ids
        .push(request.input.instrument_ids[0].clone());
    assert!(evaluation_request(&request).is_err());
}
