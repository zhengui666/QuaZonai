//! Readable audit receipts are not runtime permits.
#[path = "../../../tests/support/paper_execution_consume.rs"]
mod fixture;
use contracts::{control::CommandResult, delivery::*};
use serde_json::json;
use utoipa::PartialSchema;

#[test]
fn consumption_request_binds_existing_identity_without_caller_economics_or_receipt_fields() {
    let (request, _, _, _) = fixture::example();
    let original = serde_json::to_value(request).unwrap();
    let parsed: PaperInitialExecutionConsumeV1 = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
    for field in [
        "package_artifact_id",
        "consumed_at",
        "state",
        "starting_capital",
        "claim",
        "credential_id",
        "permit",
        "reset",
    ] {
        let mut changed = original.clone();
        changed[field] = json!(true);
        assert!(
            serde_json::from_value::<PaperInitialExecutionConsumeV1>(changed).is_err(),
            "{field}"
        );
    }
    for field in [
        "paper_initialization",
        "release_id",
        "external_claim_id",
        "owner_instance_id",
    ] {
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PaperInitialExecutionConsumeV1>(changed).is_err(),
            "{field}"
        );
    }
}

#[test]
fn receipt_carries_canonical_claim_and_only_the_consumed_fact() {
    let (_, claim, result, _) = fixture::example();
    let original = serde_json::to_value(result).unwrap();
    let parsed: CommandResult<PaperInitialExecutionViewV1> =
        serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
    assert_eq!(
        original["resource"]["claim"],
        serde_json::to_value(claim).unwrap()
    );
    assert_eq!(original["resource"]["state"], json!("CONSUMED"));
    for state in ["AVAILABLE", "READY", "RUNNING", "NOT_CONSUMED"] {
        let mut changed = original.clone();
        changed["resource"]["state"] = json!(state);
        assert!(
            serde_json::from_value::<CommandResult<PaperInitialExecutionViewV1>>(changed).is_err()
        );
    }
    for field in ["permit", "fresh", "allowed", "secret", "reset"] {
        let mut changed = original.clone();
        changed["resource"][field] = json!(true);
        assert!(
            serde_json::from_value::<CommandResult<PaperInitialExecutionViewV1>>(changed).is_err()
        );
    }
    let mut missing = original;
    missing["resource"].as_object_mut().unwrap().remove("claim");
    assert!(serde_json::from_value::<CommandResult<PaperInitialExecutionViewV1>>(missing).is_err());
}

#[test]
fn consume_schemas_do_not_add_permit_booleans_or_duplicate_claim_identity() {
    let request = serde_json::to_value(PaperInitialExecutionConsumeV1::schema()).unwrap();
    assert!(request["properties"].get("owner_instance_id").is_some());
    assert!(request["properties"].get("package_artifact_id").is_none());
    let view = serde_json::to_value(PaperInitialExecutionViewV1::schema()).unwrap();
    assert!(view["properties"].get("claim").is_some());
    for field in [
        "permit",
        "allowed",
        "release_id",
        "handoff_id",
        "project_id",
        "external_claim_id",
    ] {
        assert!(view["properties"].get(field).is_none());
    }
    let state = serde_json::to_value(PaperInitialExecutionStateV1::schema()).unwrap();
    assert_eq!(state["enum"], json!(["CONSUMED"]));
}
