//! Actual native fold/model/estimator execution. No database or policy authority.
use super::{validation_folds, ScoreCalibration, MAX_VALIDATION_FOLDS, MAX_VALIDATION_INDICES};
use crate::{
    catalog::{load_catalog, NativeBarSeries},
    forecast::features,
    signals::WasmSignal,
};
use anyhow::{ensure, Result};
use contracts::{
    brief::TargetKind, evidence::MetricStatus, research::SplitKind, science::*, DbCounter, SchemaV1,
};
use nautilus_model::instruments::Instrument;
use ndarray::{Array2, ArrayView1};
use ndarray_stats::{CorrelationExt, DeviationExt};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn count(value: usize) -> Result<DbCounter> {
    DbCounter::new(u64::try_from(value)?).map_err(anyhow::Error::msg)
}

/// Every disjoint block starts a new model; only the task's unspent fuel survives.
fn predict(
    module: &[u8],
    features: &[Option<[f64; 8]>],
    indices: &[usize],
    remaining: &mut u64,
) -> Result<Vec<f64>> {
    let mut result = Vec::with_capacity(indices.len());
    for block in indices.chunk_by(|a, b| *b == *a + 1) {
        let mut model = WasmSignal::new(module, u32::try_from(block.len())?, *remaining)?;
        for &ordinal in block {
            let input = features
                .get(ordinal)
                .copied()
                .flatten()
                .ok_or_else(|| anyhow::anyhow!("VALIDATION_FEATURE_MISSING"))?;
            result.push(model.predict(input)?);
        }
        *remaining = model.remaining_fuel();
    }
    Ok(result)
}

fn labels(series: &NativeBarSeries, indices: &[usize], horizon: usize) -> Result<Vec<f64>> {
    indices
        .iter()
        .map(|&ordinal| {
            let future = series
                .bars
                .get(ordinal + horizon)
                .ok_or_else(|| anyhow::anyhow!("VALIDATION_LABEL_MISSING"))?;
            let value = future.close.as_f64() / series.bars[ordinal].close.as_f64() - 1.0;
            ensure!(value.is_finite(), "VALIDATION_LABEL_NONFINITE");
            Ok(value)
        })
        .collect()
}

pub(super) fn metric(
    kind: NativeAlphaMetricKind,
    prediction: &[f64],
    labels: &[f64],
) -> Result<NativeValidationMetricV1> {
    use NativeAlphaMetricKind::*;
    let absent = |status, reason: &str| NativeValidationMetricV1 {
        kind,
        value: None,
        status,
        reason_code: Some(reason.into()),
    };
    if prediction.is_empty() {
        return Ok(absent(
            MetricStatus::InsufficientData,
            if labels.is_empty() {
                "NO_COMPLETE_LABELS"
            } else {
                "CALIBRATION_UNAVAILABLE"
            },
        ));
    }
    ensure!(
        prediction.len() == labels.len() && prediction.iter().chain(labels).all(|x| x.is_finite()),
        "VALIDATION_METRIC_INPUT"
    );
    let value = match kind {
        PearsonIc => {
            if prediction.len() < 2
                || prediction.iter().all(|x| *x == prediction[0])
                || labels.iter().all(|x| *x == labels[0])
            {
                return Ok(absent(
                    MetricStatus::InsufficientData,
                    "CORRELATION_VARIATION_REQUIRED",
                ));
            }
            let values = prediction.iter().chain(labels).copied().collect();
            let matrix = Array2::from_shape_vec((2, prediction.len()), values)?;
            matrix.pearson_correlation()?[[0, 1]]
        }
        ReturnRmse => ArrayView1::from(prediction).root_mean_sq_err(&ArrayView1::from(labels))?,
    };
    if !value.is_finite() {
        return Ok(absent(MetricStatus::Failed, "NATIVE_METRIC_NONFINITE"));
    }
    Ok(NativeValidationMetricV1 {
        kind,
        value: Some(value),
        status: MetricStatus::Ok,
        reason_code: None,
    })
}

