//! Selected observable facts only. Redacted placeholders are synthetic test input.
use super::{
    projection, Account, AccountState, NativeFailure, NativeModel, Observation, TokenCounts, Turn,
};
use serde_json::json;

#[test]
fn native_account_and_turn_error_projections_do_not_retain_private_fields() {
    let account: AccountState = serde_json::from_value(json!({
        "requiresOpenaiAuth":true,
        "account":{"type":"chatgpt","planType":"pro","email":"[REDACTED_SECRET]",
            "accessToken":"[REDACTED_SECRET]","refreshToken":"[REDACTED_SECRET]"}
    }))
    .unwrap();
    account.validate().unwrap();
    assert!(matches!(account.account, Some(Account::Chatgpt { .. })));
    assert!(!format!("{account:?}").contains("REDACTED"));
    let turn: Turn = serde_json::from_value(json!({
        "id":"native-turn","status":"failed","startedAt":1,"completedAt":2,"durationMs":1000,
        "error":{"message":"[REDACTED_SECRET]"},
        "items":[{"type":"reasoning","text":"[REDACTED_SECRET]"}]
    }))
    .unwrap();
    turn.validate().unwrap();
    assert!(turn.has_error);
    assert!(!format!("{turn:?}").contains("REDACTED"));
}

#[test]
fn cumulative_usage_is_not_replaced_by_the_last_tool_loop_request() {
    let raw = serde_json::value::to_raw_value(&json!({
        "threadId":"native-thread","turnId":"native-turn",
        "tokenUsage":{
            "last":{"inputTokens":1,"cachedInputTokens":0,"outputTokens":1,"reasoningOutputTokens":0,"totalTokens":2},
            "total":{"inputTokens":30,"cachedInputTokens":2,"outputTokens":10,"reasoningOutputTokens":3,"totalTokens":40}
        }
    })).unwrap();
    let Some(Observation::Usage { total, .. }) =
        projection::notification("thread/tokenUsage/updated", Some(&raw)).unwrap()
    else {
        panic!("native cumulative usage expected");
    };
    assert_eq!(total.total, 40);
    let baseline = TokenCounts {
        input: 10,
        cached_input: 1,
        output: 4,
        reasoning_output: 1,
        total: 14,
    };
    let delta = total.since(baseline).unwrap();
    assert_eq!(delta.total, 26);
    assert_eq!(delta.input, 20);
    assert_eq!(delta.output, 6);
    assert!(matches!(
        baseline.since(total),
        Err(NativeFailure::Contract)
    ));
}

#[test]
fn unselected_native_events_are_not_interpreted_as_observable_facts() {
    let raw = serde_json::value::to_raw_value(&json!({
        "threadId":false,"turnId":false,"private":"[REDACTED_SECRET]"
    }))
    .unwrap();
    for method in [
        "item/reasoning/textDelta",
        "item/reasoning/summaryTextDelta",
        "codex/event/raw_response_item",
    ] {
        assert!(projection::notification(method, Some(&raw))
            .unwrap()
            .is_none());
    }
    // Selected methods still enforce their real fields rather than silently
    // swallowing corruption and proceeding as if a terminal was confirmed.
    assert!(projection::notification("turn/completed", Some(&raw)).is_err());
}

#[test]
fn contradictory_native_terminal_times_do_not_become_receipts() {
    for value in [
        json!({"id":"turn","status":"completed","startedAt":2,"completedAt":1,"durationMs":0,"error":null}),
        json!({"id":"turn","status":"completed","startedAt":1,"completedAt":2,"durationMs":-1,"error":null}),
        json!({"id":"turn","status":"completed","startedAt":1,"completedAt":2,"durationMs":1,"error":{"message":"[REDACTED_SECRET]"}}),
    ] {
        let turn: Turn = serde_json::from_value(value).unwrap();
        assert!(turn.validate().is_err());
    }
}

#[test]
fn native_catalog_effort_and_tier_definitions_must_be_unambiguous() {
    let value = json!({
        "id":"native-model","model":"native-model","displayName":"Native model",
        "hidden":false,"isDefault":false,"defaultReasoningEffort":"native-effort",
        "supportedReasoningEfforts":[{"reasoningEffort":"native-effort","description":"Observed effort"}],
        "serviceTiers":[{"id":"native-tier","name":"Native tier","description":"Observed tier"}],
        "defaultServiceTier":"native-tier"
    });
    let model: NativeModel = serde_json::from_value(value).unwrap();
    model.validate().unwrap();
    let mut duplicate = model.clone();
    duplicate
        .supported_reasoning_efforts
        .push(duplicate.supported_reasoning_efforts[0].clone());
    assert!(duplicate.validate().is_err());
    let mut missing = model.clone();
    missing.default_reasoning_effort = "unadvertised".into();
    assert!(missing.validate().is_err());
    let mut tier = model;
    tier.default_service_tier = Some("unadvertised".into());
    assert!(tier.validate().is_err());
}
