use chrono::{Duration, Utc};
use contracts::DbCounter;
use domain::execution_limits::*;

#[test]
fn no_deadline_does_not_expire_but_finite_parent_is_preserved() {
    let now = Utc::now();
    assert_eq!(deadline(now, None).unwrap(), None);
    assert!(!expired(None, now + Duration::days(365)));
    let end = now + Duration::seconds(120);
    assert_eq!(earlier(None, Some(end)), Some(end));
    assert_eq!(earlier(Some(end), None), Some(end));
    assert_eq!(
        earlier(Some(end), Some(end + Duration::seconds(1))),
        Some(end)
    );
    assert!(!expired(Some(end), now));
    assert!(expired(Some(end), end));
    assert!(deadline(now, Some(0)).is_err());
}

#[test]
fn absent_cpu_totals_remove_rate_without_replacing_explicit_memory() {
    assert_eq!(native_cpu_rate(None, None).unwrap(), None);
    assert_eq!(native_cpu_rate(None, Some(10)).unwrap(), None);
    assert_eq!(
        native_cpu_rate(Some(DbCounter::new(30).unwrap()), Some(10)).unwrap(),
        Some(3)
    );
    assert!(native_cpu_rate(None, Some(0)).is_err());
}

#[test]
fn long_explicit_data_jobs_are_not_rejected_by_an_unrelated_day_cap() {
    let limits = contracts::lifecycle::JobLimitsV1 {
        schema_version: contracts::SchemaV1,
        experiments: 0,
        cpu_seconds: None,
        wall_seconds: Some(86_401),
        memory_mib: Some(1024),
        output_bytes: None,
    };
    domain::data::bounded_native_limits(&limits).unwrap();
    let absent = contracts::lifecycle::JobLimitsV1 {
        wall_seconds: None,
        ..limits.clone()
    };
    domain::data::bounded_native_limits(&absent).unwrap();
    let zero = contracts::lifecycle::JobLimitsV1 {
        wall_seconds: Some(0),
        ..limits
    };
    assert!(domain::data::bounded_native_limits(&zero).is_err());
}

#[test]
fn memory_none_is_not_a_zero_or_physical_capacity_allocation() {
    for memory in [None, Some(1), Some(u32::MAX)] {
        let limits = contracts::lifecycle::JobLimitsV1 {
            schema_version: contracts::SchemaV1,
            experiments: 0,
            cpu_seconds: None,
            wall_seconds: None,
            memory_mib: memory,
            output_bytes: None,
        };
        domain::data::bounded_native_limits(&limits).unwrap();
        assert_eq!(forward_evaluation(&limits).unwrap().memory_mib, memory);
    }
    assert!(native_cpu_rate(Some(DbCounter::new(0).unwrap()), Some(1)).is_err());
    assert!(native_cpu_rate(Some(DbCounter::new(1).unwrap()), None).is_err());
    assert_eq!(
        native_cpu_rate(Some(DbCounter::new(31).unwrap()), Some(10)).unwrap(),
        Some(4)
    );
}
