//! Original rolling inputs and generated targets; never a qualification decision.
use super::{bad, simulation};
use crate::DomainError;
use contracts::{science::*, DbCounter};

pub fn shape(value: &NativePortfolioStudyResultV1) -> Result<(), DomainError> {
    if !(1..=256).contains(&value.frames.len()) || value.consumed_fuel == DbCounter::ZERO {
        return Err(bad("study.frames"));
    }
    for (index, frame) in value.frames.iter().enumerate() {
        crate::portfolio::allocation_result(&frame.input, &frame.allocation)?;
        if index + 1 < value.frames.len() && frame.allocation.targets.is_none() {
            return Err(bad("study.after_infeasible"));
        }
    }
    match (&value.simulation_request, &value.simulation) {
        (Some(request), Some(result))
            if value.frames.iter().all(|f| f.allocation.targets.is_some()) =>
        {
            if request.target_points.len() != value.frames.len() {
                return Err(bad("study.target_count"));
            }
            simulation::binding(request, result)
        }
        (None, None) if value.frames.last().unwrap().allocation.targets.is_none() => Ok(()),
        _ => Err(bad("study.simulation")),
    }
}

pub fn binding(
    request: &NativePortfolioStudyRequestV1,
    value: &NativePortfolioStudyResultV1,
) -> Result<(), DomainError> {
    shape(value)?;
    let cutoffs = crate::execution::portfolio_study_cutoffs(request)?;
    let maximum_fuel = request
        .members
        .iter()
        .map(|m| m.parameters.total_fuel.get() / cutoffs.len() as u64)
        .sum::<u64>()
        * cutoffs.len() as u64;
    if value.frames.len() > cutoffs.len()
        || value.consumed_fuel.get() > maximum_fuel
        || value.simulation.is_some() && value.frames.len() != cutoffs.len()
    {
        return Err(bad("study.execution_count"));
    }
    let mandate = &request.mandate;
    let ttl = u64::from(mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000;
    let mut return_values = 0_usize;
    for (index, frame) in value.frames.iter().enumerate() {
        let input = &frame.input;
        let forecasts = &input.forecasts;
        let cutoff = cutoffs[index];
        let mut selection = request.source_selection.clone();
        selection.event_end_ns = cutoff;
        selection.decision_cutoff_ns = cutoff;
        let source_assets = crate::execution::portfolio_study_liquidity_assets(
            request,
            cutoff,
            forecasts.forecast_asof_ns,
            forecasts.decision_asof_ns,
            &frame.bar_notionals,
        )?;
        let mut expected = crate::execution::portfolio_costs(
            &selection,
            mandate,
            &request.execution_settings,
            &source_assets,
            &frame.slippage_references,
        )?;
        if expected.len() != input.assets.len() {
            return Err(bad("study.assets"));
        }
        for (asset, actual) in expected.iter_mut().zip(&input.assets) {
            asset.current_weight = actual.current_weight.clone();
        }
        return_values = return_values
            .checked_add(input.return_history.end_ns.len() * input.assets.len())
            .ok_or_else(|| bad("study.input_limit"))?;
        if frame.cutoff_ns != cutoff
            || return_values > contracts::portfolio::MAX_RETURN_VALUES
            || input.objective != mandate.objective
            || input.risk != mandate.risk_measure
            || input.base_currency != mandate.base_currency
            || input.exposure_tolerance != mandate.exposure_tolerance
            || input.constraints != mandate.constraints
            || input.optimizer != mandate.optimizer
            || input.alpha_ensemble != mandate.alpha_ensemble
            || input.covariance_estimator != mandate.covariance_estimator
            || expected != input.assets
            || forecasts.decision_asof_ns < cutoff
            || forecasts.decision_asof_ns.get() >= cutoff.get() + ttl
            || cutoffs
                .get(index + 1)
                .is_some_and(|next| forecasts.decision_asof_ns >= *next)
            || forecasts.forecast_asof_ns < selection.event_start_ns
            || forecasts.forecast_asof_ns >= cutoff
            || forecasts.bar_types != selection.bar_types
            || forecasts.max_input_age_seconds != mandate.rebalance_schedule.max_input_age_seconds
            || forecasts.members.len() != request.members.len()
            || frame
                .slippage_references
                .iter()
                .any(|r| r.event_ns != forecasts.forecast_asof_ns)
            || input
                .return_history
                .end_ns
                .iter()
                .any(|t| *t < selection.event_start_ns || *t >= cutoff)
            || input
                .return_history
                .available_ns
                .iter()
                .any(|t| *t > cutoff)
        {
            return Err(bad("study.original_input"));
        }
        for (actual, original) in forecasts.members.iter().zip(&request.members) {
            if actual.alpha_id != original.alpha_id
                || actual.alpha_version_id != original.alpha_version_id
                || actual.ensemble_weight != original.ensemble_weight
                || actual.available_ns > cutoff
                || actual.horizon_value.get()
                    != u64::from(original.parameters.label_horizon_observations)
            {
                return Err(bad("study.original_model"));
            }
        }
        if index == 0
            && (input.capital_assumption != mandate.capital_assumption
                || input.current_cash_weight.as_decimal() != &1.into()
                || input
                    .assets
                    .iter()
                    .any(|a| a.current_weight.as_decimal() != &0.into()))
        {
            return Err(bad("study.initial_account"));
        }
        if let Some(replay) = &value.simulation_request {
            let point = &replay.target_points[index];
            if point.asof_ns != forecasts.decision_asof_ns
                || point.valid_until_ns.get() != cutoff.get() + ttl
                || Some(&point.targets) != frame.allocation.targets.as_ref()
                || Some(&point.cash_weight) != frame.allocation.cash_weight.as_ref()
            {
                return Err(bad("study.generated_target"));
            }
        }
    }
    if let Some(replay) = &value.simulation_request {
        let mut selected = request.source_selection.clone();
        selected.event_start_ns = request.evaluation_start_ns;
        if replay.settlements != request.settlements
            || replay.selection != selected
            || serde_json::to_value(&replay.settings).map_err(|_| bad("study.settings"))?
                != serde_json::to_value(&request.execution_settings)
                    .map_err(|_| bad("study.settings"))?
        {
            return Err(bad("study.simulation_source"));
        }
    }
    Ok(())
}
