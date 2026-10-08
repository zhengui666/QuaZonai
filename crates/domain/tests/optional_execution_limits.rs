use chrono::{Duration, Utc};
use contracts::{DbCounter, budget::BudgetV1};
use domain::{admission::job_resource_limits, execution_limits::*};

fn budget() -> BudgetV1 {
    serde_json::from_value(serde_json::json!({
        "schema_version":1,"max_experiments":32,"max_parallel_runs":1,
        "max_turns_per_mission":64,"max_repair_turns":8,"max_wall_seconds":null,
        "max_cpu_seconds":null,"max_memory_mib":1024,"max_output_bytes":null,
        "max_cycles_per_day":12,"min_cycle_interval_seconds":0,"max_tokens":null,
        "max_cost_decimal":null,"cost_currency":null,"cost_enforcement":"UNAVAILABLE"
    }))
    .unwrap()
}

#[test]
fn absent_is_not_a_sentinel_or_a_bypass_for_a_finite_budget() {
    let mut b = budget();
    assert_eq!(b.max_wall_seconds, None);
    assert_eq!(b.max_cpu_seconds, None);
    assert_eq!(b.max_tokens, None);
    assert_eq!(b.max_output_bytes, None);
    let output = Some(DbCounter::new(1024).unwrap());
    job_resource_limits(&b, None, Some(1024), None).unwrap();
    job_resource_limits(&b, Some(3600), Some(1024), output).unwrap();
    b.max_wall_seconds = Some(120);
    assert!(job_resource_limits(&b, None, Some(1024), output).is_err());
    assert!(job_resource_limits(&b, Some(121), Some(1024), output).is_err());
    job_resource_limits(&b, Some(120), Some(1024), output).unwrap();
    b.max_output_bytes = output;
    assert!(job_resource_limits(&b, Some(120), Some(1024), None).is_err());
    assert!(
        job_resource_limits(
            &b,
            Some(120),
            Some(1024),
            Some(DbCounter::new(1025).unwrap())
        )
        .is_err()
    );
}

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
    let b = budget();
    assert!(job_resource_limits(&b, None, Some(1025), None).is_err());
    job_resource_limits(&b, None, Some(1024), None).unwrap();
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
    let absent = contracts::lifecycle::JobLimitsV1 { wall_seconds: None, ..limits.clone() };
    domain::data::bounded_native_limits(&absent).unwrap();
    let zero = contracts::lifecycle::JobLimitsV1 { wall_seconds: Some(0), ..limits };
    assert!(domain::data::bounded_native_limits(&zero).is_err());
}

#[test]
fn memory_none_is_not_a_zero_or_physical_capacity_allocation() {
    let mut b = budget();
    assert!(job_resource_limits(&b, None, None, None).is_err());
    b.max_memory_mib = None;
    for memory in [None, Some(1), Some(u32::MAX)] {
        job_resource_limits(&b, None, memory, None).unwrap();
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
