//! Bind native daily statistics to the exact original source window; never calculate them here.
use crate::DomainError;
use contracts::{
    evidence::{MetricStatus, MetricValueV1},
    forward::*,
    science::NativeStatisticGroup,
    Id, SchemaV1,
};
use std::collections::BTreeSet;

// Calendar UTC days include weekends. These are deliberately not 252 trading-day statistics.
type StatisticDefinition = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<f64>,
    bool,
);
const STATISTICS: [StatisticDefinition; 3] = [
    (
        "FORWARD_DAILY_RETURN_MEAN",
        "Average (Return)",
        "nautilus-analysis.ReturnsAverage",
        "RETURN_PER_DAY",
        None,
        true,
    ),
    (
        "FORWARD_RETURN_VOLATILITY",
        "Returns Volatility (365 days)",
        "nautilus-analysis.ReturnsVolatility",
        "ANNUALIZED_RETURN_STDDEV",
        Some(365.0),
        false,
    ),
    (
        "FORWARD_SHARPE_RATIO",
        "Sharpe Ratio (365 days)",
        "nautilus-analysis.SharpeRatio",
        "RATIO",
        Some(365.0),
        true,
    ),
];
fn invalid() -> DomainError {
    DomainError::Invalid("forward_evaluation_binding")
}
fn window(w: &ForwardWindowViewV1) -> Result<(), DomainError> {
    crate::control::text(&w.stream_id, 1, 200, false)?;
    let start = w.window_start.ok_or_else(invalid)?;
    let end = w.window_end.ok_or_else(invalid)?;
    if !w.is_contiguous
        || !w.reason_codes.is_empty()
        || w.returns_frequency != Some(ForwardReturnsFrequencyV1::UtcDay)
        || start >= end
        || start.timestamp() < 0
        || start.timestamp() % 86400 != 0
        || end.timestamp() % 86400 != 0
        || start.timestamp_subsec_nanos() != 0
        || end.timestamp_subsec_nanos() != 0
        || w.complete_observations.get() != (end - start).num_days() as u64
        || w.complete_observations.get() > 1_000_000
        || w.latest_message_ids.is_empty()
        || w.latest_message_ids.len() > 255
        || w.latest_message_ids.iter().collect::<BTreeSet<_>>().len() != w.latest_message_ids.len()
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn request(r: &NativeForwardRequestV1) -> Result<(), DomainError> {
    window(&r.window)?;
    if !(1..=255).contains(&r.sources.len()) {
        return Err(invalid());
    }
    let first = &r.sources[0];
    let mut ids = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for s in &r.sources {
        if !ids.insert(s.id)
            || !artifacts.insert(s.report_artifact_id)
            || s.handoff_id != r.window.handoff_id
            || s.stream_id != r.window.stream_id
            || s.project_id != first.project_id
            || s.release_id != first.release_id
            || s.downstream_id != first.downstream_id
            || s.window_start < r.window.window_start.ok_or_else(invalid)?
            || s.window_end > r.window.window_end.ok_or_else(invalid)?
        {
            return Err(invalid());
        }
    }
    if r.window
        .latest_message_ids
        .iter()
        .any(|id| !ids.contains(id))
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn shape(r: &NativeForwardResultV1) -> Result<(), DomainError> {
    window(&r.window)?;
    if r.native_version != "0.63.0" || r.statistics.len() != STATISTICS.len() {
        return Err(invalid());
    }
    for (s, (_, key, _, _, _, _)) in r.statistics.iter().zip(STATISTICS) {
        if s.group != NativeStatisticGroup::Returns
            || s.native_key != key
            || s.currency.is_some()
            || match (s.value, s.reason_code.as_deref()) {
                (Some(v), None) => !v.is_finite(),
                (None, Some("NATIVE_STATISTIC_UNAVAILABLE")) => false,
                _ => true,
            }
        {
            return Err(invalid());
        }
    }
    Ok(())
}
pub fn binding(
    requested: &NativeForwardRequestV1,
    result: &NativeForwardResultV1,
) -> Result<(), DomainError> {
    request(requested)?;
    shape(result)?;
    if requested.window != result.window {
        return Err(invalid());
    }
    Ok(())
}
pub fn metrics(
    evaluation: Id,
    artifact: Id,
    requested: &NativeForwardRequestV1,
    result: &NativeForwardResultV1,
) -> Result<(Vec<MetricValueV1>, Vec<crate::evidence::MetricCapability>), DomainError> {
    binding(requested, result)?;
    let mut records = Vec::new();
    let mut capabilities = Vec::new();
    for (s, (code, _, method, unit, annualization, higher)) in
        result.statistics.iter().zip(STATISTICS)
    {
        let record = MetricValueV1 {
            schema_version: SchemaV1,
            evaluation_id: evaluation,
            metric_code: code.into(),
            scope: "forward".into(),
            value: s.value,
            status: if s.value.is_some() {
                MetricStatus::Ok
            } else {
                MetricStatus::Failed
            },
            reason_code: s.reason_code.clone(),
            unit: unit.into(),
            period_start: result.window.window_start.ok_or_else(invalid)?,
            period_end: result.window.window_end.ok_or_else(invalid)?,
            observation_count: result.window.complete_observations,
            frequency: "UTC_DAY".into(),
            annualization_factor: annualization,
            method_id: method.into(),
            method_version: result.native_version.clone(),
            source_artifact_id: artifact,
            higher_is_better: Some(higher),
        };
        crate::evidence::validate_metric(&record)?;
        capabilities.push(crate::evidence::MetricCapability {
            metric_code: record.metric_code.clone(),
            method_id: record.method_id.clone(),
            method_version: record.method_version.clone(),
            unit: record.unit.clone(),
            frequency: record.frequency.clone(),
        });
        records.push(record);
    }
    Ok((records, capabilities))
}
