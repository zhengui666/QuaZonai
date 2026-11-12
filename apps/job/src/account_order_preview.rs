//! Read-only native order construction. No cache insertion, IDs consumed from the
//! account strategy, or submission. Adapter validation is not venue acceptance.
use anyhow::{ensure, Result};
use nautilus_common::{clock::VirtualClock, factories::OrderFactory};
use nautilus_model::{
    enums::{OrderSide, OrderType, TimeInForce},
    identifiers::{StrategyId, TraderId},
    instruments::{Instrument, InstrumentAny},
    orders::Order,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc, str::FromStr};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBracketProtection {
    pub enabled: bool,
    pub stop_loss: String,
    pub take_profit: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum OrderPreview {
    StopLoss {
        quantity: String,
        side: OrderSide,
        trigger_price: String,
    },
    TakeProfit {
        quantity: String,
        side: OrderSide,
        trigger_price: String,
    },
    Bracket {
        quantity: String,
        side: OrderSide,
        entry_price: String,
        stop_loss: String,
        take_profit: String,
    },
}

pub fn preview(instrument: &InstrumentAny, request: OrderPreview, mode: &str) -> Result<Value> {
    let (quantity, side) = match &request {
        OrderPreview::StopLoss { quantity, side, .. }
        | OrderPreview::TakeProfit { quantity, side, .. }
        | OrderPreview::Bracket { quantity, side, .. } => (quantity, *side),
    };
    ensure!(
        matches!(side, OrderSide::Buy | OrderSide::Sell),
        "account_order_preview_side"
    );
    let raw_quantity = Decimal::from_str(quantity)?;
    ensure!(
        raw_quantity > Decimal::ZERO,
        "account_order_preview_quantity"
    );
    let quantity = instrument.try_make_qty_from_decimal(raw_quantity, Some(true))?;
    ensure!(
        quantity.as_decimal() == raw_quantity
            && quantity.as_decimal() % instrument.size_increment().as_decimal() == Decimal::ZERO,
        "account_order_preview_quantity_increment"
    );
    ensure!(
        instrument.min_quantity().is_none_or(|min| quantity >= min),
        "account_order_preview_minimum_quantity"
    );
    let price = |text: &str| -> Result<_> {
        let raw = Decimal::from_str(text)?;
        ensure!(raw > Decimal::ZERO, "account_order_preview_price");
        let value = instrument.try_make_price_from_decimal(raw)?;
        ensure!(
            value.as_decimal() == raw
                && raw % instrument.price_increment().as_decimal() == Decimal::ZERO,
            "account_order_preview_price_increment"
        );
        Ok(value)
    };
    // Detached factory: a preview must not advance persistent live order IDs.
    let mut factory = OrderFactory::new(
        TraderId::from("PREVIEW-001"),
        StrategyId::from("PREVIEW-001"),
        None,
        None,
        Rc::new(RefCell::new(VirtualClock::default())),
        false,
        true,
    );
    let id = instrument.id();
    let orders = match request {
        OrderPreview::StopLoss { trigger_price, .. } => vec![factory.stop_market(
            id,
            side,
            quantity,
            price(&trigger_price)?,
            None,
            Some(TimeInForce::Gtc),
            None,
            Some(true),
            Some(false),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )],
        OrderPreview::TakeProfit { trigger_price, .. } => vec![factory.market_if_touched(
            id,
            side,
            quantity,
            price(&trigger_price)?,
            None,
            Some(TimeInForce::Gtc),
            None,
            Some(true),
            Some(false),
            None,
            None,
            None,
            None,
            None,
            None,
        )],
        OrderPreview::Bracket {
            entry_price,
            stop_loss,
            take_profit,
            ..
        } => {
            let entry = price(&entry_price)?;
            let stop = price(&stop_loss)?;
            let profit = price(&take_profit)?;
            ensure!(
                if side == OrderSide::Buy {
                    stop < entry && entry < profit
                } else {
                    profit < entry && entry < stop
                },
                "account_order_preview_bracket_price_order"
            );
            factory
                .bracket()
                .instrument_id(id)
                .order_side(side)
                .quantity(quantity)
                .entry_order_type(OrderType::Limit)
                .entry_price(entry)
                .time_in_force(TimeInForce::Ioc)
                .tp_order_type(OrderType::MarketIfTouched)
                .tp_trigger_price(profit)
                .tp_post_only(false)
                .sl_trigger_price(stop)
                .call()
        }
    };
    for order in &orders {
        nautilus_hyperliquid::execution::validate_order_for_hyperliquid(order)?;
    }
    let rows: Vec<_> = orders.iter().map(|order| json!({
        "preview_order_id": order.client_order_id().to_string(),
        "instrument_id": order.instrument_id().to_string(), "side": order.order_side().to_string(),
        "order_type": order.order_type().to_string(), "quantity": order.quantity().to_string(),
        "price": order.price().map(|p| p.to_string()), "trigger_price": order.trigger_price().map(|p| p.to_string()),
        "time_in_force": order.time_in_force().to_string(), "reduce_only": order.is_reduce_only(),
        "contingency_type": order.contingency_type().map(|c| c.to_string()),
        "parent_preview_order_id": order.parent_order_id().map(|id| id.to_string()),
        "linked_preview_order_ids": order.linked_order_ids().map(|ids| ids.iter().map(ToString::to_string).collect::<Vec<_>>())
    })).collect();
    Ok(
        json!({"preview_only": true, "submitted": false, "automatic_protection_enabled": false,
        "validation": "NATIVE_CONSTRUCTION_AND_HYPERLIQUID_ADAPTER_ONLY", "mode": mode,
        "execution_semantics": if mode == "paper" { "SANDBOX_SIMULATION_NOT_VENUE_PROOF" } else { "HYPERLIQUID_ADAPTER_STAGES_BRACKET_CHILDREN_AFTER_ENTRY_FILL_NOT_ATOMIC_OCO" },
        "limitations": ["NO_BALANCE_OR_MARKET_ACCEPTANCE_CHECK", "NO_SLIPPAGE_OR_STOP_FILL_GUARANTEE", "TARGET_REBALANCE_AND_CANCEL_POLICY_NOT_CONFIGURED"],
        "orders": rows}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        identifiers::{InstrumentId, Symbol},
        instruments::CurrencyPair,
        types::{Currency, Price, Quantity},
    };

    fn instrument() -> InstrumentAny {
        InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID"))
                .raw_symbol(Symbol::from("BTC-USDC-SPOT"))
                .base_currency(Currency::BTC())
                .quote_currency(Currency::USDC())
                .price_precision(2)
                .size_precision(2)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.01"))
                .ts_event(1.into())
                .ts_init(1.into())
                .build()
                .unwrap(),
        )
    }
    fn bracket() -> Value {
        json!({"kind":"BRACKET", "side":"BUY", "quantity":"1.00", "entry_price":"100.00", "stop_loss":"90.00", "take_profit":"110.00"})
    }
    #[test]
    fn bracket_uses_native_oto_ouo_links_and_reduce_only_exit_legs() {
        let result = preview(
            &instrument(),
            serde_json::from_value(bracket()).unwrap(),
            "live",
        )
        .unwrap();
        assert_eq!(result["submitted"], false);
        assert_eq!(result["automatic_protection_enabled"], false);
        let rows = result["orders"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        let entry = rows.iter().find(|r| r["order_type"] == "LIMIT").unwrap();
        assert_eq!(entry["contingency_type"], "OTO");
        assert_eq!(entry["time_in_force"], "IOC");
        assert_eq!(entry["reduce_only"], false);
        for child in rows.iter().filter(|r| r["order_type"] != "LIMIT") {
            assert_eq!(child["contingency_type"], "OUO");
            assert_eq!(child["parent_preview_order_id"], entry["preview_order_id"]);
            assert_eq!(child["reduce_only"], true);
            assert_eq!(child["side"], "SELL");
        }
    }
    #[test]
    fn standalone_conditions_use_native_stop_and_touch_types() {
        for (kind, order_type) in [
            ("STOP_LOSS", "STOP_MARKET"),
            ("TAKE_PROFIT", "MARKET_IF_TOUCHED"),
        ] {
            let request = serde_json::from_value(
                json!({"kind":kind,"side":"SELL","quantity":"1.00","trigger_price":"100.00"}),
            )
            .unwrap();
            let result = preview(&instrument(), request, "paper").unwrap();
            assert_eq!(result["orders"][0]["order_type"], order_type);
            assert_eq!(result["orders"][0]["reduce_only"], true);
            assert_eq!(
                result["execution_semantics"],
                "SANDBOX_SIMULATION_NOT_VENUE_PROOF"
            );
        }
    }
    #[test]
    fn preview_rejects_rounded_quantities_prices_and_wrong_bracket_direction() {
        for (field, value) in [
            ("quantity", "1.001"),
            ("entry_price", "100.001"),
            ("stop_loss", "101.00"),
            ("take_profit", "99.00"),
        ] {
            let mut request = bracket();
            request[field] = json!(value);
            assert!(preview(
                &instrument(),
                serde_json::from_value(request).unwrap(),
                "paper"
            )
            .is_err());
        }
        let mut request = bracket();
        request["kind"] = json!("TRAILING_STOP");
        assert!(serde_json::from_value::<OrderPreview>(request).is_err());
    }
}
