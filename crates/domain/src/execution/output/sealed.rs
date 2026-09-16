//! Exact held-out associations. No refitting or recalculation of native metrics.
use super::{bad, forecast, validation};
use crate::DomainError;
use contracts::{
    brief::TargetKind,
    evidence::{MetricRequirementV1, MetricStatus, MetricValueV1},
    science::*,
    Id, SchemaV1,
};
use std::collections::BTreeMap;

pub fn policy(
    requirements: &[MetricRequirementV1],
    bars: &NativeBarSelectionV1,
    horizon: u32,
) -> Result<(), DomainError> {
    let invalid = || {
        crate::research::invalid(
            "sealed_metric_requirements",
            "NATIVE_ALPHA_POLICY_UNSUPPORTED",
        )
    };
    let instruments = super::instruments(bars)?;
    if !(1..=64).contains(&requirements.len())
        || !(1..=100_000).contains(&horizon)
        || !requirements.iter().any(|r| r.required)
    {
        return Err(invalid());
    }
    for r in requirements.iter().filter(|r| r.required) {
        let index = r
            .scope
            .strip_prefix("asset:")
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(invalid)?;
        let instrument = instruments.get(index).ok_or_else(invalid)?;
        let (_, method, _) = [
            NativeAlphaMetricKind::PearsonIc,
            NativeAlphaMetricKind::ReturnRmse,
        ]
        .into_iter()
        .map(validation::method)
        .find(|(code, _, _)| *code == r.metric_code)
        .ok_or_else(invalid)?;
        if r.scope != format!("asset:{index}") || !r.method_allowlist.iter().any(|m| m == method) {
            return Err(invalid());
        }
        let frequency = format!(
            "{};horizon={horizon}",
            &bars.bar_types[index][instrument.len() + 1..]
        );
        crate::control::text(&frequency, 1, 120, false).map_err(|_| invalid())?;
    }
    Ok(())
}

/// Convert native values without recalculating them or inventing a validation fold.
pub fn metrics(
    evaluation: Id,
    artifact: Id,
    input: &NativeAlphaSealedRequestV1,
    calibration: Option<&NativeFrozenCalibrationV1>,
    value: &NativeAlphaSealedResultV1,
) -> Result<(Vec<MetricValueV1>, Vec<crate::evidence::MetricCapability>), DomainError> {
    binding(input, calibration, value)?;
    let mut records = Vec::with_capacity(value.assets.len() * 2);
    let mut capabilities = BTreeMap::new();
    for (index, (asset, points)) in value
        .assets
        .iter()
        .zip(
            value
                .forecast
                .points
                .chunk_by(|a, b| a.instrument_id == b.instrument_id),
        )
        .enumerate()
    {
        let first = points
            .iter()
            .find(|p| p.label_return.is_some())
            .unwrap_or(&points[0]);
        let last = points
            .iter()
            .rfind(|p| p.label_return.is_some())
            .unwrap_or(points.last().unwrap());
        let period_start =
            chrono::DateTime::from_timestamp_micros((first.event_ns.get() / 1000) as i64)
                .ok_or_else(|| bad("sealed.metric_period"))?;
        let period_end = chrono::DateTime::from_timestamp_micros(
            last.label_available_ns
                .unwrap_or(last.available_ns)
                .get()
                .div_ceil(1000) as i64,
        )
        .ok_or_else(|| bad("sealed.metric_period"))?;
        let frequency = format!(
            "{};horizon={}",
            &asset.bar_type[asset.instrument_id.len() + 1..],
            input.forecast.parameters.label_horizon_observations
        );
        crate::control::text(&frequency, 1, 120, false)?;
        for native in &asset.metrics {
            let (code, method, unit) = validation::method(native.kind);
            let capability = crate::evidence::MetricCapability {
                metric_code: code.into(),
                method_id: method.into(),
                method_version: "0.7.0".into(),
                unit: unit.into(),
                frequency: frequency.clone(),
            };
            capabilities.insert((code, frequency.clone()), capability);
            let record = MetricValueV1 {
                schema_version: SchemaV1,
                evaluation_id: evaluation,
                metric_code: code.into(),
                scope: format!("asset:{index}"),
                value: native.value,
                status: native.status,
                reason_code: native.reason_code.clone(),
                unit: unit.into(),
                period_start,
                period_end,
                observation_count: asset.observation_count,
                frequency: frequency.clone(),
                annualization_factor: None,
                method_id: method.into(),
                method_version: "0.7.0".into(),
                source_artifact_id: artifact,
                higher_is_better: Some(native.kind == NativeAlphaMetricKind::PearsonIc),
            };
            crate::evidence::validate_metric(&record)?;
            records.push(record);
        }
    }
    Ok((records, capabilities.into_values().collect()))
}

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

pub(super) fn shape(value: &NativeAlphaSealedResultV1) -> Result<(), DomainError> {
    forecast::shape(&value.forecast)?;
    if value.expected_returns.len() != value.forecast.points.len()
        || value.assets.len()
            != value
                .forecast
                .points
                .chunk_by(|a, b| a.instrument_id == b.instrument_id)
                .count()
        || value.calibration_source_report_artifact_id.is_some()
            != value.calibration_fit_end_available_ns.is_some()
    {
        return Err(bad("sealed.output_shape"));
    }
    for (point, expected) in value.forecast.points.iter().zip(&value.expected_returns) {
        if expected.is_some() != point.forecast.is_some()
            || expected.is_some_and(|v| !v.is_finite())
        {
            return Err(bad("sealed.return_shape"));
        }
    }
    for asset in &value.assets {
        crate::control::text(&asset.bar_type, 1, 300, false)?;
        crate::control::text(&asset.instrument_id, 1, 200, false)?;
        if asset.metrics.len() != 2 {
            return Err(bad("sealed.metric_shape"));
        }
        for metric in &asset.metrics {
            if metric.value.is_some_and(|v| !v.is_finite())
                || (metric.status == MetricStatus::Ok) != metric.value.is_some()
                || (metric.status == MetricStatus::Ok) == metric.reason_code.is_some()
            {
                return Err(bad("sealed.metric_shape"));
            }
        }
    }
    Ok(())
}

pub fn binding(
    input: &NativeAlphaSealedRequestV1,
    calibration: Option<&NativeFrozenCalibrationV1>,
    value: &NativeAlphaSealedResultV1,
) -> Result<(), DomainError> {
    request(input, calibration)?;
    shape(value)?;
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
