//! Frozen risk configuration and publication checks, not another optimizer.
use crate::DomainError;
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{portfolio::*, DecimalValue};

pub fn risk_budgeting<'a>(
    objective: AllocationObjective,
    model: &'a NativeModelRefV1,
    constraints: &PortfolioConstraintsV1,
) -> Result<Option<&'a RiskBudgetSettingsV1>, DomainError> {
    let settings = super::optimizer_settings(model)?.risk_budgeting.as_ref();
    if objective != AllocationObjective::RiskBudgeting {
        return if settings.is_none() {
            Ok(None)
        } else {
            Err(DomainError::Invalid("portfolio_risk_budgeting"))
        };
    }
    let settings = settings.ok_or(DomainError::Invalid("portfolio_risk_budgeting"))?;
    if !(1..=MAX_ALLOCATION_ASSETS).contains(&settings.assets.len())
        || !settings.risky_gross_exposure.is_positive()
        || settings.risky_gross_exposure.as_decimal() > constraints.max_gross_exposure.as_decimal()
    {
        return Err(DomainError::Invalid("portfolio_risk_budgeting"));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut total = BigDecimal::from(0);
    for asset in &settings.assets {
        crate::control::text(&asset.instrument_id, 1, 200, false)?;
        if !ids.insert(&asset.instrument_id)
            || !asset.share.is_fraction()
            || (constraints.long_only
                && asset.share.is_positive()
                && asset.sign == RiskBudgetSign::Short)
        {
            return Err(DomainError::Invalid("portfolio_risk_budgeting"));
        }
        total += asset.share.as_decimal();
    }
    if total != BigDecimal::from(1) {
        return Err(DomainError::Invalid("portfolio_risk_budgeting"));
    }
    Ok(Some(settings))
}

pub fn cvar_confidence(
    risk: AllocationRisk,
    model: &NativeModelRefV1,
) -> Result<Option<&DecimalValue>, DomainError> {
    let confidence = super::optimizer_settings(model)?.cvar_confidence.as_ref();
    match (risk, confidence) {
        (AllocationRisk::Variance, None) => Ok(None),
        (AllocationRisk::Cvar, Some(value))
            if value.is_positive() && value.as_decimal() < &BigDecimal::from(1) =>
        {
            Ok(Some(value))
        }
        _ => Err(DomainError::Invalid("portfolio_cvar_confidence")),
    }
}

// Called only after allocation_input validates the original aligned, finite history.
pub fn cvar_tail_coefficient(count: usize, confidence: &DecimalValue) -> Result<f64, DomainError> {
    let tail = ((BigDecimal::from(1) - confidence.as_decimal()) * BigDecimal::from(count as u64))
        .to_f64()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or(DomainError::Invalid("portfolio_cvar_number_range"))?;
    let coefficient = 1.0 / tail;
    if !coefficient.is_finite() {
        return Err(DomainError::Invalid("portfolio_cvar_number_range"));
    }
    Ok(coefficient)
}

// Verifies the original empirical CVaR dual optimum, including nonunique tail ties.
pub(super) fn cvar_marginal(
    history: &[Vec<f64>],
    weights: &ndarray::Array1<f64>,
    confidence: &DecimalValue,
    witness: &CvarRiskBudgetWitnessV1,
    risk: f64,
    tolerance: f64,
) -> Result<ndarray::Array1<f64>, DomainError> {
    let invalid = || DomainError::Invalid("allocation_cvar_risk_witness");
    let count = history[0].len();
    let p = &witness.scenario_weights;
    let cap = cvar_tail_coefficient(count, confidence)?;
    if p.len() != count
        || !risk.is_finite()
        || risk <= 0.0
        || p.iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > cap * (1.0 + tolerance))
        || (p.iter().sum::<f64>() - 1.0).abs() > tolerance
    {
        return Err(invalid());
    }
    let matrix = ndarray::Array2::from_shape_vec(
        (weights.len(), count),
        history.iter().flatten().copied().collect(),
    )
    .map_err(|_| invalid())?;
    let marginal = -matrix.dot(&ndarray::ArrayView1::from(p.as_slice()));
    let witnessed = weights.dot(&marginal);
    if marginal.iter().any(|v| !v.is_finite())
        || !witnessed.is_finite()
        || (witnessed - risk).abs() > risk * tolerance
    {
        return Err(invalid());
    }
    Ok(marginal)
}

pub(super) fn expected_shortfall(
    history: &[Vec<f64>],
    weights: &ndarray::Array1<f64>,
    confidence: &DecimalValue,
) -> Result<f64, DomainError> {
    let invalid = || DomainError::Invalid("portfolio_cvar_number_range");
    let count = history[0].len();
    let matrix = ndarray::Array2::from_shape_vec(
        (weights.len(), count),
        history.iter().flatten().copied().collect(),
    )
    .map_err(|_| invalid())?;
    let mut losses = (-weights.dot(&matrix)).to_vec();
    if losses.iter().any(|v| !v.is_finite()) {
        return Err(invalid());
    }
    let index = (confidence.as_decimal() * BigDecimal::from(count as u64))
        .to_usize()
        .filter(|v| *v < count)
        .ok_or_else(invalid)?;
    let coefficient = cvar_tail_coefficient(count, confidence)?;
    let eta = *losses.select_nth_unstable_by(index, f64::total_cmp).1;
    let risk = eta
        + losses
            .iter()
            .map(|loss| (loss - eta).max(0.0) * coefficient)
            .sum::<f64>();
    if !risk.is_finite() {
        return Err(invalid());
    }
    Ok(risk)
}
