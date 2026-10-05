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

pub fn decision_reason(code: &str, reason: &str) -> Result<(), DomainError> {
    text(code, 1, 120, false)?;
    text(reason, 1, 2000, true)
}

pub fn downstream_capabilities(
    value: &contracts::delivery::DownstreamCapabilitiesV1,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), DomainError> {
    if !(1..=2).contains(&value.accepted_package_versions.len())
        || value.accepted_package_versions.len() == 2
            && value.accepted_package_versions[0] == value.accepted_package_versions[1]
        || !(1..=2).contains(&value.environments.len())
        || value.environments.len() == 2 && value.environments[0] == value.environments[1]
        || !(1..=64).contains(&value.market_capability_versions.len())
        || value
            .market_capability_versions
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != value.market_capability_versions.len()
        || value.checked_at > now + chrono::Duration::seconds(5)
        || value.checked_at < now - chrono::Duration::seconds(60)
    {
        return Err(DomainError::Invalid("downstream_capabilities"));
    }
    for version in &value.market_capability_versions {
        text(version, 1, 200, false)?;
    }
    Ok(())
}

/// Freeze the same strict numerical criteria used by formal evaluations.
pub fn automation_policy(
    request: &contracts::delivery::AutomationPolicyContentV1,
) -> Result<(), DomainError> {
    if request.required_paper_observations == 0
        || request.required_paper_observations > i32::MAX as u32
        || request.max_rebalances_per_day == 0
        || request.max_rebalances_per_day > i32::MAX as u32
        || request.minimum_paper_elapsed_seconds.get() == 0
        || request.max_feedback_age_seconds.get() == 0
    {
        return Err(DomainError::Invalid("automation_policy_bounds"));
    }
    crate::research::metric_requirements(
        &request.promotion_metric_requirements,
        "promotion_metric_requirements",
    )?;
    crate::research::metric_requirements(
        &request.degradation_metric_requirements,
        "degradation_metric_requirements",
    )
}

/// Common transport fields retain their typed source branch. This is a borrowed
/// view, never a conversion of strategy evidence into forecast qualifications.
pub struct PackageDeliveryRef<'a> {
    pub release_id: contracts::Id,
    pub project_id: contracts::Id,
    pub candidate_id: contracts::Id,
    pub mandate_id: contracts::Id,
    pub package_schema_version: contracts::settings::PackageSchemaVersion,
    pub asof: chrono::DateTime<chrono::Utc>,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub base_currency: &'a String,
    pub capital_assumption: &'a contracts::DecimalValue,
    pub exposure_tolerance: &'a contracts::DecimalValue,
    pub cost_assumption_ref: contracts::Id,
    pub cash_weight: &'a contracts::DecimalValue,
    pub targets: &'a [contracts::delivery::PackageTargetV1],
    pub compatible_market_capabilities: &'a [String],
    pub input_revision_refs: &'a [contracts::Id],
}

pub fn package_delivery(
    package: &contracts::strategy_portfolio::TargetPackageEnvelopeV2,
) -> PackageDeliveryRef<'_> {
    use contracts::{settings::PackageSchemaVersion, strategy_portfolio::TargetPackageEnvelopeV2};
    macro_rules! common {
        ($p:expr, $version:expr) => {
            PackageDeliveryRef {
                release_id: $p.release_id,
                project_id: $p.project_id,
                candidate_id: $p.candidate_id,
                mandate_id: $p.mandate_id,
                package_schema_version: $version,
                asof: $p.asof,
                valid_from: $p.valid_from,
                valid_until: $p.valid_until,
                base_currency: &$p.base_currency,
                capital_assumption: &$p.capital_assumption,
                exposure_tolerance: &$p.exposure_tolerance,
                cost_assumption_ref: $p.cost_assumption_ref,
                cash_weight: &$p.cash_weight,
                targets: &$p.targets,
                compatible_market_capabilities: &$p.compatible_market_capabilities,
                input_revision_refs: &$p.input_revision_refs,
            }
        };
    }
    match package {
        TargetPackageEnvelopeV2::Forecast(p) => common!(p, PackageSchemaVersion::V1),
        TargetPackageEnvelopeV2::TargetDecision(p) => common!(p, PackageSchemaVersion::V2),
    }
}

