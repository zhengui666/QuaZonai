//! Research authoring checks, separate from live native freeze/admission evidence.
use crate::{admission, control, research::invalid, DomainError};
use contracts::{
    brief::*,
    research::{DataPartition, FieldIssue},
};
use std::collections::BTreeSet;

pub fn content(content: &BriefContentV1, bindings: &[BriefBindingV1]) -> Result<(), DomainError> {
    let mut issues: Vec<FieldIssue> = Vec::new();
    let mut record = |field: String, code: &'static str| {
        if let DomainError::Fields(mut fields) = invalid(field, code) {
            issues.append(&mut fields);
        }
    };
    for (field, value) in [
        ("hypothesis", &content.hypothesis),
        ("economic_rationale", &content.economic_rationale),
    ] {
        if control::text(value, 1, 8000, true).is_err() {
            record(format!("content.{field}"), "TEXT_RANGE");
        }
    }
    if iso_currency::Currency::from_code(&content.base_currency).is_none() {
        record("content.base_currency".into(), "ISO_CURRENCY_REQUIRED");
    }
    if !matches!(
        (content.horizon_kind, content.horizon_value),
        (HorizonKind::VariableInterval, None)
    ) && !matches!((content.horizon_kind, content.horizon_value), (HorizonKind::FixedBars | HorizonKind::FixedDuration, Some(n)) if n.get()>0)
    {
        record("content.horizon_value".into(), "HORIZON_SHAPE");
    }
    if admission::validate_budget(&content.budget, &content.stop_rule).is_err() {
        record("content.budget".into(), "BUDGET_OR_STOP_RULE");
    }
    if !(1..=64).contains(&bindings.len()) {
        record("bindings".into(), "BINDING_COUNT");
    }
    let mut seen = BTreeSet::new();
    for (index, binding) in bindings.iter().enumerate() {
        if !seen.insert(binding.dataset_revision_id) {
            record(
                format!("bindings.{index}.dataset_revision_id"),
                "DUPLICATE_DATASET",
            );
        }
        if (binding.role == DataPartition::Sealed
            && binding.access_policy == DataAccess::ResearchRead)
            || (binding.role != DataPartition::Sealed
                && binding.access_policy == DataAccess::EvaluatorOnly)
        {
            record(
                format!("bindings.{index}.access_policy"),
                "PARTITION_AUTHORITY",
            );
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(DomainError::Fields(issues))
    }
}
