//! Allocation ownership and publication bounds, independent of the numerical solver.
use crate::{control, DomainError};
use bigdecimal::BigDecimal;
use contracts::{portfolio::*, DecimalValue};
use std::collections::BTreeSet;
mod risk;
pub use risk::{cvar_confidence, risk_budgeting};
mod liquidity;
pub use liquidity::{
    bar_liquidity_age, bar_liquidity_assumption, bar_liquidity_values, rolling_liquidity_policy,
};

fn invalid() -> DomainError {
    DomainError::Invalid("portfolio_allocation")
}

/// Identity is preserved verbatim. Empty, padded or control-bearing aliases must
/// not create additional initialization scopes for the same native account.
pub fn paper_account_scope(trader_id: &str, account_id: &str) -> Result<(), DomainError> {
    for value in [trader_id, account_id] {
        control::text(value, 1, 200, false)?;
        if value.trim() != value {
            return Err(DomainError::Invalid("paper_initialization_scope"));
        }
    }
    Ok(())
}

/// Compare a supplied lineage to its server-resolved original, never infer it
/// from a caller label, current mandate or new session identity.
pub fn paper_initialization_binding(
    actual: &contracts::science::PaperInitializationRefV1,
    original: &contracts::science::PaperInitializationRefV1,
    downstream_id: contracts::Id,
) -> Result<(), DomainError> {
    paper_account_scope(&actual.trader_id, &actual.account_id)?;
    if actual != original || actual.downstream_id != downstream_id {
        return Err(DomainError::Invalid("paper_initialization_binding"));
    }
    Ok(())
}

/// Model-capital shape only. Store owns one-time scope admission, original
/// artifact identity and real receipt clocks; this function grants no execution.
pub fn paper_weights_source(
    request: &contracts::science::NativePortfolioBuildRequestV1,
) -> Result<(), DomainError> {
    use contracts::science::PortfolioWeightsSourceV1;
    let invalid = || DomainError::Invalid("paper_initial_weights_binding");
    let weights = &request.current_weights;
    let root = weights.paper_initialization.as_ref();
    if let Some(root) = root {
        paper_account_scope(&root.trader_id, &root.account_id)?;
    }
    match (&weights.source, root) {
        (PortfolioWeightsSourceV1::PaperInitialCapital { account_start }, Some(root)) => {
            paper_account_scope(&account_start.trader_id, &account_start.account_id)?;
            let until = request
                .selection
                .decision_cutoff_ns
                .get()
                .checked_add(
                    u64::from(request.mandate.rebalance_schedule.target_ttl_seconds)
                        * 1_000_000_000,
                )
                .ok_or_else(invalid)?;
            if root.artifact_id != request.current_weights_artifact_id
                || root.downstream_id != account_start.downstream_id
                || root.trader_id != account_start.trader_id
                || root.account_id != account_start.account_id
                || account_start.execution_assumptions_id
                    != request.mandate.execution_assumptions_id
                || account_start.base_currency != request.execution_settings.base_currency
                || account_start.base_currency != request.mandate.base_currency
                || weights.base_currency != account_start.base_currency
                || account_start.starting_capital != request.execution_settings.starting_capital
                || account_start.starting_capital != request.mandate.capital_assumption
                || !account_start.starting_capital.is_positive()
                || weights.cash_weight.as_decimal() != &BigDecimal::from(1)
                || weights
                    .weights
                    .iter()
                    .any(|weight| weight.weight.as_decimal() != &BigDecimal::from(0))
                || weights.asof_ns != request.selection.decision_cutoff_ns
                || weights.available_ns != request.selection.decision_cutoff_ns
                || weights.valid_until_ns.get() != until
            {
                return Err(invalid());
            }
        }
        (PortfolioWeightsSourceV1::PaperInitialCapital { .. }, None) => return Err(invalid()),
        (PortfolioWeightsSourceV1::ForwardSnapshot { downstream_id, .. }, Some(root))
            if *downstream_id != root.downstream_id =>
        {
            return Err(invalid());
        }
        _ => {}
    }
    Ok(())
}

