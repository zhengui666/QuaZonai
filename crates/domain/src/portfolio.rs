//! Allocation ownership and publication bounds, independent of the numerical solver.
use crate::{control, DomainError};
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{portfolio::*, DecimalValue};
use std::collections::{BTreeMap, BTreeSet};
mod covariance;
pub use covariance::sample_covariance;
mod risk;
pub use risk::{cvar_confidence, cvar_tail_coefficient, risk_budgeting};
mod liquidity;
pub use liquidity::{
    bar_liquidity_age, bar_liquidity_assumption, bar_liquidity_values, rolling_liquidity_policy,
};

fn invalid() -> DomainError {
    DomainError::Invalid("portfolio_allocation")
}

pub fn build_selection(request: &PortfolioBuildRequestV1) -> Result<(), DomainError> {
    crate::data::bounded_native_limits(&request.limits)?;
    if !(2..=MAX_ALLOCATION_ASSETS).contains(&request.members.len()) {
        return Err(DomainError::Invalid("portfolio_members"));
    }
    let mut qualifications = BTreeSet::new();
    if request
        .members
        .iter()
        .any(|m| !qualifications.insert(m.qualification_id))
    {
        return Err(DomainError::Invalid("portfolio_duplicate_qualification"));
    }
    ensemble_weights(request.members.iter().map(|m| &m.ensemble_weight))
}

pub fn simulation_settings(
    settings: &contracts::science::NativeSimulationSettingsV1,
) -> Result<(), DomainError> {
    simulation_models(settings)?;
    if !settings.starting_capital.is_positive()
        || !settings.leverage.is_positive()
        || settings.leverage.as_decimal() > &BigDecimal::from(100)
        || !settings.exposure_tolerance.is_positive()
        || settings.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !(1..=86_400_000).contains(&settings.snapshot_interval_ms)
        || iso_currency::Currency::from_code(&settings.base_currency).is_none()
        || !(1..=256).contains(&settings.fee_rates.len())
        || (settings.account_kind == contracts::science::NativeAccountKind::Cash
            && settings.leverage.as_decimal() != &BigDecimal::from(1))
    {
        return Err(DomainError::Invalid("simulation_settings"));
    }
    let mut instruments = BTreeSet::new();
    for rate in &settings.fee_rates {
        control::text(&rate.instrument_id, 1, 200, false)?;
        if !instruments.insert(&rate.instrument_id) {
            return Err(DomainError::Invalid("simulation_fee_identity"));
        }
    }
    Ok(())
}

/// Freeze the actual native execution models; no fallback, RNG or fee algorithm here.
pub fn simulation_models(
    settings: &contracts::science::NativeSimulationSettingsV1,
) -> Result<(&NautilusFillParametersV1, &NautilusLatencyParametersV1), DomainError> {
    let unavailable = || DomainError::CapabilityUnavailable("native_simulation_models");
    let fill = match &settings.fill_model {
        NativeModelRefV1::NautilusDefaultFill {
            upstream_class,
            upstream_version,
            parameters,
            ..
        } if upstream_class == NAUTILUS_FILL_CLASS
            && upstream_version == NAUTILUS_EXECUTION_VERSION =>
        {
            parameters
        }
        _ => return Err(unavailable()),
    };
    match &settings.fee_model {
        NativeModelRefV1::NautilusMakerTaker {
            upstream_class,
            upstream_version,
            ..
        } if upstream_class == NAUTILUS_FEE_CLASS
            && upstream_version == NAUTILUS_EXECUTION_VERSION => {}
        _ => return Err(unavailable()),
    }
    let latency = match &settings.latency_model {
        NativeModelRefV1::NautilusStaticLatency {
            upstream_class,
            upstream_version,
            parameters,
            ..
        } if upstream_class == NAUTILUS_LATENCY_CLASS
            && upstream_version == NAUTILUS_EXECUTION_VERSION =>
        {
            parameters
        }
        _ => return Err(unavailable()),
    };
    if !fill.prob_fill_on_limit.is_fraction()
        || !fill.prob_slippage.is_fraction()
        || [
            latency.insert_latency_ns,
            latency.update_latency_ns,
            latency.cancel_latency_ns,
        ]
        .iter()
        .any(|value| {
            latency
                .base_latency_ns
                .get()
                .checked_add(value.get())
                .is_none()
        })
        || latency.base_latency_ns.get() + latency.insert_latency_ns.get() == 0
    {
        return Err(DomainError::Invalid("native_simulation_parameters"));
    }
    Ok((fill, latency))
}
fn ordered(low: &DecimalValue, high: &DecimalValue) -> Result<(), DomainError> {
    if low.as_decimal() > high.as_decimal() {
        return Err(invalid());
    }
    Ok(())
}
fn within(value: &BigDecimal, low: &BigDecimal, high: &BigDecimal, tolerance: &BigDecimal) -> bool {
    value >= &(low - tolerance) && value <= &(high + tolerance)
}
fn under(value: &BigDecimal, limit: &BigDecimal, tolerance: &BigDecimal) -> bool {
    value <= &(limit + tolerance)
}

