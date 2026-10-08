//! Controlled native components only. These rows are not market/PIT evidence.
use super::*;
use contracts::{
    SchemaV1,
    science::{NativeAccountKind, NativeFeeRateV1},
};
use nautilus_backtest::config::{BacktestEngineConfig, SimulatedVenueConfig};
use nautilus_common::{clock::TestClock, logging::logger::LoggerConfig};
use nautilus_core::{UUID4, UnixNanos};
use nautilus_model::{
    data::{BarType, Data},
    enums::{BookType, OmsType},
    identifiers::{InstrumentId, Symbol},
    instruments::CurrencyPair,
    types::{Currency, Money, Price, Quantity},
};
use rust_decimal::Decimal;
use std::str::FromStr;
#[path = "../../../../tests/support/execution_models.rs"]
mod models;
#[path = "../../tests/support/native_portfolio.rs"]
mod native_portfolio;

fn money(value: &str) -> contracts::DecimalValue {
    value.parse().unwrap()
}
fn settings() -> NativeSimulationSettingsV1 {
    NativeSimulationSettingsV1 {
        multi_currency_spot_cash: Some(NativeSpotCashPolicyV1 {
            schema_version: SchemaV1,
            mode: NativeSpotCashModeV1::MultiCurrencyCash,
            report_currency: "USD".into(),
            price_method: NativeSpotPriceMethodV1::ClosedBarClose,
            allowed_instrument_ids: vec!["USDT/USD.SIM".into()],
            returns_policy: ReportCurrencyDailyPolicyV1::FreshSimulationNoExternalFlows,
            daily_sampling: NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries,
            maximum_price_age_ns: count(1_000_000_000).unwrap(),
        }),
        schema_version: SchemaV1,
        base_currency: "USD".into(),
        starting_capital: money("1000"),
        account_kind: NativeAccountKind::Cash,
        leverage: money("1"),
        fee_model: models::fee(),
        fill_model: models::fill(),
        latency_model: models::latency(1),
        snapshot_interval_ms: 1000,
        exposure_tolerance: money("0.000001"),
        fee_rates: vec![NativeFeeRateV1 {
            instrument_id: "USDT/USD.SIM".into(),
            maker: money("0"),
            taker: money("0"),
        }],
    }
}
fn market() -> (NativeMarketData, Vec<ClosedBarSourceRow>) {
    let instrument = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from_str("USDT/USD.SIM").unwrap())
            .raw_symbol(Symbol::new("USDT/USD"))
            .base_currency(Currency::from_str("USDT").unwrap())
            .quote_currency(Currency::from_str("USD").unwrap())
            .price_precision(2)
            .size_precision(2)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.01"))
            .maker_fee(Decimal::ZERO)
            .taker_fee(Decimal::ZERO)
            .margin_init(Decimal::ONE)
            .margin_maint(Decimal::ONE)
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    let kind = BarType::from_str("USDT/USD.SIM-1-MINUTE-LAST-EXTERNAL").unwrap();
    let bars: Vec<_> = [native_portfolio::FIRST_TIME, native_portfolio::SECOND_TIME]
        .into_iter()
        .map(|time| {
            Bar::new_checked(
                kind,
                Price::from("2.00"),
                Price::from("2.00"),
                Price::from("2.00"),
                Price::from("2.00"),
                Quantity::from("1.00"),
                time.into(),
                time.into(),
            )
            .unwrap()
        })
        .collect();
    let rows = bars
        .iter()
        .enumerate()
        .map(|(i, bar)| ClosedBarSourceRow {
            event_time: NativeSpotBarEventTimeV1::CloseExclusive,
            source_row_key: format!("controlled-original:{i}"),
            bar_open_ns: count(bar.ts_event.as_u64() - 60_000_000_000).unwrap(),
            bar_close_ns: count(bar.ts_event.as_u64()).unwrap(),
            native: *bar,
        })
        .collect();
    (
        NativeMarketData {
            series: vec![crate::catalog::NativeBarSeries {
                instrument,
                instrument_updates: vec![],
                bar_type: kind,
                bars,
            }],
            rows: 2,
        },
        rows,
    )
}
fn state(capacity: usize) -> State {
    let (market, rows) = market();
    State::new(
        &settings(),
        msgbus::get_message_bus()
            .borrow()
            .instance_id
            .to_string()
            .parse()
            .unwrap(),
        Id::new(),
        &market,
        rows,
        Some(capacity),
    )
    .unwrap()
}
fn snapshot() -> PortfolioSnapshot {
    let mut fixture = native_portfolio::CashPortfolio::new();
    fixture
        .portfolio
        .build_snapshot(&fixture.account_id)
        .unwrap()
}

