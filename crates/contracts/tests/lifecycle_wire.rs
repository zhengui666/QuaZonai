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
        event_type: RunEventKind::StateChanged,
        occurred_at: "2026-09-06T00:00:00Z".parse().unwrap(),
        payload: RunStatePayload {
            schema_version: SchemaV1,
            state: RunState::CancelRequested,
            reason: RunReason::CancelRequested,
        },
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
