use super::*;
use contracts::{research::DataOrigin, science::NativeFeeRateV1, DbCounter, SchemaV1};
use nautilus_common::clock::VirtualClock;
use nautilus_model::{
    enums::{LiquiditySide, OrderSide, OrderType},
    instruments::stubs::{audusd_sim, gbpusd_sim},
    orders::OrderTestBuilder,
};
use std::str::FromStr;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}

fn source() -> FrozenMakerTakerFeesV1 {
    FrozenMakerTakerFeesV1 {
        schema_version: SchemaV1,
        source_reference: "Independent synthetic fee oracle; not market fee evidence".into(),
        source_version: "fixture-venue-policy-1".into(),
        origin: DataOrigin::Fixture,
        venue: "SIM".into(),
        currency: "USD".into(),
        currency_precision: 2,
        available_at_ns: count(0),
        valid_from_ns: count(10),
        valid_until_ns: count(100),
        rates: vec![
            NativeFeeRateV1 {
                instrument_id: "AUD/USD.SIM".into(),
                maker: "0".parse().unwrap(),
                taker: "0".parse().unwrap(),
            },
            NativeFeeRateV1 {
                instrument_id: "GBP/USD.SIM".into(),
                maker: "-0.0002".parse().unwrap(),
                taker: "0.001".parse().unwrap(),
            },
        ],
    }
}

fn order(instrument: &InstrumentAny, side: LiquiditySide) -> OrderAny {
    let mut order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument.id())
        .side(OrderSide::Buy)
        .quantity(Quantity::from("1000"))
        .build();
    order.set_liquidity_side(side);
    order
}

fn model() -> (FrozenFeeModel, Rc<RefCell<dyn Clock>>) {
    let clock: Rc<RefCell<dyn Clock>> = Rc::new(RefCell::new(VirtualClock::new()));
    set_time(&clock, 20);
    let source = source();
    (
        FrozenFeeModel {
            model: native_model(&source).unwrap(),
            source,
            selected: [
                InstrumentId::from("AUD/USD.SIM"),
                InstrumentId::from("GBP/USD.SIM"),
            ]
            .into_iter()
            .collect(),
            clock: clock.clone(),
        },
        clock,
    )
}

fn set_time(clock: &Rc<RefCell<dyn Clock>>, value: u64) {
    clock
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<VirtualClock>()
        .unwrap()
        .set_time(value.into());
}

#[test]
fn official_override_uses_currency_rounding_including_signed_half_even_ties() {
    let (model, _) = model();
    let instrument = InstrumentAny::CurrencyPair(gbpusd_sim());
    // Independently fixed decimal oracle. Native MakerTaker uses Money::from_decimal,
    // which rounds each fill to USD's two decimals, including half-even ties.
    for (side, quantity, price, expected) in [
        (LiquiditySide::Taker, "2", "0.40000", "0.00"), // 0.0008
        (LiquiditySide::Maker, "2", "0.40000", "0.00"), // -0.00016
        (LiquiditySide::Taker, "5", "1.00000", "0.00"), // 0.005 -> even 0.00
        (LiquiditySide::Taker, "15", "1.00000", "0.02"), // 0.015 -> even 0.02
        (LiquiditySide::Taker, "25", "1.00000", "0.02"), // 0.025 -> even 0.02
        (LiquiditySide::Maker, "25", "1.00000", "0.00"), // -0.005 -> even 0.00
        (LiquiditySide::Maker, "75", "1.00000", "-0.02"), // -0.015 -> even -0.02
        (LiquiditySide::Maker, "125", "1.00000", "-0.02"), // -0.025 -> even -0.02
    ] {
        let commission = model
            .get_commission(
                &order(&instrument, side),
                Quantity::from(quantity),
                Price::from(price),
                &instrument,
            )
            .unwrap();
        assert_eq!(
            commission.as_decimal(),
            Decimal::from_str_exact(expected).unwrap()
        );
        assert_eq!(commission.currency, instrument.quote_currency());
        let encoded = serde_json::to_vec(&commission).unwrap();
        assert_eq!(
            serde_json::from_slice::<Money>(&encoded).unwrap(),
            commission
        );
        assert_eq!(
            Money::from_str(&commission.to_string()).unwrap(),
            commission
        );
    }
    let zero = InstrumentAny::CurrencyPair(audusd_sim());
    assert_eq!(
        model
            .get_commission(
                &order(&zero, LiquiditySide::Taker),
                Quantity::from("100"),
                Price::from("1.00000"),
                &zero
            )
            .unwrap()
            .as_decimal(),
        Decimal::ZERO
    );
    let order = order(&instrument, LiquiditySide::Taker);
    let half = model
        .get_commission(
            &order,
            Quantity::from("5"),
            Price::from("1.00000"),
            &instrument,
        )
        .unwrap();
    let combined = model
        .get_commission(
            &order,
            Quantity::from("10"),
            Price::from("1.00000"),
            &instrument,
        )
        .unwrap();
    assert_eq!((half + half).as_decimal(), Decimal::ZERO);
    assert_eq!(
        combined.as_decimal(),
        Decimal::from_str_exact("0.01").unwrap()
    );
}

