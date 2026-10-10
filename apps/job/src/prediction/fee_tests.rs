use super::*;
#[path = "../../tests/support/polymarket.rs"]
mod fixture;
use nautilus_model::{
    enums::{LiquiditySide, OrderSide, OrderType},
    orders::builder::OrderTestBuilder,
};
use std::str::FromStr;

fn order(
    instrument: &InstrumentAny,
    side: OrderSide,
    liquidity: LiquiditySide,
    reduce: bool,
    id: &str,
) -> OrderAny {
    let mut order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument.id())
        .side(side)
        .quantity(Quantity::from("10.000000"))
        .reduce_only(reduce)
        .client_order_id(id.into())
        .build();
    if let OrderAny::Market(ref mut market) = order {
        market.set_liquidity_side(liquidity);
    } else {
        panic!("market order required");
    }
    order
}

fn commission(
    instrument: &InstrumentAny,
    quantity: &str,
    price: &str,
    side: OrderSide,
    liquidity: LiquiditySide,
) -> Result<Money> {
    SettlementAwarePolymarketFee.get_commission(
        &order(instrument, side, liquidity, false, "ordinary-fill"),
        Quantity::from(quantity),
        Price::from(price),
        instrument,
    )
}

#[test]
fn buy_and_sell_sub_collateral_fills_preserve_original_official_money() {
    let instrument = &fixture::instruments("0.05", 18 * fixture::STEP)[0];
    for side in [OrderSide::Buy, OrderSide::Sell] {
        let order = order(
            instrument,
            side,
            LiquiditySide::Taker,
            false,
            "ordinary-fill",
        );
        let qty = Quantity::from("2.000000");
        let px = Price::from("0.4000");
        let official = PolymarketFeeModel
            .get_commission(&order, qty, px, instrument)
            .unwrap();
        let actual = SettlementAwarePolymarketFee
            .get_commission(&order, qty, px, instrument)
            .unwrap();
        assert_eq!(actual, official);
        assert_eq!(actual.as_decimal(), Decimal::from_str("0.024").unwrap());
        assert_eq!(actual.currency, instrument.quote_currency());
    }
}

#[test]
fn five_decimal_rounding_is_checked_per_fill_against_exact_frozen_bound() {
    let instrument = &fixture::instruments("0.05", 18 * fixture::STEP)[0];
    for (quantity, accepted) in [
        ("0.000400", true),
        ("0.000480", false),
        ("0.000499", false),
        ("0.000500", true),
    ] {
        let order = order(
            instrument,
            OrderSide::Buy,
            LiquiditySide::Taker,
            false,
            "rounding-fill",
        );
        let qty = Quantity::from(quantity);
        let px = Price::from("0.4000");
        let oracle = PolymarketFeeModel
            .get_commission(&order, qty, px, instrument)
            .unwrap();
        let actual = SettlementAwarePolymarketFee.get_commission(&order, qty, px, instrument);
        if accepted {
            assert_eq!(actual.unwrap(), oracle, "quantity={quantity}");
        } else {
            assert_eq!(oracle.as_decimal(), Decimal::from_str("0.00001").unwrap());
            assert!(
                actual
                    .unwrap_err()
                    .to_string()
                    .contains("POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND"),
                "quantity={quantity}"
            );
        }
    }
    let bound = bigdecimal::BigDecimal::from_str("0.000480").unwrap()
        * bigdecimal::BigDecimal::from_str("0.4").unwrap()
        * domain::prediction::planning_fee(&native_payload(instrument).unwrap())
            .unwrap()
            .as_decimal();
    assert_eq!(
        bound,
        bigdecimal::BigDecimal::from_str("0.00000960096").unwrap()
    );
}

#[test]
fn two_small_native_fills_are_not_combined_before_rounding_or_oracle_comparison() {
    let instrument = &fixture::instruments("0.05", 18 * fixture::STEP)[0];
    let order = order(
        instrument,
        OrderSide::Buy,
        LiquiditySide::Taker,
        false,
        "two-fills",
    );
    let px = Price::from("0.4000");
    let mut total = Decimal::ZERO;
    for _ in 0..2 {
        let qty = Quantity::from("0.000250");
        let oracle = PolymarketFeeModel
            .get_commission(&order, qty, px, instrument)
            .unwrap();
        let actual = SettlementAwarePolymarketFee
            .get_commission(&order, qty, px, instrument)
            .unwrap();
        assert_eq!(actual, oracle);
        total += actual.as_decimal();
    }
    assert_eq!(total, Decimal::ZERO);
    assert!(
        PolymarketFeeModel
            .get_commission(&order, Quantity::from("0.000500"), px, instrument)
            .unwrap()
            .as_decimal()
            > total
    );
}

