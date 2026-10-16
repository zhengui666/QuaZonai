//! Offline order-shape contracts against the official Nautilus Polymarket 0.63 API.
//!
//! These tests construct native orders in memory and call the two public static
//! validators. They create no execution client, signer, credential or transport,
//! and never submit an order. The synthetic symbol is not venue-verified.
//!
//! Acceptance here is only format acceptance: it proves no account connection,
//! balance, inventory, market availability, tick/expiry compatibility, signing,
//! submission, fill or reconciliation. The Paper-shaped case reproduces the
//! current OrderFactory argument shape; it does not run TargetReplay or an
//! idea-to-live path. Existing Paper sizing and execution remain unchanged.
//!
//! Pinned upstream: nautechsystems/nautilus_trader at
//! a0400251110653b6d8ae6a9b5b89c4543fa85a2d, crates/adapters/polymarket/src/
//! execution/order_builder.rs. In 0.63, LIMIT quote quantities are unsupported.

use std::{cell::RefCell, rc::Rc};

use nautilus_common::{clock::TestClock, factories::OrderFactory};
use nautilus_core::{UnixNanos, UUID4};
use nautilus_model::{
    enums::{OrderSide, OrderType, TimeInForce},
    events::OrderDeniedReason,
    identifiers::{ClientOrderId, InstrumentId, StrategyId, TraderId},
    orders::{MarketOrder, Order, OrderAny, OrderTestBuilder},
    types::{Price, Quantity},
};
use nautilus_polymarket::execution::order_builder::PolymarketOrderBuilder;

const SYNTHETIC_INSTRUMENT: &str = "FORMAT-ONLY.POLYMARKET";

fn market(side: OrderSide, tif: TimeInForce, quote_quantity: bool, reduce_only: bool) -> OrderAny {
    OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from(SYNTHETIC_INSTRUMENT))
        .side(side)
        .quantity(Quantity::from("10.00"))
        .time_in_force(tif)
        .quote_quantity(quote_quantity)
        .reduce_only(reduce_only)
        .build()
}

fn limit(
    side: OrderSide,
    tif: TimeInForce,
    quote_quantity: bool,
    reduce_only: bool,
    post_only: bool,
) -> OrderAny {
    let mut builder = OrderTestBuilder::new(OrderType::Limit);
    builder
        .instrument_id(InstrumentId::from(SYNTHETIC_INSTRUMENT))
        .side(side)
        .quantity(Quantity::from("10.00"))
        .price(Price::from("0.50"))
        .time_in_force(tif)
        .quote_quantity(quote_quantity)
        .reduce_only(reduce_only)
        .post_only(post_only);
    if tif == TimeInForce::Gtd {
        // A well-formed native expiry, not evidence of a valid live expiry window.
        builder.expire_time(UnixNanos::from(2_000_000_000_000_000_000_u64));
    }
    builder.build()
}

fn validate_unchanged(
    order: &OrderAny,
    validate: fn(&OrderAny) -> Result<(), OrderDeniedReason>,
) -> Result<(), OrderDeniedReason> {
    let before = serde_json::to_value(order).expect("serialize original native order");
    let outcome = validate(order);
    assert_eq!(
        serde_json::to_value(order).expect("serialize validated native order"),
        before,
        "format validation must not convert quantity, TIF, flags or order state"
    );
    outcome
}

fn validate_market(order: &OrderAny) -> Result<(), OrderDeniedReason> {
    validate_unchanged(order, PolymarketOrderBuilder::validate_market_order)
}

fn validate_limit(order: &OrderAny) -> Result<(), OrderDeniedReason> {
    validate_unchanged(order, PolymarketOrderBuilder::validate_limit_order)
}

fn assert_denied(result: Result<(), OrderDeniedReason>, expected: &str) {
    let reason = result.expect_err("official validator must reject this order shape");
    assert!(
        reason.to_string().contains(expected),
        "expected {expected:?}, received {reason}"
    );
}

#[test]
fn market_buy_accepts_explicit_pusd_amount_with_ioc_or_fok() {
    for tif in [TimeInForce::Ioc, TimeInForce::Fok] {
        let order = market(OrderSide::Buy, tif, true, false);
        assert!(order.is_quote_quantity());
        assert_eq!(order.quantity(), Quantity::from("10.00"));
        validate_market(&order).expect("explicit quote-denominated market BUY format");
    }
}

#[test]
fn market_buy_rejects_shares_without_converting_to_pusd() {
    for tif in [TimeInForce::Ioc, TimeInForce::Fok] {
        let order = market(OrderSide::Buy, tif, false, false);
        assert_denied(validate_market(&order), "quote_quantity=true");
    }
}

#[test]
fn market_sell_accepts_explicit_shares_with_ioc_or_fok() {
    for tif in [TimeInForce::Ioc, TimeInForce::Fok] {
        let order = market(OrderSide::Sell, tif, false, false);
        assert!(!order.is_quote_quantity());
        validate_market(&order).expect("explicit share-denominated market SELL format");
    }
}

