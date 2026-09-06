//! QZ preparation invariants only. No split algorithm, estimator or capability registry.
use crate::{control::text, evidence::thresholds, DomainError};
use contracts::research::*;
use std::collections::BTreeSet;

/// Static field diagnostics contain no user data, native paths or secret values.
pub fn invalid(field: impl Into<String>, code: &str) -> DomainError {
    DomainError::Fields(vec![FieldIssue {
        field: field.into(),
        code: code.into(),
        message: "该字段不满足研究准备合同，请检查类型、范围和引用关系。".into(),
    }])
}
fn bounded_text(
    field: impl Into<String>,
    value: &str,
    max: usize,
    multiline: bool,
) -> Result<(), DomainError> {
    text(value, 1, max, multiline).map_err(|_| invalid(field, "TEXT_RANGE"))
}
pub fn input_set(request: &InputSetCreate) -> Result<(), DomainError> {
    if !request
        .decision_cutoff
        .timestamp_subsec_nanos()
        .is_multiple_of(1000)
    {
        return Err(invalid("decision_cutoff", "DATABASE_TIME_PRECISION"));
    }
    if !(1..=256).contains(&request.items.len()) {
        return Err(invalid("items", "ITEM_COUNT"));
    }
    let mut seen = BTreeSet::new();
    for (index, item) in request.items.iter().enumerate() {
        let identity = match item {
            InputItemV1::Dataset {
                dataset_revision_id,
                role,
            } => {
                let allowed = match request.purpose {
                    InputPurpose::Discovery => *role == DataPartition::Discovery,
                    InputPurpose::Validation => *role == DataPartition::Validation,
                    InputPurpose::Sealed => *role == DataPartition::Sealed,
                    InputPurpose::Forward => *role == DataPartition::Forward,
                    InputPurpose::Portfolio => {
                        matches!(role, DataPartition::Discovery | DataPartition::Validation)
                    }
                };
                if !allowed {
                    return Err(invalid(
                        format!("items.{index}.role"),
                        "PARTITION_PURPOSE_MISMATCH",
                    ));
                }
                (0, *dataset_revision_id)
            }
            InputItemV1::Artifact { artifact_id, .. } => (1, *artifact_id),
        };
        if !seen.insert(identity) {
            return Err(invalid(format!("items.{index}"), "DUPLICATE_INPUT"));
        }
    }
    Ok(())
}
pub fn split(request: &SplitPolicyV1) -> Result<(), DomainError> {
    for (field, value) in [
        ("train_size", request.train_size),
        ("test_size", request.test_size),
    ] {
        if value.get() == 0 {
            return Err(invalid(
                format!("split_policy.{field}"),
                "POSITIVE_COUNT_REQUIRED",
            ));
        }
    }
    if !request.interval_validation_required {
        return Err(invalid(
            "split_policy.interval_validation_required",
            "INTERVAL_VALIDATION_REQUIRED",
        ));
    }
    if request
        .train_size
        .checked_add(request.test_size.get())
        .and_then(|n| n.checked_add(request.purge_observations.get()))
        .and_then(|n| n.checked_add(request.embargo_observations.get()))
        .is_none()
    {
        return Err(invalid("split_policy", "COUNT_OVERFLOW"));
    }
    if request
        .label_horizon_observations
        .is_some_and(|n| n.get() == 0)
    {
        return Err(invalid(
            "split_policy.label_horizon_observations",
            "POSITIVE_COUNT_REQUIRED",
        ));
    }
    match request.kind {
        SplitKind::WalkForward => {
            if request.step_size.is_none_or(|n| n.get() == 0) {
                return Err(invalid("split_policy.step_size", "POSITIVE_COUNT_REQUIRED"));
            }
            if request.group_count.is_some() || request.test_group_count.is_some() {
                return Err(invalid(
                    "split_policy.group_count",
                    "WALK_FORWARD_HAS_NO_GROUPS",
                ));
            }
        }
        SplitKind::CpcvFixedHorizon => {
            if request.step_size.is_some() {
                return Err(invalid("split_policy.step_size", "CPCV_HAS_NO_STEP"));
            }
            if request.label_horizon_observations.is_none() {
                return Err(invalid(
                    "split_policy.label_horizon_observations",
                    "FIXED_HORIZON_REQUIRED",
                ));
            }
            if !matches!((request.group_count, request.test_group_count), (Some(g),Some(t)) if g>=2 && t>=1 && t<g)
            {
                return Err(invalid(
                    "split_policy.test_group_count",
                    "CPCV_GROUP_COUNTS",
                ));
            }
        }
    }
    Ok(())
}
pub fn evaluation_policy(request: &EvaluationPolicyCreate) -> Result<(), DomainError> {
    bounded_text("question", &request.question, 8000, true)?;
    split(&request.split_policy)?;
    let s = &request.selection;
    for (field, value) in [
        ("metric_code", &s.metric_code),
        ("metric_scope", &s.metric_scope),
        ("method_id", &s.method_id),
        ("method_version", &s.method_version),
        ("unit", &s.unit),
        ("frequency", &s.frequency),
    ] {
        bounded_text(format!("selection.{field}"), value, 120, false)?;
    }
    if s.candidate_count == 0 {
        return Err(invalid(
            "selection.candidate_count",
            "POSITIVE_COUNT_REQUIRED",
        ));
    }
    for (field, n) in [
        ("minimum_observations", request.minimum_observations),
        (
            "maximum_sealed_uses_per_lineage",
            request.maximum_sealed_uses_per_lineage,
        ),
    ] {
        if n == 0 || n > i32::MAX as u32 {
            return Err(invalid(field, "POSTGRES_INTEGER_RANGE"));
        }
    }
    if request.validity_seconds.get() == 0 {
        return Err(invalid("validity_seconds", "POSITIVE_COUNT_REQUIRED"));
    }
    if !request.maximum_missing_fraction.is_fraction() {
        return Err(invalid("maximum_missing_fraction", "FRACTION_RANGE"));
    }
    if request.required_capabilities.len() > 64 {
        return Err(invalid("required_capabilities", "CAPABILITY_COUNT"));
    }
    let mut capabilities = BTreeSet::new();
    for (index, c) in request.required_capabilities.iter().enumerate() {
        bounded_text(format!("required_capabilities.{index}"), c, 120, false)?;
        if !capabilities.insert(c) {
            return Err(invalid("required_capabilities", "DUPLICATE_CAPABILITY"));
        }
    }
    if !(1..=64).contains(&request.metric_requirements.len()) {
        return Err(invalid("metric_requirements", "METRIC_COUNT"));
    }
    let mut metrics = BTreeSet::new();
    let mut selected = false;
    for (index, m) in request.metric_requirements.iter().enumerate() {
        let field = format!("metric_requirements.{index}");
        bounded_text(format!("{field}.metric_code"), &m.metric_code, 120, false)?;
        bounded_text(format!("{field}.scope"), &m.scope, 120, false)?;
        if !metrics.insert((&m.metric_code, &m.scope)) {
            return Err(invalid(field, "DUPLICATE_METRIC"));
        }
        thresholds(m)
            .map_err(|_| invalid(format!("{field}.threshold_low"), "EXACT_THRESHOLD_BOUNDS"))?;
        if !(1..=64).contains(&m.method_allowlist.len()) {
            return Err(invalid(format!("{field}.method_allowlist"), "METHOD_COUNT"));
        }
        let mut methods = BTreeSet::new();
        for (j, method) in m.method_allowlist.iter().enumerate() {
            bounded_text(format!("{field}.method_allowlist.{j}"), method, 120, false)?;
            if !methods.insert(method) {
                return Err(invalid(
                    format!("{field}.method_allowlist"),
                    "DUPLICATE_METHOD",
                ));
            }
        }
        if m.metric_code == s.metric_code && m.scope == s.metric_scope {
            if !m.required || !m.method_allowlist.contains(&s.method_id) {
                return Err(invalid("selection", "REQUIRED_ALLOWED_METRIC"));
            }
            selected = true;
        }
    }
    if !selected {
        return Err(invalid("selection", "REQUIRED_ALLOWED_METRIC"));
    }
    Ok(())
}
