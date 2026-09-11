//! Native SDK calls against explicit HTTP fault fixtures; not a database E2E claim.
use super::*;

#[tokio::test]
async fn frozen_brief_is_exactly_bound_and_draft_or_missing_freeze_is_rejected() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    let original = api.state.responses.lock().unwrap().brief.clone();
    let call = || {
        request(
            "research.get_brief",
            json!({"brief_id":api.binding.brief_id}),
        )
    };
    let result = serde_json::to_value(client.call_tool(call()).await.unwrap()).unwrap();
    assert_ne!(result["isError"], true);
    assert_eq!(body(&result), original);
    for (field, bad) in [
        ("id", json!(Id::new())),
        ("project_id", json!(Id::new())),
        ("state", json!("DRAFT")),
        ("frozen_at", Value::Null),
    ] {
        {
            let mut values = api.state.responses.lock().unwrap();
            values.brief = original.clone();
            values.brief[field] = bad;
        }
        let result = serde_json::to_value(client.call_tool(call()).await.unwrap()).unwrap();
        assert_eq!(result["isError"], true);
        assert_eq!(body(&result)["code"], "MCP_AUTHORITY_REJECTED");
    }
    let hits = api.state.responses.lock().unwrap().hits;
    let other = request("research.get_brief", json!({"brief_id":Id::new()}));
    let result = serde_json::to_value(client.call_tool(other).await.unwrap()).unwrap();
    assert_eq!(body(&result)["code"], "MCP_AUTHORITY_REJECTED");
    assert_eq!(api.state.responses.lock().unwrap().hits, hits);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn runtime_attempt_takeover_or_expiry_revokes_the_next_native_tool_call() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    api.state.responses.lock().unwrap().run["active_attempt_id"] = json!(Id::new());
    let result = run(&client, api.binding.run_id).await;
    assert_eq!(body(&result)["code"], "MCP_AUTHORITY_REJECTED");
    {
        let mut values = api.state.responses.lock().unwrap();
        values.run["active_attempt_id"] = json!(api.binding.attempt_id);
        values.identity["expires_at"] = json!(Utc::now() - ChronoDuration::seconds(1));
    }
    let result = run(&client, api.binding.run_id).await;
    assert_eq!(body(&result)["code"], "MCP_AUTHORITY_REJECTED");
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn mission_deadline_ends_native_transport_without_remote_cancellation() {
    let api = api().await;
    api.state.responses.lock().unwrap().run["deadline_at"] =
        json!(Utc::now() + ChronoDuration::seconds(10));
    let (client, task) = connected(&api).await;
    let result = tokio::time::timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(Failure::Deadline)));
    let _ = client.cancel().await;
    // Only startup's identity and Run reads; no cancellation or polling request.
    assert_eq!(api.state.responses.lock().unwrap().hits, 2);
}
