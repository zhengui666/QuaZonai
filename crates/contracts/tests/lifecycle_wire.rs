use contracts::{lifecycle::*, runs::RunState, DbCounter, Id, SchemaV1};
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
fn compatible_extension_envelopes_reject_invalid_names_versions_and_oversized_payloads() {
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
    for payload in [
        json!({"schema_version":2}),
        json!([]),
        json!(null),
        json!({"schema_version":1,"data":"x".repeat(65536)}),
    ] {
        let mut bad = valid.clone();
        bad["payload"] = payload;
        assert!(from_value::<RunEventV1>(bad).is_err());
    }
    let mut bad = valid;
    bad["event_type"] = json!("run.created");
    assert!(
        from_value::<RunEventV1>(bad).is_err(),
        "known state events remain strict"
    );
}
