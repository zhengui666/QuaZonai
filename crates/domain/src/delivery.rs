//! Package shape and original target binding only; no release/approval authority.
use crate::{control::text, DomainError};
use contracts::{
    delivery::TargetPackageV1,
    portfolio::{CandidateDetailV1, MandateViewV1},
    science::PortfolioTargetsV1,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn target_package(
    package: &TargetPackageV1,
    original: &PortfolioTargetsV1,
    mandate: &MandateViewV1,
    candidate: &CandidateDetailV1,
) -> Result<(), DomainError> {
    let invalid = || DomainError::Invalid("target_package_binding");
    crate::portfolio::mandate(&mandate.content)?;
    if package.candidate_id != original.candidate_id
        || package.candidate_id != candidate.header.id
        || package.project_id != candidate.header.project_id
        || package.project_id != mandate.project_id
        || package.mandate_id != mandate.id
        || package.mandate_id != candidate.header.mandate_id
        || package.base_currency != mandate.content.base_currency
        || package.capital_assumption != mandate.content.capital_assumption
        || package.exposure_tolerance != mandate.content.exposure_tolerance
        || package.constraints_summary != mandate.content.constraints
        || package.cost_assumption_ref != mandate.content.execution_assumptions_id
        || package.current_weights_source != candidate.header.current_weights_source
        || candidate.header.cash_weight.as_ref() != Some(&package.cash_weight)
        || package.asof != candidate.header.decision_asof
        || package.qualification_refs
            != candidate
                .members
                .iter()
                .map(|member| member.qualification_id)
                .collect::<Vec<_>>()
        || candidate.targets.len() != original.targets.len()
        || package.base_currency != original.base_currency
        || package.asof != original.asof
        || package.valid_from < package.asof
        || package.valid_until <= package.valid_from
        || package.valid_until > original.valid_until
        || package.cash_weight != original.cash_weight
        || !(1..=256).contains(&package.targets.len())
        || package.targets.len() != original.targets.len()
        || package.engine_versions.is_empty()
        || package.engine_versions.len() > 64
        || !(1..=64).contains(&package.compatible_market_capabilities.len())
        || package.limitations.len() > 64
    {
        return Err(invalid());
    }
    let stored_targets = candidate
        .targets
        .iter()
        .map(|target| (&target.instrument_id, target))
        .collect::<BTreeMap<_, _>>();
    if stored_targets.len() != candidate.targets.len() {
        return Err(invalid());
    }
    for source in &original.targets {
        let stored = stored_targets
            .get(&source.instrument_id)
            .ok_or_else(invalid)?;
        if stored.currency != source.currency
            || stored.target_weight != source.weight
            || stored.asof != original.asof
            || stored.valid_until != original.valid_until
        {
            return Err(invalid());
        }
    }
    for (refs, minimum) in [
        (&package.qualification_refs, 2),
        (&package.evaluation_refs, 1),
        (&package.input_revision_refs, 1),
        (&package.provenance_artifact_refs, 1),
    ] {
        if !(minimum..=256).contains(&refs.len())
            || refs.iter().collect::<BTreeSet<_>>().len() != refs.len()
        {
            return Err(invalid());
        }
    }
    for (name, version) in &package.engine_versions {
        text(name, 1, 120, false)?;
        text(version, 1, 200, false)?;
    }
    if package
        .compatible_market_capabilities
        .iter()
        .collect::<BTreeSet<_>>()
        .len()
        != package.compatible_market_capabilities.len()
    {
        return Err(invalid());
    }
    for capability in &package.compatible_market_capabilities {
        text(capability, 1, 200, false)?;
    }
    for limitation in &package.limitations {
        text(limitation, 1, 2000, true)?;
    }
    let mut identities = BTreeSet::new();
    let mut total = package.cash_weight.as_decimal().clone();
    for (target, source) in package.targets.iter().zip(&original.targets) {
        text(&target.instrument_id, 1, 200, false)?;
        if !identities.insert(&target.instrument_id)
            || target.instrument_id != source.instrument_id
            || target.currency != package.base_currency
            || target.currency != source.currency
            || target.target_weight != source.weight
        {
            return Err(invalid());
        }
        total += target.target_weight.as_decimal();
    }
    if (total - bigdecimal::BigDecimal::from(1)).abs() > *package.exposure_tolerance.as_decimal() {
        return Err(invalid());
    }
    Ok(())
}
