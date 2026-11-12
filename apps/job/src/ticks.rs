//! Original TradeTick catalog/cohort data and coverage; no factor evaluator.
//! Native source decimals remain untouched. Derived statistical marks are finite f64.
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use contracts::{ticks::*, DbCounter};
use nautilus_model::{data::TradeTick, enums::AggressorSide};
use std::collections::BTreeMap;

pub mod catalog;
pub mod cohort_cache;
pub mod economic;

pub const FEATURE_SCHEMA: &str = "qz.tick-cohort-features.v1: cohort_vwap,previous_cohort_vwap,fast_ema,slow_ema,cohort_quantity,trade_count,elapsed_event_ns";
pub const FLOW_FEATURE_SCHEMA: &str = "qz.tick-cohort-flow-features.v1: cohort_vwap,previous_cohort_vwap,fast_ema,slow_ema,cohort_quantity,trade_count,elapsed_event_ns,signed_quantity";
pub const RECEIVED_FEATURE_SCHEMA: &str = "qz.tick-received-cohort-features.v1: cohort_vwap,previous_cohort_vwap,fast_ema,slow_ema,cohort_quantity,trade_count,elapsed_received_ns";
pub const RECEIVED_FLOW_FEATURE_SCHEMA: &str = "qz.tick-received-cohort-flow-features.v1: cohort_vwap,previous_cohort_vwap,fast_ema,slow_ema,cohort_quantity,trade_count,elapsed_received_ns,signed_quantity";

pub const RECEIVED_FLOW_BOOK_FEATURE_SCHEMA: &str = "qz.tick-received-cohort-flow-book-features.v1: cohort_vwap,previous_cohort_vwap,fast_ema,slow_ema,cohort_quantity,trade_count,elapsed_received_ns,signed_quantity,native_book_imbalance_ratio,book_spread_bps,book_bid_depth_5_quote,book_ask_depth_5_quote";

pub fn feature_schema(request: &NativeTradeTickFactorEvaluationRequestV1) -> &'static str {
    if request.parameters.uses_book_features() {
        return RECEIVED_FLOW_BOOK_FEATURE_SCHEMA;
    }
    let input = &request.input;
    match (input.uses_received_time(), input.is_flow_cache()) {
        (true, true) => RECEIVED_FLOW_FEATURE_SCHEMA,
        (true, false) => RECEIVED_FEATURE_SCHEMA,
        (false, true) => FLOW_FEATURE_SCHEMA,
        (false, false) => FEATURE_SCHEMA,
    }
}

#[derive(Clone, Debug)]
struct Cohort {
    /// Sampling/decision clock; original exchange timestamps remain separate.
    event: u64,
    source_event_start: u64,
    source_event_end: u64,
    vwap: f64,
    quantity: f64,
    count: usize,
    signed_quantity: Option<f64>,
}

/// Bound positive source-clock age; unsynchronized venue clocks may be ahead.
pub fn source_age_within(event: u64, received: u64, maximum: u64) -> bool {
    received.checked_sub(event).is_none_or(|age| age <= maximum)
}

fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

/// Integer/decimal reductions precede finite feature conversion, making tie order irrelevant.
#[cfg(test)]
fn cohorts(trades: &[TradeTick], maximum: u32) -> Result<Vec<Cohort>> {
    cohorts_with_flow(trades, maximum, false)
}

#[cfg(test)]
fn cohorts_with_flow(trades: &[TradeTick], maximum: u32, flow: bool) -> Result<Vec<Cohort>> {
    sampled_cohorts(trades, maximum, flow, false)
}

