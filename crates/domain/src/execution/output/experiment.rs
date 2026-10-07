//! Correlate fresh target-policy folds with frozen inputs and native simulation.
//! This does not claim an external policy was trained without prior data access.
use super::{bad, simulation};
use crate::{
    execution::{experiment_request, features, validation::validation_folds},
    DomainError,
};
use bigdecimal::BigDecimal;
use contracts::{science::*, DbCounter, Id};
use std::collections::BTreeMap;

pub(super) fn shape(value: &NativeExperimentEvaluationResultV1) -> Result<(), DomainError> {
    experiment_request(&value.request)?;
    features::artifact_ids(&value.feature_artifact_ids)?;
    crate::control::text(&value.instrument_id, 1, 200, false)?;
    if value.native_versions
        != BTreeMap::from([
            ("nautilus-backtest".into(), "0.63.0".into()),
            ("solow-cv".into(), "0.7.3".into()),
            ("wasmi".into(), "2.0.0".into()),
        ])
        || value.source_row_count.get() == 0
        || value.source_row_count.get() > u64::from(MAX_EXPERIMENT_ROWS)
        || usize::from(value.feature_count) > MAX_EXPERIMENT_FEATURES
        || value.feature_count == 0
        || !(1..=MAX_EXPERIMENT_FOLDS).contains(&value.folds.len())
        || value.consumed_fuel.get() > 1_000_000_000
    {
        return Err(bad("experiment_result"));
    }
    let mut total = 0usize;
    for (index, fold) in value.folds.iter().enumerate() {
        total = total
            .checked_add(fold.decisions.len())
            .ok_or_else(|| bad("experiment_decisions"))?;
        if usize::from(fold.fold_index) != index
            || fold.decisions.is_empty()
            || total > MAX_EXPERIMENT_DECISIONS
            || fold.training_ordinals.len() < 3
            || fold.training_ordinals.len() > MAX_EXPERIMENT_ROWS as usize
            || fold.training_ordinals.windows(2).any(|w| w[0] >= w[1])
            || fold
                .training_ordinals
                .iter()
                .any(|n| u64::from(*n) >= value.source_row_count.get())
            || fold.decisions.windows(2).any(|w| {
                w[0].ordinal >= w[1].ordinal
                    || w[0].event_ns >= w[1].event_ns
                    || w[0].decision_ns > w[1].decision_ns
                    || w[0].label_end_ns >= w[1].label_end_ns
                    || w[0].label_available_ns > w[1].label_available_ns
            })
            || fold.training_end_available_ns >= fold.decisions[0].decision_ns
        {
            return Err(bad("experiment_fold"));
        }
        for point in &fold.decisions {
            if point.event_ns > point.decision_ns
                || u64::from(point.ordinal) >= value.source_row_count.get()
                || point.label_end_ns.is_none_or(|n| n <= point.event_ns)
                || point.label_available_ns.is_none_or(|n| {
                    n < point.label_end_ns.unwrap_or(DbCounter::ZERO) || n < point.decision_ns
                })
                || point.label_return.is_none_or(|n| !n.is_finite())
                || point.features.len() != usize::from(value.feature_count)
                || point.target_weight.as_decimal() < &BigDecimal::from(0)
                || point.target_weight.as_decimal() > &BigDecimal::from(1)
            {
                return Err(bad("experiment_decision"));
            }
            for feature in &point.features {
                if feature.value.is_some_and(|n| !n.is_finite())
                    || feature.value.is_some() == feature.missing_reason.is_some()
                {
                    return Err(bad("experiment_feature_value"));
                }
                if feature.missing_reason == Some(FeatureMissingReasonV1::NotYetAvailable) {
                    if feature.event_ns.is_some()
                        || feature.observed_available_ns.is_some()
                        || feature.effective_available_ns.is_some()
                        || feature.sequence.is_some()
                    {
                        return Err(bad("experiment_feature_unavailable"));
                    }
                } else if feature.event_ns.is_none()
                    || feature.effective_available_ns.is_none()
                    || feature.sequence.is_none()
                    || feature.event_ns > feature.effective_available_ns
                    || feature
                        .effective_available_ns
                        .is_some_and(|n| n > point.decision_ns)
                    || matches!((feature.event_ns, feature.observed_available_ns), (Some(e), Some(a)) if a < e)
                {
                    return Err(bad("experiment_feature_clock"));
                }
            }
        }
        simulation::binding(&fold.simulation_request, &fold.simulation)?;
        super::spot_cash_report::dataset(&fold.simulation, value.dataset_revision_id)?;
    }
    Ok(())
}

