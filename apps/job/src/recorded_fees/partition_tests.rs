use super::*;
use crate::recorded_fees::tests::{charge, market, set_time};
use nautilus_common::clock::VirtualClock;
use nautilus_model::{
    data::BarType,
    identifiers::{InstrumentId, Symbol},
};

fn source(id: &str) -> RecordedTradeFeesV1 {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "source": POLYMARKET_V2_SOURCE,
        "revision": "test-only-not-market-evidence", "currency": "pUSD",
        "records": [
            {"id":format!("{id}-a"),"instrument_id":id,"ts_event_ns":"1","source_quantity":"4","quantity":"4","fee_usdc":"0.04","usdc_amount":"2","price":"0.5","maker_direction":"BUY","taker_direction":"SELL"},
            {"id":format!("{id}-b"),"instrument_id":id,"ts_event_ns":"2","source_quantity":"6","quantity":"6","fee_usdc":"0.12","usdc_amount":"3","price":"0.5","maker_direction":"BUY","taker_direction":"SELL"},
            {"id":format!("{id}-c"),"instrument_id":id,"ts_event_ns":(MINUTE_NS+1).to_string(),"source_quantity":"10","quantity":"10","fee_usdc":"0","usdc_amount":"5","price":"0.5","maker_direction":"BUY","taker_direction":"SELL"}
        ]
    })).unwrap()
}

fn clock() -> Rc<RefCell<dyn Clock>> {
    let clock: Rc<RefCell<dyn Clock>> = Rc::new(RefCell::new(VirtualClock::new()));
    set_time(&clock, MINUTE_NS);
    clock
}

fn two_markets() -> NativeMarketData {
    let mut combined = market();
    let mut other = market().series.remove(0);
    let InstrumentAny::CurrencyPair(pair) = &mut other.instrument else {
        unreachable!()
    };
    pair.id = InstrumentId::from("456.POLYMARKET");
    pair.raw_symbol = Symbol::new("456");
    other.bar_type = BarType::from_str("456.POLYMARKET-1-MINUTE-LAST-EXTERNAL").unwrap();
    for bar in &mut other.bars {
        bar.bar_type = other.bar_type;
    }
    combined.series.push(other);
    combined.rows = 4;
    combined
}

fn write_part(root: &Path, id: &str, value: &RecordedTradeFeesV1) {
    std::fs::create_dir_all(root.join("recorded-fees")).unwrap();
    std::fs::write(
        root.join("recorded-fees").join(format!("{id}.json")),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

#[test]
fn per_market_original_fee_files_load_only_selected_markets_and_preserve_fifo() {
    let root = tempfile::tempdir().unwrap();
    write_part(root.path(), "123.POLYMARKET", &source("123.POLYMARKET"));
    // This unselected source must not be decoded, nor silently consumed.
    std::fs::write(
        root.path().join("recorded-fees/999.POLYMARKET.json"),
        "not selected JSON",
    )
    .unwrap();
    let model = RecordedFeeModel::load(root.path(), &market(), clock()).unwrap();
    assert_eq!(
        charge(&model, "5.000000").unwrap().as_decimal(),
        Decimal::from_str("0.06").unwrap()
    );
    assert_eq!(
        charge(&model, "5.000000").unwrap().as_decimal(),
        Decimal::from_str("0.10").unwrap()
    );
    assert!(charge(&model, "0.000001").is_err());
    let report = model.report();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(
        report.fills[0].allocations[0].original.id,
        "123.POLYMARKET-a"
    );
    assert_eq!(
        report.fills[0].allocations[1].original.id,
        "123.POLYMARKET-b"
    );
}

#[test]
fn legacy_original_file_remains_authoritative_and_invalid_legacy_never_falls_back() {
    let root = tempfile::tempdir().unwrap();
    write_part(root.path(), "123.POLYMARKET", &source("123.POLYMARKET"));
    let legacy = root.path().join(RECORDED_FEES_FILE);
    std::fs::write(
        &legacy,
        serde_json::to_vec(&source("123.POLYMARKET")).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.path().join("recorded-fees/123.POLYMARKET.json"),
        "invalid unused partition",
    )
    .unwrap();
    let model = RecordedFeeModel::load(root.path(), &market(), clock()).unwrap();
    assert_eq!(
        charge(&model, "5.000000").unwrap().as_decimal(),
        Decimal::from_str("0.06").unwrap()
    );
    std::fs::write(&legacy, "invalid original source").unwrap();
    assert!(RecordedFeeModel::load(root.path(), &market(), clock())
        .unwrap_err()
        .to_string()
        .contains("RECORDED_FEE_INVALID_SOURCE"));
}

#[test]
fn selected_market_partition_is_required_and_cannot_change_instrument_or_revision() {
    let root = tempfile::tempdir().unwrap();
    write_part(root.path(), "123.POLYMARKET", &source("123.POLYMARKET"));
    assert!(RecordedFeeModel::load(root.path(), &two_markets(), clock())
        .unwrap_err()
        .to_string()
        .contains("RECORDED_FEE_FILE_REQUIRED"));
    write_part(root.path(), "456.POLYMARKET", &source("123.POLYMARKET"));
    assert!(RecordedFeeModel::load(root.path(), &two_markets(), clock())
        .unwrap_err()
        .to_string()
        .contains("RECORDED_FEE_PARTITION_INSTRUMENT_MISMATCH"));
    let mut other = source("456.POLYMARKET");
    other.revision = "different-original-revision".into();
    write_part(root.path(), "456.POLYMARKET", &other);
    assert!(RecordedFeeModel::load(root.path(), &two_markets(), clock())
        .unwrap_err()
        .to_string()
        .contains("RECORDED_FEE_PARTITION_SOURCE_MISMATCH"));
    write_part(root.path(), "456.POLYMARKET", &source("456.POLYMARKET"));
    let model = RecordedFeeModel::load(root.path(), &two_markets(), clock()).unwrap();
    assert_eq!(model.state.borrow().buckets.len(), 4);
}
