//! Trusted native catalog/Wasm adapter for factor-only scientific evidence.
//! The immutable model is never fitted here and never receives any future label.
use anyhow::{ensure, Result};
use contracts::{factors::*, DbCounter, SchemaV1};
use domain::factors::{
    evaluation_request, exclusion_reason, partition_bounds, partition_for_event,
};
use ndarray::Array2;
use ndarray_stats::CorrelationExt;
use std::{collections::BTreeMap, path::Path};

mod catalog_scan;
pub use catalog_scan::{catalog_factor_scope, scan_factor_catalog, scan_factor_catalog_with_scope};

/// Native raw Pearson calculation, without calibration or score restrictions.
/// Bad numerical evidence is unavailable, never repaired or reported as zero.
pub fn pearson_ic(scores: &[f64], labels: &[f64]) -> Result<NativeFactorIcV1> {
    ensure!(scores.len() == labels.len(), "FACTOR_METRIC_INPUT_LENGTH");
    use NativeFactorIcStatusV1::*;
    let absent = |status| NativeFactorIcV1 {
        value: None,
        status,
    };
    if !scores.iter().chain(labels).all(|value| value.is_finite()) {
        return Ok(absent(NonfiniteData));
    }
    if scores.len() < 2 {
        return Ok(absent(InsufficientSamples));
    }
    if scores.iter().all(|value| *value == scores[0]) {
        return Ok(absent(ConstantScore));
    }
    if labels.iter().all(|value| *value == labels[0]) {
        return Ok(absent(ConstantLabel));
    }
    let matrix = Array2::from_shape_vec(
        (2, scores.len()),
        scores.iter().chain(labels).copied().collect(),
    )?;
    let value = matrix.pearson_correlation()?[[0, 1]];
    if !value.is_finite() {
        return Ok(absent(NonfiniteResult));
    }
    Ok(NativeFactorIcV1 {
        value: Some(value),
        status: Available,
    })
}

fn count(value: usize) -> Result<DbCounter> {
    DbCounter::new(u64::try_from(value)?).map_err(anyhow::Error::msg)
}

#[derive(Default)]
struct MetricPairs {
    observations: usize,
    warmup: usize,
    incomplete: usize,
    crossing: usize,
    unavailable: usize,
    scores: Vec<f64>,
    labels: Vec<f64>,
}

impl MetricPairs {
    fn push(&mut self, point: &NativeFactorPointV1) {
        use NativeFactorExclusionReasonV1::*;
        self.observations += 1;
        match point.exclusion_reason {
            Some(IndicatorWarmup) => self.warmup += 1,
            Some(LabelNotComplete) => self.incomplete += 1,
            Some(LabelCrossesPartition) => self.crossing += 1,
            Some(PredictionOutsidePartition) => self.unavailable += 1,
            None => {
                self.scores
                    .push(point.observation.forecast.expect("included native score"));
                self.labels.push(
                    point
                        .observation
                        .label_return
                        .expect("included native label"),
                );
            }
        }
    }

    fn finish(self) -> Result<NativeFactorMetricV1> {
        Ok(NativeFactorMetricV1 {
            counts: NativeFactorCountsV1 {
                observations: count(self.observations)?,
                included: count(self.scores.len())?,
                excluded: count(self.observations - self.scores.len())?,
                indicator_warmup: count(self.warmup)?,
                incomplete_labels: count(self.incomplete)?,
                cross_boundary_labels: count(self.crossing)?,
                prediction_outside_partition: count(self.unavailable)?,
            },
            pearson_ic: pearson_ic(&self.scores, &self.labels)?,
        })
    }
}

fn metric<'a>(
    points: impl Iterator<Item = &'a NativeFactorPointV1>,
) -> Result<NativeFactorMetricV1> {
    let mut pairs = MetricPairs::default();
    for point in points {
        pairs.push(point);
    }
    pairs.finish()
}

