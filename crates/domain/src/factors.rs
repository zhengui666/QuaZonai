//! Shared temporal membership and label censoring, independent of asset ordinals.
use crate::{research::invalid, DomainError};
use contracts::{
    factors::{
        NativeFactorEvaluationRequestV1, NativeFactorExclusionReasonV1, NativeFactorPartitionV1,
    },
    science::NativeForecastPointV1,
    DbCounter,
};

pub fn evaluation_request(request: &NativeFactorEvaluationRequestV1) -> Result<(), DomainError> {
    crate::execution::forecast_request(&request.forecast)?;
    let selection = &request.forecast.selection;
    if request.train_end_ns <= selection.event_start_ns
        || request.train_end_ns >= selection.event_end_ns
    {
        return Err(invalid("train_end_ns", "FACTOR_PARTITION_BOUNDARIES"));
    }
    if request.validation_end_ns <= request.train_end_ns
        || request.validation_end_ns > selection.event_end_ns
    {
        return Err(invalid("validation_end_ns", "FACTOR_PARTITION_BOUNDARIES"));
    }
    Ok(())
}

/// All instruments use these exact half-open intervals, including empty intervals of data.
pub fn partition_bounds(
    request: &NativeFactorEvaluationRequestV1,
    partition: NativeFactorPartitionV1,
) -> (DbCounter, DbCounter) {
    use NativeFactorPartitionV1::*;
    match partition {
        Train => (
            request.forecast.selection.event_start_ns,
            request.train_end_ns,
        ),
        Validation => (request.train_end_ns, request.validation_end_ns),
        Heldout => (
            request.validation_end_ns,
            request.forecast.selection.event_end_ns,
        ),
    }
}

pub fn partition_for_event(
    request: &NativeFactorEvaluationRequestV1,
    event_ns: DbCounter,
) -> Option<NativeFactorPartitionV1> {
    use NativeFactorPartitionV1::*;
    if event_ns < request.forecast.selection.event_start_ns
        || event_ns >= request.forecast.selection.event_end_ns
    {
        None
    } else if event_ns < request.train_end_ns {
        Some(Train)
    } else if event_ns < request.validation_end_ns {
        Some(Validation)
    } else {
        Some(Heldout)
    }
}

/// Called on original validated native forecast observations. Reasons are disjoint:
/// warmup, unavailable prediction, incomplete label, then a crossing complete label.
/// An available label alone is insufficient: its event horizon must also stay in the fold.
pub fn exclusion_reason(
    request: &NativeFactorEvaluationRequestV1,
    partition: NativeFactorPartitionV1,
    point: &NativeForecastPointV1,
    label_event_ns: Option<DbCounter>,
) -> Option<NativeFactorExclusionReasonV1> {
    use NativeFactorExclusionReasonV1::*;
    if point.forecast.is_none() {
        return Some(IndicatorWarmup);
    }
    let (start, end) = partition_bounds(request, partition);
    if point.available_ns < start || point.available_ns >= end {
        return Some(PredictionOutsidePartition);
    }
    let (Some(_), Some(available), Some(event)) =
        (point.label_return, point.label_available_ns, label_event_ns)
    else {
        return Some(LabelNotComplete);
    };
    if event < start || event >= end || available < start || available >= end {
        return Some(LabelCrossesPartition);
    }
    None
}
