#[path = "../../../tests/support/runtime.rs"]
mod support;
use chrono::{Duration, Utc};
use contracts::{runtime::*, DbCounter};
use domain::runtime::{capabilities, pinned_image};
use serde_json::json;

#[test]
fn only_complete_bounded_current_native_observations_are_accepted() {
    let now = Utc::now();
    let baseline = support::capabilities(now);
    assert!(capabilities(&baseline, now).is_ok());
    let mut value = baseline.clone();
    value.protocol_versions.clear();
    assert!(capabilities(&value, now).is_err());
    value = baseline.clone();
    value.job_kinds.push(value.job_kinds[0]);
    assert!(capabilities(&value, now).is_err());
    value = baseline.clone();
    value.image_refs.pop();
    assert!(capabilities(&value, now).is_err());
    value = baseline.clone();
    value.image_refs[1].job_kind = value.image_refs[0].job_kind;
    assert!(capabilities(&value, now).is_err());
    value = baseline.clone();
    value.image_refs[0].image_ref = "registry.example/job:latest".into();
    assert!(capabilities(&value, now).is_err());
    value = baseline.clone();
    value.max_output_bytes = DbCounter::new(0).unwrap();
    assert!(capabilities(&value, now).is_err());
    value = baseline.clone();
    value.engine_versions.clear();
    assert!(capabilities(&value, now).is_err());
    value = baseline.clone();
    value.checked_at = now + Duration::seconds(6);
    assert!(capabilities(&value, now).is_err());
    value.checked_at = now - Duration::seconds(301);
    assert!(capabilities(&value, now).is_err());
}

#[test]
fn unsupported_venue_data_and_duplicate_evidence_schema_do_not_become_capabilities() {
    let now = Utc::now();
    let mut value = support::capabilities(now);
    value.venues[0]
        .data_kinds
        .push(RuntimeDataKind::Fundamental);
    assert!(capabilities(&value, now).is_err());
    value = support::capabilities(now);
    value.venues.push(value.venues[0].clone());
    assert!(capabilities(&value, now).is_err());
    value = support::capabilities(now);
    value
        .artifact_schemas
        .push(value.artifact_schemas[0].clone());
    assert!(capabilities(&value, now).is_err());
    value = support::capabilities(now);
    value.solver_capabilities = vec!["linear".into(), "linear".into()];
    assert!(capabilities(&value, now).is_err());
}

#[test]
fn native_oci_digest_is_not_an_application_qualification_or_mutable_tag() {
    assert!(pinned_image(&format!(
        "registry.example/job@sha256:{}",
        "a".repeat(64)
    )));
    for invalid in [
        "registry.example/job:latest",
        "registry.example/job@sha256:123",
        "file:///private",
        "registry.example/job@sha256:ZZZZ",
        "@sha256:",
    ] {
        assert!(!pinned_image(invalid));
    }
    assert!(!pinned_image(&format!(
        "registry.example/job@sha256:{}",
        "A".repeat(64)
    )));
}

#[test]
fn probe_and_observation_wire_types_reject_injected_success_and_credentials() {
    let now = Utc::now();
    let valid = serde_json::to_value(support::capabilities(now)).unwrap();
    for field in [
        "origin",
        "credential",
        "qualification",
        "secret",
        "available_actions",
    ] {
        let mut invalid = valid.clone();
        invalid[field] = json!("injected");
        assert!(serde_json::from_value::<RuntimeCapabilitiesV1>(invalid).is_err());
    }
    assert!(serde_json::from_value::<RuntimeProbeRequestV1>(json!({
        "schema_version":1,"expected_revision":"1","capabilities":valid
    }))
    .is_err());
    assert!(serde_json::from_value::<RuntimeCapabilitiesV1>({
        let mut invalid = valid;
        invalid["isolation_profile"] = json!("NO_SANDBOX");
        invalid
    })
    .is_err());
}
