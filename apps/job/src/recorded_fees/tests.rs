use super::*;
use crate::catalog::NativeBarSeries;
use contracts::SchemaV1;
use nautilus_common::clock::TestClock;
use nautilus_model::{
    data::{Bar, BarType},
    enums::{LiquiditySide, OrderSide, OrderType},
    identifiers::{InstrumentId, Symbol},
    instruments::CurrencyPair,
    orders::OrderTestBuilder,
};

const ID: &str = "123.POLYMARKET";

pub(super) fn market() -> NativeMarketData {
    let instrument = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from(ID))
            .raw_symbol(Symbol::new("123"))
            .base_currency(Currency::USDC())
            .quote_currency(Currency::from_str("pUSD").unwrap())
            .price_precision(4)
            .size_precision(6)
            .price_increment(Price::from("0.0001"))
            .size_increment(Quantity::from("0.000001"))
            .ts_event(0.into())
            .ts_init(0.into())
            .build()
            .unwrap(),
    );
    let bar_type = BarType::from_str(&format!("{ID}-1-MINUTE-LAST-EXTERNAL")).unwrap();
    let bars = [MINUTE_NS, 2 * MINUTE_NS]
        .into_iter()
        .map(|end| {
            Bar::new(
                bar_type,
                Price::from("0.5000"),
                Price::from("0.5000"),
                Price::from("0.5000"),
                Price::from("0.5000"),
                Quantity::from("10.000000"),
                end.into(),
                end.into(),
            )
        })
        .collect();
    NativeMarketData {
        series: vec![NativeBarSeries {
            instrument,
            instrument_updates: vec![],
            bar_type,
            bars,
        }],
        rows: 2,
    }
}
fn record(id: &str, time: u64, quantity: &str, fee: &str) -> RecordedTradeFeeV1 {
    RecordedTradeFeeV1 {
        id: id.into(),
        instrument_id: ID.into(),
        ts_event_ns: DbCounter::new(time).unwrap(),
        source_quantity: quantity.parse().unwrap(),
        quantity: quantity.parse().unwrap(),
        fee_usdc: fee.parse().unwrap(),
        usdc_amount: "5".parse().unwrap(),
        price: "0.5".parse().unwrap(),
        maker_direction: "BUY".into(),
        taker_direction: "SELL".into(),
    }
}
fn source() -> RecordedTradeFeesV1 {
    RecordedTradeFeesV1 {
        schema_version: SchemaV1,
        source: POLYMARKET_V2_SOURCE.into(),
        revision: "test-only-not-market-evidence".into(),
        currency: "pUSD".into(),
        records: vec![
            record("a", 1, "4", "0.04"),
            record("b", 2, "6", "0.12"),
            record("c", MINUTE_NS + 1, "10", "0"),
        ],
    }
}
pub(super) fn model() -> (RecordedFeeModel, Rc<RefCell<dyn Clock>>) {
    let clock: Rc<RefCell<dyn Clock>> = Rc::new(RefCell::new(TestClock::new()));
    set_time(&clock, MINUTE_NS);
    (
        RecordedFeeModel::new(source(), &market(), clock.clone()).unwrap(),
        clock,
    )
}
pub(super) fn set_time(clock: &Rc<RefCell<dyn Clock>>, time: u64) {
    clock
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<TestClock>()
        .unwrap()
        .set_time(time.into());
}
fn order() -> OrderAny {
    let mut order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from(ID))
        .side(OrderSide::Buy)
        .quantity(Quantity::from("10.000000"))
        // Deliberately differs from both actual fill times.
        .ts_init(7.into())
        .build();
    order.set_liquidity_side(LiquiditySide::Taker);
    order
}
pub(super) fn charge(model: &RecordedFeeModel, quantity: &str) -> Result<Money> {
    model.get_commission(
        &order(),
        Quantity::from(quantity),
        Price::from("0.5000"),
        &market().series[0].instrument,
    )
}

#[test]
fn original_records_are_allocated_once_not_averaged_into_a_rate() {
    let (model, _) = model();
    assert_eq!(
        charge(&model, "5.000000").unwrap().as_decimal(),
        Decimal::from_str("0.06").unwrap()
    );
    assert_eq!(
        charge(&model, "5.000000").unwrap().as_decimal(),
        Decimal::from_str("0.10").unwrap()
    );
    let report = model.report();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(report.fills[0].fill_ns.get(), MINUTE_NS);
    assert_eq!(report.fills[0].allocations[0].original.id, "a");
    assert_eq!(
        report.fills[0].allocations[0].allocated_quantity,
        "4".parse().unwrap()
    );
    assert_eq!(report.fills[0].allocations[1].original.id, "b");
    assert_eq!(report.fills[1].allocations.len(), 1);
    assert!(charge(&model, "0.000001")
        .unwrap_err()
        .to_string()
        .contains("QUANTITY_EXHAUSTED"));
}

