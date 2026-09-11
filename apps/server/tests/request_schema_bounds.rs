use contracts::control::CredentialIssue;
use serde_json::{json, Value};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(CredentialIssue)))]
struct CredentialDocument;

#[test]
fn every_declared_idempotency_header_has_the_actual_transport_bounds() {
    let document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let mut count = 0;
    for (path, item) in document["paths"].as_object().unwrap() {
        for method in ["get", "post", "patch", "put", "delete"] {
            let Some(parameters) = item[method]["parameters"].as_array() else {
                continue;
            };
            for parameter in parameters {
                if parameter["name"] != "Idempotency-Key" {
                    continue;
                }
                count += 1;
                assert_eq!(parameter["in"], "header", "{method} {path}");
                assert_eq!(parameter["required"], true, "{method} {path}");
                assert_eq!(parameter["schema"]["type"], "string");
                assert_eq!(parameter["schema"]["minLength"], 1);
                assert_eq!(parameter["schema"]["maxLength"], 200);
                assert_eq!(
                    parameter["schema"]["pattern"],
                    r"^[!-~]([ -~]*[!-~])?(?![\s\S])"
                );
            }
        }
    }
    assert!(count >= 10, "route coverage unexpectedly disappeared");
}

#[test]
fn isolated_credential_schema_is_unique_but_wire_duplicates_are_not_silently_removed() {
    let document = serde_json::to_value(CredentialDocument::openapi()).unwrap();
    let scopes = &document["components"]["schemas"]["CredentialIssue"]["properties"]["scope_codes"];
    assert_eq!(scopes["uniqueItems"], true);
    assert_eq!(scopes["minItems"], 1);
    assert_eq!(scopes["maxItems"], 10);
    let reference = scopes["items"]["$ref"].as_str().unwrap();
    assert!(document
        .pointer(reference.strip_prefix('#').unwrap())
        .is_some());
    let duplicate: CredentialIssue = serde_json::from_value(json!({
        "schema_version": 1,
        "scope_codes": ["RUN_READ", "RUN_READ"],
        "expires_at": "2030-09-08T00:00:00Z"
    }))
    .unwrap();
    assert_eq!(duplicate.scope_codes.len(), 2);
    assert!(domain::control::scopes(&duplicate).is_err());
}
