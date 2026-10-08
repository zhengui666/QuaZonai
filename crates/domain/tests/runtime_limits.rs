//! Runtime resource shape and generated-map boundary counterparts.
#[path = "../../../tests/support/runtime.rs"]
mod support;
use chrono::Utc;
use contracts::{lifecycle::JobLimitsV1, DbCounter, SchemaV1};
use domain::{runtime, DomainError};

#[test]
fn runtime_capacity_is_separate_from_cumulative_cpu_accounting() {
    let mut capabilities = support::capabilities(Utc::now());
    capabilities.max_wall_seconds = Some(3600);
    capabilities.max_output_bytes = Some(DbCounter::new(64 * 1024 * 1024).unwrap());
    let limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 1,
        cpu_seconds: Some(DbCounter::new(1_000_000).unwrap()),
        wall_seconds: capabilities.max_wall_seconds,
        memory_mib: Some(capabilities.max_memory_mib),
        output_bytes: capabilities.max_output_bytes,
    };
    assert!(runtime::job_limits(&capabilities, &limits).is_ok());
    for value in [0, capabilities.max_wall_seconds.unwrap() + 1] {
        let invalid = JobLimitsV1 {
            wall_seconds: Some(value),
            ..limits.clone()
        };
        assert!(matches!(
            runtime::job_limits(&capabilities, &invalid),
            Err(DomainError::CapabilityUnavailable("runtime_wall_seconds"))
        ));
    }
    for value in [0, capabilities.max_memory_mib + 1] {
        let invalid = JobLimitsV1 {
            memory_mib: Some(value),
            ..limits.clone()
        };
        assert!(matches!(
            runtime::job_limits(&capabilities, &invalid),
            Err(DomainError::CapabilityUnavailable("runtime_memory_mib"))
        ));
    }
    for value in [0, capabilities.max_output_bytes.unwrap().get() + 1] {
        let invalid = JobLimitsV1 {
            output_bytes: Some(DbCounter::new(value).unwrap()),
            ..limits.clone()
        };
        assert!(matches!(
            runtime::job_limits(&capabilities, &invalid),
            Err(DomainError::CapabilityUnavailable("runtime_output_bytes"))
        ));
    }
}

#[test]
fn engine_version_bounds_include_native_unicode_key_semantics() {
    let now = Utc::now();
    let mut capabilities = support::capabilities(now);
    for count in [0, 1, 64, 65] {
        capabilities.engine_versions = (0..count)
            .map(|index| (format!("engine-{index}"), "1".into()))
            .collect();
        assert_eq!(
            runtime::capabilities(&capabilities, now).is_ok(),
            (1..=64).contains(&count)
        );
    }
    for valid in [
        "a".to_owned(),
        "a".repeat(120),
        "😀".repeat(120),
        "\u{feff}".to_owned(),
    ] {
        capabilities.engine_versions = [(valid.clone(), valid)].into();
        assert!(runtime::capabilities(&capabilities, now).is_ok());
    }
    for invalid in [
        "".to_owned(),
        "a".repeat(121),
        "😀".repeat(121),
        " ".to_owned(),
        "\t".to_owned(),
        "\n".to_owned(),
        "a\0".to_owned(),
        "a\u{85}".to_owned(),
        "\u{3000}".to_owned(),
    ] {
        capabilities.engine_versions = [(invalid.clone(), "1".into())].into();
        assert!(runtime::capabilities(&capabilities, now).is_err());
        capabilities.engine_versions = [("engine".into(), invalid)].into();
        assert!(runtime::capabilities(&capabilities, now).is_err());
    }
}

#[test]
fn optional_wall_capability_preserves_uncapped_jobs_with_a_finite_request_ceiling() {
    let now = Utc::now();
    let mut capabilities = support::capabilities(now);
    capabilities.max_wall_seconds = None;
    let limits = JobLimitsV1 { schema_version: SchemaV1, experiments: 0,
        cpu_seconds: None, wall_seconds: None,
        memory_mib: Some(capabilities.max_memory_mib), output_bytes: None };
    runtime::job_limits(&capabilities, &limits).unwrap();
    capabilities.max_wall_seconds = Some(3600);
    runtime::job_limits(&capabilities, &limits).unwrap();
    capabilities.engine_versions.remove("optional-wall-time");
    assert!(matches!(runtime::job_limits(&capabilities, &limits),
        Err(DomainError::CapabilityUnavailable("runtime_wall_seconds"))));
    capabilities.max_wall_seconds = Some(0);
    assert!(runtime::capabilities(&capabilities, now).is_err());
}

#[test]
fn absent_output_capacity_has_no_default_and_explicit_capacity_is_authoritative() {
    let mut capabilities = support::capabilities(Utc::now());
    assert_eq!(capabilities.max_output_bytes, None);
    let mut limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: None,
        wall_seconds: None,
        memory_mib: Some(64),
        output_bytes: None,
    };
    assert!(runtime::job_limits(&capabilities, &limits).is_ok());
    limits.output_bytes = Some(DbCounter::new(64 * 1024 * 1024 + 1).unwrap());
    assert!(runtime::job_limits(&capabilities, &limits).is_ok());
    capabilities.max_output_bytes = Some(DbCounter::new(64 * 1024 * 1024).unwrap());
    assert!(runtime::job_limits(&capabilities, &limits).is_err());
    limits.output_bytes = None;
    // Preserve the existing advertised optional-output-budget protocol.
    assert!(runtime::job_limits(&capabilities, &limits).is_ok());
    limits.output_bytes = capabilities.max_output_bytes;
    assert!(runtime::job_limits(&capabilities, &limits).is_ok());
    let legacy = serde_json::to_value(&capabilities).unwrap();
    assert_eq!(legacy["max_output_bytes"], "67108864");
    let replay: contracts::runtime::RuntimeCapabilitiesV1 = serde_json::from_value(legacy).unwrap();
    assert_eq!(replay.max_output_bytes, capabilities.max_output_bytes);
}

#[test]
fn new_absent_resource_quotas_require_both_capabilities_independently() {
    let capabilities = support::capabilities(Utc::now());
    let limits = JobLimitsV1 {
        schema_version: SchemaV1, experiments: 0, cpu_seconds: None,
        wall_seconds: None, memory_mib: None, output_bytes: None,
    };
    runtime::job_limits(&capabilities, &limits).unwrap();
    for name in ["optional-cpu-rate", "optional-memory-limit"] {
        let mut legacy = capabilities.clone();
        legacy.engine_versions.remove(name);
        assert!(runtime::job_limits(&legacy, &limits).is_err());
    }
}
