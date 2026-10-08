//! Real owner HTTP/Store boundary. Test-native envelopes here do not establish native E2E execution.
#[path = "../../../crates/store/tests/capital_exit_support/mod.rs"]
mod capital_exit_support;
mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use contracts::{Id, SchemaV1, capital_exit::*};
use serde_json::json;
use sqlx::PgPool;

#[test]
fn capital_exit_http_contracts_have_exact_routes_closed_types_and_auth_boundaries() {
    let api: serde_json::Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    for (path, method) in [
        (
            "/api/v2/projects/{project_id}/capital-exit-previews",
            "post",
        ),
        ("/api/v2/projects/{project_id}/capital-exits", "post"),
        ("/api/v2/projects/{project_id}/capital-exits", "get"),
        ("/api/v2/capital-exits/{id}", "get"),
        ("/api/v2/capital-exits/{id}/pause", "post"),
        ("/api/v2/capital-exits/{id}/cancel", "post"),
        ("/api/v2/capital-exits/{id}/resume", "post"),
        ("/api/v2/capital-exits/{id}/reconcile-withdrawal", "post"),
        ("/api/v2/downstream/capital-exits", "get"),
        ("/api/v2/capital-exits/{id}/claim", "post"),
        ("/api/v2/capital-exits/{id}/evidence", "post"),
        ("/api/v2/downstream/capital-exit-assessments", "get"),
        ("/api/v2/downstream/capital-exit-assessments", "post"),
    ] {
        let operation = &api["paths"][path][method];
        assert!(operation.is_object(), "{method} {path}");
        if method == "post" {
            assert!(
                operation["parameters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["name"] == "Idempotency-Key" && v["required"] == true),
                "{path}"
            );
        }
    }
    assert_eq!(
        api["components"]["schemas"]["CapitalExitPreviewRequestV1"]["additionalProperties"],
        false
    );
    assert_eq!(
        api["paths"]["/api/v2/capital-exits/{id}/claim"]["post"]["security"],
        json!([{"MachineBearer":[]}])
    );
    let owner = &api["paths"]["/api/v2/projects/{project_id}/capital-exits"]["post"]["security"];
    assert_eq!(
        owner,
        &json!([{"BrowserSession":[]},{"OwnerDeviceBearer":[]}])
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_preview_http_is_read_only_strict_idempotent_and_owner_only(pool: PgPool) {
    let http = support::fixture_with_runtime_targets(
        pool.clone(),
        Some(server::runtime_transport::RuntimeTargets::default()),
    )
    .await;
    support::local_session(&http).await;
    let f = capital_exit_support::setup(&pool, false).await;
    let path = format!(
        "/api/v2/projects/{}/capital-exit-previews",
        f.observation.binding.project_id
    );
    let body = serde_json::to_value(f.request()).unwrap();
    let reply = support::owner_command(&http, "POST", &path, "preview-1", body.clone()).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.body["resource"]["capability"], "BLOCKED");
    assert!(reply.body["resource"]["managed_account_key"].is_null());
    let replay = support::owner_command(&http, "POST", &path, "preview-1", body.clone()).await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], reply.body["resource"]);
    let mut changed = body.clone();
    changed["scope"]["amount"] = json!("101");
    let conflict = support::owner_command(&http, "POST", &path, "preview-1", changed).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    let mut unknown = body.clone();
    unknown["managed_account_key"] = json!("forged-key");
    let invalid = support::owner_command(&http, "POST", &path, "unknown-key", unknown).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut number = body;
    number["scope"]["amount"] = json!(100);
    let invalid = support::owner_command(&http, "POST", &path, "number", number).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut missing = support::owner_request(
        &http,
        "POST",
        &path,
        "missing",
        serde_json::to_value(f.request()).unwrap(),
    )
    .await;
    missing.headers_mut().remove("idempotency-key");
    let invalid = support::exchange(&http.app, missing).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.capital_exit_intents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let no_auth = support::exchange(
        &http.app,
        Request::builder()
            .method("GET")
            .uri(format!("/api/v2/capital-exits/{}", Id::new()))
            .header(header::HOST, "localhost")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(no_auth.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_http_start_pause_and_route_body_mismatch_preserve_original_intent(
    pool: PgPool,
) {
    let http = support::fixture_with_runtime_targets(
        pool.clone(),
        Some(server::runtime_transport::RuntimeTargets::default()),
    )
    .await;
    support::local_session(&http).await;
    let f = capital_exit_support::setup(&pool, true).await;
    let assessment = f.assessment();
    f.store
        .submit_capital_exit_assessment(
            &f.actor,
            &assessment.external_message_id,
            &assessment,
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
    let preview = f.preview("preview-http").await;
    let request = CapitalExitStartV1 {
        schema_version: SchemaV1,
        preview_id: preview.id,
        expected_account_control_revision: preview.expected_account_control_revision.unwrap(),
        acknowledged_plan_artifact_id: preview.plan_artifact_id,
        expected_source_observation_id: f.observation_id,
    };
    let path = format!(
        "/api/v2/projects/{}/capital-exits",
        f.observation.binding.project_id
    );
    let started = support::owner_command(
        &http,
        "POST",
        &path,
        "start-http",
        serde_json::to_value(&request).unwrap(),
    )
    .await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.body);
    let view: CapitalExitViewV1 = serde_json::from_value(started.body["resource"].clone()).unwrap();
    assert_eq!(view.state, CapitalExitStateV1::Requested);
    assert_eq!(view.funds.verified_withdrawable_amount, None);
    let pause = CapitalExitActionV1::Pause {
        schema_version: SchemaV1,
        expected_revision: view.revision,
    };
    let bad = support::owner_command(
        &http,
        "POST",
        &format!("/api/v2/capital-exits/{}/cancel", view.id),
        "wrong-route",
        serde_json::to_value(&pause).unwrap(),
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let paused = support::owner_command(
        &http,
        "POST",
        &format!("/api/v2/capital-exits/{}/pause", view.id),
        "pause-http",
        serde_json::to_value(&pause).unwrap(),
    )
    .await;
    assert_eq!(paused.status, StatusCode::ACCEPTED, "{}", paused.body);
    assert_eq!(
        paused.body["resource"]["funds"]["reserved_amount"]["amount"],
        "100"
    );
    let replay = support::owner_command(
        &http,
        "POST",
        &path,
        "start-http",
        serde_json::to_value(&request).unwrap(),
    )
    .await;
    assert_eq!(replay.status, StatusCode::ACCEPTED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(
        replay.body["resource"]["id"],
        started.body["resource"]["id"]
    );
    let list = support::owner_command(&http, "GET", &path, "read", serde_json::Value::Null).await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"].as_array().unwrap().len(), 1);
}