#[test]
fn frozen_source_bar_and_closed_boundary_must_match_exactly() {
    let (market, mut rows) = market();
    rows[0].native.close = Price::from("3.00");
    assert!(
        State::new(
            &settings(),
            UUID4::new().to_string().parse().unwrap(),
            Id::new(),
            &market,
            rows,
            Some(10)
        )
        .is_err()
    );
    let (market, mut rows) = self::market();
    rows[0].bar_open_ns = rows[0].bar_close_ns;
    assert!(
        State::new(
            &settings(),
            UUID4::new().to_string().parse().unwrap(),
            Id::new(),
            &market,
            rows,
            Some(10)
        )
        .is_err()
    );
}

#[test]
fn unknown_repeated_or_rewritten_native_bar_is_rejected() {
    let mut state = state(10);
    let (_, rows) = market();
    let bar = rows[0].native;
    let mut modified = bar;
    modified.close = Price::from("3.00");
    assert!(
        state
            .observe_bar(&modified, modified.ts_init.as_u64())
            .is_err()
    );
    state.observe_bar(&bar, bar.ts_init.as_u64()).unwrap();
    assert!(state.observe_bar(&bar, bar.ts_init.as_u64()).is_err());
}

#[test]
fn true_native_balances_can_be_valued_before_any_artifact_exists() {
    let mut state = state(10);
    let (_, rows) = market();
    let native = snapshot();
    state
        .observe_bar(&rows[0].native, native_portfolio::FIRST_TIME)
        .unwrap();
    state
        .observe_snapshot(&native, native_portfolio::FIRST_TIME)
        .unwrap();
    let ObservedSpotCashRecord::Snapshot {
        frame,
        valuation,
        native: retained,
        ..
    } = &state.records[1]
    else {
        panic!("snapshot record missing");
    };
    assert_eq!(
        frame.snapshot.binding.event_id.to_string(),
        native.event_id.to_string()
    );
    assert_eq!(
        serde_json::to_value(retained).unwrap(),
        serde_json::to_value(&native).unwrap()
    );
    let ReportCurrencyValuationOutcomeV1::Complete { total, .. } = &valuation.outcome else {
        panic!("valuation unavailable");
    };
    assert_eq!(total, &money("1000.49691356"));
    assert_eq!(frame.prices[0].observed_sequence.get(), 1);
    assert_eq!(frame.snapshot.binding.snapshot_sequence.get(), 2);
}

#[test]
fn native_total_equity_is_not_substituted_for_cash_inventory() {
    let mut state = state(10);
    let (_, rows) = market();
    let mut native = snapshot();
    // Deliberately conflicting fixture isolates the adapter selection boundary.
    native.total_equity = vec![Money::from("999999 USD")];
    state
        .observe_bar(&rows[0].native, native_portfolio::FIRST_TIME)
        .unwrap();
    state
        .observe_snapshot(&native, native_portfolio::FIRST_TIME)
        .unwrap();
    let ObservedSpotCashRecord::Snapshot { valuation, .. } = &state.records[1] else {
        panic!("snapshot missing");
    };
    let ReportCurrencyValuationOutcomeV1::Complete { total, .. } = &valuation.outcome else {
        panic!("valuation missing");
    };
    assert_eq!(total, &money("1000.49691356"));
}

#[test]
fn late_or_wrong_clock_snapshot_is_not_backdated() {
    let mut state = state(10);
    let mut native = snapshot();
    native.ts_event = (native_portfolio::FIRST_TIME - 1).into();
    assert!(
        state
            .observe_snapshot(&native, native_portfolio::FIRST_TIME)
            .is_err()
    );
    native.ts_event = native.ts_init;
    assert!(
        state
            .observe_snapshot(&native, native_portfolio::SECOND_TIME)
            .is_err()
    );
}

#[test]
fn wrong_native_cash_kind_or_currency_is_not_projected() {
    let mut state = state(10);
    let mut native = snapshot();
    native.base_currency = Some(Currency::from_str("USD").unwrap());
    assert!(
        state
            .observe_snapshot(&native, native_portfolio::FIRST_TIME)
            .is_err()
    );
    native.base_currency = None;
    native.account_type = AccountType::Margin;
    assert!(
        state
            .observe_snapshot(&native, native_portfolio::FIRST_TIME)
            .is_err()
    );
}

#[test]
fn bounded_capture_and_snapshot_event_identity_fail_closed() {
    let mut limited = state(1);
    let (_, rows) = market();
    let native = snapshot();
    limited
        .observe_bar(&rows[0].native, native_portfolio::FIRST_TIME)
        .unwrap();
    assert!(
        limited
            .observe_snapshot(&native, native_portfolio::FIRST_TIME)
            .is_err()
    );
    let mut state = state(10);
    state
        .observe_snapshot(&native, native_portfolio::FIRST_TIME)
        .unwrap();
    assert!(
        state
            .observe_snapshot(&native, native_portfolio::FIRST_TIME)
            .is_err()
    );
}

