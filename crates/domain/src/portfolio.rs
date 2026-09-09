//! Allocation ownership and publication bounds, independent of the numerical solver.
use crate::{control, DomainError};
use bigdecimal::BigDecimal;
use contracts::{portfolio::*, DecimalValue};
use std::collections::{BTreeMap, BTreeSet};

fn invalid() -> DomainError {
    DomainError::Invalid("portfolio_allocation")
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

/// Does not authorize an Alpha, infer a quote, or fit an estimator.
pub fn allocation_input(input: &AllocationInputV1) -> Result<(), DomainError> {
    let count = input.assets.len();
    let constraints = &input.constraints;
    if !(1..=MAX_ALLOCATION_ASSETS).contains(&count)
        || input.covariance.len() != count
        || input
            .covariance
            .iter()
            .any(|row| row.len() != count || row.iter().any(|v| !v.is_finite()))
        || !input.capital_assumption.is_positive()
        || !input.risk_aversion.is_positive()
        || !input.exposure_tolerance.is_positive()
        || input.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !input.settings.solver_tolerance.is_positive()
        || input.settings.solver_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !(1..=100_000).contains(&input.settings.max_iterations)
        || constraints.group_bounds.len() > MAX_ALLOCATION_GROUPS
        || constraints.asset_overrides.len() > count
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
        || iso_currency::Currency::from_code(&input.base_currency).is_none()
    {
        return Err(invalid());
    }
    ordered(&constraints.min_cash_weight, &constraints.max_cash_weight)?;
    ordered(&constraints.min_asset_weight, &constraints.max_asset_weight)?;
    ordered(&constraints.min_net_exposure, &constraints.max_net_exposure)?;
    if constraints.long_only && !constraints.min_asset_weight.is_nonnegative() {
        return Err(invalid());
    }
    let mut identities = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let mut current_total = input.current_cash_weight.as_decimal().clone();
    for asset in &input.assets {
        control::text(&asset.instrument_id, 1, 200, false)?;
        if !identities.insert(asset.instrument_id.as_str())
            || asset.currency != input.base_currency
            || !asset.expected_return.is_finite()
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
    let mut overrides = BTreeSet::new();
    for value in &constraints.asset_overrides {
        if !identities.contains(value.instrument_id.as_str())
            || !overrides.insert(value.instrument_id.as_str())
            || (constraints.long_only && !value.min.is_nonnegative())
        {
            return Err(invalid());
        }
        ordered(&value.min, &value.max)?;
    }
    let mut bounded_groups = BTreeSet::new();
    for value in &constraints.group_bounds {
        if !groups.contains(value.group_id.as_str())
            || !bounded_groups.insert(value.group_id.as_str())
        {
            return Err(invalid());
        }
        ordered(&value.min, &value.max)?;
    }
    Ok(())
}

/// Validate the *stored decimal targets*, not just the solver's in-memory floats.
/// An inaccurate status is accepted only when the frozen numerical policy allows it.
pub fn allocation_result(
    input: &AllocationInputV1,
    result: &AllocationResultV1,
) -> Result<(), DomainError> {
    allocation_input(input)?;
    if result.objective_value.is_some_and(|v| !v.is_finite())
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
            || result.reason_code.as_ref().is_none_or(|v| v.is_empty())
        {
            return Err(invalid());
        }
        return Ok(());
    }
    if input.risk != AllocationRisk::Variance
        || input.objective == AllocationObjective::RiskBudgeting
    {
        return Err(DomainError::CapabilityUnavailable("allocation_objective"));
    }
    if result.reason_code.is_some()
        || (result.solver_status == SolverStatus::AcceptableInaccurate
            && !input.settings.accept_inaccurate)
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
    // A caller must never claim a currently unsupported nonlinear constraint was checked.
    if constraints.max_ex_ante_risk.is_some() {
        return Err(DomainError::CapabilityUnavailable(
            "allocation_ex_ante_risk",
        ));
    }
    Ok(())
}
