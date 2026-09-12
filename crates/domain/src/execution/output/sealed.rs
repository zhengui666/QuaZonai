//! Exact held-out associations. No refitting or recalculation of native metrics.
use super::{bad, forecast, validation};
use crate::DomainError;
use contracts::{brief::TargetKind, evidence::MetricStatus, science::*};
use std::collections::BTreeMap;

pub fn request(
    request: &NativeAlphaSealedRequestV1,
    calibration: Option<&NativeFrozenCalibrationV1>,
) -> Result<(), DomainError> {
    crate::execution::forecast_request(&request.forecast)?;
    if request.research_available_through_ns >= request.forecast.selection.decision_cutoff_ns {
        return Err(bad("sealed.research_cutoff"));
    }
    match (request.target_kind, calibration) {
        (TargetKind::ExpectedReturn, None) => Ok(()),
        (TargetKind::Score, Some(model)) => {
            validation::frozen_calibration(model)?;
            if model.horizon_observations.get()
                != u64::from(request.forecast.parameters.label_horizon_observations)
                || model.fit_end_available_ns > request.research_available_through_ns
                || !model
                    .assets
                    .iter()
                    .map(|a| &a.bar_type)
                    .eq(&request.forecast.selection.bar_types)
            {
                return Err(bad("sealed.calibration_binding"));
            }
            Ok(())
        }
        _ => Err(bad("sealed.calibration_target_kind")),
    }
}

pub fn binding(
    input: &NativeAlphaSealedRequestV1,
    calibration: Option<&NativeFrozenCalibrationV1>,
    value: &NativeAlphaSealedResultV1,
) -> Result<(), DomainError> {
    request(input, calibration)?;
    forecast::binding(&input.forecast, &value.forecast)?;
    if value.expected_returns.len() != value.forecast.points.len()
        || value.assets.len() != input.forecast.selection.bar_types.len()
        || value.calibration_source_report_artifact_id
            != calibration.map(|m| m.source_report_artifact_id)
        || value.calibration_fit_end_available_ns != calibration.map(|m| m.fit_end_available_ns)
        || value.native_versions
            != BTreeMap::from([
                ("ndarray".into(), "0.17.1".into()),
                ("ndarray-stats".into(), "0.7.0".into()),
            ])
    {
        return Err(bad("sealed.output_binding"));
    }
    let mut offset = 0;
    for (index, points) in value
        .forecast
        .points
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
        .enumerate()
    {
        let asset = &value.assets[index];
        let returns = &value.expected_returns[offset..offset + points.len()];
        offset += points.len();
        if asset.instrument_id != points[0].instrument_id
            || asset.bar_type != input.forecast.selection.bar_types[index]
            || asset.metrics.len() != 2
        {
            return Err(bad("sealed.asset_binding"));
        }
        for (point, expected) in points.iter().zip(returns) {
            if expected.is_some() != point.forecast.is_some()
                || expected.is_some_and(|v| !v.is_finite())
                || point.forecast.is_some()
                    && point.available_ns <= input.research_available_through_ns
                || input.target_kind == TargetKind::ExpectedReturn && *expected != point.forecast
            {
                return Err(bad("sealed.prediction"));
            }
        }
        let labelled = points
            .iter()
            .filter(|p| p.label_return.is_some())
            .collect::<Vec<_>>();
        if asset.observation_count.get() != labelled.len() as u64 {
            return Err(bad("sealed.observation_count"));
        }
        let variation = labelled.len() >= 2
            && labelled.iter().any(|p| p.forecast != labelled[0].forecast)
            && labelled
                .iter()
                .any(|p| p.label_return != labelled[0].label_return);
        for (m, kind) in asset.metrics.iter().zip([
            NativeAlphaMetricKind::PearsonIc,
            NativeAlphaMetricKind::ReturnRmse,
        ]) {
            if m.kind != kind {
                return Err(bad("sealed.metric_kind"));
            }
            if labelled.is_empty() {
                if m.status != MetricStatus::InsufficientData
                    || m.value.is_some()
                    || m.reason_code.as_deref() != Some("NO_COMPLETE_LABELS")
                {
                    return Err(bad("sealed.missing_labels"));
                }
                continue;
            }
            if kind == NativeAlphaMetricKind::PearsonIc
                && variation == (m.status == MetricStatus::InsufficientData)
            {
                return Err(bad("sealed.correlation_variation"));
            }
            match (m.status, m.value, m.reason_code.as_deref()) {
                (MetricStatus::Ok, Some(v), None)
                    if v.is_finite() && (kind != NativeAlphaMetricKind::ReturnRmse || v >= 0.0) => {
                }
                (MetricStatus::InsufficientData, None, Some("CORRELATION_VARIATION_REQUIRED"))
                    if kind == NativeAlphaMetricKind::PearsonIc && !variation => {}
                (MetricStatus::Failed, None, Some("NATIVE_METRIC_NONFINITE")) => {}
                _ => return Err(bad("sealed.metric")),
            }
        }
    }
    Ok(())
}
