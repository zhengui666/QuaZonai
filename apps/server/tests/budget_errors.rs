//! Actual Axum Problem serialization; domain tags cannot leak private diagnostics.
use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
use domain::DomainError;
use serde_json::Value;
use server::error::ApiError;
use store::StoreError;

#[tokio::test]
async fn frozen_budget_exhaustion_is_distinct_nonretryable_and_has_a_safe_resource() {
    for resource in [
        "artifact_output_bytes",
        "experiments",
        "cpu_seconds",
        "parallel_runs",
        "standalone_parallel_runs",
        "tokens",
        "estimated_cost",
        "mission_turns",
        "repair_turns",
        "job_resource_limit",
        "private/diagnostic?credential=DO_NOT_REFLECT",
    ] {
        let response = ApiError::from(StoreError::Domain(DomainError::BudgetExhausted(resource)))
            .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        assert!(!response.headers().contains_key("retry-after"));
        let request_id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = to_bytes(response.into_body(), 16384).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "BUDGET_EXHAUSTED");
        assert_eq!(body["type"], "urn:quazonai:problem:budget-exhausted");
        assert_eq!(body["status"], 429);
        assert_eq!(body["retryable"], false);
        assert_eq!(body["request_id"], request_id);
        assert!(body.get("current_revision").is_none());
        assert_eq!(body["safe_next_actions"], serde_json::json!([]));
        let public = if resource.starts_with("private/") {
            "budget"
        } else {
            resource
        };
        assert_eq!(body["field_errors"][0]["field"], public);
        assert_eq!(body["field_errors"][0]["code"], "BUDGET_EXHAUSTED");
        assert!(!String::from_utf8(bytes.to_vec())
            .unwrap()
            .contains("DO_NOT_REFLECT"));
    }
}

#[tokio::test]
async fn native_auth_rate_limit_keeps_its_retry_after_and_retryable_semantics() {
    let response = ApiError::from(StoreError::AuthRateLimited {
        retry_after_seconds: 7,
    })
    .into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "7");
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap();
    assert_eq!(body["code"], "AUTH_RATE_LIMITED");
    assert_eq!(body["retryable"], true);
    assert_eq!(
        body["safe_next_actions"],
        serde_json::json!(["RETRY_AFTER"])
    );
}
