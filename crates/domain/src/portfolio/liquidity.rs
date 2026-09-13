use crate::DomainError;
use contracts::{
    execution::{NativeBarNotionalV1, NativeDatasetQualityV1},
    execution_assumptions::BarLiquidityAssumptionV1,
    DbCounter,
};

pub fn bar_liquidity_assumption(value: &BarLiquidityAssumptionV1) -> Result<(), DomainError> {
    if value.maximum_age_seconds == 0
        || !value.participation_limit.is_positive()
        || !value.participation_limit.is_fraction()
    {
        return Err(DomainError::Invalid("bar_liquidity_assumption"));
    }
    Ok(())
}

pub fn rolling_liquidity_policy(
    value: &contracts::science::NativeRollingBarLiquidityPolicyV1,
) -> Result<(), DomainError> {
    if value.maximum_age_seconds == 0
        || !value.participation_limit.is_positive()
        || !value.participation_limit.is_fraction()
    {
        return Err(DomainError::Invalid("rolling_liquidity_policy"));
    }
    Ok(())
}

/// A frozen historical per-rebalance ceiling, never an estimate of future depth.
pub fn bar_liquidity_values<'a>(
    assumption: &BarLiquidityAssumptionV1,
    quality: &'a NativeDatasetQualityV1,
    currency: &str,
    at: DbCounter,
) -> Result<&'a [NativeBarNotionalV1], DomainError> {
    bar_liquidity_assumption(assumption)?;
    crate::catalogs::bar_notionals(quality)?;
    let values = quality
        .last_bar_notionals
        .as_deref()
        .ok_or(DomainError::Invalid("bar_liquidity_not_measured"))?;
    if quality.selection.decision_cutoff_ns > at {
        return Err(DomainError::Invalid("bar_liquidity_expired_or_mismatched"));
    }
    bar_liquidity_age(values, currency, assumption.maximum_age_seconds, at)?;
    Ok(values)
}

pub fn bar_liquidity_age(
    values: &[NativeBarNotionalV1],
    currency: &str,
    maximum_age_seconds: u32,
    at: DbCounter,
) -> Result<(), DomainError> {
    let age = u64::from(maximum_age_seconds) * 1_000_000_000;
    if maximum_age_seconds == 0
        || values.is_empty()
        || values.iter().any(|value| {
            value.currency != currency
                || value.available_ns > at
                || value.event_ns > at
                || at.get().saturating_sub(value.event_ns.get()) >= age
        })
    {
        return Err(DomainError::Invalid("bar_liquidity_expired_or_mismatched"));
    }
    Ok(())
}
