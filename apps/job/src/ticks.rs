//! Explicit TradeTick cohorts -> causal generic features -> factor-only evidence.
//! Native source decimals remain untouched. Derived statistical marks are finite f64.
use crate::{feature_model::FeatureModel, signals::SignalModule};
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use contracts::{factors::*, science::*, ticks::*, DbCounter, SchemaV1};
use domain::ticks::{evaluation_request, partition_bounds, partition_for_event};
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage,
    indicator::{Indicator, MovingAverage},
};
use nautilus_model::{data::TradeTick, enums::AggressorSide};
use std::{collections::BTreeMap, path::Path};

pub mod catalog;
pub mod cohort_cache;
pub mod economic;
mod stream;

pub const FEATURE_SCHEMA: &str = "qz.tick-cohort-features.v1: cohort_vwap,previous_cohort_vwap,fast_ema,slow_ema,cohort_quantity,trade_count,elapsed_event_ns";
pub const FLOW_FEATURE_SCHEMA: &str = "qz.tick-cohort-flow-features.v1: cohort_vwap,previous_cohort_vwap,fast_ema,slow_ema,cohort_quantity,trade_count,elapsed_event_ns,signed_quantity";

#[derive(Clone, Debug)]
struct Cohort {
    event: u64,
    vwap: f64,
    quantity: f64,
    count: usize,
    signed_quantity: Option<f64>,
}

fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

/// Integer/decimal reductions precede finite feature conversion, making tie order irrelevant.
fn cohorts(trades: &[TradeTick], maximum: u32) -> Result<Vec<Cohort>> {
    cohorts_with_flow(trades, maximum, false)
}

fn cohorts_with_flow(trades: &[TradeTick], maximum: u32, flow: bool) -> Result<Vec<Cohort>> {
    trades
        .chunk_by(|a, b| a.ts_event == b.ts_event)
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
                event: rows[0].ts_event.as_u64(),
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
    let first = series
        .values()
        .flat_map(|rows| rows.iter().map(|t| t.ts_event.as_u64()))
        .min()
        .ok_or_else(|| anyhow::anyhow!("TICK_EMPTY_SELECTION"))?;
    let last = series
        .values()
        .flat_map(|rows| rows.iter().map(|t| t.ts_event.as_u64()))
        .max()
        .expect("nonempty selection");
    let count = series.values().map(Vec::len).sum::<usize>();
    let cohort_count = series
        .values()
        .map(|rows| rows.chunk_by(|a, b| a.ts_event == b.ts_event).count())
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
                    rows.chunk_by(|a, b| a.ts_event == b.ts_event).count(),
                )?)?,
                first_event_ns: rows
                    .first()
                    .map(|t| counter(t.ts_event.as_u64()))
                    .transpose()?,
                last_event_ns: rows
                    .last()
                    .map(|t| counter(t.ts_event.as_u64()))
                    .transpose()?,
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
        availability: NativeTickAvailabilityV1::EventTimeProxyUnverified,
        limitations: vec![
            "EVENT_TIME_PROXY_UNVERIFIED".into(),
            "NOT_PIT_QUALIFIED".into(),
            "NOT_INDEPENDENT_MARKET_COMPLETENESS_EVIDENCE".into(),
            "CURRENT_DEFINITIONS_ARE_REPRESENTATION_METADATA_NOT_HISTORICAL_CONTRACT_OR_FEES"
                .into(),
            "VWAP_IS_A_DERIVED_STATISTICAL_MARK_NOT_EXECUTABLE_PRICE".into(),
        ],
        source_evidence: BTreeMap::new(),
    })
}

pub fn evaluate(
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
    wasm: &[u8],
    summary_only: bool,
) -> Result<NativeTradeTickFactorEvaluationResultV1> {
    evaluation_request(request)?;
    if request.input.is_cohort_cache() {
        return stream::evaluate(root, request, wasm, summary_only);
    }
    let series = catalog::load(root, request)?;
    let mut result = evaluate_series(request, series, wasm, summary_only)?;
    result.coverage.source_evidence = catalog::source_evidence(root, request)?;
    Ok(result)
}

fn feature(value: f64, event: u64, ordinal: u32) -> Result<FeatureValueV1> {
    ensure!(value.is_finite(), "TICK_FEATURE_NONFINITE");
    Ok(FeatureValueV1 {
        value: Some(value),
        missing_reason: None,
        event_ns: Some(counter(event)?),
        observed_available_ns: None,
        effective_available_ns: Some(counter(event)?),
        sequence: Some(counter(u64::from(ordinal))?),
    })
}

