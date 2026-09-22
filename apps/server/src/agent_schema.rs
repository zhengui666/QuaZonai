//! Focused, offline discovery over the actual native OpenAPI, not another schema registry.
use serde_json::{json, Map, Value};
use server::client::{Failure, Result};
use std::collections::BTreeSet;

fn schemas(document: &Value) -> Result<&Map<String, Value>> {
    document
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .ok_or(Failure::Contract)
}

fn pointer_token(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn references(document: &Value, value: &Value, pending: &mut Vec<String>) -> Result<()> {
    match value {
        Value::Object(fields) => {
            if let Some(reference) = fields.get("$ref") {
                let reference = reference.as_str().ok_or(Failure::Contract)?;
                let suffix = reference
                    .strip_prefix("#/components/schemas/")
                    .ok_or(Failure::Contract)?;
                // Validate the whole pointer, including a possible nested definition.
                // Discovery never resolves an external URI or opens another file.
                if document.pointer(&reference[1..]).is_none() {
                    return Err(Failure::Contract);
                }
                let name = suffix
                    .split('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .ok_or(Failure::Contract)?
                    .replace("~1", "/")
                    .replace("~0", "~");
                pending.push(name);
            }
            for child in fields.values() {
                references(document, child, pending)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                references(document, child, pending)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn select(document: &Value, name: &str) -> Result<Value> {
    let all = schemas(document)?;
    if !all.contains_key(name) {
        return Err(Failure::Input);
    }
    let mut pending = vec![name.to_owned()];
    let mut included = Map::new();
    while let Some(name) = pending.pop() {
        if included.contains_key(&name) {
            continue;
        }
        let schema = all.get(&name).ok_or(Failure::Contract)?;
        references(document, schema, &mut pending)?;
        included.insert(name, schema.clone());
    }
    Ok(json!({
        "schema_version": 1,
        "name": name,
        "schema": {"$ref": format!("#/components/schemas/{}", pointer_token(name))},
        "components": {"schemas": included}
    }))
}

pub fn describe(name: Option<&str>) -> Result<Value> {
    let source = server::openapi_json().map_err(|_| Failure::Contract)?;
    let document: Value = serde_json::from_str(&source).map_err(|_| Failure::Contract)?;
    match name {
        Some(name) => select(&document, name),
        None => {
            let names: BTreeSet<_> = schemas(&document)?.keys().collect();
            Ok(json!({"schema_version": 1, "schemas": names}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_preserves_native_constraints_and_handles_cycles() {
        let source = json!({"components":{"schemas":{
            "Request":{"type":"object","required":["revision"],"additionalProperties":false,
                "properties":{"revision":{"$ref":"#/components/schemas/Counter"},"child":{"$ref":"#/components/schemas/Request"}}},
            "Counter":{"type":"string","pattern":"^[0-9]+$"},
            "Unrelated":{"type":"boolean"}
        }}});
        let result = select(&source, "Request").unwrap();
        assert_eq!(result["components"]["schemas"].as_object().unwrap().len(), 2);
        for name in ["Request", "Counter"] {
            assert_eq!(result["components"]["schemas"][name], source["components"]["schemas"][name]);
        }
        assert_eq!(result["schema"]["$ref"], "#/components/schemas/Request");
    }

    #[test]
    fn escaped_component_names_and_nested_pointers_resolve_without_rewriting_schemas() {
        let source = json!({"components":{"schemas":{
            "A/B":{"$ref":"#/components/schemas/X~0Y/properties/value"},
            "X~Y":{"properties":{"value":{"type":"string"}}}
        }}});
        let result = select(&source, "A/B").unwrap();
        assert_eq!(result["schema"]["$ref"], "#/components/schemas/A~1B");
        assert_eq!(result["components"]["schemas"], source["components"]["schemas"]);
    }

    #[test]
    fn unknown_missing_malformed_and_external_references_fail_closed() {
        let empty = json!({"components":{"schemas":{}}});
        assert!(matches!(select(&empty, "Unknown"), Err(Failure::Input)));
        for reference in [json!("https://example.invalid/schema"), json!("#/components/schemas/Missing"), json!(7)] {
            let source = json!({"components":{"schemas":{"Request":{"$ref":reference}}}});
            assert!(matches!(select(&source, "Request"), Err(Failure::Contract)));
        }
    }
}