#[test]
fn explicit_zero_schedule_and_negative_maker_rebate_are_preserved() {
    for (rate, liquidity) in [("0", LiquiditySide::Taker), ("0.05", LiquiditySide::Maker)] {
        let instrument = &fixture::instruments(rate, 18 * fixture::STEP)[0];
        let order = order(instrument, OrderSide::Sell, liquidity, false, "fee-oracle");
        let qty = Quantity::from("2.000000");
        let px = Price::from("0.4000");
        let actual = SettlementAwarePolymarketFee
            .get_commission(&order, qty, px, instrument)
            .unwrap();
        assert_eq!(
            actual,
            PolymarketFeeModel
                .get_commission(&order, qty, px, instrument)
                .unwrap()
        );
        if liquidity == LiquiditySide::Maker {
            assert!(actual.as_decimal() < Decimal::ZERO);
        } else {
            assert_eq!(actual.as_decimal(), Decimal::ZERO);
        }
    }
}

#[test]
fn zero_quantity_or_price_and_unknown_liquidity_fail_closed() {
    let instrument = &fixture::instruments("0.05", 18 * fixture::STEP)[0];
    for (qty, px) in [("0.000000", "0.4000"), ("2.000000", "0.0000")] {
        assert!(commission(instrument, qty, px, OrderSide::Buy, LiquiditySide::Taker).is_err());
    }
    assert!(commission(
        instrument,
        "10.000000",
        "0.4000",
        OrderSide::Buy,
        LiquiditySide::NoLiquiditySide
    )
    .is_err());
}

#[test]
fn missing_invalid_or_changed_frozen_fee_evidence_is_never_zero_fee() {
    let original = &fixture::instruments("0.05", 18 * fixture::STEP)[0];
    for (field, value) in [
        ("fee_schedule", serde_json::Value::Null),
        ("exponent", serde_json::json!(2)),
        ("takerOnly", serde_json::json!(false)),
        ("rate", serde_json::json!(-0.01)),
        ("rate", serde_json::json!(1)),
        ("rebateRate", serde_json::json!(-0.1)),
        ("rebateRate", serde_json::json!(1.1)),
        ("rebateRate", serde_json::json!("invalid")),
    ] {
        let mut payload = serde_json::to_value(original).unwrap();
        if field == "fee_schedule" {
            payload["BinaryOption"]["info"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        } else {
            payload["BinaryOption"]["info"]["fee_schedule"][field] = value;
        }
        let instrument: InstrumentAny = serde_json::from_value(payload).unwrap();
        assert!(
            commission(
                &instrument,
                "10.000000",
                "0.4000",
                OrderSide::Buy,
                LiquiditySide::Taker
            )
            .is_err(),
            "{field}"
        );
    }
    let mut payload = serde_json::to_value(original).unwrap();
    payload["BinaryOption"]["currency"] = "USD".into();
    let instrument: InstrumentAny = serde_json::from_value(payload).unwrap();
    assert!(commission(
        &instrument,
        "10.000000",
        "0.4000",
        OrderSide::Buy,
        LiquiditySide::Taker
    )
    .is_err());
    let ordinary = InstrumentAny::CurrencyPair(nautilus_model::instruments::stubs::audusd_sim());
    assert!(commission(
        &ordinary,
        "10.000000",
        "0.4000",
        OrderSide::Buy,
        LiquiditySide::Taker
    )
    .is_err());
}

#[test]
fn only_native_reduce_only_expiration_orders_have_zero_redemption_fee() {
    let instrument = &fixture::instruments("0.05", 18 * fixture::STEP)[0];
    let settlement = order(
        instrument,
        OrderSide::Sell,
        LiquiditySide::Taker,
        true,
        "EXPIRATION-POLYMARKET-fixture",
    );
    for px in ["0.0000", "1.0000"] {
        assert_eq!(
            SettlementAwarePolymarketFee
                .get_commission(
                    &settlement,
                    Quantity::from("2.000000"),
                    Price::from(px),
                    instrument
                )
                .unwrap(),
            Money::zero(instrument.quote_currency())
        );
    }
    for (reduce, id) in [
        (true, "ordinary-reduction"),
        (false, "EXPIRATION-POLYMARKET-fixture"),
    ] {
        let ordinary = order(
            instrument,
            OrderSide::Sell,
            LiquiditySide::Taker,
            reduce,
            id,
        );
        assert!(SettlementAwarePolymarketFee
            .get_commission(
                &ordinary,
                Quantity::from("0.000480"),
                Price::from("0.4000"),
                instrument
            )
            .unwrap_err()
            .to_string()
            .contains("POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND"));
    }
}

#[test]
fn whole_order_collateral_floor_remains_one() {
    let instrument = &fixture::instruments("0.05", 18 * fixture::STEP)[0];
    assert!(order_notional(
        instrument,
        fixture::STEP,
        Decimal::from_str("0.999999").unwrap()
    )
    .is_err());
    assert!(order_notional(instrument, fixture::STEP, Decimal::ONE).is_ok());
}
