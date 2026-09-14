//! Validate downstream observation structure, not issuer authority or portfolio eligibility.
use crate::{control::text, DomainError};
use contracts::forward::DownstreamWeightsSubmitV1;
use std::collections::BTreeSet;

mod window;
pub use window::{window, ForwardWindow, ForwardWindowSource};

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

/// Structural observations only; a complete report is not a qualification.
pub fn message(request: &contracts::forward::ForwardMessageSubmitV1) -> Result<(), DomainError> {
    text(&request.external_message_id, 1, 200, false)?;
    report(&request.report)
}
fn report(r: &contracts::forward::ForwardReportContentV1) -> Result<(), DomainError> {
    for value in [&r.external_claim_id, &r.issuer_version, &r.stream_id] {
        text(value, 1, 200, false)?;
    }
    if r.sequence.get() == 0
        || r.message_revision == 0
        || r.message_revision > i32::MAX as u32
        || (r.message_revision == 1) != r.supersedes_message_id.is_none()
        || [r.window_start, r.window_end, r.issued_at]
            .iter()
            .any(|t| t.timestamp_subsec_nanos() % 1000 != 0)
        || r.window_end <= r.window_start
        || r.issued_at < r.window_end
        || r.returns.len() > 10000
        || (r.complete && (r.returns.is_empty() || r.returns.iter().any(|p| p.value.is_none())))
    {
        return Err(DomainError::Invalid("forward_report"));
    }
    let start = r
        .window_start
        .timestamp_nanos_opt()
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(DomainError::Invalid("forward_time"))?;
    let end = r
        .window_end
        .timestamp_nanos_opt()
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(DomainError::Invalid("forward_time"))?;
    let mut previous = start;
    for point in &r.returns {
        if point.timestamp_ns.get() <= previous || point.timestamp_ns.get() > end {
            return Err(DomainError::Invalid("forward_sample_time"));
        }
        match (point.value, point.reason_code.as_deref()) {
            (Some(value), None) if value.is_finite() => {}
            (None, Some("NATIVE_RETURN_UNAVAILABLE")) => {}
            _ => return Err(DomainError::Invalid("forward_sample_value")),
        }
        previous = point.timestamp_ns.get();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::{forward::*, science::NativeReturnV1, DbCounter, Id, SchemaV1};
    #[test]
    fn report_requires_bounded_ordered_observations_and_explicit_missing_data() {
        let start = chrono::DateTime::from_timestamp(1800000000, 0).unwrap();
        let end = start + chrono::Duration::seconds(1);
        let mut request = ForwardMessageSubmitV1 {
            schema_version: SchemaV1,
            external_message_id: "original".into(),
            report: ForwardReportContentV1 {
                schema_version: SchemaV1,
                project_id: Id::new(),
                handoff_id: Id::new(),
                external_claim_id: "claim".into(),
                issuer_version: "native/1".into(),
                stream_id: "stream".into(),
                sequence: DbCounter::new(1).unwrap(),
                message_revision: 1,
                supersedes_message_id: None,
                window_start: start,
                window_end: end,
                issued_at: end,
                complete: true,
                returns: vec![NativeReturnV1 {
                    timestamp_ns: DbCounter::new(end.timestamp_nanos_opt().unwrap() as u64)
                        .unwrap(),
                    value: Some(0.01),
                    reason_code: None,
                }],
            },
        };
        assert!(message(&request).is_ok());
        request
            .report
            .returns
            .push(request.report.returns[0].clone());
        assert!(message(&request).is_err());
        request.report.returns.pop();
        request.report.returns[0].value = None;
        request.report.returns[0].reason_code = Some("NATIVE_RETURN_UNAVAILABLE".into());
        assert!(message(&request).is_err());
        request.report.complete = false;
        assert!(message(&request).is_ok());
        request.report.returns[0].reason_code = None;
        assert!(message(&request).is_err());
        request.report.returns.clear();
        assert!(message(&request).is_ok());
        request.report.message_revision = 2;
        assert!(message(&request).is_err());
        request.report.supersedes_message_id = Some(Id::new());
        assert!(message(&request).is_ok());
        request.report.issued_at += chrono::Duration::nanoseconds(1);
        assert!(message(&request).is_err());
        let mut value = serde_json::to_value(&request).unwrap();
        value["report"]["account_nav"] = serde_json::json!(100);
        assert!(serde_json::from_value::<ForwardMessageSubmitV1>(value).is_err());
    }
}
