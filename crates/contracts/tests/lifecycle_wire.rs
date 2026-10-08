use contracts::{DbCounter, Id, SchemaV1, lifecycle::*, runs::RunState};
use serde_json::{from_value, json, to_value};

#[test]
fn run_commands_and_persistent_events_reject_unsupported_or_ambiguous_wire_values() {
    let command = json!({"schema_version":1,"expected_revision":"9007199254740993"});
    let parsed: RunCancelV1 = from_value(command.clone()).unwrap();
    assert_eq!(to_value(parsed).unwrap(), command);
    for invalid in [
        json!({"schema_version":2,"expected_revision":"1"}),
        json!({"schema_version":1,"expected_revision":1}),
        json!({"schema_version":1,"expected_revision":"1","state":"CANCELLED"}),
        json!({"schema_version":1}),
    ] {
        assert!(from_value::<RunCancelV1>(invalid).is_err());
    }
    let event = RunEventV1 {
        schema_version: SchemaV1,
        run_id: Id::new(),
        seq: DbCounter::new(42).unwrap(),
        attempt_id: None,
        event_type: RunEventKind::StateChanged.code().into(),
        occurred_at: "2026-09-06T00:00:00Z".parse().unwrap(),
        payload: to_value(RunStatePayload {
            schema_version: SchemaV1,
            state: RunState::CancelRequested,
            reason: RunReason::CancelRequested,
        })
        .unwrap(),
    };
    let valid = to_value(&event).unwrap();
    assert_eq!(valid["event_type"], "run.state_changed");
    assert_eq!(valid["seq"], "42");
    assert_eq!(from_value::<RunEventV1>(valid.clone()).unwrap(), event);
    let mut unknown = valid.clone();
    unknown["payload"]["reason"] = json!("FAKE_APPROVAL");
    assert!(from_value::<RunEventV1>(unknown).is_err());
    let mut secret = valid;
    secret["provider_key"] = json!("must-not-be-an-event-field");
    assert!(from_value::<RunEventV1>(secret).is_err());
}

#[test]
fn compatible_extension_envelopes_reject_invalid_names_and_versions() {
    let valid = json!({"schema_version":1,"run_id":Id::new(),"seq":"2","attempt_id":null,
        "event_type":"run.observations_processed","occurred_at":"2026-09-06T00:00:00Z",
        "payload":{"schema_version":1,"completed":"2"}});
    assert_eq!(
        to_value(from_value::<RunEventV1>(valid.clone()).unwrap()).unwrap(),
        valid
    );
    for name in [
        "".to_owned(),
        "Run.invalid".into(),
        "run.x\ninjected".into(),
        "a".repeat(121),
        "run.🦀".into(),
    ] {
        let mut bad = valid.clone();
        bad["event_type"] = json!(name);
        assert!(from_value::<RunEventV1>(bad).is_err());
    }
    for payload in [json!({"schema_version":2}), json!([]), json!(null)] {
        let mut bad = valid.clone();
        bad["payload"] = payload;
        assert!(from_value::<RunEventV1>(bad).is_err());
    }
    let mut large = valid.clone();
    large["payload"] = json!({"schema_version":1,"data":"x".repeat(65537)});
    assert_eq!(to_value(from_value::<RunEventV1>(large.clone()).unwrap()).unwrap(), large);
    let mut bad = valid;
    bad["event_type"] = json!("run.created");
    assert!(
        from_value::<RunEventV1>(bad).is_err(),
        "known state events remain strict"
    );
}

#[test]
fn generated_event_payload_requires_an_extensible_version_one_object() {
    use utoipa::PartialSchema;
    let schema = to_value(RunEventV1::schema()).unwrap();
    let payload = &schema["properties"]["payload"];
    assert_eq!(payload["type"], "object");
    assert_eq!(payload["required"], json!(["schema_version"]));
    assert_eq!(payload["properties"]["schema_version"]["type"], "integer");
    assert_eq!(payload["properties"]["schema_version"]["enum"], json!([1]));
    assert_eq!(payload["additionalProperties"], true);
}

