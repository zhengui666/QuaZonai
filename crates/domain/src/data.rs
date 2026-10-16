//! Native catalog keys, calculation limits and recorded feature provenance.
use crate::{control::text, research::invalid, DomainError};

fn bad(field: &str) -> DomainError {
    invalid(field, "DATA_CONTRACT_INVALID")
}

pub fn registry_key(value: &str) -> Result<(), DomainError> {
    text(value, 1, 512, false).map_err(|_| bad("native_catalog_ref"))?;
    if value.trim() != value
        || value.contains("://")
        || value.contains(['\\', '?', '#'])
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(bad("native_catalog_ref"));
    }
    Ok(())
}

/// Shared zero-trial allocations; the actual Runtime and owning Cycle impose further bounds.
pub fn bounded_native_limits(
    limits: &contracts::lifecycle::JobLimitsV1,
) -> Result<(), DomainError> {
    if limits.experiments != 0 {
        return Err(bad("limits.experiments"));
    }
    if limits.cpu_seconds.is_some_and(|cpu| cpu.get() == 0) {
        return Err(bad("limits.cpu_seconds"));
    }
    if limits.wall_seconds.is_some_and(|seconds| seconds == 0) {
        return Err(bad("limits.wall_seconds"));
    }
    if limits.memory_mib == Some(0) {
        return Err(bad("limits.memory_mib"));
    }
    if limits.output_bytes.is_some_and(|output| output.get() == 0) {
        return Err(bad("limits.output_bytes"));
    }
    Ok(())
}

mod recorded_features;
pub use recorded_features::{
    recorded_feature_binding, recorded_feature_intent, recorded_feature_matches,
    recorded_feature_provenance, recorded_feature_register,
};
