use contracts::{runs::RunKind, settings::*, Id, Revision, SchemaV1};
use domain::settings::*;
use serde_json::json;

fn runtime() -> RuntimeCreate {
    RuntimeCreate {
        schema_version: SchemaV1,
        configuration: RuntimeConfigurationV1 {
            name: "Native research runtime".into(),
            endpoint: "https://runtime.example".into(),
            tls_policy: TlsPolicy::SystemCa,
            allowed_capabilities: vec![RunKind::AlphaEvaluate, RunKind::PortfolioSimulate],
            enabled: true,
            development_http: false,
        },
        credential_ref: Id::new(),
        ca_certificate_ref: None,
    }
}

#[test]
fn endpoint_requires_an_exact_origin_and_explicit_literal_loopback_development() {
    for good in [
        "https://runtime.example",
        "https://runtime.example:9443/",
        "https://[::1]:9443",
    ] {
        assert!(endpoint(good, false).is_ok(), "{good}");
        assert!(endpoint(good, true).is_err());
    }
    for good in ["http://127.0.0.1:8090", "http://[::1]:8090/"] {
        assert!(endpoint(good, true).is_ok());
        assert!(endpoint(good, false).is_err());
    }
    for bad in [
        "",
        " https://runtime.example",
        "https://runtime.example\n",
        "http://localhost",
        "http://runtime.example",
        "file:///etc/passwd",
        "ftp://127.0.0.1",
        "https://user:secret@runtime.example",
        "https://runtime.example/other",
        "https://runtime.example/?token=secret",
        "https://runtime.example/#secret",
        "https://runtime.example\\private",
        "https://runtime.exa\tmple",
    ] {
        assert!(endpoint(bad, false).is_err(), "{bad:?}");
        assert!(endpoint(bad, true).is_err(), "{bad:?}");
    }
}

#[test]
fn native_ca_and_capability_constraints_do_not_silently_coerce_configuration() {
    let mut request = runtime();
    assert!(runtime_create(&request).is_ok());
    request
        .configuration
        .allowed_capabilities
        .push(RunKind::AlphaEvaluate);
    assert!(runtime_create(&request).is_err());
    request.configuration.allowed_capabilities.clear();
    assert!(runtime_create(&request).is_err());
    request = runtime();
    request.configuration.tls_policy = TlsPolicy::PinnedCa;
    assert!(runtime_create(&request).is_err());
    request.ca_certificate_ref = Some(Id::new());
    assert!(runtime_create(&request).is_ok());
    request.configuration.tls_policy = TlsPolicy::SystemCa;
    assert!(runtime_create(&request).is_err());
    request = runtime();
    request.configuration.endpoint = "http://127.0.0.1:80".into();
    request.configuration.development_http = true;
    assert!(runtime_create(&request).is_ok());
    request.configuration.tls_policy = TlsPolicy::PinnedCa;
    request.ca_certificate_ref = Some(Id::new());
    assert!(runtime_create(&request).is_err());
}

#[test]
fn partial_native_reference_rotation_retains_but_does_not_remove_authentication() {
    let mut request = RuntimeUpdate {
        schema_version: SchemaV1,
        expected_revision: Revision::INITIAL,
        configuration: runtime().configuration,
        credential_ref: None,
        ca_certificate_ref: None,
    };
    assert!(runtime_update(&request).is_ok());
    request.configuration.tls_policy = TlsPolicy::PinnedCa;
    assert!(runtime_update(&request).is_ok()); // stored CA checked under the real row lock
    request.configuration.tls_policy = TlsPolicy::SystemCa;
    request.ca_certificate_ref = Some(Id::new());
    assert!(runtime_update(&request).is_err());
}

#[test]
fn secret_intent_excludes_plaintext_and_value_validation_is_purpose_bounded() {
    let intent = IntegrationSecretIntent {
        schema_version: SchemaV1,
        purpose: IntegrationSecretPurpose::Runtime,
        label: "运行环境".into(),
    };
    assert!(secret_intent(&intent).is_ok());
    let mut serialized = serde_json::to_value(&intent).unwrap();
    serialized["value"] = json!("SECRET_SENTINEL");
    assert!(serde_json::from_value::<IntegrationSecretIntent>(serialized).is_err());
    for good in [
        "opaque_native_capability_32_bytes_minimum",
        "a.b-c_123".repeat(4).as_str(),
        "a+b/c=".repeat(6).as_str(),
    ] {
        assert!(secret_value(intent.purpose, good).is_ok());
    }
    for purpose in [
        IntegrationSecretPurpose::Downstream,
        IntegrationSecretPurpose::CustomProvider,
    ] {
        assert!(secret_value(purpose, "a").is_ok());
        assert!(secret_value(purpose, &"x".repeat(8192)).is_ok());
        assert!(secret_value(purpose, &"x".repeat(8193)).is_err());
    }
    for length in [0, 1, 31] {
        assert!(secret_value(intent.purpose, &"x".repeat(length)).is_err());
    }
    assert!(secret_value(intent.purpose, &"x".repeat(32)).is_ok());
    for bad in ["", " abc", "abc ", "abc\n", "abc\0", "中文"] {
        assert!(secret_value(intent.purpose, bad).is_err());
    }
    assert!(secret_value(intent.purpose, &"x".repeat(8192)).is_ok());
    assert!(secret_value(intent.purpose, &"x".repeat(8193)).is_err());
    assert!(secret_value(IntegrationSecretPurpose::TlsCa, "a\nb\n").is_ok()); // native PEM parsing is a separate mandatory step
    assert!(secret_value(IntegrationSecretPurpose::TlsCa, &"x".repeat(65537)).is_err());
}

#[test]
fn strict_wire_rejects_injected_probes_authority_and_unknown_versions() {
    let mut value = serde_json::to_value(runtime()).unwrap();
    for injected in [
        "readiness",
        "actual_capabilities",
        "secret",
        "project_id",
        "created_by",
        "protocol_version",
    ] {
        let mut bad = value.clone();
        bad[injected] = json!("READY");
        assert!(serde_json::from_value::<RuntimeCreate>(bad).is_err());
    }
    value["configuration"]["allowed_capabilities"] = json!(["SHELL_EXEC"]);
    assert!(serde_json::from_value::<RuntimeCreate>(value).is_err());
    let mut c = DownstreamConfigurationV1 {
        name: "Target-only recipient".into(),
        endpoint: "https://downstream.example".into(),
        accepted_package_versions: vec![PackageSchemaVersion::V1],
        environments: DownstreamEnvironments::Paper,
        enabled: true,
        development_http: false,
    };
    assert!(downstream_configuration(&c).is_ok());
    c.accepted_package_versions.push(PackageSchemaVersion::V1);
    assert!(downstream_configuration(&c).is_err());
    c.accepted_package_versions.clear();
    assert!(downstream_configuration(&c).is_err());
    let mut value = serde_json::to_value(c).unwrap();
    value["accepted_package_versions"] = json!(["2"]);
    assert!(serde_json::from_value::<DownstreamConfigurationV1>(value).is_err());
}
