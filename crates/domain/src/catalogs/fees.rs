use super::bad;
use crate::{control::text, DomainError};
use contracts::{
    catalogs::FrozenMakerTakerFeesV1, research::DataOrigin, science::NativeBarSelectionV1,
};
use std::collections::BTreeSet;

/// Source-owner declaration, separately frozen from request fee hints. This
/// validates its scope and exact rates, not the truth of a provider's provenance.
pub fn maker_taker_fee_source(source: &FrozenMakerTakerFeesV1) -> Result<(), DomainError> {
    text(&source.source_reference, 1, 2000, false)?;
    text(&source.source_version, 1, 120, false)?;
    text(&source.venue, 1, 120, false)?;
    text(&source.currency, 1, 40, false)?;
    if source.origin == DataOrigin::LegacyUnknown
        || source.currency_precision > 16
        || source.valid_from_ns >= source.valid_until_ns
        || source.available_at_ns >= source.valid_until_ns
        || source.rates.is_empty()
        || source.rates.len() > 65_536
    {
        return Err(bad("execution_fees.source"));
    }
    let mut ids = BTreeSet::new();
    for rate in &source.rates {
        text(&rate.instrument_id, 1, 200, false)?;
        if !ids.insert(&rate.instrument_id)
            || rate.instrument_id.rsplit_once('.').map(|(_, venue)| venue)
                != Some(source.venue.as_str())
            || rate.maker.as_decimal().abs() >= bigdecimal::BigDecimal::from(1)
            || rate.taker.as_decimal().abs() >= bigdecimal::BigDecimal::from(1)
        {
            return Err(bad("execution_fees.rates"));
        }
    }
    Ok(())
}

/// The historical replay cannot know a policy first observed after it began.
/// Runtime also checks actual fill-clock time, including delayed fills.
pub fn maker_taker_fee_selection(
    source: &FrozenMakerTakerFeesV1,
    selection: &NativeBarSelectionV1,
) -> Result<(), DomainError> {
    maker_taker_fee_source(source)?;
    if source.available_at_ns > selection.event_start_ns
        || source.valid_from_ns > selection.event_start_ns
        || source.valid_until_ns <= selection.decision_cutoff_ns
        || selection.event_start_ns >= selection.event_end_ns
        || selection.decision_cutoff_ns < selection.event_start_ns
    {
        return Err(bad("execution_fees.window"));
    }
    Ok(())
}
