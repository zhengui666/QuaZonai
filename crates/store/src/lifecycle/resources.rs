//! Native scientific resource calculation, independent of Agent orchestration.
use crate::StoreError;
use contracts::{lifecycle::JobLimitsV1, runtime::RuntimeCapabilitiesV1};
use domain::DomainError;

pub(crate) fn native_cpu(
    limits: &JobLimitsV1,
    capabilities: &RuntimeCapabilitiesV1,
) -> Result<Option<u32>, StoreError> {
    domain::runtime::job_limits(capabilities, limits)?;
    if limits.wall_seconds == Some(0) {
        return Err(DomainError::BudgetExhausted("wall_seconds").into());
    }
    let cpu = domain::execution_limits::native_cpu_rate(limits.cpu_seconds, limits.wall_seconds)?;
    if cpu.is_some_and(|cpu| cpu == 0 || cpu > capabilities.max_cpu) {
        return Err(DomainError::CapabilityUnavailable("native_cpu_capacity").into());
    }
    Ok(cpu)
}

#[test]
fn cpu_capacity_uses_the_actual_bounded_wall_allocation() {
    use contracts::{DbCounter, SchemaV1};
    let mut capabilities: RuntimeCapabilitiesV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/runtime-capabilities.fixture.json"
    ))
    .unwrap();
    let mut limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: Some(DbCounter::new(10).unwrap()),
        wall_seconds: Some(5),
        memory_mib: Some(1024),
        output_bytes: Some(DbCounter::new(1024).unwrap()),
    };
    assert_eq!(native_cpu(&limits, &capabilities).unwrap(), Some(2));
    limits.wall_seconds = Some(4);
    assert!(native_cpu(&limits, &capabilities).is_err());
    limits.wall_seconds = Some(0);
    assert!(native_cpu(&limits, &capabilities).is_err());
    limits.cpu_seconds = None;
    limits.wall_seconds = None;
    assert!(native_cpu(&limits, &capabilities).is_err());
    for capability in [
        "optional-wall-time",
        "optional-cpu-budget",
        "optional-cpu-rate",
    ] {
        capabilities
            .engine_versions
            .insert(capability.into(), "1".into());
    }
    assert_eq!(native_cpu(&limits, &capabilities).unwrap(), None);
}