pub fn simulation_settings(
    settings: &contracts::science::NativeSimulationSettingsV1,
) -> Result<(), DomainError> {
    // Fail closed until the native adapter, fees and result consumers are wired.
    // Never silently execute the new mode through the legacy single-currency path.
    if settings.multi_currency_spot_cash.is_some() {
        return Err(DomainError::CapabilityUnavailable(
            "multi_currency_spot_cash_not_integrated",
        ));
    }
    simulation_models(settings)?;
    if !settings.starting_capital.is_positive()
        || !settings.leverage.is_positive()
        || settings.leverage.as_decimal() > &BigDecimal::from(100)
        || !settings.exposure_tolerance.is_positive()
        || settings.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !(1..=86_400_000).contains(&settings.snapshot_interval_ms)
        || !contracts::research_currency::supported(&settings.base_currency)
        || settings.fee_rates.is_empty()
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
        NativeModelRefV1::FrozenSpotFeeScenario { native_version, parameters, .. }
            if native_version == NAUTILUS_EXECUTION_VERSION
                && settings.multi_currency_spot_cash.is_some()
                && parameters.schedule.source.status == contracts::spot_fees::SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability
                && matches!(parameters.acceptance, contracts::spot_fees::SpotCashFeeAcceptanceV1::PublicRateScenarioUnverifiedApplicability) => {}
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
    if input.members.len() < 2
        || input.instrument_ids.is_empty()
        || input.max_input_age_seconds == 0
        || age > u64::from(input.max_input_age_seconds) * 1_000_000_000
        || input.horizon_value.get() == 0
        || input.bar_types.len() != input.instrument_ids.len()
        || !contracts::research_currency::supported(&input.base_currency)
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
    if alphas.is_empty()
        || (input
            .members
            .iter()
            .filter(|m| m.ensemble_weight.is_positive())
            .count()
            > 1
            && alphas.len() < 2)
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn ensemble_weights<'a>(
    weights: impl Iterator<Item = &'a DecimalValue>,
) -> Result<(), DomainError> {
    let mut sum = BigDecimal::from(0);
    let mut count = 0usize;
    let mut positive = 0;
    for weight in weights {
        if !weight.is_nonnegative() {
            return Err(DomainError::Invalid("ensemble_weights"));
        }
        count += 1;
        positive += usize::from(weight.is_positive());
        sum += weight.as_decimal();
    }
    if count < 2 || positive == 0 || sum != BigDecimal::from(1) {
        return Err(DomainError::Invalid("ensemble_weights"));
    }
    Ok(())
}

/// Structural mandate checks shared with actual allocation and publication.
/// Asset/group membership and feasibility still require the frozen native input.
pub fn portfolio_constraints(constraints: &PortfolioConstraintsV1) -> Result<(), DomainError> {
    if constraints.cash_funding.is_some()
        && (!constraints.long_only
            || !constraints.min_cash_weight.is_nonnegative()
            || constraints.max_gross_exposure.as_decimal() > &BigDecimal::from(1))
    {
        return Err(DomainError::Invalid(
            "cash_funding_requires_unlevered_long_only",
        ));
    }
    if !constraints.max_gross_exposure.is_nonnegative()
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
            ));
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
    cash_funding_method(
        &content.constraints,
        content.objective,
        content.risk_measure,
        optimizer_settings(&content.optimizer)?,
    )?;
    rebalance_schedule(&content.rebalance_schedule)?;
    if !content.capital_assumption.is_positive()
        || !content.exposure_tolerance.is_positive()
        || content.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !contracts::research_currency::supported(&content.base_currency)
    {
        return Err(DomainError::Invalid("portfolio_mandate"));
    }
    risk_budgeting(content.objective, &content.optimizer, &content.constraints)?;
    Ok(())
}

fn cash_funding_method(
    constraints: &PortfolioConstraintsV1,
    objective: AllocationObjective,
    risk: AllocationRisk,
    settings: &AllocatorSettingsV1,
) -> Result<(), DomainError> {
    if settings.cash_numerical_output.is_some() && constraints.cash_funding.is_none() {
        return Err(DomainError::Invalid(
            "cash_numerical_output_requires_cash_funding",
        ));
    }
    if constraints.cash_funding.is_some()
        && (objective != AllocationObjective::MaxUtility || risk != AllocationRisk::Variance)
    {
        return Err(DomainError::Invalid("cash_funding_method_unsupported"));
    }
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
    cash_funding_method(
        constraints,
        input.objective,
        input.risk,
        optimizer_settings(&input.optimizer)?,
    )?;
    if constraints.cash_funding.is_some()
        && (!input.current_cash_weight.is_nonnegative()
            || input
                .assets
                .iter()
                .any(|asset| !asset.current_weight.is_nonnegative()))
    {
        return Err(DomainError::Invalid(
            "cash_funding_current_borrowing_or_shorting",
        ));
    }
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
    if count < 1
        || !input.capital_assumption.is_positive()
        || !input.exposure_tolerance.is_positive()
        || input.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || constraints.asset_overrides.len() > count
        || !contracts::research_currency::supported(&input.base_currency)
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
    if count < 2
        || history.available_ns.len() != count
        || history.instrument_ids != forecasts.instrument_ids
        || history.bar_types != forecasts.bar_types
        || history.base_currency != forecasts.base_currency
        || history.horizon_kind != forecasts.horizon_kind
        || history.horizon_value != forecasts.horizon_value
        || history.asset_returns.len() != history.instrument_ids.len()
        || !history.asset_returns.len().checked_mul(count).is_some()
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
        || parameters.max_iterations == 0
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