fn evaluate_series(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    series: BTreeMap<String, Vec<TradeTick>>,
    wasm: &[u8],
    summary_only: bool,
) -> Result<NativeTradeTickFactorEvaluationResultV1> {
    let coverage = coverage(request, &series)?;
    let module = SignalModule::new(wasm, request.parameters.total_fuel.is_some())?;
    let mut remaining = request.parameters.total_fuel.map(|fuel| fuel.get());
    let mut points = Vec::new();
    let mut predictions = 0usize;
    for (instrument_id, trades) in series {
        let cohorts = cohorts(&trades, request.sampling.maximum_cohort_rows)?;
        if cohorts.is_empty() {
            continue;
        }
        let mut model = FeatureModel::new(&module, Some(u64::try_from(cohorts.len())?), remaining)?;
        let mut fast =
            ExponentialMovingAverage::new(request.parameters.fast_period_cohorts as usize, None);
        let mut slow =
            ExponentialMovingAverage::new(request.parameters.slow_period_cohorts as usize, None);
        let mut future_index = 0;
        for (index, current) in cohorts.iter().enumerate() {
            fast.update_raw(current.vwap);
            slow.update_raw(current.vwap);
            let ordinal = u32::try_from(index)?;
            let prediction = if index > 0 && fast.initialized() && slow.initialized() {
                let previous = &cohorts[index - 1];
                let features = [
                    feature(current.vwap, current.event, ordinal)?,
                    feature(previous.vwap, previous.event, ordinal)?,
                    feature(fast.value(), current.event, ordinal)?,
                    feature(slow.value(), current.event, ordinal)?,
                    feature(current.quantity, current.event, ordinal)?,
                    feature(current.count as f64, current.event, ordinal)?,
                    feature(
                        (current.event - previous.event) as f64,
                        current.event,
                        ordinal,
                    )?,
                ];
                let value = model.predict(current.event, current.event, ordinal, &features)?;
                predictions += 1;
                Some(value)
            } else {
                None
            };
            // The model has returned before any future cohort is used for the label.
            let future = if prediction.is_some() {
                match request.target {
                    NativeTickTargetV1::FixedTradeCohorts { horizon_cohorts } => index
                        .checked_add(horizon_cohorts as usize)
                        .and_then(|future| cohorts.get(future)),
                    NativeTickTargetV1::FixedEventTime { horizon_ns } => {
                        let threshold = current
                            .event
                            .checked_add(horizon_ns.get())
                            .ok_or_else(|| anyhow::anyhow!("TICK_LABEL_TIME_RANGE"))?;
                        while cohorts
                            .get(future_index)
                            .is_some_and(|future| future.event < threshold)
                        {
                            future_index += 1;
                        }
                        cohorts.get(future_index)
                    }
                }
            } else {
                None
            };
            let label = future.map(|future| future.vwap / current.vwap - 1.0);
            ensure!(label.is_none_or(f64::is_finite), "TICK_LABEL_NONFINITE");
            let event_ns = counter(current.event)?;
            let observation = NativeForecastPointV1 {
                instrument_id: instrument_id.clone(),
                ordinal,
                event_ns,
                available_ns: event_ns,
                forecast: prediction,
                forecast_reason: prediction
                    .is_none()
                    .then_some(ForecastMissingReason::IndicatorWarmup),
                label_return: label,
                label_available_ns: future.map(|future| counter(future.event)).transpose()?,
                label_reason: if prediction.is_none() {
                    Some(ForecastMissingReason::IndicatorWarmup)
                } else {
                    future
                        .is_none()
                        .then_some(ForecastMissingReason::LabelNotComplete)
                },
            };
            let partition = partition_for_event(request, event_ns)
                .ok_or_else(|| anyhow::anyhow!("TICK_EVENT_OUTSIDE_SELECTION"))?;
            let label_event_ns = future.map(|future| counter(future.event)).transpose()?;
            let (_, end) = partition_bounds(request, partition);
            let exclusion_reason = if prediction.is_none() {
                Some(NativeFactorExclusionReasonV1::IndicatorWarmup)
            } else if future.is_none() {
                Some(NativeFactorExclusionReasonV1::LabelNotComplete)
            } else if label_event_ns.is_some_and(|event| event >= end) {
                Some(NativeFactorExclusionReasonV1::LabelCrossesPartition)
            } else {
                None
            };
            points.push(NativeFactorPointV1 {
                observation,
                partition,
                label_event_ns,
                exclusion_reason,
            });
        }
        remaining = model.remaining_fuel();
    }
    ensure!(predictions > 0, "TICK_NO_PREDICTIONS");
    let partitions = [
        NativeFactorPartitionV1::Train,
        NativeFactorPartitionV1::Validation,
        NativeFactorPartitionV1::Heldout,
    ]
    .into_iter()
    .map(|partition| -> Result<_> {
        let (start_ns, end_ns) = partition_bounds(request, partition);
        let assets = request
            .input
            .instrument_ids
            .iter()
            .map(|id| -> Result<_> {
                Ok(NativeFactorAssetMetricV1 {
                    instrument_id: id.clone(),
                    metric: crate::factors::metric(points.iter().filter(|p| {
                        p.partition == partition && p.observation.instrument_id == *id
                    }))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(NativeFactorPartitionReportV1 {
            partition,
            start_ns,
            end_ns,
            metric: crate::factors::metric(points.iter().filter(|p| p.partition == partition))?,
            assets,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    Ok(NativeTradeTickFactorEvaluationResultV1{schema_version:SchemaV1,request:request.clone(),native_versions:BTreeMap::from([("nautilus-model".into(),"0.63.0".into()),("nautilus-persistence".into(),"0.63.0".into()),("nautilus-indicators".into(),"0.63.0".into()),("wasmi".into(),"2.0.0".into())]),methods:BTreeMap::from([("input".into(),"original native TradeTick; no BAR catalog or simulation".into()),("prediction".into(),crate::feature_model::ABI.into()),("features".into(),FEATURE_SCHEMA.into()),("sampling".into(),"EVENT_TIME_COHORT_V1: complete current cohort visible; earlier selected cohorts only; unknown source sequence".into()),("label".into(),request.target.label_method().into()),("partitioning".into(),"shared half-open event cuts; cross-boundary labels excluded; warmup stays within selected interval".into()),("qualification".into(),"UNQUALIFIED_EXPLORATORY; event time proxy is not original availability evidence".into()),("ic".into(),"existing ndarray-stats raw pooled Pearson correlation".into())]),consumed_fuel:request.parameters.total_fuel.zip(remaining).map(|(total,left)|counter(total.get()-left)).transpose()?,coverage,partitions,regime_partitions:None,points:(!summary_only).then_some(points)})
}

#[cfg(test)]
mod tests;
