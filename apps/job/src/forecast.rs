//! Native catalog -> causal features -> Wasm predictions, with separate labels.
//! Labels in this result are restricted evaluation evidence, not a public Agent response.
use crate::{
    catalog::{load_catalog, NativeBarSeries, NativeMarketData},
    signals::SignalModule,
};
use anyhow::{ensure, Result};
use contracts::{science::*, DbCounter, SchemaV1};
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage,
    indicator::{Indicator, MovingAverage},
};
use nautilus_model::{data::Bar, instruments::Instrument};
use std::{collections::BTreeMap, path::Path};

pub(crate) fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

/// Identical causal features for discovery and independently instantiated folds.
/// Advancing this iterator reads no future row or label and never calls the model.
pub(crate) fn features<'a>(
    series: &'a NativeBarSeries,
    parameters: &NativeForecastParametersV1,
) -> impl Iterator<Item = Option<[f64; 8]>> + 'a {
    bar_features(&series.bars, parameters)
}

pub(crate) fn bar_features<'a>(
    bars: &'a [Bar],
    parameters: &NativeForecastParametersV1,
) -> impl Iterator<Item = Option<[f64; 8]>> + 'a {
    let mut fast = ExponentialMovingAverage::new(parameters.fast_period as usize, None);
    let mut slow = ExponentialMovingAverage::new(parameters.slow_period as usize, None);
    bars.iter().enumerate().map(move |(index, bar)| {
        fast.handle_bar(bar);
        slow.handle_bar(bar);
        (index > 0 && fast.initialized() && slow.initialized()).then(|| {
            [
                bar.close.as_f64(),
                bars[index - 1].close.as_f64(),
                fast.value(),
                slow.value(),
                bar.volume.as_f64(),
                bar.open.as_f64(),
                bar.high.as_f64(),
                bar.low.as_f64(),
            ]
        })
    })
}

/// Invoke afresh for each independently authorized fold; never carry an instance
/// from training into an independent evaluation or between instruments.
pub fn forecast(
    catalog_root: &Path,
    request: &NativeForecastRequestV1,
    module: &[u8],
) -> Result<NativeForecastResultV1> {
    domain::execution::forecast_request(request)?;
    let market = load_catalog(catalog_root, &request.selection)?;
    forecast_market(&market, request, module)
}

/// Borrow the task's already selected catalog; callers must use the same selection.
/// Models and cutoffs never share mutable execution state.
pub(crate) fn forecast_market(
    market: &NativeMarketData,
    request: &NativeForecastRequestV1,
    module: &[u8],
) -> Result<NativeForecastResultV1> {
    Ok(forecast_result(forecast_points(
        market, request, module, false,
    )?))
}

/// Price-only research never creates simulation instrument definitions.
pub fn forecast_factor(
    catalog_root: &Path,
    request: &NativeForecastRequestV1,
    module: &[u8],
) -> Result<NativeForecastResultV1> {
    domain::execution::forecast_request(request)?;
    let series = crate::catalog::load_price_bars(catalog_root, &request.selection)?;
    let rows = series.iter().map(|(_, bars)| bars.len()).sum();
    let points = forecast_bar_points(
        series
            .iter()
            .map(|(kind, bars)| (kind.instrument_id().to_string(), bars.as_slice())),
        rows,
        series.len(),
        request,
        module,
        false,
    )?;
    Ok(forecast_result(points))
}