#[test]
fn market_sell_rejects_quote_amount_without_converting_to_shares() {
    for tif in [TimeInForce::Ioc, TimeInForce::Fok] {
        let order = market(OrderSide::Sell, tif, true, false);
        assert_denied(validate_market(&order), "quote_quantity=false");
    }
}

#[test]
fn market_gtc_is_rejected_without_replacing_the_time_in_force() {
    for side in [OrderSide::Buy, OrderSide::Sell] {
        let order = market(side, TimeInForce::Gtc, side == OrderSide::Buy, false);
        assert!(matches!(
            validate_market(&order),
            Err(OrderDeniedReason::UnsupportedTimeInForce(TimeInForce::Gtc))
        ));
    }
}

#[test]
fn market_gtd_is_rejected_by_native_construction_before_adapter_validation() {
    // The native model disallows this shape. Do not bypass it to fabricate an
    // adapter result, or use a panicking constructor as evidence of acceptance.
    let result = MarketOrder::new_checked(
        TraderId::from("FORMAT-001"),
        StrategyId::from("FORMAT-001"),
        InstrumentId::from(SYNTHETIC_INSTRUMENT),
        ClientOrderId::from("FORMAT-001"),
        OrderSide::Buy,
        Quantity::from("10.00"),
        TimeInForce::Gtd,
        UUID4::new(),
        UnixNanos::default(),
        false,
        true,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let error = result.expect_err("native MARKET GTD construction must fail");
    assert!(error
        .to_string()
        .contains("GTD not supported for Market orders"));
}

#[test]
fn market_reduce_only_is_rejected_without_removing_the_flag() {
    for side in [OrderSide::Buy, OrderSide::Sell] {
        let order = market(side, TimeInForce::Ioc, side == OrderSide::Buy, true);
        assert_denied(validate_market(&order), "Reduce-only orders not supported");
    }
}

#[test]
fn limit_accepts_shares_for_supported_time_in_force_shapes() {
    for side in [OrderSide::Buy, OrderSide::Sell] {
        for tif in [
            TimeInForce::Gtc,
            TimeInForce::Gtd,
            TimeInForce::Ioc,
            TimeInForce::Fok,
        ] {
            let order = limit(side, tif, false, false, false);
            validate_limit(&order).expect("share-denominated LIMIT format");
        }
    }
}

#[test]
fn limit_quote_quantity_is_rejected_by_pinned_063_contract() {
    for side in [OrderSide::Buy, OrderSide::Sell] {
        let order = limit(side, TimeInForce::Gtc, true, false, false);
        assert_denied(
            validate_limit(&order),
            "Quote quantity not supported for limit orders",
        );
    }
}

#[test]
fn limit_reduce_only_is_rejected_without_removing_the_flag() {
    for side in [OrderSide::Buy, OrderSide::Sell] {
        let order = limit(side, TimeInForce::Gtc, false, true, false);
        assert_denied(validate_limit(&order), "Reduce-only orders not supported");
    }
}

#[test]
fn limit_post_only_accepts_gtc_and_gtd_shapes() {
    for side in [OrderSide::Buy, OrderSide::Sell] {
        for tif in [TimeInForce::Gtc, TimeInForce::Gtd] {
            let order = limit(side, tif, false, false, true);
            validate_limit(&order).expect("resting post-only LIMIT format");
        }
    }
}

#[test]
fn limit_post_only_rejects_ioc_and_fok_without_changing_instruction() {
    for side in [OrderSide::Buy, OrderSide::Sell] {
        for tif in [TimeInForce::Ioc, TimeInForce::Fok] {
            let order = limit(side, tif, false, false, true);
            assert_denied(
                validate_limit(&order),
                "Post-only orders require GTC or GTD",
            );
        }
    }
}

#[test]
fn paper_factory_argument_shape_is_not_a_live_execution_contract() {
    let mut factory = OrderFactory::new(
        TraderId::from("FORMAT-001"),
        StrategyId::from("FORMAT-001"),
        None,
        None,
        Rc::new(RefCell::new(TestClock::new())),
        false,
        false,
    );
    for (side, reduce_only, denial) in [
        (OrderSide::Buy, false, "quote_quantity=true"),
        (OrderSide::Sell, true, "Reduce-only orders not supported"),
    ] {
        // Reproduce only simulation.rs's current argument shape. This is not
        // execution of TargetReplay, its sizing policy, or an original target.
        let order = factory.market(
            InstrumentId::from(SYNTHETIC_INSTRUMENT),
            side,
            Quantity::from("10.00"),
            None,
            Some(reduce_only),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(order.time_in_force(), TimeInForce::Gtc);
        assert!(!order.is_quote_quantity());
        assert_eq!(order.is_reduce_only(), reduce_only);
        assert_denied(validate_market(&order), denial);
    }
}