#[test]
fn exact_money_serde_and_exact_parser_preserve_digits_while_standard_from_str_rounds() {
    let currency = nautilus_model::types::Currency::USD();
    for (value, doubled) in [("0.0008", "0.0016"), ("-0.00016", "-0.00032")] {
        let amount = Decimal::from_str_exact(value).unwrap();
        let exact = Money::from_decimal_exact(amount, currency).unwrap();
        assert_eq!(exact.as_decimal(), amount);
        assert_eq!(exact.to_string(), format!("{value} USD"));
        assert_eq!(
            serde_json::from_slice::<Money>(&serde_json::to_vec(&exact).unwrap()).unwrap(),
            exact
        );
        assert_eq!(Money::from_str_exact(&exact.to_string()).unwrap(), exact);
        // Upstream serde deliberately calls the exact parser. The ordinary
        // FromStr trait is a different API and still rounds to USD cents.
        assert_eq!(
            Money::from_str(&exact.to_string()).unwrap().as_decimal(),
            Decimal::ZERO
        );
        assert_eq!(
            (exact + exact).as_decimal(),
            Decimal::from_str_exact(doubled).unwrap()
        );
        assert_eq!(
            Money::from_decimal(amount, currency).unwrap().as_decimal(),
            Decimal::ZERO
        );
    }
}

#[test]
fn actual_fill_clock_and_source_identity_are_enforced() {
    let (model, clock) = model();
    let instrument = InstrumentAny::CurrencyPair(gbpusd_sim());
    let order = order(&instrument, LiquiditySide::Taker);
    for (time, accepted) in [(9, false), (10, true), (99, true), (100, false)] {
        set_time(&clock, time);
        assert_eq!(
            model
                .get_commission(
                    &order,
                    Quantity::from("2"),
                    Price::from("0.40000"),
                    &instrument
                )
                .is_ok(),
            accepted
        );
    }
    set_time(&clock, 20);
    let other = InstrumentAny::CurrencyPair(audusd_sim());
    assert!(model
        .get_commission(&order, Quantity::from("2"), Price::from("0.40000"), &other)
        .is_err());
}

#[test]
fn missing_malformed_or_future_source_never_becomes_zero_fees() {
    let directory = tempfile::tempdir().unwrap();
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec!["GBP/USD.SIM-1-MINUTE-LAST-EXTERNAL".into()],
        event_start_ns: count(10),
        event_end_ns: count(80),
        decision_cutoff_ns: count(90),
        maximum_rows: 10,
    };
    assert!(load(directory.path(), &selection).unwrap().is_none());
    let mut original = source();
    write(directory.path(), &original).unwrap();
    assert_eq!(
        load(directory.path(), &selection).unwrap().unwrap(),
        original
    );
    assert!(
        write(directory.path(), &original).is_err(),
        "source cannot be overwritten"
    );
    original.available_at_ns = count(11);
    std::fs::write(
        directory.path().join(MAKER_TAKER_FEES_FILE),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    assert!(load(directory.path(), &selection).is_err());
    std::fs::write(directory.path().join(MAKER_TAKER_FEES_FILE), b"{}").unwrap();
    assert!(load(directory.path(), &selection).is_err());
}
