use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn artifact_download_contract_is_binary_not_a_json_integer_array() {
    let spec: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let operation = &spec["paths"]["/api/v2/artifacts/{id}/content"]["get"];
    let content = &operation["responses"]["200"]["content"];
    assert_eq!(content.as_object().unwrap().len(), 1);
    let schema = &content["application/octet-stream"]["schema"];
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["format"], "binary");
    assert!(schema.get("items").is_none());
    assert!(content.get("application/json").is_none());
}

#[test]
fn every_http_operation_has_a_globally_unique_client_identity() {
    let spec: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let mut identities = BTreeMap::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for method in ["get", "post", "put", "patch", "delete", "head", "options"] {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let id = operation["operationId"]
                .as_str()
                .expect("each generated client operation needs an identity");
            assert!(!id.is_empty(), "empty identity for {method} {path}");
            let previous = identities.insert(id.to_owned(), format!("{method} {path}"));
            assert!(
                previous.is_none(),
                "duplicate operationId {id}: {previous:?} and {method} {path}"
            );
        }
    }
    assert!(identities.len() >= 20);
}

#[test]
fn run_and_brief_clients_retain_distinct_operations_and_dtos() {
    let spec: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let paths = &spec["paths"];
    for (path, method, expected) in [
        ("/api/v2/runs", "get", "list_runs"),
        ("/api/v2/runs/{id}", "get", "get_run"),
        ("/api/v2/projects/{id}/briefs", "get", "list_briefs"),
        ("/api/v2/briefs/{id}", "get", "get_brief"),
        ("/api/v2/projects/{id}/briefs", "post", "create_brief"),
    ] {
        assert_eq!(paths[path][method]["operationId"], expected);
    }
    let response = |path: &str| {
        paths[path]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(
        response("/api/v2/runs/{id}"),
        "#/components/schemas/RunSnapshotV1"
    );
    assert_eq!(
        response("/api/v2/briefs/{id}"),
        "#/components/schemas/BriefView"
    );
}
