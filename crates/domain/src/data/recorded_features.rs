//! Pure original-buffer and provenance checks. Store owns source/grant authority.
use super::bad;
use crate::DomainError;
use contracts::{
    catalogs::{DataRevisionPolicy, RuntimeCatalogMetadataV1},
    data::{
        RecordedFeatureRegisterIntentV1, RecordedFeatureRegisterV1, RecordedFeatureSourceBindingV1,
    },
    research::PitStatus,
    science::{FeatureAvailabilityV1, FeatureObservationsV1},
    strategy_portfolio::StrategyInputProvenanceV1,
    DbCounter,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn recorded_feature_intent(value: &RecordedFeatureRegisterIntentV1) -> Result<(), DomainError> {
    crate::catalogs::recorded_feature_part_key(&value.feature_part_key)?;
    if value.byte_count.get() == 0 {
        return Err(bad("feature.content_size"));
    }
    Ok(())
}

/// Parse the original UTF-8 buffer once for validation. Return the typed value
/// only for checks; the caller must store request.content.as_bytes() unchanged.
pub fn recorded_feature_register(
    value: &RecordedFeatureRegisterV1,
) -> Result<FeatureObservationsV1, DomainError> {
    crate::catalogs::recorded_feature_part_key(&value.feature_part_key)?;
    if value.content.is_empty() || value.content.contains('\0') {
        return Err(bad("feature.content_size"));
    }
    let parsed: FeatureObservationsV1 =
        serde_json::from_str(&value.content).map_err(|_| bad("feature.content"))?;
    recorded_observations(&parsed)?;
    Ok(parsed)
}

fn recorded_observations(value: &FeatureObservationsV1) -> Result<(), DomainError> {
    crate::execution::features::observations(value)?;
    if value
        .feature_schema
        .iter()
        .any(|definition| definition.availability != FeatureAvailabilityV1::Observed)
    {
        return Err(bad("feature.recorded_availability"));
    }
    Ok(())
}

/// Compare the original size and parsed facts with the frozen native descriptor.
/// Store owns the immutable Dataset/artifact association and original-byte replay.
pub fn recorded_feature_matches(
    metadata: &RuntimeCatalogMetadataV1,
    part_key: &str,
    byte_count: DbCounter,
    content: &FeatureObservationsV1,
) -> Result<(), DomainError> {
    let attachments = metadata
        .recorded_feature_inputs
        .as_ref()
        .ok_or_else(|| bad("feature.recorded_source_unsupported"))?;
    crate::catalogs::recorded_feature_inputs(
        attachments,
        metadata.partition,
        metadata.available_through,
    )?;
    recorded_observations(content)?;
    let fragment = attachments
        .fragments
        .iter()
        .find(|part| part.part_key == part_key)
        .ok_or_else(|| bad("feature.part_key"))?;
    if fragment.byte_count != byte_count
        || fragment.observations.get() != content.observations.len() as u64
        || content.partition != attachments.partition
        || content.observations.iter().map(|row| row.event_ns).min() != Some(fragment.min_event_ns)
        || content.observations.iter().map(|row| row.event_ns).max() != Some(fragment.max_event_ns)
        || content
            .observations
            .iter()
            .filter_map(|row| row.observed_available_ns)
            .min()
            != Some(fragment.min_observed_available_ns)
        || content
            .observations
            .iter()
            .filter_map(|row| row.observed_available_ns)
            .max()
            != Some(fragment.max_observed_available_ns)
    {
        return Err(bad("feature.frozen_descriptor"));
    }
    Ok(())
}

/// Identity/history validation deliberately performs no time-dependent license check.
pub fn recorded_feature_binding(value: &RecordedFeatureSourceBindingV1) -> Result<(), DomainError> {
    crate::catalogs::recorded_feature_part_key(&value.feature_part_key)?;
    if value.pit_status == PitStatus::Verified
        && value.revision_policy != DataRevisionPolicy::AsKnownThen
    {
        return Err(bad("feature.source_binding.pit"));
    }
    Ok(())
}

/// Validate optional provenance without inventing bindings for old artifacts.
/// Actual Dataset roles and license/DataUse checks belong to the science caller.
pub fn recorded_feature_provenance(value: &StrategyInputProvenanceV1) -> Result<(), DomainError> {
    let Some(bindings) = &value.feature_source_bindings else {
        return Ok(());
    };
    let mut datasets = BTreeMap::new();
    let mut parts = BTreeSet::new();
    for (artifact, binding) in bindings {
        recorded_feature_binding(binding)?;
        if value.feature_artifact_origins.get(artifact) != Some(&binding.origin)
            || !parts.insert((binding.dataset_revision_id, &binding.feature_part_key))
        {
            return Err(bad("feature.source_binding.artifact"));
        }
        let source = (
            binding.source_id,
            binding.data_use_grant_id,
            binding.native_metadata_artifact_id,
            binding.origin,
            binding.pit_status,
            binding.revision_policy,
        );
        if datasets
            .insert(binding.dataset_revision_id, source)
            .is_some_and(|prior| prior != source)
        {
            return Err(bad("feature.source_binding.dataset"));
        }
        if binding.dataset_revision_id == value.dataset_revision_id
            && (binding.origin != value.market_data_origin
                || binding.pit_status != value.pit_status
                || binding.revision_policy != value.revision_policy)
        {
            return Err(bad("feature.source_binding.market"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
