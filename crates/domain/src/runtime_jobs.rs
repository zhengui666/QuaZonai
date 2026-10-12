//! Minimal immutable file-protocol shape used by the native job adapter.
use crate::{control::text, research::invalid, runtime, DomainError};
use chrono::{DateTime, Utc};
use contracts::{research::ArtifactInputRole, runtime_jobs::*, Id};
use std::collections::BTreeSet;

/// The fixed native job reports fresh, local compiler cgroup OOM evidence with
/// this exit status. It is not a compiler or signal exit code; GNU timeout
/// preserves it when the job finishes before its deadline.
pub const NATIVE_MEMORY_LIMIT_EXIT_CODE: i32 = 70;

fn bad(field: &str) -> DomainError {
    invalid(field, "INVALID_RUNTIME_JOB_CONTRACT")
}

pub fn external_id(run: Id, attempt: u32) -> Result<String, DomainError> {
    if attempt == 0 {
        return Err(bad("attempt_no"));
    }
    Ok(format!("{run}/{attempt}"))
}

/// Canonical native identity. This is never joined to a host path by a caller.
pub fn parse_external_id(value: &str) -> Result<(Id, u32), DomainError> {
    if !(38..=47).contains(&value.len()) {
        return Err(bad("external_job_id"));
    }
    let (run, attempt) = value
        .split_once('/')
        .ok_or_else(|| bad("external_job_id"))?;
    let run = Id::try_from(run.to_owned()).map_err(|_| bad("external_job_id"))?;
    let attempt = attempt.parse::<u32>().map_err(|_| bad("external_job_id"))?;
    if external_id(run, attempt)? != value {
        return Err(bad("external_job_id"));
    }
    Ok((run, attempt))
}

fn time(value: DateTime<Utc>) -> Result<(), DomainError> {
    if !value.timestamp_subsec_nanos().is_multiple_of(1000) {
        return Err(bad("timestamp"));
    }
    Ok(())
}

/// Native immutable versions must also be representable by the shared HTTP header.
pub fn storage_version(value: &str) -> Result<(), DomainError> {
    if !(1..=120).contains(&value.len()) || !value.bytes().all(|byte| (b'!'..=b'~').contains(&byte))
    {
        return Err(bad("storage_version"));
    }
    Ok(())
}

pub fn spec_shape(value: &JobSpecV1) -> Result<(), DomainError> {
    if value.external_job_id != external_id(value.run_id, value.attempt_no)? {
        return Err(bad("external_job_id"));
    }
    if !runtime::pinned_image(&value.image_ref) {
        return Err(bad("image_ref"));
    }
    if let Some(deadline) = value.deadline_at {
        time(deadline)?;
    }
    let limits = &value.limits;
    if limits.cpu == Some(0)
        || (limits.cpu_seconds.is_some() && limits.cpu.is_none())
        || limits.cpu_seconds.is_some_and(|value| value.get() == 0)
        || limits.memory_mib == Some(0)
        || limits.wall_seconds == Some(0)
        || limits.output_bytes.is_some_and(|value| value.get() == 0)
        || limits
            .wall_seconds
            .zip(limits.cpu_seconds)
            .zip(limits.cpu)
            .is_some_and(|((wall, cpu), rate)| {
                u128::from(cpu.get()) > u128::from(rate) * u128::from(wall)
            })
    {
        return Err(bad("limits"));
    }
    if value.inputs.is_empty() {
        return Err(bad("inputs"));
    }
    let mut identities = BTreeSet::new();
    let mut native_datasets = BTreeSet::new();
    let mut total_bytes = 0u64;
    for input in &value.inputs {
        match input {
            RuntimeInputV1::Dataset {
                revision_id,
                registered_ref,
                storage_version: version,
                ..
            } => {
                text(registered_ref, 1, 512, false).map_err(|_| bad("inputs.registered_ref"))?;
                storage_version(version)?;
                if !identities.insert(*revision_id)
                    || *revision_id == value.parameters_artifact_id
                    || !native_datasets.insert((registered_ref, version))
                {
                    return Err(bad("inputs"));
                }
            }
            RuntimeInputV1::Artifact {
                artifact_id,
                storage_version: version,
                byte_count,
                role,
            } => {
                storage_version(version)?;
                if !identities.insert(*artifact_id)
                    || byte_count.get() == 0
                    || (*artifact_id == value.parameters_artifact_id
                        && *role != ArtifactInputRole::Parameters)
                {
                    return Err(bad("inputs"));
                }
                total_bytes = total_bytes
                    .checked_add(byte_count.get())
                    .ok_or_else(|| bad("inputs.byte_count"))?;
            }
        }
    }
    let mut schemas = BTreeSet::new();
    if value.requested_output_schemas.is_empty() {
        return Err(bad("requested_output_schemas"));
    }
    for schema in &value.requested_output_schemas {
        text(&schema.name, 1, 120, false).map_err(|_| bad("requested_output_schemas"))?;
        text(&schema.version, 1, 40, false).map_err(|_| bad("requested_output_schemas"))?;
        if !schemas.insert((&schema.name, &schema.version)) {
            return Err(bad("requested_output_schemas"));
        }
    }
    Ok(())
}