pub fn validate_alpha(
    root: &Path,
    request: &NativeAlphaValidationRequestV1,
    module: &[u8],
) -> Result<NativeAlphaValidationResultV1> {
    let forecast = &request.forecast;
    domain::execution::alpha_validation_request(request)?;
    let parameters = &forecast.parameters;
    let horizon = parameters.label_horizon_observations as usize;
    let market = load_catalog(root, &forecast.selection)?;
    let mut remaining = parameters.total_fuel.get();
    let mut folds = Vec::new();
    let mut index_count = 0_usize;
    let mut unique_test_observations = 0_usize;
    for series in market.series {
        let mut unique = BTreeSet::new();
        let features = features(&series, parameters).collect::<Vec<_>>();
        let eligible = features
            .iter()
            .enumerate()
            .filter_map(|(i, value)| {
                (value.is_some() && i + horizon < series.bars.len()).then_some(i)
            })
            .collect::<Vec<_>>();
        ensure!(
            eligible.windows(2).all(|w| w[1] == w[0] + 1),
            "VALIDATION_NONCONTIGUOUS_OBSERVATIONS"
        );
        let native = validation_folds(&request.split_policy, eligible.len())?;
        ensure!(
            folds.len() + native.len() <= MAX_VALIDATION_FOLDS,
            "VALIDATION_FOLD_LIMIT"
        );
        for fold in &native {
            index_count = index_count
                .checked_add(fold.train.len() + fold.test.len())
                .ok_or_else(|| anyhow::anyhow!("VALIDATION_INDEX_LIMIT"))?;
            ensure!(
                index_count <= MAX_VALIDATION_INDICES,
                "VALIDATION_INDEX_LIMIT"
            );
        }
        let instrument_id = series.instrument.id().to_string();
        for (fold_index, fold) in native.into_iter().enumerate() {
            let train = fold.train.iter().map(|&i| eligible[i]).collect::<Vec<_>>();
            let test = fold.test.iter().map(|&i| eligible[i]).collect::<Vec<_>>();
            if request.split_policy.kind == SplitKind::WalkForward {
                ensure!(
                    series.bars[train[train.len() - 1] + horizon].ts_init
                        < series.bars[test[0]].ts_init,
                    "VALIDATION_TRAIN_LABEL_FROM_FUTURE"
                );
            }
            let train_scores = predict(module, &features, &train, &mut remaining)?;
            let test_scores = predict(module, &features, &test, &mut remaining)?;
            // The model has finished before these labels are read. Neither
            // training nor testing model state ever receives a future label.
            let train_labels = labels(&series, &train, horizon)?;
            let test_labels = labels(&series, &test, horizon)?;
            let (calibration, returns) = match request.target_kind {
                TargetKind::ExpectedReturn => (None, test_scores.clone()),
                TargetKind::Score => {
                    let observations = count(train.len())?;
                    if train_scores.iter().all(|x| *x == train_scores[0]) {
                        (
                            Some(NativeCalibrationV1 {
                                status: MetricStatus::InsufficientData,
                                reason_code: Some("CALIBRATION_CONSTANT_SCORE".into()),
                                intercept: None,
                                slope: None,
                                training_observations: observations,
                            }),
                            Vec::new(),
                        )
                    } else {
                        match ScoreCalibration::fit(&train_scores, &train_labels).and_then(
                            |model| {
                                let returns = model.predict(&test_scores)?;
                                Ok((model.coefficients(), returns))
                            },
                        ) {
                            Ok(([intercept, slope], returns)) => (
                                Some(NativeCalibrationV1 {
                                    status: MetricStatus::Ok,
                                    reason_code: None,
                                    intercept: Some(intercept),
                                    slope: Some(slope),
                                    training_observations: observations,
                                }),
                                returns,
                            ),
                            Err(_) => (
                                Some(NativeCalibrationV1 {
                                    status: MetricStatus::Failed,
                                    reason_code: Some("CALIBRATION_FIT_UNAVAILABLE".into()),
                                    intercept: None,
                                    slope: None,
                                    training_observations: observations,
                                }),
                                Vec::new(),
                            ),
                        }
                    }
                }
            };
            let metrics = vec![
                metric(NativeAlphaMetricKind::PearsonIc, &test_scores, &test_labels)?,
                metric(NativeAlphaMetricKind::ReturnRmse, &returns, &test_labels)?,
            ];
            let mut test_points = Vec::with_capacity(test.len());
            for (i, &ordinal) in test.iter().enumerate() {
                unique.insert(ordinal);
                let bar = &series.bars[ordinal];
                test_points.push(NativeValidationPointV1 {
                    observation: NativeForecastPointV1 {
                        instrument_id: instrument_id.clone(),
                        ordinal: u32::try_from(ordinal)?,
                        event_ns: DbCounter::new(bar.ts_event.as_u64())
                            .map_err(anyhow::Error::msg)?,
                        available_ns: DbCounter::new(bar.ts_init.as_u64())
                            .map_err(anyhow::Error::msg)?,
                        forecast: Some(test_scores[i]),
                        forecast_reason: None,
                        label_return: Some(test_labels[i]),
                        label_available_ns: Some(
                            DbCounter::new(series.bars[ordinal + horizon].ts_init.as_u64())
                                .map_err(anyhow::Error::msg)?,
                        ),
                        label_reason: None,
                    },
                    expected_return: returns.get(i).copied(),
                });
            }
            folds.push(NativeValidationFoldV1 {
                instrument_id: instrument_id.clone(),
                bar_type: series.bar_type.to_string(),
                source_row_count: count(series.bars.len())?,
                fold_index: u16::try_from(fold_index)?,
                training_end_available_ns: DbCounter::new(
                    series.bars[train[train.len() - 1] + horizon]
                        .ts_init
                        .as_u64(),
                )
                .map_err(anyhow::Error::msg)?,
                training_ordinals: train
                    .into_iter()
                    .map(u32::try_from)
                    .collect::<Result<_, _>>()?,
                test_points,
                calibration,
                metrics,
            });
        }
        unique_test_observations += unique.len();
    }
    Ok(NativeAlphaValidationResultV1 {
        schema_version: SchemaV1,
        native_versions: BTreeMap::from([
            ("nautilus-indicators".into(), "0.63.0".into()),
            ("nautilus-persistence".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
            ("solow-cv".into(), "0.7.3".into()),
            ("linregress".into(), "0.5.4".into()),
            ("ndarray-stats".into(), "0.7.0".into()),
        ]),
        consumed_fuel: DbCounter::new(parameters.total_fuel.get() - remaining)
            .map_err(anyhow::Error::msg)?,
        unique_test_observations: count(unique_test_observations)?,
        folds,
    })
}
