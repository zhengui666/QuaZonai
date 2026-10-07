//! Forward execution inheritance does not change scientific/source validity.
#[path = "../../../tests/support/runtime.rs"]
mod support;
use contracts::{lifecycle::JobLimitsV1, DbCounter, SchemaV1};
use domain::{execution_limits::forward_evaluation, DomainError};
fn source(cpu: Option<u64>, wall: Option<u32>, memory: u32, output: Option<u64>) -> JobLimitsV1 {
    JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 3,
        cpu_seconds: cpu.map(|n| DbCounter::new(n).unwrap()),
        wall_seconds: wall,
        memory_mib: memory,
        output_bytes: output.map(|n| DbCounter::new(n).unwrap()),
    }
}
#[test]
fn absent_source_caps_stay_absent_with_memory_and_no_trial() {
    let result = forward_evaluation(&source(None, None, 2048, None)).unwrap();
    assert_eq!(
        result,
        JobLimitsV1 {
            experiments: 0,
            ..source(None, None, 512, None)
        }
    );
}
#[test]
fn finite_measurement_keeps_legacy_ceilings_and_narrows_to_original() {
    for (input, expected) in [
        (
            source(Some(300), Some(3600), 4096, Some(67108864)),
            source(Some(30), Some(60), 512, Some(1048576)),
        ),
        (
            source(Some(4), Some(5), 128, Some(512)),
            source(Some(4), Some(5), 128, Some(512)),
        ),
        (
            source(Some(30), Some(1), 512, Some(1048576)),
            source(Some(1), Some(1), 512, Some(1048576)),
        ),
        (
            source(None, Some(90), 512, None),
            source(None, Some(60), 512, None),
        ),
    ] {
        let result = forward_evaluation(&input).unwrap();
        assert_eq!(
            result,
            JobLimitsV1 {
                experiments: 0,
                ..expected
            }
        );
        if let (Some(cpu), Some(wall)) = (result.cpu_seconds, result.wall_seconds) {
            assert!(
                cpu.get() <= u64::from(wall),
                "fixed one-core execution must cover the finite request"
            );
        }
    }
}
#[test]
fn invalid_original_limits_and_unenforced_cpu_are_refused() {
    for input in [
        source(Some(0), Some(1), 512, None),
        source(None, Some(0), 512, None),
        source(None, None, 0, None),
        source(None, None, 512, Some(0)),
    ] {
        assert!(matches!(
            forward_evaluation(&input),
            Err(DomainError::Invalid("forward_parent_limits"))
        ));
    }
    assert!(matches!(
        forward_evaluation(&source(Some(1), None, 512, None)),
        Err(DomainError::CapabilityUnavailable(
            "independent_cpu_enforcement"
        ))
    ));
}
#[test]
fn absent_caps_require_each_native_runtime_capability() {
    let result = forward_evaluation(&source(None, None, 512, None)).unwrap();
    let caps = support::capabilities(chrono::Utc::now());
    assert!(domain::runtime::job_limits(&caps, &result).is_ok());
    for name in [
        "optional-wall-time",
        "optional-cpu-budget",
        "optional-output-budget",
    ] {
        let mut old = caps.clone();
        old.engine_versions.remove(name);
        assert!(matches!(
            domain::runtime::job_limits(&old, &result),
            Err(DomainError::CapabilityUnavailable(_))
        ));
    }
}
