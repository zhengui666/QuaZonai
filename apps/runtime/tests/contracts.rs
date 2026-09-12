//! Generated wire contracts and native launch shape. Actual kernel/OCI behavior is
//! verified separately by the mandatory native_oci target, not by these assertions.
use serde_json::Value;
use utoipa::OpenApi;

#[test]
fn generated_download_contract_declares_the_actual_native_media_and_payloads() {
    let document = serde_json::to_value(runtime::http::RuntimeApi::openapi()).unwrap();
    let content = &document["paths"]["/runtime/v1/jobs/{external_job_id}/artifacts/{storage_ref}"]
        ["get"]["responses"]["200"]["content"];
    assert_eq!(content.as_object().unwrap().len(), 2);
    assert_eq!(content["application/wasm"]["schema"]["type"], "string");
    assert_eq!(content["application/wasm"]["schema"]["format"], "binary");
    assert_eq!(
        content["application/json"]["schema"]["$ref"],
        "#/components/schemas/NativeJsonOutputV1"
    );
    let schemas = &document["components"]["schemas"];
    let variants = schemas["NativeJsonOutputV1"]["anyOf"]
        .as_array()
        .or_else(|| schemas["NativeJsonOutputV1"]["oneOf"].as_array())
        .unwrap();
    let expected = [
        "NativeModelCompilationV1",
        "NativeDataQualityReportV1",
        "NativeForecastResultV1",
        "NativeAlphaValidationResultV1",
        "AllocationResultV1",
        "NativeSimulationResultV1",
    ]
    .map(|name| serde_json::json!({"$ref":format!("#/components/schemas/{name}")}));
    assert_eq!(variants.as_slice(), expected.as_slice());
    fn references(value: &Value, root: &Value) {
        match value {
            Value::Object(fields) => {
                if let Some(Value::String(reference)) = fields.get("$ref") {
                    if let Some(pointer) = reference.strip_prefix('#') {
                        assert!(root.pointer(pointer).is_some(), "generated native response contains an unresolved reference: {reference}");
                    }
                }
                for value in fields.values() {
                    references(value, root);
                }
            }
            Value::Array(values) => {
                for value in values {
                    references(value, root);
                }
            }
            _ => {}
        }
    }
    references(&document, &document);
}

#[test]
fn native_image_identity_and_compatibility_contract_are_fixed() {
    assert!(
        include_str!("../../../runtimes/native/native-job.Dockerfile").contains(&format!(
            "io.quazonai.native-stack=\"{}\"",
            runtime::engine::NATIVE_STACK
        ))
    );
    let id = format!("sha256:{}", "a".repeat(64));
    assert!(domain::runtime::pinned_image(&id));
    assert!(domain::runtime::pinned_image(&format!(
        "registry.example/native@{id}"
    )));
    for invalid in [
        "native:latest".to_owned(),
        "sha256:abc".to_owned(),
        format!("sha256:{}", "a".repeat(65)),
        format!("sha256:{}", "A".repeat(64)),
        format!("{id}\n"),
        format!("@{id}"),
        format!("bad name@{id}"),
    ] {
        assert!(!domain::runtime::pinned_image(&invalid));
    }
}