/// Only validates supplied original metadata. Store must resolve these identities
/// against immutable evidence, current qualifications and data-use permissions.
pub fn portfolio_forecast_alignment(input: &PortfolioForecastInputV1) -> Result<(), DomainError> {
    let invalid = || DomainError::Invalid("portfolio_forecasts");
    let Some(age) = input
        .decision_asof_ns
        .get()
        .checked_sub(input.forecast_asof_ns.get())
    else {
        return Err(invalid());
    };
    if !(2..=MAX_ALLOCATION_ASSETS).contains(&input.members.len())
        || !(1..=MAX_ALLOCATION_ASSETS).contains(&input.instrument_ids.len())
        || input.max_input_age_seconds == 0
        || age > u64::from(input.max_input_age_seconds) * 1_000_000_000
        || input.horizon_value.get() == 0
        || input.bar_types.len() != input.instrument_ids.len()
        || iso_currency::Currency::from_code(&input.base_currency).is_none()
    {
        return Err(invalid());
    }
    if input.horizon_kind != contracts::brief::HorizonKind::FixedBars {
        return Err(DomainError::CapabilityUnavailable(
            "portfolio_forecast_horizon",
        ));
    }
    let mut instruments = BTreeSet::new();
    for id in &input.instrument_ids {
        control::text(id, 1, 200, false)?;
        if !instruments.insert(id) {
            return Err(invalid());
        }
    }
    let mut alphas = BTreeSet::new();
    let mut versions = BTreeSet::new();
    for member in &input.members {
        if member.ensemble_weight.is_positive() {
            alphas.insert(member.alpha_id);
        }
        if !versions.insert(member.alpha_version_id)
            || member.forecast_unit != contracts::evidence::ForecastUnit::ReturnPerHorizon
            || member.horizon_kind != input.horizon_kind
            || member.horizon_value != input.horizon_value
            || member.base_currency != input.base_currency
            || member.asof_ns != input.forecast_asof_ns
            || member.available_ns < member.asof_ns
            || member.available_ns > input.decision_asof_ns
            || member.instrument_ids != input.instrument_ids
            || member.bar_types != input.bar_types
            || member.forecasts.len() != input.instrument_ids.len()
            || member.forecasts.iter().any(|value| !value.is_finite())
        {
            return Err(invalid());
        }
    }
    if alphas.len() < 2 {
        return Err(invalid());
    }
    Ok(())
}

