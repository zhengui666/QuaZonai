//! Actual TCP/TLS, not a fake HTTP client or a production science acceptance.
#[path = "support/runtime_native.rs"]
mod native;
use axum::http::StatusCode;
use contracts::{runtime::RuntimeProbeFailure, SchemaV1};
use native::*;
use server::runtime_transport::{RuntimeTarget, RuntimeTargets, RuntimeTransport};
use std::sync::atomic::Ordering;
use store::lifecycle::RuntimeSnapshot;

fn snapshot(endpoint: String, development_http: bool) -> RuntimeSnapshot {
    RuntimeSnapshot {
        schema_version: SchemaV1,
        endpoint,
        credential_ref: contracts::Id::new().to_string(),
        tls_policy: "SYSTEM_CA".into(),
        ca_certificate_ref: None,
        development_http,
        protocol_version: "1".into(),
        allowed_capabilities: vec!["DATA_VALIDATE".into()],
    }
}
fn http_client(server: &NativeServer) -> RuntimeTransport {
    let endpoint = format!("http://{}", server.address);
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: endpoint.clone(),
            addresses: vec![server.address],
        }],
        true,
    )
    .unwrap();
    RuntimeTransport::new(&targets, &snapshot(endpoint, true), SECRET.as_bytes(), None).unwrap()
}

#[tokio::test]
async fn native_http_returns_the_actual_strict_observation_once() {
    let expected = capabilities(chrono::Utc::now());
    let server = native_http(
        StatusCode::OK,
        serde_json::to_vec(&expected).unwrap(),
        None,
        false,
    )
    .await;
    let received = http_client(&server).capabilities().await.unwrap();
    assert_eq!(
        serde_json::to_value(received).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(server.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn short_runtime_credentials_fail_authentication_before_any_native_request() {
    let server = native_http(StatusCode::OK, b"{}".to_vec(), None, false).await;
    let endpoint = format!("http://{}", server.address);
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: endpoint.clone(),
            addresses: vec![server.address],
        }],
        true,
    )
    .unwrap();
    for value in [
        b"a".as_slice(),
        b"schema_version".as_slice(),
        b"1234567890123456789012345678901".as_slice(),
    ] {
        assert!(matches!(
            RuntimeTransport::new(&targets, &snapshot(endpoint.clone(), true), value, None),
            Err(RuntimeProbeFailure::Authentication)
        ));
    }
    assert_eq!(server.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn redirection_never_transfers_the_runtime_credential_to_another_listener() {
    let sink = native_http(StatusCode::OK, b"{}".to_vec(), None, false).await;
    let redirect = native_http(
        StatusCode::TEMPORARY_REDIRECT,
        Vec::new(),
        Some(format!("http://{}/runtime/v1/capabilities", sink.address)),
        false,
    )
    .await;
    assert!(http_client(&redirect).capabilities().await.is_err());
    assert_eq!(redirect.requests.load(Ordering::SeqCst), 1);
    assert_eq!(sink.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn bounded_chunked_responses_and_retryable_http_failures_do_not_bypass_limits() {
    let server = native_http(StatusCode::OK, vec![b'x'; 1024 * 1024 + 1], None, true).await;
    assert_eq!(
        http_client(&server).capabilities().await.unwrap_err(),
        RuntimeProbeFailure::ResponseLimit
    );
    let unavailable = native_http(
        StatusCode::SERVICE_UNAVAILABLE,
        SECRET.as_bytes().to_vec(),
        None,
        false,
    )
    .await;
    assert_eq!(
        http_client(&unavailable).capabilities().await.unwrap_err(),
        RuntimeProbeFailure::Unavailable
    );
    assert_eq!(unavailable.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn duplicate_fields_and_decoded_secret_reflection_are_rejected() {
    let mut observation = capabilities(chrono::Utc::now());
    observation.runtime_version = format!("native-{SECRET}");
    let reflected = serde_json::to_string(&observation)
        .unwrap()
        .replace("AL1z", "AL1\\u007a");
    let server = native_http(StatusCode::OK, reflected.into_bytes(), None, false).await;
    assert_eq!(
        http_client(&server).capabilities().await.unwrap_err(),
        RuntimeProbeFailure::ContractUnsupported
    );
    let mut duplicate = serde_json::to_string(&capabilities(chrono::Utc::now())).unwrap();
    duplicate.insert_str(1, "\"schema_version\":1,");
    let server = native_http(StatusCode::OK, duplicate.into_bytes(), None, false).await;
    assert_eq!(
        http_client(&server).capabilities().await.unwrap_err(),
        RuntimeProbeFailure::ContractUnsupported
    );
}

#[test]
fn deployment_allowlist_rejects_metadata_ambient_resolution_and_address_mismatch() {
    for address in [
        "169.254.169.254:443",
        "100.100.100.200:443",
        "168.63.129.16:443",
        "0.0.0.0:443",
        "224.0.0.1:443",
        "[::ffff:127.0.0.1]:443",
        "[fe80::1]:443",
        "[fd00:ec2::254]:443",
        "[2002:7f00:1::1]:443",
    ] {
        assert!(
            RuntimeTargets::new(
                vec![RuntimeTarget {
                    origin: "https://runtime.example".into(),
                    addresses: vec![address.parse().unwrap()]
                }],
                false
            )
            .is_err(),
            "{address}"
        );
    }
    assert!(RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: "http://localhost:80".into(),
            addresses: vec!["127.0.0.1:80".parse().unwrap()]
        }],
        true
    )
    .is_err());
    assert!(RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: "https://10.0.0.1".into(),
            addresses: vec!["10.0.0.2:443".parse().unwrap()]
        }],
        false
    )
    .is_err());
    assert!(RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: "https://runtime.example".into(),
            addresses: vec!["10.0.0.2:8443".parse().unwrap()]
        }],
        false
    )
    .is_err());
    assert!(RuntimeTransport::new(
        &RuntimeTargets::default(),
        &snapshot("https://runtime.example".into(), false),
        SECRET.as_bytes(),
        None
    )
    .is_err());
}

#[tokio::test]
async fn pinned_dns_preserves_sni_and_requires_the_exact_native_ca() {
    let tls = native_tls().await;
    let endpoint = tls.endpoint();
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: endpoint.clone(),
            addresses: vec![tls.server.address],
        }],
        false,
    )
    .unwrap();
    let mut config = snapshot(endpoint.clone(), false);
    config.tls_policy = "PINNED_CA".into();
    config.ca_certificate_ref = Some(contracts::Id::new().to_string());
    assert!(RuntimeTransport::new(&targets, &config, SECRET.as_bytes(), None).is_err());
    let client =
        RuntimeTransport::new(&targets, &config, SECRET.as_bytes(), Some(&tls.ca)).unwrap();
    client.capabilities().await.unwrap();
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 1);
    let untrusted = RuntimeTransport::new(
        &targets,
        &snapshot(endpoint, false),
        SECRET.as_bytes(),
        None,
    )
    .unwrap();
    assert!(untrusted.capabilities().await.is_err());
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 1);
}