#[test]
fn native_msgbus_callback_order_is_retained_and_drop_unsubscribes() {
    let clock = Rc::new(RefCell::new(TestClock::new()));
    clock
        .borrow_mut()
        .advance_time(UnixNanos::from(native_portfolio::FIRST_TIME), true);
    let capture = Capture::attach(state(10), clock.clone()).unwrap();
    let retained = capture.state.clone();
    let (_, rows) = market();
    let native = snapshot();
    msgbus::publish_bar(
        format!("data.bars.{}", rows[0].native.bar_type).into(),
        &rows[0].native,
    );
    msgbus::publish_portfolio_snapshot(
        format!("events.portfolio.{}", native.account_id).into(),
        &native,
    );
    assert!(retained.borrow().failure.is_none());
    assert_eq!(retained.borrow().records.len(), 2);
    drop(capture);
    msgbus::publish_bar(
        format!("data.bars.{}", rows[0].native.bar_type).into(),
        &rows[0].native,
    );
    assert!(retained.borrow().failure.is_none());
    assert_eq!(retained.borrow().records.len(), 2);
}

#[test]
fn native_callback_failure_survives_the_callback_boundary() {
    let clock = Rc::new(RefCell::new(TestClock::new()));
    clock
        .borrow_mut()
        .advance_time(UnixNanos::from(native_portfolio::FIRST_TIME), true);
    let capture = Capture::attach(state(10), clock).unwrap();
    let (_, rows) = market();
    let mut changed = rows[0].native;
    changed.close = Price::from("9.00");
    msgbus::publish_bar(format!("data.bars.{}", changed.bar_type).into(), &changed);
    assert!(
        capture
            .state
            .borrow()
            .failure
            .as_ref()
            .unwrap()
            .contains("BAR_OR_CLOCK_MISMATCH")
    );
    assert!(capture.state.borrow().records.is_empty());
}

#[test]
fn fresh_official_backtest_records_real_run_identity_without_claiming_no_flows() {
    let (market, rows) = market();
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .build()
            .unwrap(),
        shutdown_on_error: true,
        bypass_logging: true,
        ..Default::default()
    })
    .unwrap();
    engine
        .add_venue(
            SimulatedVenueConfig::builder()
                .venue(market.series[0].instrument.venue())
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Cash)
                .book_type(BookType::L1_MBP)
                .starting_balances(vec![Money::from("1000 USD")])
                .allow_cash_borrowing(false)
                .default_leverage(Decimal::ONE)
                .bar_execution(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    engine.add_instrument(&market.series[0].instrument).unwrap();
    engine
        .add_data(
            market.series[0]
                .bars
                .iter()
                .copied()
                .map(Data::Bar)
                .collect(),
            None,
            true,
            true,
        )
        .unwrap();
    let observed = record_native_run(&mut engine, &settings(), Id::new(), &market, rows, 100);
    let actual_instance = engine.instance_id().to_string();
    let actual_run = engine.run_id().unwrap().to_string();
    engine.dispose();
    let tape = observed.unwrap();
    assert_eq!(tape.native_instance_id.to_string(), actual_instance);
    assert_eq!(tape.native_run_id.to_string(), actual_run);
    assert!(tape.flow_evidence.is_none());
    assert_eq!(
        tape.records
            .iter()
            .filter(|r| matches!(r, ObservedSpotCashRecord::Bar { .. }))
            .count(),
        2
    );
    assert!(
        tape.records
            .iter()
            .any(|r| matches!(r, ObservedSpotCashRecord::Snapshot { .. }))
    );
}

#[test]
fn numeric_equality_does_not_allow_native_precision_rewrites() {
    let (market, mut rows) = market();
    rows[0].native.close = Price::from("2.000");
    assert_eq!(market.series[0].bars[0], rows[0].native);
    assert!(
        State::new(
            &settings(),
            UUID4::new().to_string().parse().unwrap(),
            Id::new(),
            &market,
            rows,
            Some(10)
        )
        .is_err()
    );
    for volume in [false, true] {
        let mut state = state(10);
        let (_, rows) = self::market();
        let mut bar = rows[0].native;
        if volume {
            bar.volume = Quantity::from("1.000");
        } else {
            bar.close = Price::from("2.000");
        }
        assert_eq!(bar, rows[0].native);
        assert!(state.observe_bar(&bar, bar.ts_init.as_u64()).is_err());
    }
}

