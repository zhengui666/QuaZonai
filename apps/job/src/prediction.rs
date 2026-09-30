//! Thin prediction-market adaptation. Nautilus owns every fill, position and cash movement.
use crate::catalog::NativeMarketData;
use anyhow::{ensure, Result};
use bigdecimal::BigDecimal;
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
    str::FromStr,
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
        let quantity_decimal = quantity.as_decimal();
        let price_decimal = price.as_decimal();
        ensure!(
            quantity_decimal > Decimal::ZERO && price_decimal > Decimal::ZERO,
            "POLYMARKET_TRADING_FILL_RANGE"
        );
        let commission = PolymarketFeeModel.get_commission(order, quantity, price, instrument)?;
        ensure!(
            commission.currency == instrument.quote_currency(),
            "POLYMARKET_COMMISSION_CURRENCY_MISMATCH"
        );
        // Native rounding applies to each partial fill, not the submitted order.
        // Preserve the frozen cost bound without imposing an amount floor on fills.
        let rate = domain::prediction::planning_fee(&native_payload(instrument)?)?;
        let gross = BigDecimal::from_str(&quantity_decimal.to_string())?
            * BigDecimal::from_str(&price_decimal.to_string())?;
        let actual_fee = BigDecimal::from_str(&commission.as_decimal().to_string())?;
        ensure!(
            actual_fee <= gross * rate.as_decimal(),
            "POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND"
        );
        Ok(commission)
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

/// Verify the frozen source inventory before giving any close to the native engine.
/// The complete condition is checked even when only one sibling is traded. A close
/// added to a BAR directory cannot acquire authority merely by being on disk.
pub(crate) fn catalog_closes(
    root: &Path,
    market: &NativeMarketData,
    selection: &NativeBarSelectionV1,
    groups: &[contracts::settlement::NativeSettlementGroupV1],
) -> Result<Vec<InstrumentClose>> {
    let binaries = market
        .series
        .iter()
        .filter_map(|s| match &s.instrument {
            InstrumentAny::BinaryOption(b) => Some((b.id, b)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let selected = binaries.keys().map(ToString::to_string).collect::<Vec<_>>();
    domain::prediction::settlement_scope(groups, &selected, selection)?;
    if binaries.is_empty() {
        return Ok(Vec::new());
    }
    let mut expected = BTreeMap::new();
    for group in groups {
        for outcome in &group.outcomes {
            let id = outcome
                .instrument_id
                .parse::<nautilus_model::identifiers::InstrumentId>()?;
            ensure!(
                expected.insert(id, outcome).is_none(),
                "DUPLICATE_POLYMARKET_SETTLEMENT"
            );
        }
    }
    let ids = binaries
        .keys()
        .chain(expected.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut catalog = ParquetDataCatalog::from_uri(
        root.to_str()
            .ok_or_else(|| anyhow::anyhow!("CATALOG_PATH_ENCODING"))?,
        None,
        Some(4096),
        None,
        None,
    )?;
    // All already-available closes for these identities must be registered. Only
    // their later selection for replay uses the requested half-open holding window.
    let query = catalog.query::<InstrumentClose>(
        Some(ids.iter().map(ToString::to_string).collect()),
        None,
        Some(UnixNanos::from(selection.decision_cutoff_ns.get())),
        None,
        None,
        true,
    )?;
    let mut seen = BTreeSet::new();
    let mut closes = Vec::new();
    for record in query.take(ids.len() + 1) {
        let Data::InstrumentClose(close) = record? else {
            anyhow::bail!("CATALOG_NATIVE_TYPE_MISMATCH");
        };
        let original = expected
            .get(&close.instrument_id)
            .ok_or_else(|| anyhow::anyhow!("POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"))?;
        let price: contracts::DecimalValue = close
            .close_price
            .to_string()
            .parse()
            .map_err(anyhow::Error::msg)?;
        ensure!(
            close.close_type == InstrumentCloseType::ContractExpired
                && close.ts_init >= close.ts_event
                && close.ts_event.as_u64() == original.ts_event.get()
                && close.ts_init.as_u64() == original.ts_init.get()
                && price == original.close_price,
            "POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"
        );
        ensure!(
            seen.insert(close.instrument_id),
            "DUPLICATE_POLYMARKET_SETTLEMENT"
        );
        if let Some(binary) = binaries.get(&close.instrument_id) {
            ensure!(
                close.ts_event >= binary.activation_ns
                    && close.close_price.precision == binary.price_precision,
                "POLYMARKET_CLOSE_CONTRACT_INVALID"
            );
            if close.ts_init.as_u64() >= selection.event_start_ns.get()
                && close.ts_init.as_u64() < selection.event_end_ns.get()
            {
                closes.push(close);
            }
        }
    }
    ensure!(
        seen.len() == expected.len(),
        "POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"
    );
    // Catalogs do not preserve cross-type arrival order. A BAR for any selected
    // portfolio member can submit orders, so its receipt tie with a close cannot
    // be resolved by choosing an insertion order in the native replay vector.
    let close_receipts = closes
        .iter()
        .map(|close| close.ts_init)
        .collect::<BTreeSet<_>>();
    ensure!(
        market
            .series
            .iter()
            .flat_map(|series| &series.bars)
            .all(|bar| !close_receipts.contains(&bar.ts_init)),
        "POLYMARKET_CLOSE_BAR_RECEIPT_AMBIGUOUS"
    );
    // Sort by receipt time without inventing a cross-record arrival sequence.
    closes.sort_by_key(|c| c.ts_init);
    Ok(closes)
}

pub(crate) fn close_events(
    root: &Path,
    market: &NativeMarketData,
    selection: &NativeBarSelectionV1,
    groups: &[contracts::settlement::NativeSettlementGroupV1],
) -> Result<Vec<InstrumentClose>> {
    let closes = catalog_closes(root, market, selection, groups)?;
    for series in &market.series {
        if let InstrumentAny::BinaryOption(binary) = &series.instrument {
            if binary.expiration_ns.as_u64() < selection.event_end_ns.get() {
                ensure!(
                    closes.iter().any(|c| c.instrument_id == binary.id),
                    "POLYMARKET_PENDING_RESOLUTION"
                );
            }
        }
    }
    Ok(closes)
}

/// Native instruments remain the authority for the target's usable trading window.
pub(crate) fn target_window(
    instruments: &[InstrumentAny],
    asof_ns: u64,
    until_ns: u64,
) -> Result<()> {
    let definitions = instruments
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    let ids = instruments
        .iter()
        .map(|i| i.id().to_string())
        .collect::<Vec<_>>();
    domain::prediction::target_window(&definitions, &ids, asof_ns, until_ns)?;
    Ok(())
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
