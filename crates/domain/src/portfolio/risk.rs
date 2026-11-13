//! Frozen risk configuration and publication checks, not another optimizer.
use crate::DomainError;
use bigdecimal::BigDecimal;
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
    if settings.assets.is_empty()
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