pub fn strategy_target_package(
    package: &contracts::strategy_portfolio::TargetPackageV2,
    report: &contracts::strategy_portfolio::NativeStrategyCompositionResultV1,
    candidate: &contracts::strategy_portfolio::StrategyPortfolioCandidateV1,
) -> Result<(), DomainError> {
    use contracts::{forward::ForwardEnvironmentV1, strategy_portfolio::*};
    let invalid = || DomainError::Invalid("strategy_package_binding");
    let StrategyCompositionOutcomeV1::CurrentDecision {
        account_start,
        target,
        ..
    } = &report.outcome
    else {
        return Err(invalid());
    };
    let StrategyPortfolioPurposeV1::CurrentDecision {
        account_start: declared,
        ..
    } = &candidate.purpose
    else {
        return Err(invalid());
    };
    let mandate = &report.request.mandate;
    if package.project_id != candidate.project_id
        || package.candidate_id != candidate.id
        || package.mandate_id != candidate.mandate_id
        || package.source.run_id != candidate.run_id
        || package.source.accepted_attempt_id != candidate.accepted_attempt_id
        || package.source.report_artifact_id != candidate.report_artifact_id
        || package.source.alpha_version_ids
            != candidate
                .members
                .iter()
                .map(|m| m.alpha_version_id)
                .collect::<Vec<_>>()
        || package.account_start != **account_start
        || package.account_start != *declared
        || package.execution_environment != ForwardEnvironmentV1::Paper
        || package.base_currency != mandate.base_currency
        || package.capital_assumption != mandate.capital_assumption
        || package.exposure_tolerance != mandate.exposure_tolerance
        || package.constraints_summary != mandate.constraints
        || package.cost_assumption_ref != mandate.execution_assumptions_id
        || package.account_start.execution_assumptions_id != package.cost_assumption_ref
        || package.account_start.base_currency != package.base_currency
        || package.account_start.starting_capital != package.capital_assumption
        || package.asof != candidate.decision_asof
        || package
            .asof
            .timestamp_nanos_opt()
            .and_then(|v| u64::try_from(v).ok())
            != Some(target.asof_ns.get())
        || package.valid_from < package.asof
        || package.valid_until <= package.valid_from
        || package
            .valid_until
            .timestamp_nanos_opt()
            .and_then(|v| u64::try_from(v).ok())
            .is_none_or(|v| v > target.valid_until_ns.get())
        || package.targets.len() != target.targets.len()
        || !(1..=256).contains(&package.targets.len())
        || package.cash_weight != target.cash_weight
        || package.cash_weight != candidate.cash_weight
        || package.engine_versions.is_empty()
        || package.engine_versions != report.native_versions
        || !package
            .provenance_artifact_refs
            .contains(&candidate.report_artifact_id)
        || !package
            .input_revision_refs
            .contains(&report.request.input_provenance.dataset_revision_id)
    {
        return Err(invalid());
    }
    if serde_json::to_value(&package.source.input_provenance).map_err(|_| invalid())?
        != serde_json::to_value(&report.request.input_provenance).map_err(|_| invalid())?
        || serde_json::to_value(&candidate.input_provenance).map_err(|_| invalid())?
            != serde_json::to_value(&report.request.input_provenance).map_err(|_| invalid())?
        || serde_json::to_value(&package.execution_settings).map_err(|_| invalid())?
            != serde_json::to_value(&report.request.settings).map_err(|_| invalid())?
    {
        return Err(invalid());
    }
    if candidate.targets.len() != target.targets.len() {
        return Err(invalid());
    }
    let stored = candidate
        .targets
        .iter()
        .map(|t| (&t.instrument_id, t))
        .collect::<BTreeMap<_, _>>();
    if stored.len() != candidate.targets.len() {
        return Err(invalid());
    }
    let mut seen = BTreeSet::new();
    let mut total = package.cash_weight.as_decimal().clone();
    for (target, original) in package.targets.iter().zip(&target.targets) {
        let candidate_target = stored.get(&original.instrument_id).ok_or_else(invalid)?;
        if candidate_target.weight != original.weight
            || candidate_target.currency != original.currency
        {
            return Err(invalid());
        }
        if !seen.insert(&target.instrument_id)
            || target.instrument_id != original.instrument_id
            || target.currency != original.currency
            || target.currency != package.base_currency
            || target.target_weight != original.weight
            || !target.target_weight.is_nonnegative()
        {
            return Err(invalid());
        }
        total += target.target_weight.as_decimal();
    }
    if !package.cash_weight.is_nonnegative()
        || (total - bigdecimal::BigDecimal::from(1)).abs()
            > *package.exposure_tolerance.as_decimal()
    {
        return Err(invalid());
    }
    Ok(())
}
