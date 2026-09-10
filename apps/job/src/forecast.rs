//! Native catalog -> causal features -> bounded Wasm predictions, with separate labels.
//! Labels in this result are restricted evaluation evidence, not a public Agent response.
use crate::{catalog::load_catalog, signals::WasmSignal};
use anyhow::{ensure, Result};
use contracts::{science::*, DbCounter, SchemaV1};
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage,
    indicator::{Indicator, MovingAverage},
};
use nautilus_model::instruments::Instrument;
use std::{collections::BTreeMap, path::Path};

fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

/// Invoke afresh for each independently authorized fold; never carry an instance
/// from training into an independent evaluation or between instruments.
pub fn forecast(
    catalog_root: &Path,
    request: &NativeForecastRequestV1,
    module: &[u8],
) -> Result<NativeForecastResultV1> {
    let parameters = &request.parameters;
    ensure!(
        (1..=10_000).contains(&parameters.fast_period)
            && (2..=10_000).contains(&parameters.slow_period)
            && parameters.fast_period < parameters.slow_period
            && (1..=100_000).contains(&parameters.label_horizon_observations)
            && (1..=crate::signals::MAX_SIGNAL_FUEL).contains(&parameters.total_fuel.get()),
        "FORECAST_PARAMETERS_INVALID"
    );
    let market = load_catalog(catalog_root, &request.selection)?;
    let mut remaining = parameters.total_fuel.get();
    let mut points = Vec::with_capacity(market.rows);
    let mut prediction_count = 0_u64;
    for series in market.series {
        ensure!(
            series.bars.len() >= parameters.slow_period as usize,
            "FORECAST_INSUFFICIENT_WARMUP"
        );
        let mut model = WasmSignal::new(module, u32::try_from(series.bars.len())?, remaining)?;
        let mut fast = ExponentialMovingAverage::new(parameters.fast_period as usize, None);
        let mut slow = ExponentialMovingAverage::new(parameters.slow_period as usize, None);
        for (index, bar) in series.bars.iter().enumerate() {
            fast.handle_bar(bar);
            slow.handle_bar(bar);
            let close = bar.close.as_f64();
            let prediction = if index > 0 && fast.initialized() && slow.initialized() {
                let previous = series.bars[index - 1].close.as_f64();
                let value = model.predict([
                    close,
                    previous,
                    fast.value(),
                    slow.value(),
                    bar.volume.as_f64(),
                    bar.open.as_f64(),
                    bar.high.as_f64(),
                    bar.low.as_f64(),
                ])?;
                prediction_count += 1;
                Some(value)
            } else {
                None
            };
            // No label is formed for a warmup row that never called predict.
            // A future price is read only after a successful prediction returns;
            // it never enters the module or the current feature state.
            let future = prediction.and_then(|_| {
                series
                    .bars
                    .get(index + parameters.label_horizon_observations as usize)
            });
            let label = future.map(|future| future.close.as_f64() / close - 1.0);
            ensure!(label.is_none_or(f64::is_finite), "FORECAST_LABEL_NONFINITE");
            points.push(NativeForecastPointV1 {
                instrument_id: series.instrument.id().to_string(),
                ordinal: u32::try_from(index)?,
                event_ns: counter(bar.ts_event.as_u64())?,
                available_ns: counter(bar.ts_init.as_u64())?,
                forecast: prediction,
                forecast_reason: prediction
                    .is_none()
                    .then_some(ForecastMissingReason::IndicatorWarmup),
                label_return: label,
                label_available_ns: future
                    .map(|future| counter(future.ts_init.as_u64()))
                    .transpose()?,
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
    Ok(NativeForecastResultV1 {
        schema_version: SchemaV1,
        native_versions: BTreeMap::from([
            ("nautilus-indicators".into(), "0.63.0".into()),
            ("nautilus-persistence".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
        ]),
        consumed_fuel: counter(parameters.total_fuel.get() - remaining)?,
        points,
    })
}
