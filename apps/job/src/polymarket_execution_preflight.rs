//! Pure external-executor order-format preflight using the official Polymarket adapter.
//!
//! No target-to-order sizing, target/claim admission, account lookup, credential,
//! clock, signer, transport, execution client or submission is performed here.
//! A future independent execution owner can retain the returned native OrderAny;
//! it must establish live source authority, account state and venue readiness
//! separately before any authorized use of an official ExecutionClient.
use std::str::FromStr;

use contracts::{execution_preflight::*, DecimalValue, SchemaV1};
use nautilus_core::{UnixNanos, UUID4};
use nautilus_model::{
    enums::{OrderSide, TimeInForce},
    identifiers::{ClientOrderId, InstrumentId, StrategyId, TraderId},
    orders::{LimitOrder, MarketOrder, OrderAny},
    types::{Price, Quantity},
};
use nautilus_polymarket::execution::order_builder::PolymarketOrderBuilder;
use rust_decimal::Decimal;

/// A format result, not an executable target receipt or a live readiness token.
#[derive(Debug)]
pub struct PolymarketPreflight {
    pub report: PolymarketExecutionPreflightReportV1,
    native_order: Option<OrderAny>,
}

impl PolymarketPreflight {
    pub fn native_order(&self) -> Option<&OrderAny> {
        self.native_order.as_ref()
    }

    /// Transfers the unchanged native order; this does not grant execution authority.
    pub fn into_native_order(self) -> Option<OrderAny> {
        self.native_order
    }
}

#[derive(Debug)]
struct Failure {
    stage: ExecutionPreflightFailureStageV1,
    reason: String,
}

fn failure(stage: ExecutionPreflightFailureStageV1, reason: impl ToString) -> Failure {
    Failure {
        stage,
        reason: reason.to_string(),
    }
}

fn finish(
    result: Result<(PolymarketExecutionPreflightV1, OrderAny), Failure>,
) -> PolymarketPreflight {
    let (outcome, native_order) = match result {
        Ok((request, order)) => (
            PolymarketExecutionPreflightOutcomeV1::FormatValidated { request },
            Some(order),
        ),
        Err(error) => (
            PolymarketExecutionPreflightOutcomeV1::Rejected {
                stage: error.stage,
                reason: error.reason,
            },
            None,
        ),
    };
    PolymarketPreflight {
        report: PolymarketExecutionPreflightReportV1 {
            schema_version: SchemaV1,
            scope: ExecutionPreflightScopeV1::OrderFormatOnly,
            native_version: "0.63.0".into(),
            outcome,
        },
        native_order,
    }
}

/// Parses the strict request directly, without a Value/map pass that could hide
/// duplicate fields. Unknown fields, units, variants and absent choices fail.
pub fn parse_and_preflight(bytes: &[u8]) -> PolymarketPreflight {
    match serde_json::from_slice::<PolymarketExecutionPreflightV1>(bytes) {
        Ok(request) => preflight(&request),
        Err(error) => finish(Err(failure(
            ExecutionPreflightFailureStageV1::RequestParsing,
            error,
        ))),
    }
}

/// Checks only caller-declared order format. No risk parameters are inferred.
pub fn preflight(request: &PolymarketExecutionPreflightV1) -> PolymarketPreflight {
    finish(construct_and_validate(request).map(|order| (request.clone(), order)))
}

fn native_decimal(value: &DecimalValue, field: &str) -> Result<Decimal, Failure> {
    Decimal::from_str_exact(&value.as_decimal().to_plain_string()).map_err(|error| {
        failure(
            ExecutionPreflightFailureStageV1::NativeRepresentation,
            format!("{field}: {error}"),
        )
    })
}

fn exact_quantity(value: &DecimalValue) -> Result<Quantity, Failure> {
    let expected = native_decimal(value, "quantity")?;
    let quantity = Quantity::from_decimal(expected).map_err(|error| {
        failure(
            ExecutionPreflightFailureStageV1::NativeRepresentation,
            error,
        )
    })?;
    if quantity.as_decimal() != expected {
        return Err(failure(
            ExecutionPreflightFailureStageV1::NativeRepresentation,
            "quantity is not exactly representable by the compiled native type",
        ));
    }
    Ok(quantity)
}

