//! Explicit optional application limits. Identity leases and network timeouts are separate.
use crate::DomainError;
use chrono::{DateTime, Duration, Utc};
use contracts::DbCounter;

pub fn earlier<T: Ord + Copy>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}
pub fn expired(deadline: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    deadline.is_some_and(|deadline| deadline <= now)
}
pub fn exceeds<T: Ord>(requested: Option<T>, maximum: Option<T>) -> bool {
    maximum.is_some_and(|maximum| requested.is_none_or(|requested| requested > maximum))
}
pub fn deadline(
    now: DateTime<Utc>,
    seconds: Option<u32>,
) -> Result<Option<DateTime<Utc>>, DomainError> {
    seconds
        .map(|seconds| {
            if seconds == 0 {
                return Err(DomainError::Invalid("wall_seconds"));
            }
            now.checked_add_signed(Duration::seconds(i64::from(seconds)))
                .ok_or(DomainError::Invalid("deadline"))
        })
        .transpose()
}

/// Finite service/core-rate choice; absent total CPU never becomes a max integer.
pub fn native_cpu_rate(cpu: Option<DbCounter>, wall: Option<u32>) -> Result<u16, DomainError> {
    match (cpu, wall) {
        (Some(cpu), Some(wall)) if wall > 0 => u16::try_from(cpu.get().div_ceil(u64::from(wall)))
            .map_err(|_| DomainError::CapabilityUnavailable("native_cpu_capacity")),
        (_, Some(0)) => Err(DomainError::Invalid("wall_seconds")),
        _ => Ok(1),
    }
}

/// Standalone forward measurement inherits the original candidate's execution
/// choices. Finite legacy ceilings remain; absent caps stay absent. This is a
/// one-core measurement, not another trial or a new research budget.
pub fn forward_evaluation(
    original: &contracts::lifecycle::JobLimitsV1,
) -> Result<contracts::lifecycle::JobLimitsV1, DomainError> {
    if original.memory_mib == 0
        || original.wall_seconds == Some(0)
        || original.cpu_seconds.is_some_and(|value| value.get() == 0)
        || original.output_bytes.is_some_and(|value| value.get() == 0)
    {
        return Err(DomainError::Invalid("forward_parent_limits"));
    }
    if original.wall_seconds.is_none() && original.cpu_seconds.is_some() {
        return Err(DomainError::CapabilityUnavailable(
            "independent_cpu_enforcement",
        ));
    }
    let wall_seconds = original.wall_seconds.map(|wall| wall.min(60));
    let cpu_seconds = original
        .cpu_seconds
        .map(|cpu| {
            DbCounter::new(cpu.get().min(30).min(u64::from(wall_seconds.unwrap_or(30))))
                .map_err(|_| DomainError::Invalid("forward_parent_limits"))
        })
        .transpose()?;
    let output_bytes = original
        .output_bytes
        .map(|output| {
            DbCounter::new(output.get().min(1024 * 1024))
                .map_err(|_| DomainError::Invalid("forward_parent_limits"))
        })
        .transpose()?;
    Ok(contracts::lifecycle::JobLimitsV1 {
        schema_version: contracts::SchemaV1,
        experiments: 0,
        cpu_seconds,
        wall_seconds,
        memory_mib: original.memory_mib.min(512),
        output_bytes,
    })
}