/// Reuses native BAR decoding, official Nautilus EMA and the same SignalModule.
/// Paths are trusted local CLI inputs, not fields accepted from an untrusted Agent.
/// This result contains all partitions' labels; the host owns held-out exposure policy.
pub fn evaluate_factor(
    catalog_root: &Path,
    request: &NativeFactorEvaluationRequestV1,
    module: &[u8],
) -> Result<NativeFactorEvaluationResultV1> {
    evaluation_request(request)?;
    let forecast = crate::forecast::forecast_factor(catalog_root, &request.forecast, module)?;
    let horizon = request.forecast.parameters.label_horizon_observations as usize;
    // Forecast retains every original row grouped by instrument. Resolve the actual
    // future event clock from that same asset, not an inferred regular time interval.
    let mut label_events = Vec::with_capacity(forecast.points.len());
    for series in forecast
        .points
        .chunk_by(|left, right| left.instrument_id == right.instrument_id)
    {
        label_events.extend(series.iter().enumerate().map(|(index, point)| {
            point
                .forecast
                .and_then(|_| series.get(index + horizon))
                .map(|future| future.event_ns)
        }));
    }
    let points = forecast
        .points
        .into_iter()
        .zip(label_events)
        .map(|(observation, label_event_ns)| -> Result<_> {
            let partition = partition_for_event(request, observation.event_ns)
                .ok_or_else(|| anyhow::anyhow!("FACTOR_EVENT_OUTSIDE_SELECTION"))?;
            let exclusion_reason =
                exclusion_reason(request, partition, &observation, label_event_ns);
            Ok(NativeFactorPointV1 {
                observation,
                partition,
                label_event_ns,
                exclusion_reason,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let partitions = [
        NativeFactorPartitionV1::Train,
        NativeFactorPartitionV1::Validation,
        NativeFactorPartitionV1::Heldout,
    ]
    .into_iter()
    .map(|partition| -> Result<_> {
        let (start_ns, end_ns) = partition_bounds(request, partition);
        let selected = || {
            points
                .iter()
                .filter(move |point| point.partition == partition)
        };
        let assets = points
            .chunk_by(|left, right| {
                left.observation.instrument_id == right.observation.instrument_id
            })
            .map(|series| -> Result<_> {
                Ok(NativeFactorAssetMetricV1 {
                    instrument_id: series[0].observation.instrument_id.clone(),
                    metric: metric(series.iter().filter(|point| point.partition == partition))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(NativeFactorPartitionReportV1 {
            partition,
            start_ns,
            end_ns,
            metric: metric(selected())?,
            assets,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let mut native_versions = forecast.native_versions;
    native_versions.extend(BTreeMap::from([
        ("nautilus-model".into(), "0.63.0".into()),
        ("ndarray".into(), "0.17.1".into()),
        ("ndarray-stats".into(), "0.7.0".into()),
    ]));
    Ok(NativeFactorEvaluationResultV1 {
        schema_version: SchemaV1,
        request: request.clone(),
        native_versions,
        methods: BTreeMap::from([
            ("input".into(), "observed native BAR prices/volume; no simulation instrument or execution assumptions".into()),
            ("prediction".into(), "wasmi.TypedFunc::call; predict(f64 x 8) -> f64; fresh instance per instrument".into()),
            ("features".into(), "nautilus_indicators::average::ema::ExponentialMovingAverage; close, previous_close, fast_ema, slow_ema, volume, open, high, low; causal prefix only".into()),
            ("label".into(), "future.close / current.close - 1; same-instrument observation horizon; never supplied to predict".into()),
            ("ic".into(), "ndarray-stats.pearson_correlation".into()),
            ("pooling".into(), "raw qualifying score/return pairs across instruments; no calibration, ranking or normalization".into()),
            ("partitioning".into(), "shared half-open event cuts; prediction availability, future event and label availability must remain within partition".into()),
        ]),
        consumed_fuel: forecast.consumed_fuel,
        points,
        partitions,
    })
}