#[test]
fn absent_execution_caps_are_null_but_actual_output_observations_stay_required() {
    use contracts::runtime_jobs::{RuntimeJobLimitsV1, RuntimeOutputV1, RuntimeResourceUsageV1};
    use utoipa::PartialSchema;
    let limits = json!({"cpu":1,"cpu_seconds":null,"memory_mib":1024,"wall_seconds":null,"output_bytes":null});
    assert_eq!(
        to_value(from_value::<RuntimeJobLimitsV1>(limits.clone()).unwrap()).unwrap(),
        limits
    );
    let mut finite = limits;
    finite["cpu_seconds"] = json!("120");
    finite["wall_seconds"] = json!(120);
    finite["output_bytes"] = json!("4096");
    assert_eq!(
        to_value(from_value::<RuntimeJobLimitsV1>(finite.clone()).unwrap()).unwrap(),
        finite
    );
    let usage = json!({"cpu_nanoseconds":null,"wall_milliseconds":"12","peak_memory_bytes":null,"output_bytes":"37"});
    assert_eq!(
        to_value(from_value::<RuntimeResourceUsageV1>(usage.clone()).unwrap()).unwrap(),
        usage
    );
    let mut missing_usage = usage;
    missing_usage["output_bytes"] = json!(null);
    assert!(from_value::<RuntimeResourceUsageV1>(missing_usage).is_err());
    let schema = to_value(RuntimeOutputV1::schema()).unwrap();
    for alternative in schema["oneOf"].as_array().unwrap() {
        assert_eq!(alternative["properties"]["byte_count"]["type"], "string");
        assert!(alternative["required"]
            .as_array()
            .unwrap()
            .contains(&json!("byte_count")));
    }
}

#[test]
fn runtime_cpu_and_memory_are_required_nullable_and_legacy_numbers_round_trip_exactly() {
    use contracts::runtime_jobs::RuntimeJobLimitsV1;
    use utoipa::PartialSchema;
    for (cpu, memory) in [
        (json!(null), json!(null)),
        (json!(1), json!(1024)),
        (json!(u32::MAX), json!(u32::MAX)),
    ] {
        let value = json!({"cpu":cpu,"cpu_seconds":null,"memory_mib":memory,"wall_seconds":null,"output_bytes":null});
        assert_eq!(
            to_value(from_value::<RuntimeJobLimitsV1>(value.clone()).unwrap()).unwrap(),
            value
        );
        for field in ["cpu", "memory_mib"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(from_value::<RuntimeJobLimitsV1>(missing).is_err());
            for invalid in [
                json!(-1),
                json!("1"),
                json!(1.5),
                json!(u64::from(u32::MAX) + 1),
            ] {
                let mut wrong = value.clone();
                wrong[field] = invalid;
                assert!(from_value::<RuntimeJobLimitsV1>(wrong).is_err());
            }
        }
    }
    let schema = to_value(RuntimeJobLimitsV1::schema()).unwrap();
    for field in ["cpu", "memory_mib"] {
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!(field))
        );
        assert_eq!(schema["properties"][field]["minimum"], json!(1));
        assert_eq!(schema["properties"][field]["maximum"], json!(u32::MAX));
    }
}

#[test]
fn optional_memory_request_defaults_preserve_explicit_numeric_choices() {
    use contracts::lifecycle::JobLimitsV1;
    let base = json!({"schema_version":1,"experiments":0,"cpu_seconds":null,"wall_seconds":null,"output_bytes":null});
    let absent: JobLimitsV1 = from_value(base.clone()).unwrap();
    assert_eq!(absent.memory_mib, None);
    for memory in [json!(null), json!(1), json!(u32::MAX)] {
        let mut value = base.clone(); value["memory_mib"] = memory;
        assert_eq!(to_value(from_value::<JobLimitsV1>(value.clone()).unwrap()).unwrap(), value);
    }
}