pub fn ensemble_weights<'a>(
    weights: impl Iterator<Item = &'a DecimalValue>,
) -> Result<(), DomainError> {
    let mut sum = BigDecimal::from(0);
    let mut count = 0;
    let mut positive = 0;
    for weight in weights {
        if !weight.is_nonnegative() {
            return Err(DomainError::Invalid("ensemble_weights"));
        }
        count += 1;
        positive += usize::from(weight.is_positive());
        sum += weight.as_decimal();
    }
    if !(2..=MAX_ALLOCATION_ASSETS).contains(&count) || positive < 2 || sum != BigDecimal::from(1) {
        return Err(DomainError::Invalid("ensemble_weights"));
    }
    Ok(())
}

/// Structural mandate checks shared with actual allocation and publication.
/// Asset/group membership and feasibility still require the frozen native input.
pub fn portfolio_constraints(constraints: &PortfolioConstraintsV1) -> Result<(), DomainError> {
    if constraints.group_bounds.len() > MAX_ALLOCATION_GROUPS
        || constraints.asset_overrides.len() > MAX_ALLOCATION_ASSETS
        || !constraints.max_gross_exposure.is_nonnegative()
        || !constraints.max_turnover_per_rebalance.is_nonnegative()
        || constraints
            .max_participation
            .as_ref()
            .is_some_and(|v| !v.is_positive() || !v.is_fraction())
        || constraints
            .max_ex_ante_risk
            .as_ref()
            .is_some_and(|v| !v.is_positive())
        || (constraints.max_participation.is_some() && constraints.liquidity_ref.is_none())
    {
        return Err(invalid());
    }
    ordered(&constraints.min_cash_weight, &constraints.max_cash_weight)?;
    ordered(&constraints.min_asset_weight, &constraints.max_asset_weight)?;
    ordered(&constraints.min_net_exposure, &constraints.max_net_exposure)?;
    if constraints.long_only && !constraints.min_asset_weight.is_nonnegative() {
        return Err(invalid());
    }
    let mut overrides = BTreeSet::new();
    for value in &constraints.asset_overrides {
        control::text(&value.instrument_id, 1, 200, false)?;
        if !overrides.insert(value.instrument_id.as_str())
            || (constraints.long_only && !value.min.is_nonnegative())
        {
            return Err(invalid());
        }
        ordered(&value.min, &value.max)?;
    }
    let mut groups = BTreeSet::new();
    for value in &constraints.group_bounds {
        control::text(&value.group_id, 1, 120, false)?;
        if !groups.insert(value.group_id.as_str()) {
            return Err(invalid());
        }
        ordered(&value.min, &value.max)?;
    }
    Ok(())
}

pub fn rebalance_schedule(schedule: &RebalanceScheduleV1) -> Result<(), DomainError> {
    let valid = match schedule.kind {
        RebalanceKind::Manual => {
            schedule.interval_seconds.is_none()
                && schedule.calendar_ref.is_none()
                && schedule.session_offset_seconds.is_none()
        }
        RebalanceKind::FixedInterval => {
            schedule.interval_seconds.is_some_and(|v| v > 0)
                && schedule.calendar_ref.is_none()
                && schedule.session_offset_seconds.is_none()
        }
        RebalanceKind::CalendarSession => {
            schedule.interval_seconds.is_none()
                && schedule.calendar_ref.is_some()
                && schedule.session_offset_seconds.is_some()
        }
    };
    control::text(&schedule.timezone, 1, 200, false)?;
    if !valid
        || schedule.max_input_age_seconds == 0
        || schedule.target_ttl_seconds == 0
        || schedule.timezone.parse::<chrono_tz::Tz>().is_err()
    {
        return Err(DomainError::Invalid("rebalance_schedule"));
    }
    if let Some(calendar) = &schedule.calendar_ref {
        control::text(calendar, 1, 200, false)?;
    }
    Ok(())
}

pub fn fixed_ensemble(model: &NativeModelRefV1) -> Result<(), DomainError> {
    match model {
        NativeModelRefV1::FixedWeightedForecast {
            upstream_class,
            upstream_version,
            ..
        } if upstream_class == FIXED_ENSEMBLE_CLASS
            && upstream_version == FIXED_ENSEMBLE_VERSION => {}
        _ => {
            return Err(DomainError::CapabilityUnavailable(
                "portfolio_ensemble_model",
            ))
        }
    }
    Ok(())
}

