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

    let content = serde_json::to_value(BriefContentV1::schema()).unwrap();
    let variants = content["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), 3);
    let positive = serde_json::to_value(contracts::Revision::schema()).unwrap();
    for kind in ["FIXED_BARS", "FIXED_DURATION"] {
        let variant = variants
            .iter()
            .find(|variant| variant["properties"]["horizon_kind"]["enum"] == json!([kind]))
            .unwrap();
        assert!(variant["required"]
            .as_array()
            .unwrap()
            .contains(&json!("horizon_value")));
        assert_eq!(variant["properties"]["horizon_value"], positive);
    }
    let variable = variants
        .iter()
        .find(|variant| {
            variant["properties"]["horizon_kind"]["enum"] == json!(["VARIABLE_INTERVAL"])
        })
        .unwrap();
    assert_eq!(variable["properties"]["horizon_value"]["type"], "null");
    assert!(!variable["required"]
        .as_array()
        .unwrap()
        .contains(&json!("horizon_value")));
    assert_eq!(variable["additionalProperties"], false);
}

fn assert_resolved_references(document: &Value, value: &Value) {
    match value {
        Value::Object(fields) => {
            if let Some(reference) = fields.get("$ref") {
                let reference = reference.as_str().expect("string schema reference");
                let pointer = reference.strip_prefix('#').expect("local schema reference");
                assert!(
                    document.pointer(pointer).is_some(),
                    "unresolved reference: {reference}"
                );
            }
            for value in fields.values() {
                assert_resolved_references(document, value);
            }
        }
        Value::Array(values) => {
            for value in values {
                assert_resolved_references(document, value);
            }
        }
        _ => {}
    }
}

#[test]
fn isolated_brief_documents_collect_nested_budget_references() {
    use utoipa::OpenApi;

    // Do not use the global document: unrelated endpoints can accidentally
    // register missing dependencies and conceal a broken manual ToSchema impl.
    #[derive(OpenApi)]
    #[openapi(components(schemas(BriefCreate)))]
    struct CreateApi;
    #[derive(OpenApi)]
    #[openapi(components(schemas(BriefUpdate)))]
    struct UpdateApi;
    #[derive(OpenApi)]
    #[openapi(components(schemas(BriefView)))]
    struct ViewApi;

    for document in [
        CreateApi::openapi(),
        UpdateApi::openapi(),
        ViewApi::openapi(),
    ] {
        let document = serde_json::to_value(document).unwrap();
        assert_resolved_references(&document, &document);
        for dependency in ["SchemaV1", "DecimalValue", "CostEnforcement"] {
            assert!(
                document["components"]["schemas"].get(dependency).is_some(),
                "missing budget dependency: {dependency}"
            );
        }
        assert_eq!(
            document["components"]["schemas"]["CostEnforcement"]["enum"],
            json!(["UNAVAILABLE", "ESTIMATED", "EXACT"])
        );
    }
}
