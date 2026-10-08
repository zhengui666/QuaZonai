//! Integrity of an uploaded runner report, not attestation of independent execution.
use crate::{research::invalid, DomainError};
use chrono::{Datelike, Timelike};
use contracts::agent_evaluation::*;
use std::collections::{BTreeMap, BTreeSet};

fn reject() -> DomainError {
    invalid("content", "AGENT_EVALUATION_REPORT")
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}
fn ids(values: &[String]) -> bool {
    !values.is_empty()
        && values.iter().all(|id| text(id, 200))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
fn settings(value: &AgentEvaluationPolicyV1) -> bool {
    text(&value.model, 200) && text(&value.reasoning_effort, 200)
}
pub fn parse(bytes: &[u8]) -> Result<AgentEvaluationReportV1, DomainError> {
    if bytes.is_empty() {
        return Err(reject());
    }
    let report = serde_json::from_slice(bytes).map_err(|_| reject())?;
    validate(&report)?;
    Ok(report)
}
/// FAIL takes precedence, then BLOCKED, then UNRUN; PASS requires all cases to pass.
pub fn aggregate(cases: &[AgentEvaluationCaseV1]) -> AgentEvaluationStatus {
    use AgentEvaluationStatus::*;
    for status in [Fail, Blocked, Unrun] {
        if cases.iter().any(|case| case.status == status) {
            return status;
        }
    }
    if cases.is_empty() {
        Unrun
    } else {
        Pass
    }
}
pub fn validate(report: &AgentEvaluationReportV1) -> Result<(), DomainError> {
    use AgentEvaluationStatus::*;
    // Chrono supports extended years and leap seconds outside RFC3339's UTC
    // end-of-day position. The typed HTTP schema uses RFC3339 date-time.
    let recorded = report.recorded_at;
    if !(0..=9999).contains(&recorded.year())
        || (recorded.nanosecond() >= 1_000_000_000
            && (recorded.hour(), recorded.minute(), recorded.second()) != (23, 59, 59))
        || !text(&report.source_revision, 200)
        || !text(&report.suite_id, 200)
        || !settings(&report.requested)
        || [&report.runner, &report.subject]
            .iter()
            .any(|v| !text(&v.name, 200) || !text(&v.version, 200))
        || [&report.tuning, &report.held_out]
            .iter()
            .any(|v| !text(&v.id, 200) || !ids(&v.case_ids))
        || report.tuning.sha256 == report.held_out.sha256
        || report.tuning.id == report.held_out.id
        || report.cases.len() < 2
        || aggregate(&report.cases) != report.status
    {
        return Err(reject());
    }
    let tuning: BTreeSet<_> = report.tuning.case_ids.iter().collect();
    let held_out: BTreeSet<_> = report.held_out.case_ids.iter().collect();
    if !tuning.is_disjoint(&held_out) || tuning.len() + held_out.len() != report.cases.len() {
        return Err(reject());
    }
    let mut seen = BTreeSet::new();
    let mut scenarios = BTreeMap::new();
    for case in &report.cases {
        let split = match case.split {
            AgentEvaluationSplit::Tuning => &tuning,
            AgentEvaluationSplit::HeldOut => &held_out,
        };
        let hash = String::from(case.scenario_sha256.clone());
        if !text(&case.id, 200)
            || !seen.insert(&case.id)
            || !split.contains(&case.id)
            || !text(&case.reason, 2000)
            || !ids(&case.required_assertions)
            || scenarios
                .insert(hash, case.split)
                .is_some_and(|previous| previous != case.split)
        {
            return Err(reject());
        }
        let required: BTreeSet<_> = case.required_assertions.iter().collect();
        let mut actual = BTreeSet::new();
        if case
            .assertions
            .iter()
            .any(|assertion| !actual.insert(&assertion.id) || !required.contains(&assertion.id))
        {
            return Err(reject());
        }
        if let Some(observed) = &case.observed {
            if !settings(&observed.settings) || !text(&observed.invocation_id, 200) {
                return Err(reject());
            }
        }
        let measurement = &case.measurements;
        if measurement.cost.as_ref().is_some_and(|value| {
            !value.amount.is_nonnegative()
                || iso_currency::Currency::from_code(&value.currency).is_none()
        }) {
            return Err(reject());
        }
        if case.status == Unrun
            && (case.observed.is_some()
                || !case.assertions.is_empty()
                || measurement.input_tokens.is_some()
                || measurement.output_tokens.is_some()
                || measurement.elapsed_ms.is_some()
                || measurement.tool_calls.is_some()
                || measurement.cost.is_some())
        {
            return Err(reject());
        }
        if matches!(case.status, Pass | Fail) {
            let observed = case.observed.as_ref().ok_or_else(reject)?;
            if actual != required {
                return Err(reject());
            }
            let all_pass = case.assertions.iter().all(|a| a.passed);
            if (case.status == Pass) != all_pass {
                return Err(reject());
            }
            if observed.settings.model != report.requested.model
                || observed.settings.reasoning_effort != report.requested.reasoning_effort
            {
                return Err(reject());
            }
        }
    }
    Ok(())
}
