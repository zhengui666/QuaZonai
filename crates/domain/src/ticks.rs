//! Pure selection, resource and temporal rules for explicit tick factor research.
use crate::DomainError;
use contracts::{
    factors::NativeFactorPartitionV1, ticks::NativeTradeTickFactorEvaluationRequestV1, DbCounter,
};
use std::collections::BTreeSet;

pub const MAXIMUM_TICK_ROWS: u32 = 1_000_000;
pub const MAXIMUM_COHORT_ROWS: u32 = 100_000;

pub fn evaluation_request(
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<(), DomainError> {
    let input = &request.input;
    let fail = |code| Err(DomainError::Invalid(code));
    if input.instrument_ids.is_empty()
        || input.instrument_ids.len() > MAXIMUM_TICK_ROWS as usize
        || input.instrument_ids.iter().any(|id| {
            id.is_empty()
                || id.len() > 200
                || id.chars().any(char::is_whitespace)
                || id.chars().any(char::is_control)
        })
        || input.instrument_ids.iter().collect::<BTreeSet<_>>().len() != input.instrument_ids.len()
    {
        return fail("TICK_INSTRUMENT_SELECTION_INVALID");
    }
    if input.event_start_ns >= input.event_end_ns
        || input.decision_cutoff_ns < input.event_end_ns
        || request.train_end_ns <= input.event_start_ns
        || request.train_end_ns >= request.validation_end_ns
        || request.validation_end_ns > input.event_end_ns
    {
        return fail("TICK_PARTITION_BOUNDARIES");
    }
    if input.maximum_rows == 0
        || input.maximum_rows > MAXIMUM_TICK_ROWS
        || request.sampling.maximum_cohort_rows == 0
        || request.sampling.maximum_cohort_rows > MAXIMUM_COHORT_ROWS
    {
        return fail("TICK_RESOURCE_LIMIT");
    }
    if input.source_revision.is_empty()
        || input.source_revision.len() > 512
        || input.source_revision.chars().any(char::is_control)
    {
        return fail("TICK_SOURCE_REVISION_REQUIRED");
    }
    let p = &request.parameters;
    if p.fast_period_cohorts == 0
        || p.slow_period_cohorts <= p.fast_period_cohorts
        || p.slow_period_cohorts > MAXIMUM_TICK_ROWS
        || request.target.horizon_cohorts == 0
        || request.target.horizon_cohorts > MAXIMUM_TICK_ROWS
        || p.total_fuel.is_some_and(|fuel| fuel.get() == 0)
    {
        return fail("TICK_PARAMETERS_INVALID");
    }
    Ok(())
}

pub fn partition_bounds(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    partition: NativeFactorPartitionV1,
) -> (DbCounter, DbCounter) {
    use NativeFactorPartitionV1::*;
    match partition {
        Train => (request.input.event_start_ns, request.train_end_ns),
        Validation => (request.train_end_ns, request.validation_end_ns),
        Heldout => (request.validation_end_ns, request.input.event_end_ns),
    }
}

pub fn partition_for_event(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    event: DbCounter,
) -> Option<NativeFactorPartitionV1> {
    use NativeFactorPartitionV1::*;
    if event < request.input.event_start_ns || event >= request.input.event_end_ns {
        None
    } else if event < request.train_end_ns {
        Some(Train)
    } else if event < request.validation_end_ns {
        Some(Validation)
    } else {
        Some(Heldout)
    }
}