pub fn binding(
    request: &NativeExperimentEvaluationRequestV1,
    dataset_revision_id: Id,
    model_artifact_id: Id,
    feature_artifact_ids: &[Id],
    value: &NativeExperimentEvaluationResultV1,
) -> Result<(), DomainError> {
    experiment_request(request)?;
    shape(value)?;
    features::artifact_ids(feature_artifact_ids)?;
    if value.dataset_revision_id != dataset_revision_id
        || value.model_artifact_id != model_artifact_id
        || serde_json::to_value(&value.request).map_err(|_| bad("experiment_result.request"))?
            != serde_json::to_value(request).map_err(|_| bad("experiment_result.request"))?
        || value.feature_artifact_ids != feature_artifact_ids
        || value.instrument_id != request.instrument_id
        || usize::from(value.feature_count) != request.feature_schema.len()
        || value.consumed_fuel > request.total_fuel
        || value.source_row_count.get() > u64::from(request.selection.maximum_rows)
    {
        return Err(bad("experiment_result.binding"));
    }
    let eligible = value
        .source_row_count
        .get()
        .checked_sub(u64::from(request.label_horizon_observations))
        .ok_or_else(|| bad("experiment_fold.rows"))?;
    let native = validation_folds(&request.split_policy, eligible as usize)
        .map_err(|_| bad("experiment_fold.split"))?;
    if native.len() != value.folds.len() {
        return Err(bad("experiment_fold.count"));
    }
    // A source ordinal names one actual bar clock, even when it appears as a
    // decision in one fold and as another point's completed label. Tail labels
    // keep their reported source clocks; no seconds-per-row approximation exists.
    let mut source_clocks = BTreeMap::new();
    let mut insert_clock = |ordinal, event, available| -> Result<(), DomainError> {
        if source_clocks
            .insert(ordinal, (event, available))
            .is_some_and(|old| old != (event, available))
        {
            return Err(bad("experiment_label.source_clock"));
        }
        Ok(())
    };
    for fold in &value.folds {
        for point in &fold.decisions {
            insert_clock(point.ordinal, point.event_ns, point.decision_ns)?;
            let label_ordinal = point
                .ordinal
                .checked_add(request.label_horizon_observations)
                .filter(|ordinal| u64::from(*ordinal) < value.source_row_count.get())
                .ok_or_else(|| bad("experiment_label.ordinal"))?;
            insert_clock(
                label_ordinal,
                point
                    .label_end_ns
                    .ok_or_else(|| bad("experiment_label.event"))?,
                point
                    .label_available_ns
                    .ok_or_else(|| bad("experiment_label.available"))?,
            )?;
        }
    }
    if source_clocks
        .values()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair[0].0 >= pair[1].0 || pair[0].1 > pair[1].1)
    {
        return Err(bad("experiment_label.source_order"));
    }
    for (fold, indices) in value.folds.iter().zip(native) {
        if fold
            .training_ordinals
            .iter()
            .map(|n| *n as usize)
            .collect::<Vec<_>>()
            != indices.train
            || fold
                .decisions
                .iter()
                .map(|n| n.ordinal as usize)
                .collect::<Vec<_>>()
                != indices.test
        {
            return Err(bad("experiment_fold.membership"));
        }
        let first = &fold.decisions[0];
        let last = fold
            .decisions
            .last()
            .ok_or_else(|| bad("experiment_fold"))?;
        let replay = &fold.simulation_request;
        let training_label = fold
            .training_ordinals
            .last()
            .and_then(|ordinal| ordinal.checked_add(request.label_horizon_observations));
        if training_label
            .and_then(|ordinal| source_clocks.get(&ordinal))
            .is_some_and(|(_, available)| *available != fold.training_end_available_ns)
            || fold.training_end_available_ns < request.selection.event_start_ns
            || !replay.settlements.is_empty()
            || replay.selection.bar_types != request.selection.bar_types
            || replay.selection.event_start_ns != first.event_ns
            || Some(replay.selection.event_end_ns.get())
                != last.label_end_ns.and_then(|n| n.get().checked_add(1))
            || Some(replay.selection.decision_cutoff_ns.get())
                != last
                    .label_available_ns
                    .map(|n| n.get().max(replay.selection.event_end_ns.get()))
            || replay.selection.event_start_ns < request.selection.event_start_ns
            || replay.selection.event_end_ns > request.selection.event_end_ns
            || replay.selection.decision_cutoff_ns > request.selection.decision_cutoff_ns
            || replay.selection.maximum_rows > request.selection.maximum_rows
            || serde_json::to_value(&replay.settings).ok()
                != serde_json::to_value(&request.settings).ok()
            || replay.target_points.len() != fold.decisions.len()
        {
            return Err(bad("experiment_fold.replay"));
        }
        for (point, target) in fold.decisions.iter().zip(&replay.target_points) {
            if point.event_ns < request.selection.event_start_ns
                || point.event_ns >= request.selection.event_end_ns
                || point.decision_ns > request.selection.decision_cutoff_ns
                || point
                    .label_end_ns
                    .is_none_or(|time| time >= request.selection.event_end_ns)
                || point
                    .label_available_ns
                    .is_none_or(|time| time > request.selection.decision_cutoff_ns)
                || point.event_ns < replay.selection.event_start_ns
                || point.event_ns >= replay.selection.event_end_ns
                || point.decision_ns > replay.selection.decision_cutoff_ns
                || point
                    .label_end_ns
                    .is_none_or(|time| time >= replay.selection.event_end_ns)
                || point
                    .label_available_ns
                    .is_none_or(|time| time > replay.selection.decision_cutoff_ns)
            {
                return Err(bad("experiment_decision.selection"));
            }
            let expiry = point
                .decision_ns
                .get()
                .checked_add(request.target_ttl_ns.get())
                .ok_or_else(|| bad("experiment_target.expiry"))?;
            if target.asof_ns != point.decision_ns
                || target.valid_until_ns.get() != expiry
                || expiry <= target.asof_ns.get()
                || target.targets.len() != 1
                || target.targets[0].instrument_id != request.instrument_id
                || target.targets[0].currency != request.settings.base_currency
                || target.targets[0].weight != point.target_weight
                || target.cash_weight.as_decimal()
                    != &(BigDecimal::from(1) - point.target_weight.as_decimal())
            {
                return Err(bad("experiment_target.binding"));
            }
            for (definition, feature) in request.feature_schema.iter().zip(&point.features) {
                if let (Some(event), Some(available)) =
                    (feature.event_ns, feature.effective_available_ns)
                {
                    let expected = match definition.availability {
                        FeatureAvailabilityV1::Observed => {
                            feature.observed_available_ns.map(|n| n.get())
                        }
                        FeatureAvailabilityV1::ModeledLag { lag_ns } => {
                            event.get().checked_add(lag_ns.get())
                        }
                    };
                    let expired = definition
                        .max_age_ns
                        .is_some_and(|age| point.decision_ns.get() - event.get() > age.get());
                    if expected != Some(available.get())
                        || expired
                            != (feature.missing_reason == Some(FeatureMissingReasonV1::Expired))
                    {
                        return Err(bad("experiment_feature.binding"));
                    }
                }
            }
        }
    }
    Ok(())
}
