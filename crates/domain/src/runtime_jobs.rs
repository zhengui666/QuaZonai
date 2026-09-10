//! The wire boundary is not an executor or a qualification engine.
use crate::{control::text, research::invalid, runtime, DomainError};
use chrono::{DateTime, Duration, Utc};
use contracts::{research::ArtifactInputRole, runtime::RuntimeCapabilitiesV1, runtime_jobs::*, Id};
use std::collections::BTreeSet;

pub const MAX_JOB_REQUEST_BYTES: usize = 1024 * 1024;
pub const MAX_RESULT_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_INPUT_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_INPUT_OBJECTS_BYTES: u64 = 256 * 1024 * 1024;

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
    time(value.deadline_at)?;
    let limits = &value.limits;
    if !(1..=1024).contains(&limits.cpu)
        || limits.cpu_seconds.get() == 0
        || limits.memory_mib == 0
        || limits.wall_seconds == 0
        || limits.output_bytes.get() == 0
        || limits.output_bytes.get() > MAX_INPUT_OBJECT_BYTES
        || u128::from(limits.cpu_seconds.get())
            > u128::from(limits.cpu) * u128::from(limits.wall_seconds)
    {
        return Err(bad("limits"));
    }
    if !(1..=256).contains(&value.inputs.len()) {
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
                    || byte_count.get() > MAX_INPUT_OBJECT_BYTES
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
    if total_bytes > MAX_INPUT_OBJECTS_BYTES {
        return Err(bad("inputs.byte_count"));
    }
    let mut schemas = BTreeSet::new();
    if !(1..=64).contains(&value.requested_output_schemas.len()) {
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

/// Called only for a new identity. A previous immutable receipt is read first,
/// so an expired deadline never changes a successful idempotent replay.
pub fn admit_spec(
    value: &JobSpecV1,
    capability: &RuntimeCapabilitiesV1,
    now: DateTime<Utc>,
) -> Result<(), DomainError> {
    spec_shape(value)?;
    runtime::capabilities(capability, now)?;
    let limits = &value.limits;
    if value.deadline_at <= now
        || !capability.job_kinds.contains(&value.job_kind)
        || !capability
            .image_refs
            .iter()
            .any(|image| image.job_kind == value.job_kind && image.image_ref == value.image_ref)
        || limits.cpu > capability.max_cpu
        || limits.memory_mib > capability.max_memory_mib
        || limits.wall_seconds > capability.max_wall_seconds
        || limits.output_bytes > capability.max_output_bytes
        || value.requested_output_schemas.iter().any(|requested| {
            !capability.artifact_schemas.iter().any(|available| {
                requested.name == available.name && requested.version == available.version
            })
        })
    {
        return Err(invalid("job_spec", "RUNTIME_JOB_NOT_ADMITTED"));
    }
    Ok(())
}

pub fn status(
    value: &RuntimeJobStatusV1,
    run: Id,
    attempt: u32,
    observed_at: DateTime<Utc>,
) -> Result<(), DomainError> {
    if value.run_id != run
        || value.attempt_no != attempt
        || value.external_job_id != external_id(run, attempt)?
        || value.finished_at.is_some() != value.state.is_terminal()
        || value.submitted_at > observed_at + Duration::seconds(5)
    {
        return Err(bad("job_status"));
    }
    time(value.submitted_at)?;
    if let Some(started) = value.started_at {
        time(started)?;
        if started < value.submitted_at || started > observed_at + Duration::seconds(5) {
            return Err(bad("job_status.started_at"));
        }
    }
    if let Some(finished) = value.finished_at {
        time(finished)?;
        if finished < value.started_at.unwrap_or(value.submitted_at)
            || finished > observed_at + Duration::seconds(5)
        {
            return Err(bad("job_status.finished_at"));
        }
    }
    let valid = match value.state {
        RuntimeJobState::Accepted => !value.has_result && value.started_at.is_none(),
        RuntimeJobState::Running => !value.has_result && value.started_at.is_some(),
        RuntimeJobState::CancelRequested => !value.has_result,
        RuntimeJobState::Succeeded => value.has_result && value.started_at.is_some(),
        RuntimeJobState::Failed => value.has_result,
        RuntimeJobState::Cancelled => value.has_result || value.started_at.is_none(),
    };
    if !valid {
        return Err(bad("job_status"));
    }
    Ok(())
}

pub fn error(code: RuntimeFailureCode) -> RuntimeJobErrorV1 {
    use RuntimeFailureClass::{InvalidInput, PermanentConfig, ResourceLimit, RetryableInfra};
    use RuntimeFailureCode as Code;
    let (class, message) = match code {
        Code::EngineUnavailable => (RetryableInfra, "Native execution engine is unavailable."),
        Code::ImageUnavailable => (PermanentConfig, "The pinned native image is unavailable."),
        Code::ContractUnsupported => (PermanentConfig, "The native job contract is unsupported."),
        Code::InputUnavailable => (InvalidInput, "An immutable job input is unavailable."),
        Code::InvalidInput => (InvalidInput, "A native job input failed validation."),
        Code::NativeJobFailed => (InvalidInput, "The native job process failed."),
        Code::InvalidOutput => (InvalidInput, "Native output failed contract validation."),
        Code::CpuLimit => (ResourceLimit, "The native job exceeded its CPU limit."),
        Code::MemoryLimit => (ResourceLimit, "The native job exceeded its memory limit."),
        Code::OutputLimit => (ResourceLimit, "The native job exceeded its output limit."),
        Code::DeadlineExceeded => (ResourceLimit, "The native job exceeded its deadline."),
    };
    RuntimeJobErrorV1 {
        class,
        code,
        safe_message: message.to_owned(),
    }
}

fn media_type(output: &RuntimeOutputV1) -> bool {
    output.media_type
        == match output.kind {
            RuntimeOutputKind::Model => "application/wasm",
            RuntimeOutputKind::Signals | RuntimeOutputKind::Targets => {
                "application/vnd.apache.arrow.file"
            }
            RuntimeOutputKind::Report
            | RuntimeOutputKind::Metrics
            | RuntimeOutputKind::DataQuality => "application/json",
        }
}

/// Validate identity, metadata and resource accounting before reading artifacts.
/// Actual bytes and scientific schemas must still be independently verified.
pub fn manifest(
    value: &ResultManifestV1,
    spec: &JobSpecV1,
    not_before: DateTime<Utc>,
    observed_at: DateTime<Utc>,
) -> Result<(), DomainError> {
    spec_shape(spec)?;
    if value.run_id != spec.run_id
        || value.attempt_no != spec.attempt_no
        || value.external_job_id != spec.external_job_id
        || value.input_set_id != spec.input_set_id
        || value.finished_at < not_before - Duration::seconds(5)
        || value.finished_at > observed_at + Duration::seconds(5)
        || !(1..=64).contains(&value.engine_versions.len())
        || value.engine_versions.iter().any(|(name, version)| {
            text(name, 1, 120, false).is_err() || text(version, 1, 120, false).is_err()
        })
        || value.artifacts.len() > 64
    {
        return Err(bad("result_manifest"));
    }
    time(value.finished_at)?;
    if let Some(started) = value.started_at {
        time(started)?;
        if started < not_before - Duration::seconds(5) || started > value.finished_at {
            return Err(bad("result_manifest.started_at"));
        }
    }
    let failed = value.state == RuntimeResultState::Failed;
    if failed != value.error.is_some()
        || value
            .error
            .as_ref()
            .is_some_and(|failure| *failure != error(failure.code))
        || (value.state != RuntimeResultState::Succeeded && !value.artifacts.is_empty())
    {
        return Err(bad("result_manifest.error"));
    }
    let mut objects = BTreeSet::new();
    let mut produced_schemas = BTreeSet::new();
    let mut total = 0u64;
    for output in &value.artifacts {
        if output.storage_version.get() != 1
            || !objects.insert(output.storage_ref)
            || output.byte_count.get() == 0
            || output.byte_count.get() > spec.limits.output_bytes.get()
            || !media_type(output)
            || !spec.requested_output_schemas.iter().any(|requested| {
                requested.name == output.schema.name && requested.version == output.schema.version
            })
        {
            return Err(bad("result_manifest.artifacts"));
        }
        total = total
            .checked_add(output.byte_count.get())
            .ok_or_else(|| bad("result_manifest.artifacts"))?;
        produced_schemas.insert((&output.schema.name, &output.schema.version));
    }
    if total != value.resource_usage.output_bytes.get() || total > spec.limits.output_bytes.get() {
        return Err(bad("result_manifest.resource_usage"));
    }
    if value.state == RuntimeResultState::Succeeded
        && (value.started_at.is_none()
            || value.finished_at > spec.deadline_at
            || value.artifacts.is_empty()
            || spec.requested_output_schemas.iter().any(|requested| {
                !produced_schemas.contains(&(&requested.name, &requested.version))
            })
            || value.resource_usage.wall_milliseconds.get()
                > u64::from(spec.limits.wall_seconds) * 1000
            || value.resource_usage.cpu_nanoseconds.is_some_and(|cpu| {
                u128::from(cpu.get()) > u128::from(spec.limits.cpu_seconds.get()) * 1_000_000_000
            })
            || value
                .resource_usage
                .peak_memory_bytes
                .is_some_and(|memory| {
                    memory.get() > u64::from(spec.limits.memory_mib) * 1024 * 1024
                }))
    {
        return Err(bad("result_manifest.success"));
    }
    // Failed jobs retain actual over-limit usage. Do not clamp usage into a false success.
    Ok(())
}