fn forecast_result(
    (consumed_fuel, points): (Option<DbCounter>, Vec<NativeForecastPointV1>),
) -> NativeForecastResultV1 {
    NativeForecastResultV1 {
        schema_version: SchemaV1,
        native_versions: BTreeMap::from([
            ("nautilus-indicators".into(), "0.63.0".into()),
            ("nautilus-persistence".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
        ]),
        consumed_fuel,
        points,
    }
}

/// Portfolio preparation only consumes the last point for each instrument.
/// Execute and validate the whole prefix, retaining no unused historical output.
/// The returned fuel and points are internal inputs, not a complete forecast report.
pub(crate) fn forecast_latest_market(
    market: &NativeMarketData,
    request: &NativeForecastRequestV1,
    module: &[u8],
) -> Result<(Option<DbCounter>, Vec<NativeForecastPointV1>)> {
    forecast_points(market, request, module, true)
}

fn forecast_points(
    market: &NativeMarketData,
    request: &NativeForecastRequestV1,
    module: &[u8],
    latest_only: bool,
) -> Result<(Option<DbCounter>, Vec<NativeForecastPointV1>)> {
    forecast_bar_points(
        market
            .series
            .iter()
            .map(|series| (series.instrument.id().to_string(), series.bars.as_slice())),
        market.rows,
        market.series.len(),
        request,
        module,
        latest_only,
    )
}

fn forecast_bar_points<'a>(
    series: impl Iterator<Item = (String, &'a [Bar])>,
    rows: usize,
    assets: usize,
    request: &NativeForecastRequestV1,
    module: &[u8],
    latest_only: bool,
) -> Result<(Option<DbCounter>, Vec<NativeForecastPointV1>)> {
    domain::execution::forecast_request(request)?;
    let parameters = &request.parameters;
    let module = SignalModule::new(module, parameters.total_fuel.is_some())?;
    let mut remaining = parameters.total_fuel.map(|fuel| fuel.get());
    let mut points = Vec::with_capacity(if latest_only { assets } else { rows });
    let mut prediction_count = 0_u64;
    for (instrument_id, bars) in series {
        ensure!(
            bars.len() >= parameters.slow_period as usize,
            "FORECAST_INSUFFICIENT_WARMUP"
        );
        let mut model = module.instantiate(u64::try_from(bars.len())?, remaining)?;
        for (index, (bar, features)) in bars.iter().zip(bar_features(bars, parameters)).enumerate()
        {
            let close = bar.close.as_f64();
            let prediction = if let Some(features) = features {
                let value = model.predict(features)?;
                prediction_count += 1;
                Some(value)
            } else {
                None
            };
            // No label is formed for a warmup row that never called predict.
            // A future price is read only after a successful prediction returns;
            // it never enters the module or the current feature state.
            let future = prediction
                .and_then(|_| bars.get(index + parameters.label_horizon_observations as usize));
            let label = future.map(|future| future.close.as_f64() / close - 1.0);
            ensure!(label.is_none_or(f64::is_finite), "FORECAST_LABEL_NONFINITE");
            // Keep every original conversion and label check, including rows
            // which a portfolio does not retain. A bad prefix must still fail.
            let ordinal = u32::try_from(index)?;
            let event_ns = counter(bar.ts_event.as_u64())?;
            let available_ns = counter(bar.ts_init.as_u64())?;
            let label_available_ns = future
                .map(|future| counter(future.ts_init.as_u64()))
                .transpose()?;
            if latest_only && index + 1 != bars.len() {
                continue;
            }
            points.push(NativeForecastPointV1 {
                instrument_id: instrument_id.clone(),
                ordinal,
                event_ns,
                available_ns,
                forecast: prediction,
                forecast_reason: prediction
                    .is_none()
                    .then_some(ForecastMissingReason::IndicatorWarmup),
                label_return: label,
                label_available_ns,
                label_reason: if prediction.is_none() {
                    Some(ForecastMissingReason::IndicatorWarmup)
                } else {
                    future
                        .is_none()
                        .then_some(ForecastMissingReason::LabelNotComplete)
                },
            });
        }
        // The next instrument receives the unspent remainder, not another full budget.
        remaining = model.remaining_fuel();
    }
    ensure!(prediction_count > 0, "FORECAST_NO_PREDICTIONS");
    Ok((
        parameters
            .total_fuel
            .zip(remaining)
            .map(|(total, left)| counter(total.get() - left))
            .transpose()?,
        points,
    ))
}

#[cfg(test)]
#[path = "forecast_tests.rs"]
mod tests;
