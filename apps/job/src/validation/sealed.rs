//! Apply only the frozen model to held-out observations; never fit or select a fold.
use super::{alpha::metric, predict_frozen_calibration};
use anyhow::{ensure, Result};
use contracts::{science::*, DbCounter, SchemaV1};
use std::{collections::BTreeMap, path::Path};

pub fn evaluate_sealed_alpha(
    catalog: &Path,
    request: &NativeAlphaSealedRequestV1,
    module: &[u8],
    calibration: Option<&NativeFrozenCalibrationV1>,
) -> Result<NativeAlphaSealedResultV1> {
    domain::execution::alpha_sealed_request(request, calibration)?;
    let horizon = DbCounter::new(u64::from(
        request.forecast.parameters.label_horizon_observations,
    ))
    .map_err(anyhow::Error::msg)?;
    let forecast = crate::forecast::forecast(catalog, &request.forecast, module)?;
    let mut expected_returns = Vec::with_capacity(forecast.points.len());
    let mut assets = Vec::new();
    for (points, bar_type) in forecast
        .points
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
        .zip(&request.forecast.selection.bar_types)
    {
        let first = points
            .iter()
            .find(|p| p.forecast.is_some())
            .ok_or_else(|| anyhow::anyhow!("SEALED_NO_PREDICTIONS"))?;
        ensure!(
            first.available_ns > request.research_available_through_ns,
            "SEALED_PREDICTION_NOT_AFTER_RESEARCH"
        );
        let scores = points.iter().filter_map(|p| p.forecast).collect::<Vec<_>>();
        let returns = match calibration {
            Some(model) => {
                predict_frozen_calibration(model, bar_type, horizon, first.available_ns, &scores)?
            }
            None => scores.clone(),
        };
        ensure!(returns.len() == scores.len(), "SEALED_RETURN_COUNT");
        let mut returns = returns.into_iter();
        let offset = expected_returns.len();
        expected_returns.extend(
            points
                .iter()
                .map(|p| p.forecast.and_then(|_| returns.next())),
        );
        let mut raw = Vec::new();
        let mut predicted = Vec::new();
        let mut labels = Vec::new();
        for (point, value) in points.iter().zip(&expected_returns[offset..]) {
            if let Some(label) = point.label_return {
                raw.push(
                    point
                        .forecast
                        .ok_or_else(|| anyhow::anyhow!("SEALED_SCORE_MISSING"))?,
                );
                predicted.push(value.ok_or_else(|| anyhow::anyhow!("SEALED_RETURN_MISSING"))?);
                labels.push(label);
            }
        }
        assets.push(NativeSealedAssetMetricsV1 {
            instrument_id: first.instrument_id.clone(),
            bar_type: bar_type.clone(),
            observation_count: DbCounter::new(labels.len() as u64).map_err(anyhow::Error::msg)?,
            metrics: vec![
                metric(NativeAlphaMetricKind::PearsonIc, &raw, &labels)?,
                metric(NativeAlphaMetricKind::ReturnRmse, &predicted, &labels)?,
            ],
        });
    }
    let result = NativeAlphaSealedResultV1 {
        schema_version: SchemaV1,
        forecast,
        expected_returns,
        calibration_source_report_artifact_id: calibration.map(|m| m.source_report_artifact_id),
        calibration_fit_end_available_ns: calibration.map(|m| m.fit_end_available_ns),
        native_versions: BTreeMap::from([
            ("ndarray".into(), "0.17.1".into()),
            ("ndarray-stats".into(), "0.7.0".into()),
        ]),
        assets,
    };
    domain::execution::check_alpha_sealed(request, calibration, &result)?;
    Ok(result)
}
