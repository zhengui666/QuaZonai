use contracts::{budget::BudgetV1, DecimalValue};
use serde_json::{json, Value};
use std::cmp::Ordering;

#[test]
fn generated_integer_bounds_match_native_wire_representations() {
    let document: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let schemas = &document["components"]["schemas"];
    for field in [
        "max_parallel_runs",
        "max_turns_per_mission",
        "max_cycles_per_day",
    ] {
        assert_eq!(
            schemas["BudgetV1"]["allOf"][0]["properties"][field]["minimum"],
            json!(1),
            "{field}"
        );
        assert_eq!(
            schemas["BudgetV1"]["allOf"][0]["properties"][field]["maximum"],
            json!(65535),
            "{field}"
        );
    }
    assert_eq!(
        schemas["BudgetV1"]["allOf"][0]["properties"]["max_repair_turns"]["minimum"],
        json!(0)
    );
    assert_eq!(
        schemas["BudgetV1"]["allOf"][0]["properties"]["max_repair_turns"]["maximum"],
        json!(65535)
    );
    for field in ["stop_on_qualified_count", "stop_on_no_improvement_trials"] {
        assert_eq!(
            schemas["StopRuleV1"]["properties"][field]["minimum"],
            json!(1),
            "{field}"
        );
        assert_eq!(
            schemas["StopRuleV1"]["properties"][field]["maximum"],
            json!(65535),
            "{field}"
        );
    }
    for field in ["max_experiments", "max_wall_seconds", "max_memory_mib"] {
        assert_eq!(
            schemas["BudgetV1"]["allOf"][0]["properties"][field]["minimum"],
            json!(1),
            "{field}"
        );
        assert_eq!(
            schemas["BudgetV1"]["allOf"][0]["properties"][field]["maximum"],
            json!(4294967295u64),
            "{field}"
        );
    }
    assert_eq!(
        schemas["BudgetV1"]["allOf"][0]["properties"]["min_cycle_interval_seconds"]["minimum"],
        json!(0)
    );
    assert_eq!(
        schemas["BudgetV1"]["allOf"][0]["properties"]["min_cycle_interval_seconds"]["maximum"],
        json!(4294967295u64)
    );
    // Revision already has the canonical strictly-positive bigint boundary,
    // covered by the shared Rust/ECMAScript corpus. Compare the complete schema
    // rather than a regex substring that could miss a weakened restriction.
    let positive = &schemas["Revision"];
    for field in ["max_cpu_seconds", "max_output_bytes"] {
        assert_eq!(
            &schemas["BudgetV1"]["allOf"][0]["properties"][field], positive,
            "{field}"
        );
    }
    let max_tokens = schemas["BudgetV1"]["allOf"][0]["properties"]["max_tokens"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(max_tokens.len(), 2);
    assert!(max_tokens.contains(&json!({"type": "null"})));
    assert!(max_tokens.contains(positive));

    let mut value = json!({"schema_version":1,"max_experiments":20,"max_parallel_runs":65535,
        "max_turns_per_mission":16,"max_repair_turns":2,"max_wall_seconds":3600,
        "max_cpu_seconds":"7200","max_memory_mib":4096,"max_output_bytes":"67108864",
        "max_cycles_per_day":3,"min_cycle_interval_seconds":120,"max_tokens":null,
        "max_cost_decimal":null,"cost_currency":null,"cost_enforcement":"UNAVAILABLE"});
    assert!(serde_json::from_value::<BudgetV1>(value.clone()).is_ok());
    // These are unsigned wire bounds, not a claim that every combination
    // satisfies cross-field admission rules such as repair_turns <= total_turns.
    for (field, maximum) in [
        ("max_repair_turns", u64::from(u16::MAX)),
        ("min_cycle_interval_seconds", u64::from(u32::MAX)),
    ] {
        for number in [0, maximum] {
            let mut valid = value.clone();
            valid[field] = json!(number);
            assert!(serde_json::from_value::<BudgetV1>(valid).is_ok(), "{field}");
        }
        for invalid in [
            json!(-1),
            json!(maximum + 1),
            json!(0.5),
            json!("0"),
            json!(null),
        ] {
            let mut rejected = value.clone();
            rejected[field] = invalid;
            assert!(
                serde_json::from_value::<BudgetV1>(rejected).is_err(),
                "{field}"
            );
        }
    }
    value["max_parallel_runs"] = json!(65536);
    assert!(serde_json::from_value::<BudgetV1>(value).is_err());
}

#[test]
fn observable_metric_comparison_preserves_exact_decimal_thresholds() {
    let exact: DecimalValue = "0.1".parse().unwrap();
    let greater: DecimalValue = "0.10000000000000001".parse().unwrap();
    assert_eq!(exact.compare_metric(0.1).unwrap(), Ordering::Equal);
    assert_eq!(greater.compare_metric(0.1).unwrap(), Ordering::Less);
    assert_eq!(
        "-0.10000000000000001"
            .parse::<DecimalValue>()
            .unwrap()
            .compare_metric(-0.1)
            .unwrap(),
        Ordering::Greater
    );
    assert_eq!(
        "0".parse::<DecimalValue>()
            .unwrap()
            .compare_metric(f64::MIN_POSITIVE)
            .unwrap(),
        Ordering::Greater
    );
    assert_eq!(exact.compare_metric(f64::MAX).unwrap(), Ordering::Greater);
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(exact.compare_metric(number).is_err());
    }
}
