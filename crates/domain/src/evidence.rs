//! Frozen metric requirements over actual upstream outputs. This is not a
//! statistical estimator or a qualification/publish authority.
use std::collections::BTreeMap;

use contracts::evidence::{
    Comparator, Decision, EvidenceStatus, MetricRequirementV1, MetricStatus, MetricValueV1,
};

use crate::DomainError;
use contracts::Id;

/// Supplied by the trusted, versioned native capability registry, not the Agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricCapability {
    pub metric_code: String,
    pub method_id: String,
    pub method_version: String,
    pub unit: String,
    pub frequency: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricGate {
    pub evidence_status: EvidenceStatus,
    pub decision: Decision,
    pub reasons: Vec<String>,
}

fn thresholds(
    requirement: &MetricRequirementV1,
) -> Result<
    (
        Option<&contracts::DecimalValue>,
        Option<&contracts::DecimalValue>,
    ),
    DomainError,
> {
    let low = requirement.threshold_low.as_ref();
    let high = requirement.threshold_high.as_ref();
    match (requirement.comparator, low, high) {
        (Comparator::Gt | Comparator::Ge, Some(_), None)
        | (Comparator::Lt | Comparator::Le, None, Some(_)) => Ok((low, high)),
        (Comparator::Between, Some(low), Some(high)) if low <= high => Ok((Some(low), Some(high))),
        _ => Err(DomainError::Invalid("metric_threshold_bounds")),
    }
}

fn compare_threshold(
    bound: &contracts::DecimalValue,
    value: f64,
) -> Result<std::cmp::Ordering, DomainError> {
    bound
        .compare_metric(value)
        .map_err(|_| DomainError::Invalid("metric_value"))
}

pub fn validate_metric(metric: &MetricValueV1) -> Result<(), DomainError> {
    if metric.metric_code.trim().is_empty()
        || metric.scope.trim().is_empty()
        || metric.unit.trim().is_empty()
        || metric.frequency.trim().is_empty()
        || metric.method_id.trim().is_empty()
        || metric.method_version.trim().is_empty()
        || metric.period_start >= metric.period_end
        || metric.value.is_some_and(|value| !value.is_finite())
        || metric
            .annualization_factor
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        return Err(DomainError::Invalid("metric_record"));
    }
    match (metric.status, metric.value, &metric.reason_code) {
        (MetricStatus::Ok, Some(_), None) => Ok(()),
        (MetricStatus::Ok, _, _) | (_, Some(_), _) => {
            Err(DomainError::Invalid("metric_value_status"))
        }
        (_, None, Some(reason)) if !reason.trim().is_empty() => Ok(()),
        _ => Err(DomainError::Invalid("metric_missing_reason")),
    }
}

pub fn evaluate_metrics(
    evaluation_id: Id,
    requirements: &[MetricRequirementV1],
    metrics: &[MetricValueV1],
    capabilities: &[MetricCapability],
) -> Result<MetricGate, DomainError> {
    if !requirements.iter().any(|requirement| requirement.required) {
        return Err(DomainError::Invalid("no_required_metrics"));
    }
    let mut index = BTreeMap::new();
    for metric in metrics {
        validate_metric(metric)?;
        if metric.evaluation_id != evaluation_id {
            return Err(DomainError::Invalid("metric_evaluation_mismatch"));
        }
        if index
            .insert((&metric.metric_code, &metric.scope), metric)
            .is_some()
        {
            return Err(DomainError::Invalid("duplicate_metric"));
        }
    }
    let mut seen = BTreeMap::new();
    let mut invalid = false;
    let mut missing = false;
    let mut unsupported = false;
    let mut rejected = false;
    let mut reasons = Vec::new();
    for requirement in requirements {
        if requirement.metric_code.trim().is_empty()
            || requirement.scope.trim().is_empty()
            || requirement.method_allowlist.is_empty()
            || seen
                .insert((&requirement.metric_code, &requirement.scope), ())
                .is_some()
        {
            return Err(DomainError::Invalid("metric_requirement"));
        }
        let (low, high) = thresholds(requirement)?;
        if !requirement.required {
            continue;
        }
        let Some(metric) = index.get(&(&requirement.metric_code, &requirement.scope)) else {
            missing = true;
            reasons.push(format!(
                "{}:{}:MISSING_METRIC",
                requirement.metric_code, requirement.scope
            ));
            continue;
        };
        let native_supported = capabilities.iter().any(|capability| {
            capability.metric_code == metric.metric_code
                && capability.method_id == metric.method_id
                && capability.method_version == metric.method_version
                && capability.unit == metric.unit
                && capability.frequency == metric.frequency
        });
        if metric.status == MetricStatus::Unsupported
            || !native_supported
            || !requirement.method_allowlist.contains(&metric.method_id)
        {
            unsupported = true;
            reasons.push(format!(
                "{}:{}:UNSUPPORTED_METHOD",
                metric.metric_code, metric.scope
            ));
            continue;
        }
        // Only recognized producers may classify the input as invalid. A stale
        // method, version, unit or frequency cannot drive stop_on_invalid_data.
        if metric.status == MetricStatus::InvalidInput {
            invalid = true;
            reasons.push(format!(
                "{}:{}:INVALID_INPUT",
                metric.metric_code, metric.scope
            ));
            continue;
        }
        if metric.status != MetricStatus::Ok
            || metric.observation_count < requirement.minimum_observations
        {
            missing = true;
            reasons.push(format!(
                "{}:{}:INSUFFICIENT_EVIDENCE",
                metric.metric_code, metric.scope
            ));
            continue;
        }
        let value = metric
            .value
            .ok_or(DomainError::Invalid("metric_value_status"))?;
        let passes = match (requirement.comparator, low, high) {
            (Comparator::Gt, Some(low), _) => {
                compare_threshold(low, value)? == std::cmp::Ordering::Greater
            }
            (Comparator::Ge, Some(low), _) => {
                compare_threshold(low, value)? != std::cmp::Ordering::Less
            }
            (Comparator::Lt, _, Some(high)) => {
                compare_threshold(high, value)? == std::cmp::Ordering::Less
            }
            (Comparator::Le, _, Some(high)) => {
                compare_threshold(high, value)? != std::cmp::Ordering::Greater
            }
            (Comparator::Between, Some(low), Some(high)) => {
                compare_threshold(low, value)? != std::cmp::Ordering::Less
                    && compare_threshold(high, value)? != std::cmp::Ordering::Greater
            }
            _ => return Err(DomainError::Invalid("metric_threshold_bounds")),
        };
        if !passes {
            rejected = true;
            reasons.push(format!(
                "{}:{}:THRESHOLD_NOT_MET",
                metric.metric_code, metric.scope
            ));
        }
    }
    if invalid || unsupported || missing {
        Ok(MetricGate {
            evidence_status: if invalid {
                EvidenceStatus::Invalid
            } else if unsupported {
                EvidenceStatus::Unsupported
            } else {
                EvidenceStatus::Incomplete
            },
            decision: Decision::Inconclusive,
            reasons,
        })
    } else {
        Ok(MetricGate {
            evidence_status: EvidenceStatus::Valid,
            decision: if rejected {
                Decision::Reject
            } else {
                Decision::Pass
            },
            reasons,
        })
    }
}