pub fn mandate(content: &MandateContentV1) -> Result<(), DomainError> {
    optimizer_settings(&content.optimizer)?;
    cvar_confidence(content.risk_measure, &content.optimizer)?;
    fixed_ensemble(&content.alpha_ensemble)?;
    sample_covariance_parameters(&content.covariance_estimator)?;
    portfolio_constraints(&content.constraints)?;
    rebalance_schedule(&content.rebalance_schedule)?;
    if !content.capital_assumption.is_positive()
        || !content.exposure_tolerance.is_positive()
        || content.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || iso_currency::Currency::from_code(&content.base_currency).is_none()
    {
        return Err(DomainError::Invalid("portfolio_mandate"));
    }
    risk_budgeting(content.objective, &content.optimizer, &content.constraints)?;
    Ok(())
}

/// Does not authorize an Alpha, infer a quote, or fit an estimator.
pub fn allocation_input(input: &AllocationInputV1) -> Result<(), DomainError> {
    optimizer_settings(&input.optimizer)?;
    cvar_confidence(input.risk, &input.optimizer)?;
    fixed_ensemble(&input.alpha_ensemble)?;
    sample_covariance_parameters(&input.covariance_estimator)?;
    portfolio_return_history(&input.return_history, &input.forecasts)?;
    let count = input.assets.len();
    let constraints = &input.constraints;
    if let Some(budget) = risk_budgeting(input.objective, &input.optimizer, constraints)? {
        if budget.assets.len() != input.assets.len()
            || budget.assets.iter().any(|b| {
                !input
                    .assets
                    .iter()
                    .any(|a| a.instrument_id == b.instrument_id)
            })
        {
            return Err(DomainError::Invalid("portfolio_risk_budget_assets"));
        }
    }
    portfolio_constraints(constraints)?;
    portfolio_forecast_alignment(&input.forecasts)?;
    ensemble_weights(
        input
            .forecasts
            .members
            .iter()
            .map(|member| &member.ensemble_weight),
    )?;
    if input.forecasts.base_currency != input.base_currency
        || input
            .assets
            .iter()
            .map(|asset| &asset.instrument_id)
            .ne(input.forecasts.instrument_ids.iter())
    {
        return Err(DomainError::Invalid("allocation_forecast_binding"));
    }
    if !(1..=MAX_ALLOCATION_ASSETS).contains(&count)
        || !input.capital_assumption.is_positive()
        || !input.exposure_tolerance.is_positive()
        || input.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || constraints.asset_overrides.len() > count
        || iso_currency::Currency::from_code(&input.base_currency).is_none()
    {
        return Err(invalid());
    }
    let mut identities = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let mut current_total = input.current_cash_weight.as_decimal().clone();
    for asset in &input.assets {
        control::text(&asset.instrument_id, 1, 200, false)?;
        if !identities.insert(asset.instrument_id.as_str())
            || asset.currency != input.base_currency
            || !asset.transaction_cost_rate.is_fraction()
            || asset.groups.len() > MAX_ALLOCATION_GROUPS
            || asset
                .available_notional
                .as_ref()
                .is_some_and(|v| !v.is_nonnegative())
            || (constraints.max_participation.is_some() && asset.available_notional.is_none())
        {
            return Err(invalid());
        }
        current_total += asset.current_weight.as_decimal();
        let mut membership = BTreeSet::new();
        for group in &asset.groups {
            control::text(group, 1, 120, false)?;
            if !membership.insert(group.as_str()) {
                return Err(invalid());
            }
            groups.insert(group.as_str());
        }
    }
    let tolerance = input.exposure_tolerance.as_decimal();
    if (current_total - BigDecimal::from(1)).abs() > *tolerance {
        return Err(DomainError::Invalid("current_weights_snapshot"));
    }
    for value in &constraints.asset_overrides {
        if !identities.contains(value.instrument_id.as_str()) {
            return Err(invalid());
        }
    }
    for value in &constraints.group_bounds {
        if !groups.contains(value.group_id.as_str()) {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Causal shape/alignment only; trusted artifact assembly must prove origin.
pub fn portfolio_return_history(
    history: &PortfolioReturnHistoryV1,
    forecasts: &PortfolioForecastInputV1,
) -> Result<(), DomainError> {
    let count = history.end_ns.len();
    if !(2..=MAX_RETURN_OBSERVATIONS).contains(&count)
        || history.available_ns.len() != count
        || history.instrument_ids != forecasts.instrument_ids
        || history.bar_types != forecasts.bar_types
        || history.base_currency != forecasts.base_currency
        || history.horizon_kind != forecasts.horizon_kind
        || history.horizon_value != forecasts.horizon_value
        || history.asset_returns.len() != history.instrument_ids.len()
        || !history
            .asset_returns
            .len()
            .checked_mul(count)
            .is_some_and(|size| size <= MAX_RETURN_VALUES)
        || history.asset_returns.iter().any(|row| {
            row.len() != count || row.iter().any(|value| !value.is_finite() || *value < -1.0)
        })
        || history
            .end_ns
            .windows(2)
            .any(|pair| pair[0].get() >= pair[1].get())
        || history
            .end_ns
            .iter()
            .zip(&history.available_ns)
            .any(|(end, available)| {
                end.get() == 0
                    || available.get() < end.get()
                    || available.get() > forecasts.decision_asof_ns.get()
            })
    {
        return Err(DomainError::Invalid("portfolio_return_history"));
    }
    Ok(())
}

pub fn optimizer_settings(model: &NativeModelRefV1) -> Result<&AllocatorSettingsV1, DomainError> {
    let NativeModelRefV1::ClarabelQp {
        upstream_class,
        upstream_version,
        parameters,
        ..
    } = model
    else {
        return Err(DomainError::CapabilityUnavailable(
            "portfolio_optimizer_model",
        ));
    };
    if upstream_class != CLARABEL_CLASS || upstream_version != CLARABEL_VERSION {
        return Err(DomainError::CapabilityUnavailable(
            "portfolio_optimizer_model",
        ));
    }
    if !parameters.risk_aversion.is_positive()
        || !parameters.solver_tolerance.is_positive()
        || parameters.solver_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !(1..=100_000).contains(&parameters.max_iterations)
    {
        return Err(DomainError::Invalid("portfolio_optimizer_parameters"));
    }
    Ok(parameters)
}

pub fn sample_covariance_parameters(
    model: &NativeModelRefV1,
) -> Result<&SampleCovarianceParametersV1, DomainError> {
    let NativeModelRefV1::SampleCovariance {
        upstream_class,
        upstream_version,
        parameters,
        ..
    } = model
    else {
        return Err(DomainError::CapabilityUnavailable(
            "portfolio_covariance_model",
        ));
    };
    if upstream_class != SAMPLE_COVARIANCE_CLASS || upstream_version != SAMPLE_COVARIANCE_VERSION {
        return Err(DomainError::CapabilityUnavailable(
            "portfolio_covariance_model",
        ));
    }
    if parameters.ddof != 1 {
        return Err(DomainError::CapabilityUnavailable(
            "portfolio_covariance_ddof",
        ));
    }
    Ok(parameters)
}

/// Validate the *stored decimal targets*, not just the solver's in-memory floats.
/// An inaccurate status is accepted only when the frozen numerical policy allows it.
pub fn allocation_result(
    input: &AllocationInputV1,
    result: &AllocationResultV1,
) -> Result<(), DomainError> {
    allocation_input(input)?;
    if result.iterations > optimizer_settings(&input.optimizer)?.max_iterations
        || result.objective_value.is_some_and(|v| !v.is_finite())
        || result
            .primal_residual
            .is_some_and(|v| !v.is_finite() || v < 0.0)
        || result
            .dual_residual
            .is_some_and(|v| !v.is_finite() || v < 0.0)
    {
        return Err(invalid());
    }
    let successful = matches!(
        result.solver_status,
        SolverStatus::Optimal | SolverStatus::AcceptableInaccurate
    );
    if !successful {
        if result.targets.is_some()
            || result.cash_weight.is_some()
            || result.cvar_risk_budget_witness.is_some()
            || result.reason_code.as_ref().is_none_or(|v| v.is_empty())
        {
            return Err(invalid());
        }
        return Ok(());
    }
    if result.cvar_risk_budget_witness.is_some()
        != (input.objective == AllocationObjective::RiskBudgeting
            && input.risk == AllocationRisk::Cvar)
    {
        return Err(DomainError::Invalid("allocation_cvar_risk_witness"));
    }
    if result.reason_code.is_some()
        || (result.solver_status == SolverStatus::AcceptableInaccurate
            && !optimizer_settings(&input.optimizer)?.accept_inaccurate)
        || result.objective_value.is_none()
        || result.primal_residual.is_none()
        || result.dual_residual.is_none()
    {
        return Err(invalid());
    }
    let targets = result.targets.as_ref().ok_or_else(invalid)?;
    let cash = result
        .cash_weight
        .as_ref()
        .ok_or_else(invalid)?
        .as_decimal();
    if targets.len() != input.assets.len() {
        return Err(invalid());
    }
    let constraints = &input.constraints;
    let tolerance = input.exposure_tolerance.as_decimal();
    let overrides: BTreeMap<_, _> = constraints
        .asset_overrides
        .iter()
        .map(|v| (v.instrument_id.as_str(), v))
        .collect();
    let mut net = BigDecimal::from(0);
    let mut gross = BigDecimal::from(0);
    let mut turnover = BigDecimal::from(0);
    let mut group_weights = BTreeMap::<&str, BigDecimal>::new();
    for (target, asset) in targets.iter().zip(&input.assets) {
        if target.instrument_id != asset.instrument_id || target.currency != asset.currency {
            return Err(invalid());
        }
        let weight = target.weight.as_decimal();
        let (low, high) = overrides
            .get(asset.instrument_id.as_str())
            .map(|v| (v.min.as_decimal(), v.max.as_decimal()))
            .unwrap_or((
                constraints.min_asset_weight.as_decimal(),
                constraints.max_asset_weight.as_decimal(),
            ));
        if !within(weight, low, high, tolerance) {
            return Err(DomainError::Invalid("asset_weight_bound"));
        }
        if constraints.long_only && weight < &(-tolerance) {
            return Err(DomainError::Invalid("long_only"));
        }
        let change = (weight - asset.current_weight.as_decimal()).abs();
        if let Some(participation) = &constraints.max_participation {
            let available = asset.available_notional.as_ref().ok_or_else(invalid)?;
            if !under(
                &(&change * input.capital_assumption.as_decimal()),
                &(available.as_decimal() * participation.as_decimal()),
                &(tolerance * input.capital_assumption.as_decimal()),
            ) {
                return Err(DomainError::Invalid("participation_bound"));
            }
        }
        net += weight;
        gross += weight.abs();
        turnover += change;
        for group in &asset.groups {
            *group_weights
                .entry(group)
                .or_insert_with(|| BigDecimal::from(0)) += weight;
        }
    }
    if (&net + cash - BigDecimal::from(1)).abs() > *tolerance
        || !within(
            cash,
            constraints.min_cash_weight.as_decimal(),
            constraints.max_cash_weight.as_decimal(),
            tolerance,
        )
        || !within(
            &net,
            constraints.min_net_exposure.as_decimal(),
            constraints.max_net_exposure.as_decimal(),
            tolerance,
        )
        || !under(
            &gross,
            constraints.max_gross_exposure.as_decimal(),
            tolerance,
        )
        || !under(
            &turnover,
            constraints.max_turnover_per_rebalance.as_decimal(),
            tolerance,
        )
    {
        return Err(DomainError::Invalid("portfolio_exposure_bound"));
    }
    for group in &constraints.group_bounds {
        let total = group_weights
            .get(group.group_id.as_str())
            .ok_or_else(invalid)?;
        if !within(
            total,
            group.min.as_decimal(),
            group.max.as_decimal(),
            tolerance,
        ) {
            return Err(DomainError::Invalid("portfolio_group_bound"));
        }
    }
    let budget = risk_budgeting(input.objective, &input.optimizer, constraints)?;
    if constraints.max_ex_ante_risk.is_some() || budget.is_some() {
        let native = |value: &BigDecimal| {
            value
                .to_f64()
                .filter(|v| v.is_finite() && (*v != 0.0 || value == &BigDecimal::from(0)))
                .ok_or(DomainError::Invalid("allocation_risk_number_range"))
        };
        let weights = ndarray::Array1::from_vec(
            targets
                .iter()
                .map(|t| native(t.weight.as_decimal()))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let (measured, marginal) = if let Some(confidence) =
            cvar_confidence(input.risk, &input.optimizer)?
        {
            let measured = risk::expected_shortfall(
                &input.return_history.asset_returns,
                &weights,
                confidence,
            )?;
            let marginal = result
                .cvar_risk_budget_witness
                .as_ref()
                .map(|witness| {
                    risk::cvar_marginal(
                        &input.return_history.asset_returns,
                        &weights,
                        confidence,
                        witness,
                        measured,
                        native(tolerance)?,
                    )
                })
                .transpose()?;
            (measured, marginal)
        } else {
            let covariance = sample_covariance(
                &input.covariance_estimator,
                &input.return_history.asset_returns,
            )
            .map_err(|_| DomainError::Invalid("allocation_risk_covariance"))?;
            let n = targets.len();
            let matrix =
                ndarray::Array2::from_shape_vec((n, n), covariance.into_iter().flatten().collect())
                    .map_err(|_| invalid())?;
            let marginal = matrix.dot(&weights);
            let variance = weights.dot(&marginal);
            if variance < 0.0 {
                return Err(DomainError::Invalid("allocation_risk_covariance"));
            }
            (variance, Some(marginal))
        };
        if let Some(budget) = budget {
            if !measured.is_finite()
                || measured <= 0.0
                || (&gross - budget.risky_gross_exposure.as_decimal()).abs() > *tolerance
            {
                return Err(DomainError::Invalid("allocation_risk_budget"));
            }
            let marginal = marginal.ok_or_else(invalid)?;
            let allowed = native(tolerance)? * measured;
            for (i, target) in targets.iter().enumerate() {
                let config = budget
                    .assets
                    .iter()
                    .find(|b| b.instrument_id == target.instrument_id)
                    .ok_or_else(invalid)?;
                let sign = if config.sign == RiskBudgetSign::Long {
                    1.0
                } else {
                    -1.0
                };
                if (config.share.is_positive() && sign * weights[i] <= 0.0)
                    || (!config.share.is_positive()
                        && target.weight.as_decimal().abs() > *tolerance)
                    || (weights[i] * marginal[i] - native(config.share.as_decimal())? * measured)
                        .abs()
                        > allowed
                {
                    return Err(DomainError::Invalid("allocation_risk_budget"));
                }
            }
        }
        let above_bound = constraints
            .max_ex_ante_risk
            .as_ref()
            .map(|bound| {
                native(&(bound.as_decimal() * (BigDecimal::from(1) + tolerance)))
                    .map(|upper| measured > upper)
            })
            .transpose()?
            .unwrap_or(false);
        if !measured.is_finite() || above_bound {
            return Err(DomainError::Invalid("allocation_ex_ante_risk_bound"));
        }
    }
    Ok(())
}
