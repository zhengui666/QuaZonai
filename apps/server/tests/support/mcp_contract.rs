//! Native SDK calls against explicit HTTP fault fixtures; not a database E2E claim.
use super::*;

#[tokio::test]
async fn frozen_brief_is_exactly_bound_and_draft_or_missing_freeze_is_rejected() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    let original = api.state.responses.lock().unwrap().brief.clone();
    let call = || request("research.get_brief", json!({}));
    let result = serde_json::to_value(client.call_tool(call()).await.unwrap()).unwrap();
    assert_ne!(result["isError"], true);
    assert_eq!(body(&result), original);
    let cycle = api.state.responses.lock().unwrap().cycle.clone();
    for field in ["id", "project_id", "brief_id"] {
        {
            let mut values = api.state.responses.lock().unwrap();
            values.cycle = cycle.clone();
            values.cycle[field] = json!(Id::new());
        }
        let result = serde_json::to_value(client.call_tool(call()).await.unwrap()).unwrap();
        assert_eq!(body(&result)["code"], "MCP_AUTHORITY_REJECTED");
    }
    api.state.responses.lock().unwrap().cycle = cycle;
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
    for arguments in [
        json!({"brief_id":api.binding.brief_id}),
        json!({"brief_id":Id::new()}),
        json!({"project_id":api.binding.project_id}),
        json!({"unknown":true}),
    ] {
        let result = client
            .call_tool(request("research.get_brief", arguments))
            .await;
        assert!(
            result.is_err() || serde_json::to_value(result.unwrap()).unwrap()["isError"] == true
        );
    }
    assert_eq!(api.state.responses.lock().unwrap().hits, hits);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn runtime_attempt_takeover_or_expiry_revokes_the_next_native_tool_call() {
    let api = api().await;
    let (client, task) = connected(&api).await;
    api.state.responses.lock().unwrap().run["active_attempt_id"] = json!(Id::new());
    let result = run(&client).await;
    assert_eq!(body(&result)["code"], "MCP_AUTHORITY_REJECTED");
    {
        let mut values = api.state.responses.lock().unwrap();
        values.run["active_attempt_id"] = json!(api.binding.attempt_id);
        values.identity["expires_at"] = json!(Utc::now() - ChronoDuration::seconds(1));
    }
    let result = run(&client).await;
    assert_eq!(body(&result)["code"], "MCP_AUTHORITY_REJECTED");
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test]
async fn mission_deadline_ends_native_transport_without_remote_cancellation() {
    let api = api().await;
    let deadline = Utc::now() + ChronoDuration::seconds(10);
    api.state.responses.lock().unwrap().run["deadline_at"] = json!(deadline);
    let original = api.state.responses.lock().unwrap().run.clone();
    let (client, task) = connected(&api).await;
    let requests = tokio::time::timeout(Duration::from_secs(15), async {
        let result = task.await.unwrap();
        assert!(matches!(result, Err(Failure::Deadline)));
        assert!(
            Utc::now() >= deadline,
            "transport must not expire before server authority"
        );
        assert!(
            client
                .call_tool(request("run.get", json!({})))
                .await
                .is_err()
        );
        let requests = api.state.responses.lock().unwrap().requests.clone();
        let _ = client.cancel().await;
        requests
    })
    .await
    .unwrap();
    let values = api.state.responses.lock().unwrap();
    assert_eq!(values.requests, requests);
    assert_eq!(values.run, original);
    // Startup and renewal only read the original identity and bound Run. The
    // renewal cadence is not a fixed request budget; an expiry may interrupt
    // the last read pair. No cancellation or other mutation may be sent.
    assert!(
        requests.len() > 2,
        "the live lease must have been rechecked"
    );
    for (index, (method, path)) in requests.iter().enumerate() {
        assert_eq!(method, &Method::GET);
        let expected = if index % 2 == 0 {
            "/api/v2/auth/machine".to_owned()
        } else {
            format!("/api/v2/runs/{}", api.binding.run_id)
        };
        assert_eq!(path, &expected);
    }
}