#[test]
fn actual_fill_clock_selects_recorded_zero_and_unmatched_clock_is_not_zero() {
    let (model, clock) = model();
    set_time(&clock, 2 * MINUTE_NS);
    assert_eq!(
        charge(&model, "2.000000").unwrap().as_decimal(),
        Decimal::ZERO
    );
    assert_eq!(model.report().fills[0].allocations[0].original.id, "c");
    set_time(&clock, 3 * MINUTE_NS + 1);
    assert!(charge(&model, "2.000000")
        .unwrap_err()
        .to_string()
        .contains("FILL_CLOCK_UNMATCHED"));
    assert_eq!(model.report().fills.len(), 1);
}

#[test]
fn failed_overallocation_does_not_consume_or_create_audit() {
    let (model, _) = model();
    assert!(charge(&model, "11.000000").is_err());
    assert!(model.report().fills.is_empty());
    assert_eq!(
        charge(&model, "10.000000").unwrap().as_decimal(),
        Decimal::from_str("0.16").unwrap()
    );
}

#[test]
fn missing_duplicate_negative_null_and_quantity_mismatch_are_errors() {
    let (_, clock) = model();
    let original = source();
    let mut cases = Vec::new();
    let mut missing = original.clone();
    missing.records.retain(|r| r.id != "c");
    cases.push(missing);
    let mut duplicate = original.clone();
    duplicate.records.push(duplicate.records[0].clone());
    cases.push(duplicate);
    let mut negative = original.clone();
    negative.records[0].fee_usdc = "-0.01".parse().unwrap();
    cases.push(negative);
    let mut quantity = original.clone();
    quantity.records[0].quantity = "3".parse().unwrap();
    cases.push(quantity);
    for invalid in cases {
        assert!(RecordedFeeModel::new(invalid, &market(), clock.clone()).is_err());
    }
    let mut null = serde_json::to_value(&original).unwrap();
    null["records"][0]["fee_usdc"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<RecordedTradeFeesV1>(null).is_err());
}

#[test]
fn proportional_fee_rounds_once_per_fill_at_native_currency_precision() {
    let (_, clock) = model();
    let mut data = source();
    data.records[0].fee_usdc = "0.000001".parse().unwrap();
    data.records[1].fee_usdc = "0".parse().unwrap();
    let model = RecordedFeeModel::new(data, &market(), clock).unwrap();
    assert_eq!(
        charge(&model, "2.000000").unwrap().as_decimal(),
        Decimal::ZERO
    ); // exact half-even
    assert_eq!(
        model.report().fills[0].allocations[0].allocated_fee_numerator,
        "1"
    );
    assert_eq!(
        model.report().fills[0].allocations[0].allocated_fee_denominator,
        "2000000"
    );
}

pub(super) fn native_scenario() -> (
    tempfile::TempDir,
    NativeMarketData,
    contracts::science::NativeSimulationRequestV1,
) {
    use contracts::{portfolio::AllocationTargetV1, science::*};
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    let directory = tempfile::tempdir().unwrap();
    let mut data = market();
    let mut source = source();
    source.records[2].fee_usdc = "0.10".parse().unwrap();
    for index in 3..=6 {
        let mut bar = data.series[0].bars[0];
        bar.ts_event = (index * MINUTE_NS).into();
        bar.ts_init = bar.ts_event;
        data.series[0].bars.push(bar);
        source.records.push(record(
            &format!("later-{index}"),
            (index - 1) * MINUTE_NS + 1,
            "10",
            "0.10",
        ));
    }
    data.rows = 6;
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    catalog
        .write_instruments(vec![data.series[0].instrument.clone()])
        .unwrap();
    catalog
        .write_to_parquet(&data.series[0].bars, None, None, None)
        .unwrap();
    std::fs::write(
        directory.path().join(RECORDED_FEES_FILE),
        serde_json::to_vec(&source).unwrap(),
    )
    .unwrap();
    let settings: NativeSimulationSettingsV1 = serde_json::from_value(serde_json::json!({
        "schema_version":1,"base_currency":"pUSD","starting_capital":"3000","account_kind":"MARGIN","leverage":"1",
        "fee_model":{"schema_version":1,"adapter_kind":"POLYMARKET_V2_RECORDED_FEES","native_version":"0.63.0","parameters":{}},
        "fill_model":{"schema_version":1,"adapter_kind":"NAUTILUS_DEFAULT_FILL","upstream_class":"nautilus_execution::models::fill::DefaultFillModel","upstream_version":"0.63.0","parameters":{"prob_fill_on_limit":"1","prob_slippage":"0","random_seed":"1"}},
        "latency_model":{"schema_version":1,"adapter_kind":"NAUTILUS_STATIC_LATENCY","upstream_class":"nautilus_execution::models::latency::StaticLatencyModel","upstream_version":"0.63.0","parameters":{"base_latency_ns":"1","insert_latency_ns":"0","update_latency_ns":"0","cancel_latency_ns":"0"}},
        "snapshot_interval_ms":60000,"exposure_tolerance":"0.00001","fee_rates":[{"instrument_id":ID,"maker":"0","taker":"0"}]
    })).unwrap();
    let end = DbCounter::new(7 * MINUTE_NS).unwrap();
    let targets = ["0.0001", "0"]
        .into_iter()
        .enumerate()
        .map(|(i, weight)| NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: DbCounter::new(if i == 0 { MINUTE_NS } else { 4 * MINUTE_NS }).unwrap(),
            valid_until_ns: end,
            targets: vec![AllocationTargetV1 {
                instrument_id: ID.into(),
                currency: "pUSD".into(),
                weight: weight.parse().unwrap(),
            }],
            cash_weight: if i == 0 { "0.9999" } else { "1" }.parse().unwrap(),
        })
        .collect();
    let request = NativeSimulationRequestV1 {
        schema_version: SchemaV1,
        settlements: vec![],
        settings,
        target_points: targets,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec![data.series[0].bar_type.to_string()],
            event_start_ns: DbCounter::new(MINUTE_NS).unwrap(),
            event_end_ns: end,
            decision_cutoff_ns: end,
            maximum_rows: 6,
        },
    };
    (directory, data, request)
}

