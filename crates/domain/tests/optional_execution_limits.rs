use chrono::{Duration, Utc};
use contracts::{budget::BudgetV1, DbCounter};
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
    job_resource_limits(&b, None, 1024, None).unwrap();
    job_resource_limits(&b, Some(3600), 1024, output).unwrap();
    b.max_wall_seconds = Some(120);
    assert!(job_resource_limits(&b, None, 1024, output).is_err());
    assert!(job_resource_limits(&b, Some(121), 1024, output).is_err());
    job_resource_limits(&b, Some(120), 1024, output).unwrap();
    b.max_output_bytes = output;
    assert!(job_resource_limits(&b, Some(120), 1024, None).is_err());
    assert!(job_resource_limits(&b, Some(120), 1024, Some(DbCounter::new(1025).unwrap())).is_err());
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
fn absent_totals_preserve_memory_and_finite_core_rate() {
    let b = budget();
    assert!(job_resource_limits(&b, None, 1025, None).is_err());
    job_resource_limits(&b, None, 1024, None).unwrap();
    assert_eq!(native_cpu_rate(None, None).unwrap(), 1);
    assert_eq!(native_cpu_rate(None, Some(10)).unwrap(), 1);
    assert_eq!(
        native_cpu_rate(Some(DbCounter::new(30).unwrap()), Some(10)).unwrap(),
        3
    );
    assert!(native_cpu_rate(None, Some(0)).is_err());
}