fn exact_price(value: &DecimalValue) -> Result<Price, Failure> {
    let expected = native_decimal(value, "price")?;
    let price = Price::from_decimal(expected).map_err(|error| {
        failure(
            ExecutionPreflightFailureStageV1::NativeRepresentation,
            error,
        )
    })?;
    if price.as_decimal() != expected {
        return Err(failure(
            ExecutionPreflightFailureStageV1::NativeRepresentation,
            "price is not exactly representable by the compiled native type",
        ));
    }
    Ok(price)
}

fn time_in_force(value: &PolymarketPreflightTimeInForceV1) -> (TimeInForce, Option<UnixNanos>) {
    match value {
        PolymarketPreflightTimeInForceV1::Gtc {} => (TimeInForce::Gtc, None),
        PolymarketPreflightTimeInForceV1::Gtd { expire_time_ns } => (
            TimeInForce::Gtd,
            Some(UnixNanos::from(expire_time_ns.get())),
        ),
        PolymarketPreflightTimeInForceV1::Ioc {} => (TimeInForce::Ioc, None),
        PolymarketPreflightTimeInForceV1::Fok {} => (TimeInForce::Fok, None),
    }
}

fn construct_and_validate(request: &PolymarketExecutionPreflightV1) -> Result<OrderAny, Failure> {
    let identity = |error| failure(ExecutionPreflightFailureStageV1::NativeIdentity, error);
    let trader_id = TraderId::new_checked(&request.trader_id).map_err(identity)?;
    let strategy_id = StrategyId::new_checked(&request.strategy_id).map_err(identity)?;
    let client_order_id = ClientOrderId::new_checked(&request.client_order_id).map_err(identity)?;
    let instrument_id = InstrumentId::from_str(&request.instrument_id)
        .map_err(|error| failure(ExecutionPreflightFailureStageV1::NativeIdentity, error))?;
    if instrument_id.venue.as_str() != "POLYMARKET" {
        return Err(failure(
            ExecutionPreflightFailureStageV1::NativeIdentity,
            "instrument venue must be explicitly POLYMARKET",
        ));
    }
    let initialization_id = UUID4::from_str(&request.initialization_id)
        .map_err(|error| failure(ExecutionPreflightFailureStageV1::NativeIdentity, error))?;
    let initialized_at = UnixNanos::from(request.initialized_at_ns.get());
    let side = match request.side {
        PolymarketPreflightSideV1::Buy => OrderSide::Buy,
        PolymarketPreflightSideV1::Sell => OrderSide::Sell,
    };
    let (amount, quote_quantity) = match &request.quantity {
        PolymarketPreflightQuantityV1::Pusd { amount } => (amount, true),
        PolymarketPreflightQuantityV1::OutcomeShares { amount } => (amount, false),
    };
    let quantity = exact_quantity(amount)?;
    let construction = |error| failure(ExecutionPreflightFailureStageV1::NativeConstruction, error);
    let order = match &request.order {
        PolymarketPreflightOrderV1::Market {
            time_in_force: tif,
            reduce_only,
        } => {
            let (tif, _) = time_in_force(tif);
            OrderAny::Market(
                MarketOrder::new_checked(
                    trader_id,
                    strategy_id,
                    instrument_id,
                    client_order_id,
                    side,
                    quantity,
                    tif,
                    initialization_id,
                    initialized_at,
                    *reduce_only,
                    quote_quantity,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .map_err(construction)?,
            )
        }
        PolymarketPreflightOrderV1::Limit {
            time_in_force: tif,
            price,
            post_only,
            reduce_only,
        } => {
            let (tif, expiry) = time_in_force(tif);
            let price = exact_price(price)?;
            OrderAny::Limit(
                LimitOrder::new_checked(
                    trader_id,
                    strategy_id,
                    instrument_id,
                    client_order_id,
                    side,
                    quantity,
                    price,
                    tif,
                    expiry,
                    *post_only,
                    *reduce_only,
                    quote_quantity,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    initialization_id,
                    initialized_at,
                )
                .map_err(construction)?,
            )
        }
    };
    let validation = match &request.order {
        PolymarketPreflightOrderV1::Market { .. } => {
            PolymarketOrderBuilder::validate_market_order(&order)
        }
        PolymarketPreflightOrderV1::Limit { .. } => {
            PolymarketOrderBuilder::validate_limit_order(&order)
        }
    };
    validation
        .map_err(|error| failure(ExecutionPreflightFailureStageV1::OfficialAdapter, error))?;
    Ok(order)
}