#[test]
fn native_shared_account_books_exactly_the_recorded_allocation_once() {
    let (directory, data, request) = native_scenario();
    #[cfg(feature = "native-paper")]
    {
        let error = crate::simulation::paper_target_strategy(
            request.settings.clone(),
            vec![],
            vec![data.series[0].bar_type],
            request.target_points[0].clone(),
            nautilus_model::identifiers::StrategyId::from("RECORDED-001"),
            nautilus_model::identifiers::ClientId::from("RECORDED-TEST"),
        )
        .err()
        .expect("recorded cost replay cannot enter Paper without a catalog and replay clock");
        assert!(error.to_string().contains("RECORDED_FEES_BACKTEST_ONLY"));
    }
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.unwrap();
    assert_eq!(report.fills.len(), 2);
    assert!(report.fills[0].fill_ns.get() < 4 * MINUTE_NS);
    assert!(report.fills[1].fill_ns.get() >= 4 * MINUTE_NS);
    assert!(report
        .fills
        .iter()
        .all(|fill| fill.bar_end_ns.get() == fill.fill_ns.get() / MINUTE_NS * MINUTE_NS));
    let allocated: BigDecimal = report
        .fills
        .iter()
        .map(|fill| fill.commission.as_decimal())
        .sum();
    assert!(allocated > BigDecimal::from(0));
    let charged = Money::from_str(
        result.canonical_result["accounts"][0]["Margin"]["base"]["commissions"]["pUSD"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        allocated,
        BigDecimal::from_str(&charged.as_decimal().to_string()).unwrap()
    );
    let balance = result
        .summary
        .iter()
        .find(|(key, _)| key.ends_with(".balance.pUSD.total"))
        .unwrap()
        .1;
    let balance = Money::from_str(balance).unwrap();
    assert_eq!(
        BigDecimal::from_str(&balance.as_decimal().to_string()).unwrap(),
        BigDecimal::from(3000) - allocated
    );
    std::fs::remove_file(directory.path().join(RECORDED_FEES_FILE)).unwrap();
    assert!(format!(
        "{:#}",
        crate::simulation::simulate(directory.path(), &request).unwrap_err()
    )
    .contains("RECORDED_FEE_FILE_REQUIRED"));
}

#[test]
fn repeating_quantity_ratio_remains_exact_until_one_currency_rounding() {
    let (_, clock) = model();
    let mut data = source();
    data.records[0].quantity = "3".parse().unwrap();
    data.records[0].source_quantity = "3".parse().unwrap();
    data.records[0].fee_usdc = "1".parse().unwrap();
    data.records[1].quantity = "7".parse().unwrap();
    data.records[1].source_quantity = "7".parse().unwrap();
    data.records[1].fee_usdc = "0".parse().unwrap();
    let model = RecordedFeeModel::new(data, &market(), clock).unwrap();
    assert_eq!(
        charge(&model, "1.000000").unwrap().as_decimal(),
        Decimal::from_str("0.333333").unwrap()
    );
    let report = model.report();
    assert_eq!(report.fills[0].allocations[0].allocated_fee_numerator, "1");
    assert_eq!(
        report.fills[0].allocations[0].allocated_fee_denominator,
        "3"
    );
}
