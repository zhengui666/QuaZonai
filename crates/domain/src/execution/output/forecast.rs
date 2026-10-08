//! Causal forecast-output associations, without evaluating or fitting the model.
use super::{bad, instruments};
use crate::{DomainError, control::text};
use contracts::science::{ForecastMissingReason, NativeForecastRequestV1, NativeForecastResultV1};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn shape(value: &NativeForecastResultV1) -> Result<(), DomainError> {
    let expected = BTreeMap::from([
        ("nautilus-indicators".to_owned(), "0.63.0".to_owned()),
        ("nautilus-persistence".to_owned(), "0.63.0".to_owned()),
        ("wasmi".to_owned(), "2.0.0".to_owned()),
    ]);
    if value.native_versions != expected
        || value.points.len() < 1
        || value.consumed_fuel == Some(contracts::DbCounter::ZERO)
    {
        return Err(bad("native_output.forecast"));
    }
    let mut completed = BTreeSet::new();
    let mut previous = None::<&contracts::science::NativeForecastPointV1>;
    let mut had_prediction = false;
    for point in &value.points {
        text(&point.instrument_id, 1, 200, false)?;
        if point.event_ns > point.available_ns
            || point.forecast.is_some_and(|number| !number.is_finite())
            || point.label_return.is_some_and(|number| !number.is_finite())
        {
            return Err(bad("native_output.forecast_point"));
        }
        match previous.filter(|prior| prior.instrument_id == point.instrument_id) {
            Some(prior)
                if prior.ordinal.checked_add(1) == Some(point.ordinal)
                    && point.event_ns > prior.event_ns
                    && point.available_ns > prior.available_ns => {}
            None if point.ordinal == 0 && completed.insert(point.instrument_id.as_str()) => {}
            _ => return Err(bad("native_output.forecast_order")),
        }
        match (point.forecast, point.forecast_reason) {
            (None, Some(ForecastMissingReason::IndicatorWarmup)) => {
                if point.label_return.is_some()
                    || point.label_available_ns.is_some()
                    || point.label_reason != Some(ForecastMissingReason::IndicatorWarmup)
                {
                    return Err(bad("native_output.warmup_label"));
                }
            }
            (Some(_), None) => {
                had_prediction = true;
                match (
                    point.label_return,
                    point.label_available_ns,
                    point.label_reason,
                ) {
                    (Some(_), Some(available), None) if available > point.available_ns => {}
                    (None, None, Some(ForecastMissingReason::LabelNotComplete)) => {}
                    _ => return Err(bad("native_output.forecast_label")),
                }
            }
            _ => return Err(bad("native_output.forecast_missing_reason")),
        }
        previous = Some(point);
    }
    if !had_prediction {
        return Err(bad("native_output.no_predictions"));
    }
    Ok(())
}

pub(super) fn binding(
    request: &NativeForecastRequestV1,
    value: &NativeForecastResultV1,
) -> Result<(), DomainError> {
    shape(value)?;
    let selected = instruments(&request.selection)?;
    let parameters = &request.parameters;
    if parameters.fast_period == 0
        || parameters.slow_period <= parameters.fast_period
        || parameters.label_horizon_observations == 0
        || !crate::execution::fuel_within_budget(value.consumed_fuel, parameters.total_fuel.map(|fuel| u128::from(fuel.get())))
        || value.points.len() > request.selection.maximum_rows as usize
    {
        return Err(bad("native_output.forecast_parameters"));
    }
    let mut groups = BTreeMap::<&str, Vec<_>>::new();
    let mut observed_order = Vec::new();
    for point in &value.points {
        if point.event_ns < request.selection.event_start_ns
            || point.event_ns >= request.selection.event_end_ns
            || point.available_ns > request.selection.decision_cutoff_ns
            || point
                .label_available_ns
                .is_some_and(|time| time > request.selection.decision_cutoff_ns)
        {
            return Err(bad("native_output.forecast_cutoff"));
        }
        if !groups.contains_key(point.instrument_id.as_str()) {
            observed_order.push(point.instrument_id.as_str());
        }
        groups
            .entry(point.instrument_id.as_str())
            .or_default()
            .push(point);
    }
    if observed_order != selected {
        return Err(bad("native_output.forecast_instruments"));
    }
    for series in groups.values() {
        if series.len() < parameters.slow_period as usize {
            return Err(bad("native_output.forecast_warmup"));
        }
        for (ordinal, point) in series.iter().enumerate() {
            let predicted = ordinal > 0 && ordinal + 1 >= parameters.slow_period as usize;
            if point.forecast.is_some() != predicted {
                return Err(bad("native_output.forecast_warmup"));
            }
            let future = predicted
                .then(|| series.get(ordinal + parameters.label_horizon_observations as usize))
                .flatten();
            if point.label_return.is_some() != future.is_some()
                || point.label_available_ns != future.map(|point| point.available_ns)
            {
                return Err(bad("native_output.forecast_horizon"));
            }
        }
    }
    Ok(())
}
