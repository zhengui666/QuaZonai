//! Frozen risk configuration and publication checks, not another optimizer.
use crate::DomainError;
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{portfolio::*, DecimalValue};

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
    let tail = ((BigDecimal::from(1) - confidence.as_decimal()) * BigDecimal::from(count as u64))
        .to_f64()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(invalid)?;
    let eta = *losses.select_nth_unstable_by(index, f64::total_cmp).1;
    let risk = eta
        + losses
            .iter()
            .map(|loss| (loss - eta).max(0.0) / tail)
            .sum::<f64>();
    if !risk.is_finite() {
        return Err(invalid());
    }
    Ok(risk)
}
