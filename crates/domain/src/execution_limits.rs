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

/// Optional execution rate. No CPU budget means no application CPU quota;
/// physical parent cgroups still apply. Finite budgets retain their original rate.
pub fn native_cpu_rate(
    cpu: Option<DbCounter>,
    wall: Option<u32>,
) -> Result<Option<u32>, DomainError> {
    match (cpu, wall) {
        (_, Some(0)) => Err(DomainError::Invalid("wall_seconds")),
        (Some(cpu), _) if cpu.get() == 0 => Err(DomainError::Invalid("cpu_seconds")),
        (Some(cpu), Some(wall)) => u32::try_from(cpu.get().div_ceil(u64::from(wall)))
            .map(Some)
            .map_err(|_| DomainError::CapabilityUnavailable("native_cpu_capacity")),
        (Some(_), None) => Err(DomainError::CapabilityUnavailable(
            "independent_cpu_enforcement",
        )),
        (None, _) => Ok(None),
    }
}

/// Standalone forward measurement inherits the original candidate's execution
/// choices. Finite legacy ceilings remain; absent caps stay absent. This is a
/// measurement, not another trial or a new research budget.
pub fn forward_evaluation(
    original: &contracts::lifecycle::JobLimitsV1,
) -> Result<contracts::lifecycle::JobLimitsV1, DomainError> {
    if original.memory_mib == Some(0)
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
    Ok(contracts::lifecycle::JobLimitsV1 {
        experiments: 0,
        ..original.clone()
    })
}
