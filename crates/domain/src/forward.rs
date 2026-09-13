//! Validate downstream observation structure, not issuer authority or portfolio eligibility.
use crate::{control::text, DomainError};
use contracts::forward::DownstreamWeightsSubmitV1;
use std::collections::BTreeSet;

pub fn weights(request: &DownstreamWeightsSubmitV1) -> Result<(), DomainError> {
    text(&request.external_message_id, 1, 200, false)?;
    if request.asof_ns > request.available_ns
        || request.available_ns >= request.valid_until_ns
        || iso_currency::Currency::from_code(&request.base_currency).is_none()
        || !(1..=256).contains(&request.weights.len())
    {
        return Err(DomainError::Invalid("forward_weights"));
    }
    let mut ids = BTreeSet::new();
    let mut total = request.cash_weight.as_decimal().clone();
    for weight in &request.weights {
        text(&weight.instrument_id, 1, 200, false)?;
        if !ids.insert(&weight.instrument_id) || weight.currency != request.base_currency {
            return Err(DomainError::Invalid("forward_weights"));
        }
        total += weight.weight.as_decimal();
    }
    if total != bigdecimal::BigDecimal::from(1) {
        return Err(DomainError::Invalid("forward_weights_total"));
    }
    Ok(())
}
