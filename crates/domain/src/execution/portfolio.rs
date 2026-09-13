//! Bind catalog-derived numerical inputs to the frozen portfolio request.
use super::*;
use contracts::{brief::TargetKind, science::*};

pub fn portfolio_build_request(request: &NativePortfolioBuildRequestV1) -> Result<(), DomainError> {
    selection(&request.selection)?;
    crate::portfolio::mandate(&request.mandate)?;
    let weights = &request.current_weights;
    let cutoff = request.selection.decision_cutoff_ns;
    if weights.asof_ns > weights.available_ns
        || weights.available_ns > cutoff
        || weights.valid_until_ns <= cutoff
        || weights.asof_ns > cutoff
        || cutoff.get() - weights.asof_ns.get()
            > u64::from(request.mandate.rebalance_schedule.max_input_age_seconds) * 1_000_000_000
        || weights.base_currency != request.mandate.base_currency
        || weights.weights.len() != request.assets.len()
    {
        return Err(bad("portfolio.current_weights"));
    }
    if let PortfolioWeightsSourceV1::ForwardSnapshot {
        external_message_id,
        ..
    } = &weights.source
    {
        crate::control::text(external_message_id, 1, 200, false)?;
    }
    let mut total = weights.cash_weight.as_decimal().clone();
    let mut instruments = BTreeSet::new();
    for (weight, asset) in weights.weights.iter().zip(&request.assets) {
        if weight.instrument_id != asset.instrument_id
            || weight.currency != weights.base_currency
            || asset.currency != weights.base_currency
            || weight.weight != asset.current_weight
            || !instruments.insert(&weight.instrument_id)
        {
            return Err(bad("portfolio.current_weights"));
        }
        total += weight.weight.as_decimal();
    }
    if (total - bigdecimal::BigDecimal::from(1)).abs()
        > *request.mandate.exposure_tolerance.as_decimal()
    {
        return Err(bad("portfolio.current_weights"));
    }
    if !(2..=256).contains(&request.members.len())
        || request.assets.len() != request.selection.bar_types.len()
    {
        return Err(bad("portfolio.members"));
    }
    crate::portfolio::ensemble_weights(request.members.iter().map(|v| &v.ensemble_weight))?;
    let mut versions = BTreeSet::new();
    let mut alphas = BTreeSet::new();
    let horizon = request.members[0].parameters.label_horizon_observations;
    for member in &request.members {
        forecast_request(&NativeForecastRequestV1 {
            schema_version: contracts::SchemaV1,
            selection: request.selection.clone(),
            parameters: member.parameters.clone(),
        })?;
        if !versions.insert(member.alpha_version_id)
            || member.parameters.label_horizon_observations != horizon
            || (member.target_kind == TargetKind::Score) != member.calibration_artifact_id.is_some()
        {
            return Err(bad("portfolio.alpha_binding"));
        }
        if member.ensemble_weight.is_positive() {
            alphas.insert(member.alpha_id);
        }
    }
    if alphas.len() < 2 {
        return Err(bad("portfolio.distinct_alphas"));
    }
    Ok(())
}

pub fn portfolio_build_result(
    request: &NativePortfolioBuildRequestV1,
    result: &NativePortfolioBuildResultV1,
) -> Result<(), DomainError> {
    portfolio_build_request(request)?;
    let input = &result.input;
    let m = &request.mandate;
    let forecasts = &input.forecasts;
    let maximum_fuel = request
        .members
        .iter()
        .map(|m| m.parameters.total_fuel.get())
        .sum::<u64>();
    if input.objective != m.objective
        || input.risk != m.risk_measure
        || input.base_currency != m.base_currency
        || input.capital_assumption != m.capital_assumption
        || input.current_cash_weight != request.current_weights.cash_weight
        || input.exposure_tolerance != m.exposure_tolerance
        || input.constraints != m.constraints
        || input.optimizer != m.optimizer
        || input.alpha_ensemble != m.alpha_ensemble
        || input.covariance_estimator != m.covariance_estimator
        || input.assets != request.assets
        || forecasts.bar_types != request.selection.bar_types
        || forecasts.decision_asof_ns != request.selection.decision_cutoff_ns
        || forecasts.forecast_asof_ns < request.selection.event_start_ns
        || forecasts.forecast_asof_ns >= request.selection.event_end_ns
        || forecasts.max_input_age_seconds != m.rebalance_schedule.max_input_age_seconds
        || forecasts.members.len() != request.members.len()
        || result.consumed_fuel.get() > maximum_fuel
        || result.allocation.iterations
            > crate::portfolio::optimizer_settings(&m.optimizer)?.max_iterations
    {
        return Err(bad("portfolio.result_binding"));
    }
    for (actual, expected) in forecasts.members.iter().zip(&request.members) {
        if actual.alpha_id != expected.alpha_id
            || actual.alpha_version_id != expected.alpha_version_id
            || actual.ensemble_weight != expected.ensemble_weight
            || actual.horizon_value.get()
                != u64::from(expected.parameters.label_horizon_observations)
        {
            return Err(bad("portfolio.forecast_binding"));
        }
    }
    if input
        .return_history
        .end_ns
        .iter()
        .any(|t| *t < request.selection.event_start_ns || *t >= request.selection.event_end_ns)
    {
        return Err(bad("portfolio.return_window"));
    }
    crate::portfolio::allocation_result(input, &result.allocation)
}
