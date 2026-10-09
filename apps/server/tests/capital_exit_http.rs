//! Real owner HTTP/Store boundary. Test-native envelopes here do not establish native E2E execution.
#[path = "../../../crates/store/tests/capital_exit_support/mod.rs"]
mod capital_exit_support;
mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use contracts::{DbCounter, Id, SchemaV1, account_observation::*, capital_exit::*};
use serde_json::json;
use sqlx::PgPool;

async fn assert_http_intent(http: &support::Fixture, expected: &CapitalExitViewV1) {
    let get = support::owner_command(
        http,
        "GET",
        &format!("/api/v2/capital-exits/{}", expected.id),
        "read",
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(get.status, StatusCode::OK, "{}", get.body);
    assert_eq!(get.body, serde_json::to_value(expected).unwrap());
    let list = support::owner_command(
        http,
        "GET",
        &format!("/api/v2/projects/{}/capital-exits", expected.project_id),
        "read",
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    assert_eq!(list.body["items"], json!([expected]));
}

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
    let preview: CapitalExitPreviewV1 =
        serde_json::from_value(reply.body["resource"].clone()).unwrap();
    assert_eq!(preview.funds.requested, capital_exit_support::money("100"));
    for amount in [
        preview.funds.native_total_cash,
        preview.funds.native_free_cash,
        preview.funds.native_locked_cash,
        preview.funds.verified_idle_cash,
        preview.funds.estimated_release,
        preview.funds.native_equity,
        preview.funds.managed_capital_before,
        preview.funds.remaining_managed_capital,
        preview.funds.estimated_execution_cost,
        preview.funds.existing_unrealized_pnl,
    ] {
        assert_eq!(amount, None);
    }
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
    let mut controlled: CapitalExitViewV1 =
        serde_json::from_value(paused.body["resource"].clone()).unwrap();
    let mut prior_revision = view.revision;
    for (operation, sequence) in [("pause", 2), ("cancel", 3)] {
        let action_path = format!("/api/v2/capital-exits/{}/{operation}", view.id);
        let action_key = format!("{operation}-http");
        let action = json!({"schema_version":1,"action":operation.to_ascii_uppercase(),"expected_revision":prior_revision});
        if operation == "cancel" {
            let cancelled =
                support::owner_command(&http, "POST", &action_path, &action_key, action.clone())
                    .await;
            assert_eq!(cancelled.status, StatusCode::ACCEPTED, "{}", cancelled.body);
            controlled = serde_json::from_value(cancelled.body["resource"].clone()).unwrap();
        }
        assert_eq!(
            controlled.state,
            if operation == "pause" {
                CapitalExitStateV1::Paused
            } else {
                CapitalExitStateV1::CancellingExit
            }
        );
        assert_eq!(controlled.revision, prior_revision.next().unwrap());
        assert_eq!(controlled.owner_command.command_id, controlled.command_id);
        assert_eq!(
            controlled.owner_command.account_control_epoch,
            controlled.account_control_epoch
        );
        assert_eq!(
            serde_json::to_value(&controlled.owner_command.instruction).unwrap(),
            json!({"action":operation.to_ascii_uppercase()})
        );
        for stale in [
            CapitalExitActionV1::Pause {
                schema_version: SchemaV1,
                expected_revision: prior_revision,
            },
            CapitalExitActionV1::Cancel {
                schema_version: SchemaV1,
                expected_revision: prior_revision,
            },
            CapitalExitActionV1::Resume {
                schema_version: SchemaV1,
                expected_revision: prior_revision,
                preview_id: preview.id,
            },
        ] {
            let conflict = support::owner_command(
                &http,
                "POST",
                &format!(
                    "/api/v2/capital-exits/{}/{}",
                    view.id,
                    stale.action().to_ascii_lowercase()
                ),
                &format!("{operation}-stale-{}", stale.action()),
                serde_json::to_value(stale).unwrap(),
            )
            .await;
            assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.body);
            assert_eq!(conflict.body["code"], "REVISION_CONFLICT");
            assert_eq!(
                conflict.body["current_revision"],
                serde_json::to_value(controlled.revision).unwrap()
            );
        }
        let mut changed = action.clone();
        changed["expected_revision"] = serde_json::to_value(controlled.revision).unwrap();
        let conflict =
            support::owner_command(&http, "POST", &action_path, &action_key, changed).await;
        assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.body);
        assert_eq!(conflict.body["code"], "IDEMPOTENCY_CONFLICT");
        let other = if operation == "pause" {
            "cancel"
        } else {
            "pause"
        };
        let conflict = support::owner_command(
            &http, "POST", &format!("/api/v2/capital-exits/{}/{other}", view.id), &action_key,
            json!({"schema_version":1,"action":other.to_ascii_uppercase(),"expected_revision":controlled.revision}),
        ).await;
        assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.body);
        assert_eq!(conflict.body["code"], "IDEMPOTENCY_CONFLICT");
        let replay = support::owner_command(&http, "POST", &action_path, &action_key, action).await;
        assert_eq!(replay.status, StatusCode::ACCEPTED, "{}", replay.body);
        assert_eq!(replay.body["replayed"], true);
        assert_eq!(
            replay.body["resource"],
            serde_json::to_value(&controlled).unwrap()
        );
        assert_http_intent(&http, &controlled).await;

        if operation == "cancel" {
            // Native acknowledgement ends cancellation, but keeps the original reservation.
            let claimed = f.claim(&controlled, "claim-cancel").await;
            let fenced = f.fence(&claimed, 1).await;
            let evidence = f.evidence(
                &fenced,
                "cancelled",
                2,
                CapitalExitEvidenceKindV1::NativeProgress {
                    phase: CapitalExitStateV1::CancelledReserved,
                    released_cash_amount: None,
                    native_order_refs: vec![],
                    native_position_refs: vec![],
                    native_report_ref: "fixture-cancelled-reserved".into(),
                    reason_codes: vec![],
                },
            );
            controlled = f
                .store
                .submit_capital_exit_evidence(
                    &f.actor,
                    view.id,
                    "cancelled",
                    &evidence,
                    |_| async { Ok(()) },
                )
                .await
                .unwrap()
                .resource;
            assert_eq!(controlled.state, CapitalExitStateV1::CancelledReserved);
            assert_http_intent(&http, &controlled).await;
        }
        assert_eq!(controlled.funds, view.funds);
        let mut assessment = f.assessment();
        assessment.external_message_id = format!("resume-assessment-{operation}");
        assessment.sequence = DbCounter::new(sequence).unwrap();
        assessment.expected_account_control_revision = controlled.account_control_revision;
        f.store
            .submit_capital_exit_assessment(
                &f.actor,
                &assessment.external_message_id,
                &assessment,
                |_| async { Ok(()) },
            )
            .await
            .unwrap();
        let refreshed = support::owner_command(
            &http,
            "POST",
            &format!("/api/v2/projects/{}/capital-exit-previews", view.project_id),
            &format!("resume-preview-{operation}"),
            serde_json::to_value(f.request()).unwrap(),
        )
        .await;
        assert_eq!(refreshed.status, StatusCode::CREATED, "{}", refreshed.body);
        let refreshed: CapitalExitPreviewV1 =
            serde_json::from_value(refreshed.body["resource"].clone()).unwrap();
        assert_eq!(refreshed.capability, CapitalExitCapabilityV1::Supported);
        let resume = CapitalExitActionV1::Resume {
            schema_version: SchemaV1,
            expected_revision: controlled.revision,
            preview_id: refreshed.id,
        };
        let resume_path = format!("/api/v2/capital-exits/{}/resume", view.id);
        let resume_key = format!("resume-{operation}");
        let resumed = support::owner_command(
            &http,
            "POST",
            &resume_path,
            &resume_key,
            serde_json::to_value(&resume).unwrap(),
        )
        .await;
        assert_eq!(resumed.status, StatusCode::ACCEPTED, "{}", resumed.body);
        let resumed: CapitalExitViewV1 =
            serde_json::from_value(resumed.body["resource"].clone()).unwrap();
        assert_eq!(resumed.state, CapitalExitStateV1::Requested);
        assert_eq!(resumed.preview_id, refreshed.id);
        assert_eq!(resumed.plan_artifact_id, refreshed.plan_artifact_id);
        assert_eq!(
            resumed.owner_command.instruction,
            CapitalExitOwnerInstructionV1::Resume {}
        );
        assert_eq!(resumed.revision, controlled.revision.next().unwrap());
        assert_eq!(
            resumed.account_control_revision,
            controlled.account_control_revision.next().unwrap()
        );
        assert!(resumed.account_control_epoch > controlled.account_control_epoch);
        assert_ne!(resumed.command_id, controlled.command_id);
        assert_eq!(resumed.external_claim_id, None);
        for current in [&controlled, &resumed] {
            assert_eq!(current.id, view.id);
            assert_eq!(current.account_source_id, view.account_source_id);
            assert_eq!(current.managed_account_key, view.managed_account_key);
            assert_eq!(current.owner_binding_ref, view.owner_binding_ref);
            assert_eq!(current.created_at, view.created_at);
            assert_eq!(current.scope, view.scope);
            assert_eq!(current.policy, view.policy);
            assert_eq!(current.funds, view.funds);
        }
        assert_http_intent(&http, &resumed).await;
        let replay = support::owner_command(
            &http,
            "POST",
            &resume_path,
            &resume_key,
            serde_json::to_value(&resume).unwrap(),
        )
        .await;
        assert_eq!(replay.status, StatusCode::ACCEPTED);
        assert_eq!(replay.body["replayed"], true);
        assert_eq!(
            replay.body["resource"],
            serde_json::to_value(&resumed).unwrap()
        );
        let conflict = support::owner_command(
            &http,
            "POST",
            &resume_path,
            &resume_key,
            serde_json::to_value(CapitalExitActionV1::Resume {
                schema_version: SchemaV1,
                expected_revision: resumed.revision,
                preview_id: preview.id,
            })
            .unwrap(),
        )
        .await;
        assert_eq!(conflict.status, StatusCode::CONFLICT);
        assert_eq!(conflict.body["code"], "IDEMPOTENCY_CONFLICT");
        assert_http_intent(&http, &resumed).await;
        prior_revision = resumed.revision;
        controlled = resumed;
    }
    let original_request: serde_json::Value =
        sqlx::query_scalar("SELECT original_request FROM app.capital_exit_intents WHERE id=$1")
            .bind(view.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(original_request, serde_json::to_value(&request).unwrap());
    let reservation: (bool, Option<uuid::Uuid>) = sqlx::query_as("SELECT reserved_amount=100,active_intent_id FROM app.managed_capital_reservations WHERE managed_account_key=$1")
        .bind(&view.managed_account_key).fetch_one(&pool).await.unwrap();
    assert_eq!(reservation, (true, Some(view.id.as_uuid())));
    let counts: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.capital_exit_intents),(SELECT count(*) FROM app.command_receipts WHERE operation='CAPITAL_EXIT_START'),(SELECT count(*) FROM app.command_receipts WHERE operation='CAPITAL_EXIT_ACTION'),(SELECT count(*) FROM pgmq.q_capital_exits)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 4, 5));
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
    assert_eq!(replay.body["resource"], started.body["resource"]);
    let mut changed_start = serde_json::to_value(&request).unwrap();
    changed_start["expected_source_observation_id"] = json!(Id::new());
    let conflict = support::owner_command(&http, "POST", &path, "start-http", changed_start).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.body);
    assert_eq!(conflict.body["code"], "IDEMPOTENCY_CONFLICT");
    assert_http_intent(&http, &controlled).await;
    let list = support::owner_command(&http, "GET", &path, "read", serde_json::Value::Null).await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_http_expired_preview_cannot_start_or_reserve(pool: PgPool) {
    let http = support::fixture_with_runtime_targets(
        pool.clone(),
        Some(server::runtime_transport::RuntimeTargets::default()),
    )
    .await;
    support::local_session(&http).await;
    support::owner_session(&http).await;
    let f = capital_exit_support::setup(&pool, true).await;
    let deadline: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT clock_timestamp()+interval '3 seconds'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut assessment = f.assessment();
    assessment.valid_until = deadline;
    f.store
        .submit_capital_exit_assessment(
            &f.actor,
            &assessment.external_message_id,
            &assessment,
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
    let preview_path = format!(
        "/api/v2/projects/{}/capital-exit-previews",
        f.observation.binding.project_id
    );
    let preview_body = serde_json::to_value(f.request()).unwrap();
    let preview = support::owner_command(
        &http,
        "POST",
        &preview_path,
        "expiring-preview",
        preview_body.clone(),
    )
    .await;
    assert_eq!(preview.status, StatusCode::CREATED, "{}", preview.body);
    let plan: CapitalExitPreviewV1 =
        serde_json::from_value(preview.body["resource"].clone()).unwrap();
    assert_eq!(plan.capability, CapitalExitCapabilityV1::Supported);
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM app.managed_capital_reservations r WHERE managed_account_key=$1",
    )
    .bind(&f.registration.managed_account_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("SELECT pg_sleep(GREATEST(0, EXTRACT(EPOCH FROM ($1::timestamptz-clock_timestamp()))::double precision)+0.05)")
        .bind(deadline).execute(&pool).await.unwrap();
    let path = format!("/api/v2/projects/{}/capital-exits", plan.project_id);
    let request = CapitalExitStartV1 {
        schema_version: SchemaV1,
        preview_id: plan.id,
        expected_account_control_revision: plan.expected_account_control_revision.unwrap(),
        acknowledged_plan_artifact_id: plan.plan_artifact_id,
        expected_source_observation_id: plan.original_observation_id,
    };
    for _ in 0..2 {
        let rejected = support::owner_command(
            &http,
            "POST",
            &path,
            "expired-start",
            serde_json::to_value(&request).unwrap(),
        )
        .await;
        assert_eq!(rejected.status, StatusCode::CONFLICT, "{}", rejected.body);
        assert_eq!(rejected.body["code"], "CAPITAL_EXIT_PREVIEW_STALE");
    }
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM app.managed_capital_reservations r WHERE managed_account_key=$1",
    )
    .bind(&f.registration.managed_account_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.capital_exit_intents),(SELECT count(*) FROM app.command_receipts WHERE operation='CAPITAL_EXIT_START'),(SELECT count(*) FROM pgmq.q_capital_exits)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    let replay = support::owner_command(
        &http,
        "POST",
        &preview_path,
        "expiring-preview",
        preview_body,
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], preview.body["resource"]);
    let list = support::owner_command(&http, "GET", &path, "read", serde_json::Value::Null).await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_http_funds_keep_unavailable_simulated_and_stale_semantics(pool: PgPool) {
    let http = support::fixture_with_runtime_targets(
        pool.clone(),
        Some(server::runtime_transport::RuntimeTargets::default()),
    )
    .await;
    support::local_session(&http).await;
    let f = capital_exit_support::setup(&pool, true).await;
    let initial = f.start().await;
    assert_eq!(
        initial.funds.withdrawability,
        CapitalExitWithdrawabilityV1::Unverified
    );
    assert_eq!(
        initial.funds.reserved_amount,
        capital_exit_support::money("100")
    );
    assert_eq!(initial.funds.released_cash_amount, None);
    assert_eq!(initial.funds.unreleased_amount, None);
    assert_eq!(initial.funds.verified_withdrawable_amount, None);
    assert_http_intent(&http, &initial).await;
    let claimed = f.claim(&initial, "funds-claim").await;
    let fenced = f.fence(&claimed, 1).await;
    let progress = f.evidence(
        &fenced,
        "released-cash",
        2,
        CapitalExitEvidenceKindV1::NativeProgress {
            phase: CapitalExitStateV1::WaitingEvidence,
            released_cash_amount: Some(capital_exit_support::money("75")),
            native_order_refs: vec![],
            native_position_refs: vec![],
            native_report_ref: "fixture-released-cash-only".into(),
            reason_codes: vec![],
        },
    );
    let released = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            "released-cash",
            &progress,
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(
        released.funds.released_cash_amount,
        Some(capital_exit_support::money("75"))
    );
    assert_eq!(
        released.funds.unreleased_amount,
        Some(capital_exit_support::money("25"))
    );
    assert_eq!(released.funds.verified_withdrawable_amount, None);
    assert_eq!(
        released.funds.withdrawability,
        CapitalExitWithdrawabilityV1::Unverified
    );
    assert_http_intent(&http, &released).await;
    let available = f.evidence(
        &released,
        "simulated-availability",
        3,
        CapitalExitEvidenceKindV1::WithdrawabilityObserved {
            available_cash: capital_exit_support::money("1000.25"),
            basis: CapitalExitAvailabilityBasisV1::ControlledSandbox,
            venue: "SIM".into(),
            native_availability_report_ref: "fixture-availability".into(),
            settlement_report_ref: "fixture-settlement".into(),
            margin_report_ref: "fixture-cash-no-margin".into(),
            open_orders_report_ref: "fixture-no-openers".into(),
        },
    );
    let simulated = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            "simulated-availability",
            &available,
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(
        simulated.funds.verified_withdrawable_amount,
        Some(capital_exit_support::money("100"))
    );
    assert_eq!(
        simulated.funds.withdrawability,
        CapitalExitWithdrawabilityV1::Simulated
    );
    assert_http_intent(&http, &simulated).await;

    let mut next = f.observation.clone();
    next.sequence = DbCounter::new(2).unwrap();
    let ns = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
    next.observed_at_ns = DbCounter::new(ns).unwrap();
    let snapshot = next.snapshot.as_mut().unwrap();
    snapshot.event_id = Id::new().to_string();
    snapshot.ts_event = DbCounter::new(ns - 2).unwrap();
    snapshot.ts_init = DbCounter::new(ns - 1).unwrap();
    snapshot.is_stale = true;
    f.store
        .submit_client_account_observation(
            &f.actor,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: f.registration.native_client_id.clone(),
                observation: next,
            },
        )
        .await
        .unwrap();
    let mut stale = simulated.clone();
    stale.last_phase = simulated.state;
    stale.state = CapitalExitStateV1::Blocked;
    stale.reason_codes = vec!["capital_exit_native_observation_stale".into()];
    stale.funds.verified_withdrawable_amount = None;
    stale.funds.withdrawability = CapitalExitWithdrawabilityV1::Stale;
    assert_http_intent(&http, &stale).await;
    let replay = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            "simulated-availability",
            &available,
            |_| async { panic!("replay must not publish") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, simulated);
    assert_http_intent(&http, &stale).await;
}
