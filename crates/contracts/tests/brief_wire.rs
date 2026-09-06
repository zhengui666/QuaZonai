use contracts::{brief::*, control::OperatorCommand, Id, SchemaV1};
use serde_json::{json, Value};
fn example() -> Value {
    serde_json::from_str(include_str!("../../../tests/contracts/research-brief.json")).unwrap()
}
#[test]
fn authoring_wire_does_not_accept_trusted_identity_or_unknown_fields() {
    let good = example();
    let request: BriefCreate = serde_json::from_value(good.clone()).unwrap();
    assert_eq!(serde_json::to_value(&request).unwrap(), good);
    for pointer in ["/schema_version", "/content/horizon_value"] {
        let mut invalid = good.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!(2);
        assert!(serde_json::from_value::<BriefCreate>(invalid).is_err());
    }
    for (field, value) in [
        ("id", json!(Id::new())),
        ("state", json!("FROZEN")),
        ("version", json!(1)),
        ("root_lineage_id", json!(Id::new())),
    ] {
        let mut invalid = good.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<BriefCreate>(invalid).is_err());
    }
    let mut invalid = good.clone();
    invalid["content"]["native_path"] = json!("/etc");
    assert!(serde_json::from_value::<BriefCreate>(invalid).is_err());
    let intent = BriefCreateIntent {
        schema_version: SchemaV1,
        project_id: Id::new(),
        request,
    };
    let command = OperatorCommand::BriefCreate(Box::new(intent.clone()));
    assert_eq!(command.operation().code(), "BRIEF_CREATE");
    assert_eq!(
        command.normalized_request().unwrap(),
        serde_json::to_value(intent).unwrap()
    );
}
#[test]
fn generated_authoring_contract_is_strict_and_exposes_binding_bounds() {
    use utoipa::PartialSchema;
    let create = serde_json::to_value(BriefCreate::schema()).unwrap();
    assert_eq!(create["additionalProperties"], false);
    assert_eq!(create["properties"]["bindings"]["minItems"], 1);
    assert_eq!(create["properties"]["bindings"]["maxItems"], 64);
    let update = serde_json::to_value(BriefUpdate::schema()).unwrap();
    assert!(update["required"]
        .as_array()
        .unwrap()
        .contains(&json!("expected_revision")));
    let view = serde_json::to_value(BriefView::schema()).unwrap();
    assert_eq!(view["properties"]["version"]["maximum"], 2147483647u64);
}