#[test]
fn bus_switch_is_detected_and_drop_cleans_the_original_without_replacing_current_bus() {
    let bus_a = msgbus::get_message_bus();
    let clock = Rc::new(RefCell::new(TestClock::new()));
    clock
        .borrow_mut()
        .advance_time(UnixNanos::from(native_portfolio::FIRST_TIME), true);
    let capture = Capture::attach(state(10), clock).unwrap();
    let retained = capture.state.clone();
    let bar_handler = Rc::downgrade(&capture.bar_handler.0);
    let snapshot_handler = Rc::downgrade(&capture.snapshot_handler.0);
    let bus_b = Rc::new(RefCell::new(MessageBus::default()));
    msgbus::set_message_bus(bus_b.clone());
    assert!(capture.ensure_attached_bus().is_err());
    drop(capture);
    assert!(bar_handler.upgrade().is_none());
    assert!(snapshot_handler.upgrade().is_none());
    assert!(Rc::ptr_eq(&msgbus::get_message_bus(), &bus_b));
    msgbus::set_message_bus(bus_a);
    let (_, rows) = market();
    msgbus::publish_bar(
        format!("data.bars.{}", rows[0].native.bar_type).into(),
        &rows[0].native,
    );
    assert!(retained.borrow().records.is_empty());
}

#[test]
fn attaching_to_a_bus_owned_by_another_engine_is_rejected() {
    let state = state(10);
    let original = msgbus::get_message_bus();
    msgbus::set_message_bus(Rc::new(RefCell::new(MessageBus::default())));
    let result = Capture::attach(state, Rc::new(RefCell::new(TestClock::new())));
    assert!(result.is_err());
    msgbus::set_message_bus(original);
}

#[test]
fn cleanup_cannot_panic_or_record_more_when_the_current_other_bus_is_borrowed() {
    let bus_a = msgbus::get_message_bus();
    let capture = Capture::attach(state(10), Rc::new(RefCell::new(TestClock::new()))).unwrap();
    let retained = capture.state.clone();
    let bus_b = Rc::new(RefCell::new(MessageBus::default()));
    msgbus::set_message_bus(bus_b.clone());
    let borrowed = bus_b.borrow_mut();
    drop(capture);
    drop(borrowed);
    assert!(Rc::ptr_eq(&msgbus::get_message_bus(), &bus_b));
    assert!(
        retained
            .borrow()
            .failure
            .as_ref()
            .unwrap()
            .contains("CLEANUP_BUS_BUSY")
    );
    msgbus::set_message_bus(bus_a);
    let (_, rows) = market();
    msgbus::publish_bar(
        format!("data.bars.{}", rows[0].native.bar_type).into(),
        &rows[0].native,
    );
    assert!(retained.borrow().records.is_empty());
}

#[test]
fn actual_engine_instrument_definition_must_equal_the_frozen_native_definition() {
    let (market, _) = market();
    for change_kind in [false, true] {
        let mut actual = market.series[0].instrument.clone();
        if change_kind {
            actual = InstrumentAny::Equity(
                nautilus_model::instruments::Equity::builder()
                    .instrument_id(actual.id())
                    .raw_symbol(Symbol::new("USDT/USD"))
                    .currency(Currency::from_str("USD").unwrap())
                    .price_precision(2)
                    .price_increment(Price::from("0.01"))
                    .lot_size(Quantity::from("1"))
                    .maker_fee(Decimal::ZERO)
                    .taker_fee(Decimal::ZERO)
                    .margin_init(Decimal::ONE)
                    .margin_maint(Decimal::ONE)
                    .ts_event(0_u64.into())
                    .ts_init(0_u64.into())
                    .build()
                    .unwrap(),
            );
        } else if let InstrumentAny::CurrencyPair(pair) = &mut actual {
            pair.base_currency = Currency::from_str("EUR").unwrap();
        }
        let mut engine = BacktestEngine::new(BacktestEngineConfig {
            logging: LoggerConfig::builder()
                .bypass_logging(true)
                .build()
                .unwrap(),
            shutdown_on_error: true,
            bypass_logging: true,
            ..Default::default()
        })
        .unwrap();
        engine
            .add_venue(
                SimulatedVenueConfig::builder()
                    .venue(actual.venue())
                    .oms_type(OmsType::Netting)
                    .account_type(AccountType::Cash)
                    .book_type(BookType::L1_MBP)
                    .starting_balances(vec![Money::from("1000 USD")])
                    .build()
                    .unwrap(),
            )
            .unwrap();
        engine.add_instrument(&actual).unwrap();
        let checked = validate_engine_instruments(&engine, &market);
        engine.dispose();
        assert!(checked.is_err());
    }
}