fn sampled_cohorts(
    trades: &[TradeTick],
    maximum: u32,
    flow: bool,
    received: bool,
) -> Result<Vec<Cohort>> {
    let clock = |trade: &TradeTick| {
        if received {
            trade.ts_init
        } else {
            trade.ts_event
        }
    };
    trades
        .chunk_by(|a, b| clock(a) == clock(b))
        .map(|rows| {
            ensure!(rows.len() <= maximum as usize, "TICK_COHORT_ROW_LIMIT");
            let mut quantity = BigDecimal::zero();
            let mut cash = BigDecimal::zero();
            let mut signed = BigDecimal::zero();
            for trade in rows {
                let price: BigDecimal = trade.price.to_string().parse()?;
                let size: BigDecimal = trade.size.to_string().parse()?;
                ensure!(
                    price > BigDecimal::zero() && size > BigDecimal::zero(),
                    "TICK_PRICE_QUANTITY_INVALID"
                );
                cash += &price * &size;
                if flow {
                    match trade.aggressor_side {
                        AggressorSide::Buy => signed += &size,
                        AggressorSide::Sell => signed -= &size,
                        AggressorSide::NoAggressor => anyhow::bail!("TICK_FLOW_AGGRESSOR_UNKNOWN"),
                    }
                }
                quantity += size;
            }
            let vwap = (&cash / &quantity)
                .to_f64()
                .ok_or_else(|| anyhow::anyhow!("TICK_FEATURE_NONFINITE"))?;
            let quantity = quantity
                .to_f64()
                .ok_or_else(|| anyhow::anyhow!("TICK_FEATURE_NONFINITE"))?;
            ensure!(
                vwap.is_finite() && vwap > 0.0 && quantity.is_finite() && quantity > 0.0,
                "TICK_FEATURE_NONFINITE"
            );
            Ok(Cohort {
                event: clock(&rows[0]).as_u64(),
                source_event_start: rows.iter().map(|row| row.ts_event.as_u64()).min().unwrap(),
                source_event_end: rows
                    .iter()
                    .map(|row| row.ts_event.as_u64())
                    .max()
                    .unwrap()
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("TICK_EVENT_TIME_RANGE"))?,
                vwap,
                quantity,
                count: rows.len(),
                signed_quantity: if flow {
                    let value = signed
                        .to_f64()
                        .ok_or_else(|| anyhow::anyhow!("TICK_FEATURE_NONFINITE"))?;
                    ensure!(value.is_finite(), "TICK_FEATURE_NONFINITE");
                    Some(value)
                } else {
                    None
                },
            })
        })
        .collect()
}

pub fn coverage(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    series: &BTreeMap<String, Vec<TradeTick>>,
) -> Result<NativeTradeTickCoverageV1> {
    let clock = |trade: &TradeTick| {
        if request.input.uses_received_time() {
            trade.ts_init.as_u64()
        } else {
            trade.ts_event.as_u64()
        }
    };
    let first = series
        .values()
        .flat_map(|rows| rows.iter().map(clock))
        .min()
        .ok_or_else(|| anyhow::anyhow!("TICK_EMPTY_SELECTION"))?;
    let last = series
        .values()
        .flat_map(|rows| rows.iter().map(clock))
        .max()
        .expect("nonempty selection");
    let count = series.values().map(Vec::len).sum::<usize>();
    let cohort_count = series
        .values()
        .map(|rows| rows.chunk_by(|a, b| clock(a) == clock(b)).count())
        .sum::<usize>();
    let instrument_coverage = request
        .input
        .instrument_ids
        .iter()
        .map(|instrument_id| -> Result<_> {
            let rows = series
                .get(instrument_id)
                .ok_or_else(|| anyhow::anyhow!("TICK_INSTRUMENT_SELECTION_MISSING"))?;
            Ok(NativeTradeTickInstrumentCoverageV1 {
                instrument_id: instrument_id.clone(),
                trade_count: counter(u64::try_from(rows.len())?)?,
                cohort_count: counter(u64::try_from(
                    rows.chunk_by(|a, b| clock(a) == clock(b)).count(),
                )?)?,
                first_event_ns: rows.first().map(|t| counter(clock(t))).transpose()?,
                last_event_ns: rows.last().map(|t| counter(clock(t))).transpose()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NativeTradeTickCoverageV1 {
        modality: NativeTickModalityV1::TradeTick,
        source_revision: request.input.source_revision.clone(),
        instrument_ids: request.input.instrument_ids.clone(),
        instrument_coverage,
        trade_count: counter(u64::try_from(count)?)?,
        cohort_count: counter(u64::try_from(cohort_count)?)?,
        first_event_ns: counter(first)?,
        last_event_ns: counter(last)?,
        source_event_start_ns: request
            .input
            .uses_received_time()
            .then(|| {
                counter(
                    series
                        .values()
                        .flatten()
                        .map(|row| row.ts_event.as_u64())
                        .min()
                        .unwrap(),
                )
            })
            .transpose()?,
        source_event_end_ns: request
            .input
            .uses_received_time()
            .then(|| {
                counter(
                    series
                        .values()
                        .flatten()
                        .map(|row| row.ts_event.as_u64())
                        .max()
                        .unwrap()
                        + 1,
                )
            })
            .transpose()?,
        availability: request.input.availability,
        limitations: vec![
            request.input.availability_name().into(),
            "NOT_PIT_QUALIFIED".into(),
            "NOT_INDEPENDENT_MARKET_COMPLETENESS_EVIDENCE".into(),
            "CURRENT_DEFINITIONS_ARE_REPRESENTATION_METADATA_NOT_HISTORICAL_CONTRACT_OR_FEES"
                .into(),
            "VWAP_IS_A_DERIVED_STATISTICAL_MARK_NOT_EXECUTABLE_PRICE".into(),
        ],
        source_evidence: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests;
