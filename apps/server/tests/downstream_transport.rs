//! Real TCP boundary; the response is a controlled protocol fixture, not readiness.
#[path = "support/runtime_native.rs"]
mod native;
use axum::http::StatusCode;
use contracts::{delivery::DownstreamCapabilitiesV1, runtime::RuntimeProbeFailure};
use native::*;
use serde_json::json;
use server::runtime_transport::{DownstreamTransport, RuntimeTarget, RuntimeTargets};
use std::sync::atomic::Ordering;

fn observation() -> serde_json::Value {
    json!({"schema_version":1,"delivery_mode":"TARGET_ONLY",
        "accepted_package_versions":["1"],"environments":["PAPER","LIVE"],
        "market_capability_versions":["fixture-market/1"],"accepting_targets":true,
        "checked_at":chrono::Utc::now()})
}

fn client(server: &NativeServer) -> DownstreamTransport {
    let endpoint = format!("http://{}", server.address);
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: endpoint.clone(),
            addresses: vec![server.address],
        }],
        true,
    )
    .unwrap();
    DownstreamTransport::new(&targets, &endpoint, true, SECRET.as_bytes()).unwrap()
}

async fn serving(payload: Vec<u8>) -> NativeServer {
    native_http_at(
        "/downstream/v1/capabilities",
        StatusCode::OK,
        payload,
        None,
        false,
    )
    .await
}

#[tokio::test]
async fn actual_target_only_observation_and_maintenance_are_preserved() {
    for accepting in [true, false] {
        let mut expected = observation();
        expected["accepting_targets"] = json!(accepting);
        let server = serving(serde_json::to_vec(&expected).unwrap()).await;
        let received = client(&server).capabilities().await.unwrap();
        assert_eq!(serde_json::to_value(received).unwrap(), expected);
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn native_boundary_rejects_incompatible_stale_duplicate_or_secret_responses() {
    for (field, value) in [
        ("schema_version", json!(2)),
        ("delivery_mode", json!("ORDERS")),
        ("accepted_package_versions", json!([])),
        ("accepted_package_versions", json!(["1", "1"])),
        ("environments", json!([])),
        ("environments", json!(["PAPER", "PAPER"])),
        ("environments", json!(["DEMO"])),
        ("market_capability_versions", json!([])),
        ("market_capability_versions", json!(["a", "a"])),
        ("market_capability_versions", json!([" "])),
        ("market_capability_versions", json!(["a".repeat(201)])),
        ("market_capability_versions", json!([SECRET])),
        (
            "checked_at",
            json!(chrono::Utc::now() - chrono::Duration::seconds(61)),
        ),
        (
            "checked_at",
            json!(chrono::Utc::now() - chrono::Duration::seconds(10)),
        ),
        (
            "checked_at",
            json!(chrono::Utc::now() + chrono::Duration::seconds(30)),
        ),
        ("broker_credentials", json!("forbidden-field")),
    ] {
        let mut wrong = observation();
        wrong[field] = value;
        let server = serving(serde_json::to_vec(&wrong).unwrap()).await;
        assert_eq!(
            client(&server).capabilities().await.unwrap_err(),
            RuntimeProbeFailure::ContractUnsupported
        );
    }
    let duplicate =
        serde_json::to_string(&observation())
            .unwrap()
            .replacen('{', "{\"schema_version\":1,", 1);
    let server = serving(duplicate.into_bytes()).await;
    assert_eq!(
        client(&server).capabilities().await.unwrap_err(),
        RuntimeProbeFailure::ContractUnsupported
    );
    let server = native_http_at(
        "/downstream/v1/capabilities",
        StatusCode::OK,
        vec![b'x'; 65537],
        None,
        true,
    )
    .await;
    assert_eq!(
        client(&server).capabilities().await.unwrap_err(),
        RuntimeProbeFailure::ResponseLimit
    );
}

#[tokio::test]
async fn redirect_and_unsaved_destinations_do_not_receive_downstream_credentials() {
    let sink = serving(serde_json::to_vec(&observation()).unwrap()).await;
    let redirect = native_http_at(
        "/downstream/v1/capabilities",
        StatusCode::TEMPORARY_REDIRECT,
        Vec::new(),
        Some(format!(
            "http://{}/downstream/v1/capabilities",
            sink.address
        )),
        false,
    )
    .await;
    assert!(client(&redirect).capabilities().await.is_err());
    assert_eq!(redirect.requests.load(Ordering::SeqCst), 1);
    assert_eq!(sink.requests.load(Ordering::SeqCst), 0);
    assert!(matches!(
        DownstreamTransport::new(
            &RuntimeTargets::default(),
            &format!("http://{}", sink.address),
            true,
            SECRET.as_bytes()
        ),
        Err(RuntimeProbeFailure::EndpointDenied)
    ));
    let server = native_http_at(
        "/downstream/v1/capabilities",
        StatusCode::UNAUTHORIZED,
        SECRET.as_bytes().to_vec(),
        None,
        false,
    )
    .await;
    assert_eq!(
        client(&server).capabilities().await.unwrap_err(),
        RuntimeProbeFailure::Authentication
    );
}

#[test]
fn timestamp_limits_and_maximal_distinct_contracts_match_domain_validation() {
    let now = chrono::Utc::now();
    let mut value: DownstreamCapabilitiesV1 = serde_json::from_value(observation()).unwrap();
    value.market_capability_versions = (0..64).map(|index| format!("market-{index}")).collect();
    for checked_at in [
        now - chrono::Duration::seconds(60),
        now + chrono::Duration::seconds(5),
    ] {
        value.checked_at = checked_at;
        domain::delivery::downstream_capabilities(&value, now).unwrap();
    }
    value.market_capability_versions.push("overflow".into());
    assert!(domain::delivery::downstream_capabilities(&value, now).is_err());
}
