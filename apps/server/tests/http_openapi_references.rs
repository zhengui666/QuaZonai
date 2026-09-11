//! Check the real route-derived document, independently of DomainContracts.
use serde_json::Value;

fn assert_local_references(document: &Value) -> usize {
    // Native JSON Pointer lookup; this is not a second JSON Schema validator.
    let mut pending = vec![document];
    let mut references = 0;
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(object) => {
                if let Some(reference) = object.get("$ref") {
                    let reference = reference.as_str().expect("OpenAPI $ref must be a string");
                    let pointer = reference
                        .strip_prefix('#')
                        .expect("generated HTTP OpenAPI must be self-contained");
                    assert!(pointer.starts_with('/'), "expected a local JSON Pointer");
                    assert!(
                        document
                            .pointer(pointer)
                            .is_some_and(|target| !target.is_null()),
                        "unresolved HTTP OpenAPI reference: {reference}"
                    );
                    references += 1;
                }
                pending.extend(object.values());
            }
            Value::Array(array) => pending.extend(array),
            _ => {}
        }
    }
    references
}

#[test]
fn generated_http_document_resolves_all_local_references() {
    let document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    assert!(document["paths"]["/api/v2/projects/{id}/briefs"]["post"].is_object());
    for dependency in ["BudgetV1", "StopRuleV1", "CostEnforcement"] {
        assert!(
            document["components"]["schemas"][dependency].is_object(),
            "missing HTTP schema dependency: {dependency}"
        );
    }
    assert!(assert_local_references(&document) > 0);
}

#[test]
#[should_panic(expected = "unresolved HTTP OpenAPI reference")]
fn missing_transitive_budget_dependency_is_detected() {
    let mut document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    document["components"]["schemas"]
        .as_object_mut()
        .unwrap()
        .remove("CostEnforcement")
        .expect("the real HTTP document must register the budget dependency");
    assert_local_references(&document);
}
