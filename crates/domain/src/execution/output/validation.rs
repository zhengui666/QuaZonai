//! Exact native fold membership, not a second splitter, estimator or PIT attestation.
use super::{bad, instruments};
use crate::{
    execution::validation::{
        validation_folds, MAX_VALIDATION_FOLDS, MAX_VALIDATION_INDICES, MAX_VALIDATION_ROWS,
    },
    DomainError,
};
use contracts::{
    brief::TargetKind,
    evidence::{MetricStatus, MetricValueV1},
    science::*,
    DbCounter, Id, SchemaV1,
};
use std::collections::{BTreeMap, BTreeSet};

fn method(kind: NativeAlphaMetricKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        NativeAlphaMetricKind::PearsonIc => (
            "PEARSON_IC",
            "ndarray-stats.pearson_correlation",
            "CORRELATION",
        ),
        NativeAlphaMetricKind::ReturnRmse => (
            "RETURN_RMSE",
            "ndarray-stats.root_mean_sq_err",
            "RETURN_PER_HORIZON",
        ),
    }
}

/// Match frozen policy intent to the registered native catalog and implemented
/// methods. No raw market rows, fitting, sample sufficiency or qualification.
pub fn policy(
    selection: &contracts::research::SelectionRuleV1,
    requirements: &[contracts::evidence::MetricRequirementV1],
    bars: &NativeBarSelectionV1,
    split: &contracts::research::SplitPolicyV1,
) -> Result<(), DomainError> {
    let invalid = || {
        crate::research::invalid(
            "content.evaluation_policy_id",
            "NATIVE_ALPHA_POLICY_UNSUPPORTED",
        )
    };
    let horizon = split.label_horizon_observations.ok_or_else(invalid)?.get();
    crate::execution::validation::policy_parameters(split, horizon)?;
    let assets = instruments(bars)?;
    let capability = |code: &str, scope: &str| {
        let (asset, fold) = scope.strip_prefix("asset:")?.split_once("/fold:")?;
        let index = asset.parse::<usize>().ok()?;
        let fold = fold.parse::<usize>().ok()?;
        if scope != format!("asset:{index}/fold:{fold}") || fold >= MAX_VALIDATION_FOLDS {
            return None;
        }
        let instrument = assets.get(index)?;
        let spec = &bars.bar_types[index][instrument.len() + 1..];
        [
            NativeAlphaMetricKind::PearsonIc,
            NativeAlphaMetricKind::ReturnRmse,
        ]
        .into_iter()
        .map(method)
        .find(|(name, _, _)| *name == code)
        .map(|(name, method, unit)| crate::evidence::MetricCapability {
            metric_code: name.into(),
            method_id: method.into(),
            method_version: "0.7.0".into(),
            unit: unit.into(),
            frequency: format!("{spec};horizon={horizon}"),
        })
    };
    let selected =
        capability(&selection.metric_code, &selection.metric_scope).ok_or_else(invalid)?;
    if selection.evaluation_kind != contracts::research::SelectionEvaluationKind::WalkForward
        || selection.method_id != selected.method_id
        || selection.method_version != selected.method_version
        || selection.unit != selected.unit
        || selection.frequency != selected.frequency
        || !requirements.iter().any(|r| {
            r.required
                && r.metric_code == selection.metric_code
                && r.scope == selection.metric_scope
        })
    {
        return Err(invalid());
    }
    for requirement in requirements.iter().filter(|r| r.required) {
        let actual =
            capability(&requirement.metric_code, &requirement.scope).ok_or_else(invalid)?;
        if !requirement.method_allowlist.contains(&actual.method_id) {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn shape(value: &NativeAlphaValidationResultV1) -> Result<(), DomainError> {
    let versions = BTreeMap::from([
        ("nautilus-indicators".into(), "0.63.0".into()),
        ("nautilus-persistence".into(), "0.63.0".into()),
        ("wasmi".into(), "2.0.0".into()),
        ("solow-cv".into(), "0.7.3".into()),
        ("linregress".into(), "0.5.4".into()),
        ("ndarray-stats".into(), "0.7.0".into()),
    ]);
    if value.native_versions != versions
        || !(1..=MAX_VALIDATION_FOLDS).contains(&value.folds.len())
        || !(1..=1_000_000_000).contains(&value.consumed_fuel.get())
    {
        return Err(bad("native_output.validation"));
    }
    let mut assets = BTreeSet::new();
    let mut prior: Option<&NativeValidationFoldV1> = None;
    let mut observations = BTreeMap::new();
    let mut rows = 0_u64;
    let mut indices = 0_usize;
    for fold in &value.folds {
        crate::control::text(&fold.instrument_id, 1, 200, false)?;
        crate::control::text(&fold.bar_type, 1, 300, false)?;
        if !(1..=MAX_VALIDATION_ROWS as u64).contains(&fold.source_row_count.get())
            || !(3..=MAX_VALIDATION_ROWS).contains(&fold.training_ordinals.len())
            || !(1..=MAX_VALIDATION_ROWS).contains(&fold.test_points.len())
        {
            return Err(bad("native_output.validation_counts"));
        }
        match prior.filter(|p| p.instrument_id == fold.instrument_id) {
            Some(p)
                if fold.fold_index == p.fold_index + 1
                    && fold.bar_type == p.bar_type
                    && fold.source_row_count == p.source_row_count => {}
            None if fold.fold_index == 0 && assets.insert(fold.instrument_id.as_str()) => {
                rows += fold.source_row_count.get();
            }
            _ => return Err(bad("native_output.validation_order")),
        }
        indices += fold.training_ordinals.len() + fold.test_points.len();
        if rows > MAX_VALIDATION_ROWS as u64
            || assets.len() > 256
            || indices > MAX_VALIDATION_INDICES
            || fold
                .training_ordinals
                .iter()
                .any(|n| u64::from(*n) >= fold.source_row_count.get())
            || fold.training_ordinals.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(bad("native_output.validation_indices"));
        }
        let calibrated = match &fold.calibration {
            None => true,
            Some(calibration) => {
                if calibration.training_observations.get() != fold.training_ordinals.len() as u64 {
                    return Err(bad("native_output.calibration_count"));
                }
                match (
                    calibration.status,
                    calibration.intercept,
                    calibration.slope,
                    calibration.reason_code.as_deref(),
                ) {
                    (MetricStatus::Ok, Some(a), Some(b), None)
                        if a.is_finite() && b.is_finite() =>
                    {
                        true
                    }
                    (
                        MetricStatus::InsufficientData,
                        None,
                        None,
                        Some("CALIBRATION_CONSTANT_SCORE"),
                    )
                    | (MetricStatus::Failed, None, None, Some("CALIBRATION_FIT_UNAVAILABLE")) => {
                        false
                    }
                    _ => return Err(bad("native_output.calibration")),
                }
            }
        };
        let mut previous: Option<&NativeForecastPointV1> = None;
        for point in &fold.test_points {
            let original = &point.observation;
            if original.instrument_id != fold.instrument_id
                || u64::from(original.ordinal) >= fold.source_row_count.get()
                || original.event_ns > original.available_ns
                || original.forecast.is_none_or(|x| !x.is_finite())
                || original.forecast_reason.is_some()
                || original.label_return.is_none_or(|x| !x.is_finite())
                || original.label_reason.is_some()
                || original
                    .label_available_ns
                    .is_none_or(|n| n <= original.available_ns)
                || point.expected_return.is_some() != calibrated
                || point.expected_return.is_some_and(|x| !x.is_finite())
                || fold
                    .training_ordinals
                    .binary_search(&original.ordinal)
                    .is_ok()
                || previous.is_some_and(|p| {
                    original.ordinal <= p.ordinal
                        || original.event_ns <= p.event_ns
                        || original.available_ns <= p.available_ns
                        || original.label_available_ns <= p.label_available_ns
                })
            {
                return Err(bad("native_output.validation_point"));
            }
            // Scores/calibration can differ by independently reset fold. Immutable
            // source times and labels for the same asset/row cannot.
            let identity = (
                original.event_ns,
                original.available_ns,
                original.label_available_ns,
                original.label_return,
            );
            if observations
                .insert((assets.len(), original.ordinal), identity)
                .is_some_and(|old| old != identity)
            {
                return Err(bad("native_output.validation_source_changed"));
            }
            previous = Some(original);
        }
        if fold.metrics.len() != 2 {
            return Err(bad("native_output.validation_metrics"));
        }
        for (metric, kind) in fold.metrics.iter().zip([
            NativeAlphaMetricKind::PearsonIc,
            NativeAlphaMetricKind::ReturnRmse,
        ]) {
            if metric.kind != kind {
                return Err(bad("native_output.validation_metric_kind"));
            }
            if kind == NativeAlphaMetricKind::PearsonIc {
                let first = &fold.test_points[0].observation;
                let variation = fold.test_points.len() >= 2
                    && fold
                        .test_points
                        .iter()
                        .any(|p| p.observation.forecast != first.forecast)
                    && fold
                        .test_points
                        .iter()
                        .any(|p| p.observation.label_return != first.label_return);
                if variation == (metric.status == MetricStatus::InsufficientData) {
                    return Err(bad("native_output.validation_metric_variation"));
                }
            }
            match (metric.status, metric.value, metric.reason_code.as_deref()) {
                (MetricStatus::Ok, Some(n), None)
                    if n.is_finite() && (kind != NativeAlphaMetricKind::ReturnRmse || n >= 0.0) => {
                }
                (MetricStatus::InsufficientData, None, Some("CORRELATION_VARIATION_REQUIRED"))
                    if kind == NativeAlphaMetricKind::PearsonIc => {}
                (MetricStatus::InsufficientData, None, Some("CALIBRATION_UNAVAILABLE"))
                    if kind == NativeAlphaMetricKind::ReturnRmse && !calibrated => {}
                (MetricStatus::Failed, None, Some("NATIVE_METRIC_NONFINITE")) => {}
                _ => return Err(bad("native_output.validation_metric")),
            }
            if !calibrated
                && kind == NativeAlphaMetricKind::ReturnRmse
                && metric.status != MetricStatus::InsufficientData
            {
                return Err(bad("native_output.validation_uncalibrated_metric"));
            }
        }
        prior = Some(fold);
    }
    if value.unique_test_observations.get() != observations.len() as u64 {
        return Err(bad("native_output.validation_unique_count"));
    }
    Ok(())
}

/// Convert only the original validated native outputs. Returned capabilities are
/// assigned here, not accepted from a caller or inferred from an arbitrary metric.
pub fn metrics(
    evaluation: Id,
    artifact: Id,
    request: &NativeAlphaValidationRequestV1,
    value: &NativeAlphaValidationResultV1,
) -> Result<(Vec<MetricValueV1>, Vec<crate::evidence::MetricCapability>), DomainError> {
    binding(request, value)?;
    let mut records = Vec::with_capacity(value.folds.len() * 2);
    let mut capabilities = BTreeMap::new();
    for (asset, folds) in value
        .folds
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
        .enumerate()
    {
        let first = &folds[0];
        // binding already checked the complete canonical instrument/BarType pair.
        let spec = &first.bar_type[first.instrument_id.len() + 1..];
        let frequency = format!(
            "{spec};horizon={}",
            request.forecast.parameters.label_horizon_observations
        );
        crate::control::text(&frequency, 1, 120, false)?;
        for fold in folds {
            let start = fold.test_points[0].observation.event_ns.get() / 1000;
            let end = fold
                .test_points
                .last()
                .unwrap()
                .observation
                .label_available_ns
                .ok_or_else(|| bad("native_output.validation_period"))?
                .get()
                .div_ceil(1000);
            let period_start = chrono::DateTime::from_timestamp_micros(start as i64)
                .ok_or_else(|| bad("native_output.validation_period"))?;
            let period_end = chrono::DateTime::from_timestamp_micros(end as i64)
                .ok_or_else(|| bad("native_output.validation_period"))?;
            for native in &fold.metrics {
                let (code, method, unit) = method(native.kind);
                let higher = native.kind == NativeAlphaMetricKind::PearsonIc;
                let observations = if higher {
                    fold.test_points.len()
                } else {
                    fold.test_points
                        .iter()
                        .filter(|p| p.expected_return.is_some())
                        .count()
                };
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
                    scope: format!("asset:{asset}/fold:{}", fold.fold_index),
                    value: native.value,
                    status: native.status,
                    reason_code: native.reason_code.clone(),
                    unit: unit.into(),
                    period_start,
                    period_end,
                    observation_count: DbCounter::new(observations as u64)
                        .map_err(|_| bad("native_output.validation_counts"))?,
                    frequency: frequency.clone(),
                    annualization_factor: None,
                    method_id: method.into(),
                    method_version: "0.7.0".into(),
                    source_artifact_id: artifact,
                    higher_is_better: Some(higher),
                };
                crate::evidence::validate_metric(&record)?;
                records.push(record);
            }
        }
    }
    Ok((records, capabilities.into_values().collect()))
}

pub(super) fn binding(
    request: &NativeAlphaValidationRequestV1,
    value: &NativeAlphaValidationResultV1,
) -> Result<(), DomainError> {
    crate::execution::alpha_validation_request(request)?;
    shape(value)?;
    let selected = instruments(&request.forecast.selection)?;
    let groups = value
        .folds
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
        .collect::<Vec<_>>();
    if groups.len() != selected.len()
        || value.consumed_fuel > request.forecast.parameters.total_fuel
    {
        return Err(bad("native_output.validation_input"));
    }
    let selection = &request.forecast.selection;
    let warmup = request.forecast.parameters.slow_period as usize - 1;
    let horizon = request.forecast.parameters.label_horizon_observations as usize;
    let mut source_rows = 0_u64;
    for (index, group) in groups.iter().enumerate() {
        let first = &group[0];
        source_rows += first.source_row_count.get();
        if first.instrument_id != selected[index]
            || first.bar_type != selection.bar_types[index]
            || source_rows > u64::from(selection.maximum_rows)
        {
            return Err(bad("native_output.validation_selection"));
        }
        let rows = (first.source_row_count.get() as usize)
            .checked_sub(warmup + horizon)
            .ok_or_else(|| bad("native_output.validation_source_rows"))?;
        let expected = validation_folds(&request.split_policy, rows)
            .map_err(|_| bad("native_output.validation_split"))?;
        if expected.len() != group.len() {
            return Err(bad("native_output.validation_missing_fold"));
        }
        for (fold, native) in group.iter().zip(expected) {
            if !fold
                .training_ordinals
                .iter()
                .map(|n| *n as usize)
                .eq(native.train.iter().map(|n| *n + warmup))
                || !fold
                    .test_points
                    .iter()
                    .map(|p| p.observation.ordinal as usize)
                    .eq(native.test.iter().map(|n| *n + warmup))
                || fold.calibration.is_some() != (request.target_kind == TargetKind::Score)
            {
                return Err(bad("native_output.validation_split_changed"));
            }
            for point in &fold.test_points {
                let p = &point.observation;
                if p.event_ns < selection.event_start_ns
                    || p.event_ns >= selection.event_end_ns
                    || p.available_ns > selection.decision_cutoff_ns
                    || p.label_available_ns
                        .is_some_and(|n| n > selection.decision_cutoff_ns)
                    || (request.target_kind == TargetKind::ExpectedReturn
                        && point.expected_return != p.forecast)
                {
                    return Err(bad("native_output.validation_cutoff_or_unit"));
                }
            }
        }
    }
    Ok(())
}
