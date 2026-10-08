//! Package shape and original target binding only; no release/approval authority.
use crate::{control::text, DomainError};
use contracts::{
    delivery::{ForecastTargetPackageV2, FrozenForwardDatasetV2},
    portfolio::{CandidateDetailV1, MandateViewV1},
    science::PortfolioTargetsV1,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn target_package(
    package: &ForecastTargetPackageV2,
    original: &PortfolioTargetsV1,
    mandate: &MandateViewV1,
    candidate: &CandidateDetailV1,
) -> Result<(), DomainError> {
    let invalid = || DomainError::Invalid("target_package_binding");
    crate::portfolio::mandate(&mandate.content)?;
    if package.current_weights.paper_initialization.is_some()
        && candidate.header.origin != contracts::research::DataOrigin::Synthetic
    {
        return Err(invalid());
    }
    if package.source.forward_dataset_revision_id != package.forward_dataset.dataset_revision_id
        || package.source.forward_metadata_artifact_id
            != package.forward_dataset.native_metadata_artifact_id
        || package.source.build_run_id != candidate.header.run_id
        || package.source.build_input_set_id != candidate.header.input_set_id
        || candidate.header.current_weights_artifact_id
            != Some(package.source.current_weights_artifact_id)
        || package.candidate_id != original.candidate_id
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

/// Pure bounded projection; the caller must have resolved this original metadata
/// through the Dataset's registration evidence and validated frozen input binding.
/// Complete per-asset histories are preserved, including zero-target assets.
pub fn freeze_forward_dataset(
    dataset_revision_id: contracts::Id,
    native_metadata_artifact_id: contracts::Id,
    selection: &contracts::science::NativeBarSelectionV1,
    metadata: &contracts::catalogs::RuntimeCatalogMetadataV1,
    instrument_ids: &[String],
) -> Result<FrozenForwardDatasetV2, DomainError> {
    let invalid = || DomainError::Invalid("forecast_forward_dataset_binding");
    let required: BTreeSet<_> = instrument_ids.iter().map(String::as_str).collect();
    if required.is_empty()
        || required.len() > 256
        || metadata.partition != contracts::research::DataPartition::Forward
    {
        return Err(invalid());
    }
    let versions = crate::catalogs::instrument_versions(&metadata.universe.instrument_definitions)?;
    if required.iter().any(|id| !versions.contains_key(id)) {
        return Err(invalid());
    }
    let mut instrument_definitions = Vec::new();
    for definition in &metadata.universe.instrument_definitions {
        let (_, payload) = crate::catalogs::instrument_definition(definition)?;
        let id = payload
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid)?;
        if required.contains(id) {
            instrument_definitions.push(definition.clone());
        }
    }
    Ok(FrozenForwardDatasetV2 {
        dataset_revision_id,
        native_metadata_artifact_id,
        storage_version: metadata.storage_version.clone(),
        data_kind: metadata.data_kind,
        partition: metadata.partition,
        origin: metadata.origin,
        pit_status: metadata.pit_status,
        revision_policy: metadata.revision_policy,
        event_start: metadata.event_start,
        event_end: metadata.event_end,
        available_through: metadata.available_through,
        row_count: metadata.row_count,
        selection: selection.clone(),
        instrument_definitions,
    })
}

/// Bind the additional V2 fields to the same original accepted Build. Store must
/// separately resolve the Build attempt/parameters/report IDs and Dataset license.
/// No execution environment or current account approval is inferred here.
pub fn forecast_source_binding(
    package: &ForecastTargetPackageV2,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    frozen: &contracts::science::NativePortfolioBuildRequestV1,
    forward_dataset: &FrozenForwardDatasetV2,
) -> Result<(), DomainError> {
    use contracts::{
        portfolio::{CandidateWeightsSourceV1, PortfolioBuildWeightsV1},
        science::PortfolioWeightsSourceV1,
    };
    let invalid = || DomainError::Invalid("forecast_package_source_binding");
    crate::portfolio::paper_weights_source(frozen)?;
    if package.current_weights.paper_initialization.is_some() {
        if build.environment != contracts::forward::ForwardEnvironmentV1::Paper
            || package.environment_origin != contracts::delivery::PackageOriginV1::Synthetic
            || forward_dataset.origin != contracts::research::DataOrigin::Real
            || forward_dataset.pit_status != contracts::research::PitStatus::Verified
            || forward_dataset.revision_policy
                != contracts::catalogs::DataRevisionPolicy::AsKnownThen
        {
            return Err(invalid());
        }
    } else if package.environment_origin == contracts::delivery::PackageOriginV1::Synthetic {
        return Err(invalid());
    }
    let source = &package.source;
    let forward_dataset_revision_id = forward_dataset.dataset_revision_id;
    if source.build_input_set_id != build.input_set_id
        || source.build_environment != build.environment
        || source.forward_dataset_revision_id != forward_dataset_revision_id
        || source.forward_metadata_artifact_id != forward_dataset.native_metadata_artifact_id
        || forward_dataset.selection != frozen.selection
        || serde_json::to_value(&package.forward_dataset).map_err(|_| invalid())?
            != serde_json::to_value(forward_dataset).map_err(|_| invalid())?
        || source.current_weights_artifact_id != frozen.current_weights_artifact_id
        || package.mandate_id != build.mandate_id
        || package.current_weights != frozen.current_weights
        || package.base_currency != frozen.current_weights.base_currency
        || package.base_currency != frozen.execution_settings.base_currency
        || package.capital_assumption != frozen.execution_settings.starting_capital
        || package.exposure_tolerance != frozen.execution_settings.exposure_tolerance
        || !package
            .input_revision_refs
            .contains(&forward_dataset_revision_id)
        || ![
            source.build_parameters_artifact_id,
            source.build_report_artifact_id,
            source.current_weights_artifact_id,
            source.forward_metadata_artifact_id,
        ]
        .iter()
        .all(|id| package.provenance_artifact_refs.contains(id))
        || serde_json::to_value(&package.execution_settings).map_err(|_| invalid())?
            != serde_json::to_value(&frozen.execution_settings).map_err(|_| invalid())?
        || package
            .valid_until
            .timestamp_nanos_opt()
            .and_then(|v| u64::try_from(v).ok())
            .is_none_or(|until| until > frozen.current_weights.valid_until_ns.get())
    {
        return Err(invalid());
    }
    let required: BTreeSet<_> = frozen
        .assets
        .iter()
        .map(|a| a.instrument_id.as_str())
        .chain(
            frozen
                .current_weights
                .weights
                .iter()
                .map(|w| w.instrument_id.as_str()),
        )
        .chain(package.targets.iter().map(|t| t.instrument_id.as_str()))
        .collect();
    let definitions =
        crate::catalogs::instrument_versions(&forward_dataset.instrument_definitions)?;
    if definitions.keys().copied().collect::<BTreeSet<_>>() != required {
        return Err(invalid());
    }
    match (
        &build.current_weights_source,
        &package.current_weights.source,
        package.current_weights_source,
    ) {
        (
            PortfolioBuildWeightsV1::PaperInitialCapital {
                downstream_id,
                trader_id,
                account_id,
            },
            PortfolioWeightsSourceV1::PaperInitialCapital { account_start },
            CandidateWeightsSourceV1::PaperInitialCapital,
        ) if *downstream_id == account_start.downstream_id
            && *trader_id == account_start.trader_id
            && *account_id == account_start.account_id => {}
        (
            PortfolioBuildWeightsV1::ForwardSnapshot { .. },
            PortfolioWeightsSourceV1::ForwardSnapshot { .. },
            CandidateWeightsSourceV1::ForwardSnapshot,
        ) => {}
        (
            PortfolioBuildWeightsV1::LastTarget {
                candidate_id: expected,
            },
            PortfolioWeightsSourceV1::LastTarget {
                candidate_id: actual,
            },
            CandidateWeightsSourceV1::LastTarget,
        ) if expected == actual => {}
        _ => return Err(invalid()),
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
    if value.accepted_package_versions != [contracts::settings::PackageSchemaVersion::V2]
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
        TargetPackageEnvelopeV2::Forecast(p) => common!(p, PackageSchemaVersion::V2),
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

/// Syntax only. Store separately authenticates the original DownstreamClaim
/// credential and atomically records at most one consumption for the root.
pub fn paper_initial_execution_request(
    request: &contracts::delivery::PaperInitialExecutionConsumeV1,
) -> Result<(), DomainError> {
    crate::portfolio::paper_account_scope(
        &request.paper_initialization.trader_id,
        &request.paper_initialization.account_id,
    )?;
    text(&request.external_claim_id, 1, 200, false)?;
    if request.external_claim_id.len() > 200
        || request.external_claim_id.trim() != request.external_claim_id
    {
        return Err(DomainError::Invalid("paper_initial_execution_claim_key"));
    }
    Ok(())
}

/// Check the already accepted original claim, not new-release admission or a new
/// scientific evaluation. Current credential, root/receipt provenance, approval,
/// revocation, decision and readiness checks remain authoritative Store work.
pub fn paper_initial_execution_claim(
    request: &contracts::delivery::PaperInitialExecutionConsumeV1,
    handoff_id: contracts::Id,
    claim: &contracts::strategy_portfolio::HandoffClaimViewV2,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), DomainError> {
    use contracts::{
        delivery::{HandoffStateV1, PackageOriginV1},
        forward::ForwardEnvironmentV1,
        science::PortfolioWeightsSourceV1,
        strategy_portfolio::TargetPackageEnvelopeV2,
    };
    paper_initial_execution_request(request)?;
    let invalid = || DomainError::Invalid("paper_initial_execution_binding");
    let TargetPackageEnvelopeV2::Forecast(package) = &claim.package else {
        return Err(invalid());
    };
    let PortfolioWeightsSourceV1::PaperInitialCapital { account_start } =
        &package.current_weights.source
    else {
        return Err(invalid());
    };
    let handoff = &claim.handoff;
    let root = &request.paper_initialization;
    if handoff.id != handoff_id
        || handoff.state != HandoffStateV1::Claimed
        || handoff.environment != ForwardEnvironmentV1::Paper
        || handoff.release_id != request.release_id
        || handoff.release_id != package.release_id
        || handoff.project_id != package.project_id
        || handoff.candidate_id != package.candidate_id
        || handoff.mandate_id != package.mandate_id
        || handoff.downstream_id != root.downstream_id
        || handoff.external_claim_id.as_deref() != Some(request.external_claim_id.as_str())
        || package.environment_origin != PackageOriginV1::Synthetic
        || package.current_weights_source
            != contracts::portfolio::CandidateWeightsSourceV1::PaperInitialCapital
        || package.source.build_environment != ForwardEnvironmentV1::Paper
        || package.current_weights.paper_initialization.as_ref() != Some(root)
        || package.source.current_weights_artifact_id != root.artifact_id
        || account_start.downstream_id != root.downstream_id
        || account_start.trader_id != root.trader_id
        || account_start.account_id != root.account_id
        || account_start.base_currency != package.base_currency
        || account_start.starting_capital != package.capital_assumption
        || !account_start.starting_capital.is_positive()
        || account_start.execution_assumptions_id != package.cost_assumption_ref
        || package.execution_settings.base_currency != package.base_currency
        || package.execution_settings.starting_capital != package.capital_assumption
        || package.execution_settings.exposure_tolerance != package.exposure_tolerance
        || package.current_weights.base_currency != package.base_currency
        || package.current_weights.cash_weight.as_decimal() != &bigdecimal::BigDecimal::from(1)
        || package.current_weights.weights.is_empty()
        || package.current_weights.weights.iter().any(|weight| {
            weight.weight.as_decimal() != &bigdecimal::BigDecimal::from(0)
                || weight.currency != package.base_currency
        })
    {
        return Err(invalid());
    }
    let claimed_at = handoff.claimed_at.ok_or_else(invalid)?;
    if package.asof > package.valid_from
        || package.valid_from >= package.valid_until
        || now < package.valid_from
        || now >= package.valid_until
        || now >= handoff.expires_at
        || claimed_at > now
        || claimed_at < handoff.offered_at
        || claimed_at >= handoff.expires_at
    {
        return Err(DomainError::Invalid("paper_initial_execution_expired"));
    }
    Ok(())
}

/// Binding checks for a response received directly by the current authenticated
/// transport call. This function does not prove transport freshness, authenticate
/// a file, grant execution, or construct a permit. Host code must keep that
/// boundary private and move its one-use permit directly into the native engine.
pub fn paper_initial_execution_response(
    request: &contracts::delivery::PaperInitialExecutionConsumeV1,
    expected_claim: &contracts::strategy_portfolio::HandoffClaimViewV2,
    current_owner_instance_id: contracts::Id,
    result: &contracts::control::CommandResult<contracts::delivery::PaperInitialExecutionViewV1>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), DomainError> {
    if result.replayed {
        return Err(DomainError::CapabilityUnavailable(
            "paper_initial_execution_recovery_required",
        ));
    }
    let invalid = || DomainError::Invalid("paper_initial_execution_response_binding");
    let view = &result.resource;
    if request.owner_instance_id != current_owner_instance_id
        || view.owner_instance_id != current_owner_instance_id
        || view.paper_initialization != request.paper_initialization
        || view.consumed_at > now
        || serde_json::to_value(&view.claim).map_err(|_| invalid())?
            != serde_json::to_value(expected_claim).map_err(|_| invalid())?
    {
        return Err(invalid());
    }
    paper_initial_execution_claim(request, expected_claim.handoff.id, &view.claim, now)?;
    if view.consumed_at < view.claim.handoff.claimed_at.ok_or_else(invalid)? {
        return Err(invalid());
    }
    paper_initial_execution_claim(
        request,
        expected_claim.handoff.id,
        &view.claim,
        view.consumed_at,
    )
}
