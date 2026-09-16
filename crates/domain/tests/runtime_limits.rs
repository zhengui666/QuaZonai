//! Runtime resource shape and generated-map boundary counterparts.
#[path = "../../../tests/support/runtime.rs"]
mod support;
use chrono::Utc;
use contracts::{lifecycle::JobLimitsV1, DbCounter, SchemaV1};
use domain::{runtime, DomainError};

#[test]
fn runtime_capacity_is_separate_from_cumulative_cpu_accounting() {
    let capabilities = support::capabilities(Utc::now());
    let limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 1,
        cpu_seconds: DbCounter::new(1_000_000).unwrap(),
        wall_seconds: capabilities.max_wall_seconds,
        memory_mib: capabilities.max_memory_mib,
        output_bytes: capabilities.max_output_bytes,
    };
    assert!(runtime::job_limits(&capabilities, &limits).is_ok());
    for value in [0, capabilities.max_wall_seconds + 1] {
        let invalid = JobLimitsV1 {
            wall_seconds: value,
            ..limits.clone()
        };
        assert!(matches!(
            runtime::job_limits(&capabilities, &invalid),
            Err(DomainError::CapabilityUnavailable("runtime_wall_seconds"))
        ));
    }
    for value in [0, capabilities.max_memory_mib + 1] {
        let invalid = JobLimitsV1 {
            memory_mib: value,
            ..limits.clone()
        };
        assert!(matches!(
            runtime::job_limits(&capabilities, &invalid),
            Err(DomainError::CapabilityUnavailable("runtime_memory_mib"))
        ));
    }
    for value in [0, capabilities.max_output_bytes.get() + 1] {
        let invalid = JobLimitsV1 {
            output_bytes: DbCounter::new(value).unwrap(),
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
