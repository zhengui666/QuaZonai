//! Thin prediction-market adaptation. Nautilus owns every fill, position and cash movement.
use crate::catalog::NativeMarketData;
use anyhow::{ensure, Result};
use contracts::science::{NativeBarSelectionV1, NativeSimulationSettingsV1};
use nautilus_core::UnixNanos;
use nautilus_execution::models::fee::{FeeModel, FeeModelHandle, MakerTakerFeeModel};
use nautilus_model::{
    data::{Data, InstrumentClose},
    enums::InstrumentCloseType,
    instruments::{Instrument, InstrumentAny},
    orders::{Order, OrderAny},
    types::{Money, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use nautilus_polymarket::models::PolymarketFeeModel;
use rust_decimal::Decimal;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// The native expiry adapter emits a reduce-only EXPIRATION order. It represents
/// redemption, not a taker trade, so dispatch that event to zero trading commission.
/// Ordinary orders always use the unchanged upstream price-dependent fee algorithm.
#[derive(Debug, Clone, Copy)]
struct SettlementAwarePolymarketFee;
impl FeeModel for SettlementAwarePolymarketFee {
    fn get_commission(
        &self,
        order: &OrderAny,
        quantity: Quantity,
        price: Price,
        instrument: &InstrumentAny,
    ) -> Result<Money> {
        ensure!(
            matches!(instrument, InstrumentAny::BinaryOption(_)),
            "BINARY_OPTION_REQUIRED"
        );
        if is_native_settlement_order(order.client_order_id().as_str()) && order.is_reduce_only() {
            return Ok(Money::zero(instrument.quote_currency()));
        }
        ensure!(
            quantity
                .as_decimal()
                .checked_mul(price.as_decimal())
                .is_some_and(|notional| notional >= Decimal::ONE),
            "POLYMARKET_RESEARCH_MINIMUM_FILL_NOTIONAL"
        );
        PolymarketFeeModel.get_commission(order, quantity, price, instrument)
    }
}

pub(crate) fn is_native_settlement_order(id: &str) -> bool {
    id.starts_with("EXPIRATION-POLYMARKET-")
}

pub(crate) fn fee_model(settings: &NativeSimulationSettingsV1) -> FeeModelHandle {
    if domain::prediction::uses_native_fee(&settings.fee_model) {
        FeeModelHandle::new(SettlementAwarePolymarketFee)
    } else {
        FeeModelHandle::new(MakerTakerFeeModel)
    }
}

pub(crate) fn native_payload(instrument: &InstrumentAny) -> Result<serde_json::Value> {
    let value = serde_json::to_value(instrument)?;
    value
        .get("BinaryOption")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("BINARY_OPTION_REQUIRED"))
}

pub(crate) fn validate_market(
    market: &NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<()> {
    let selected = domain::prediction::uses_native_fee(&settings.fee_model);
    for series in &market.series {
        match &series.instrument {
            InstrumentAny::BinaryOption(binary) => {
                ensure!(selected, "POLYMARKET_NATIVE_FEE_MODEL_REQUIRED");
                let payload = native_payload(&series.instrument)?;
                let (activation, expiration) = domain::prediction::instrument(&payload)?;
                let ceiling = domain::prediction::planning_fee(&payload)?;
                let rate = settings
                    .fee_rates
                    .iter()
                    .find(|r| r.instrument_id == binary.id.to_string())
                    .ok_or_else(|| anyhow::anyhow!("SIMULATION_FEE_MISSING"))?;
                ensure!(
                    rate.maker.as_decimal() == &bigdecimal::BigDecimal::from(0)
                        && rate.taker == ceiling,
                    "POLYMARKET_PLANNING_FEE_SOURCE_MISMATCH"
                );
                ensure!(
                    series.bars.iter().all(|b| [b.open, b.high, b.low, b.close]
                        .iter()
                        .all(|p| p.as_decimal() > Decimal::ZERO && p.as_decimal() < Decimal::ONE)),
                    "POLYMARKET_TRADING_BAR_PRICE_RANGE"
                );
                ensure!(
                    series
                        .bars
                        .iter()
                        .all(|b| b.ts_event.as_u64() >= activation
                            && b.ts_event.as_u64() <= expiration),
                    "POLYMARKET_BAR_OUTSIDE_CONTRACT_LIFETIME"
                );
            }
            _ => ensure!(!selected, "POLYMARKET_FEE_REQUIRES_BINARY_OPTION"),
        }
    }
    Ok(())
}

/// Closing records remain in the original native catalog. Forecasts never query them.
/// Query by original availability and retain only this simulation's half-open window.
/// No payout is inferred from final prices, market labels or a scheduled expiry date.
pub(crate) fn close_events(
    root: &Path,
    market: &NativeMarketData,
    selection: &NativeBarSelectionV1,
) -> Result<Vec<InstrumentClose>> {
    let binaries = market
        .series
        .iter()
        .filter_map(|s| match &s.instrument {
            InstrumentAny::BinaryOption(b) => Some((b.id, b)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    if binaries.is_empty() {
        return Ok(Vec::new());
    }
    let mut catalog = ParquetDataCatalog::from_uri(
        root.to_str()
            .ok_or_else(|| anyhow::anyhow!("CATALOG_PATH_ENCODING"))?,
        None,
        Some(4096),
        None,
        None,
    )?;
    let query = catalog.query::<InstrumentClose>(
        Some(binaries.keys().map(ToString::to_string).collect()),
        Some(UnixNanos::from(selection.event_start_ns.get())),
        Some(UnixNanos::from(selection.decision_cutoff_ns.get())),
        None,
        None,
        true,
    )?;
    let mut closes = Vec::new();
    let mut identities = BTreeSet::new();
    for record in query.take(binaries.len() + 1) {
        let Data::InstrumentClose(close) = record? else {
            anyhow::bail!("CATALOG_NATIVE_TYPE_MISMATCH");
        };
        let binary = binaries
            .get(&close.instrument_id)
            .ok_or_else(|| anyhow::anyhow!("UNMAPPED_INSTRUMENT_CLOSE"))?;
        ensure!(
            close.ts_init >= close.ts_event
                && close.ts_event >= binary.activation_ns
                && close.close_type == InstrumentCloseType::ContractExpired
                && (Decimal::ZERO..=Decimal::ONE).contains(&close.close_price.as_decimal()),
            "POLYMARKET_CLOSE_CONTRACT_INVALID"
        );
        if close.ts_init.as_u64() >= selection.event_end_ns.get() {
            continue;
        }
        ensure!(
            identities.insert(close.instrument_id),
            "DUPLICATE_POLYMARKET_SETTLEMENT"
        );
        closes.push(close);
    }
    // An unresolved expiry must not be assigned a fabricated terminal value. Historical
    // mark-to-market before expiry remains possible; cross-expiry studies require source close events.
    for (id, binary) in binaries {
        if binary.expiration_ns.as_u64() < selection.event_end_ns.get() {
            ensure!(identities.contains(&id), "POLYMARKET_PENDING_RESOLUTION");
        }
    }
    closes.sort_by_key(|c| (c.ts_init, c.ts_event));
    Ok(closes)
}

pub(crate) fn order_notional(
    instrument: &InstrumentAny,
    now: u64,
    notional: Decimal,
) -> Result<()> {
    if let InstrumentAny::BinaryOption(binary) = instrument {
        ensure!(
            now >= binary.activation_ns.as_u64() && now < binary.expiration_ns.as_u64(),
            "POLYMARKET_ORDER_OUTSIDE_CONTRACT_LIFETIME"
        );
        ensure!(
            notional >= Decimal::ONE,
            "POLYMARKET_RESEARCH_MINIMUM_NOTIONAL"
        );
    }
    Ok(())
}
