//! Transport-only regression using unchanged original Paper Poller requests.
//! Missing-resource lookup proves JSON intake, not native financial acceptance.
use super::*;
use contracts::capital_exit::{
    CapitalExitClaimV1, CapitalExitOwnerAssessmentV1, CapitalExitOwnerEvidenceV1,
};

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_native_evidence_http_preserves_history_without_body_cap(pool: PgPool) {
    let a = fixture(&pool).await;
    // Run 37716865144, store artifact 11525310860, pause_resume HTTP trace
    // line 127: exact 19,838-byte request after the original resumed fill.
    // No native timestamp, quote, event, amount, identity or reference is changed.
    let original = include_str!(
        "../../../../tests/contracts/paper-capital-resume-progress-original.json"
    ).trim_end_matches('\n').as_bytes();
    assert_eq!(original.len(), 19_838);
    assert!(original.len() > 16 * 1024);
    let evidence: CapitalExitOwnerEvidenceV1 = serde_json::from_slice(original).unwrap();
    let path = format!("/api/v2/capital-exits/{}/evidence", evidence.intent_id);
    let before = ledger(&pool).await;
    for (length, expected) in [
        (original.len(), StatusCode::NOT_FOUND),
        (2 * 1024 * 1024, StatusCode::NOT_FOUND),
        (2 * 1024 * 1024 + 1, StatusCode::NOT_FOUND),
        (3 * 1024 * 1024, StatusCode::NOT_FOUND),
    ] {
        // JSON whitespace changes transport length without manufacturing events
        // or removing any part of the original report to squeeze it under a cap.
        let mut bytes = original.to_vec();
        bytes.resize(length, b' ');
        let reply = a.http
            .post(format!("{}{path}", a.origin))
            .bearer_auth(&a.token)
            .header("content-type", "application/json")
            .header("idempotency-key", &evidence.external_message_id)
            .body(bytes)
            .send().await.unwrap();
        assert_eq!(reply.status(), expected, "evidence body length {length}");
        let problem: Value = reply.json().await.unwrap();
        assert_eq!(problem["code"], "NOT_FOUND");
    }
    // The neighboring control command has no inherited 16 KiB or implicit
    // Axum 2 MiB cap either. It still reaches the exact missing-intent lookup.
    let claim = CapitalExitClaimV1 {
        schema_version: SchemaV1,
        expected_revision: contracts::Revision::INITIAL,
        command_id: evidence.command_id,
        account_control_epoch: evidence.account_control_epoch,
        account_source_id: evidence.account_source_id,
        owner_binding_ref: evidence.owner_binding_ref.clone(),
        external_claim_id: evidence.external_claim_id.clone(),
    };
    let original_claim = serde_json::to_vec(&claim).unwrap();
    assert!(original_claim.len() < 16 * 1024);
    for (length, expected) in [
        (16 * 1024, StatusCode::NOT_FOUND),
        (16 * 1024 + 1, StatusCode::NOT_FOUND),
        (2 * 1024 * 1024 + 1, StatusCode::NOT_FOUND),
    ] {
        let mut bytes = original_claim.clone();
        bytes.resize(length, b' ');
        let reply = a.http
            .post(format!("{}/api/v2/capital-exits/{}/claim", a.origin, evidence.intent_id))
            .bearer_auth(&a.token)
            .header("content-type", "application/json")
            .header("idempotency-key", "full-body-claim")
            .body(bytes)
            .send().await.unwrap();
        assert_eq!(reply.status(), expected, "claim body length {length}");
    }
    assert_eq!(ledger(&pool).await, before);
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.capital_exit_intents), (SELECT count(*) FROM app.capital_exit_evidence)",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0), "missing resources must not create financial evidence");
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_native_assessment_http_keeps_history_intact_without_body_cap(pool: PgPool) {
    let a = fixture(&pool).await;
    // The same original run's pause_resume trace line 103 is 12,979 bytes and
    // succeeded. Its history-bearing shape can grow past the former browser limit;
    // unlike the evidence request above, that growth was not this run's failure.
    let original = include_str!(
        "../../../../tests/contracts/paper-capital-resume-assessment-original.json"
    ).trim_end_matches('\n').as_bytes();
    assert_eq!(original.len(), 12_979);
    let assessment: CapitalExitOwnerAssessmentV1 = serde_json::from_slice(original).unwrap();
    let before = ledger(&pool).await;
    for (length, expected) in [
        (original.len(), StatusCode::NOT_FOUND),
        (16 * 1024 + 1, StatusCode::NOT_FOUND),
        (2 * 1024 * 1024, StatusCode::NOT_FOUND),
        (2 * 1024 * 1024 + 1, StatusCode::NOT_FOUND),
        (3 * 1024 * 1024, StatusCode::NOT_FOUND),
    ] {
        // Preserve every native field. Only legal trailing JSON whitespace is
        // added to cross the former transport caps, never new source events.
        let mut bytes = original.to_vec();
        bytes.resize(length, b' ');
        let reply = a.http
            .post(format!("{}/api/v2/downstream/capital-exit-assessments", a.origin))
            .bearer_auth(&a.token)
            .header("content-type", "application/json")
            .header("idempotency-key", &assessment.external_message_id)
            .body(bytes)
            .send().await.unwrap();
        assert_eq!(reply.status(), expected, "assessment body length {length}");
        let problem: Value = reply.json().await.unwrap();
        assert_eq!(problem["code"], "NOT_FOUND");
    }
    assert_eq!(ledger(&pool).await, before);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.capital_exit_evidence")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "transport checks must not create an assessment receipt");
}

#[sqlx::test(migrations = "../../migrations")]
async fn ordinary_http_commands_accept_complete_large_json_and_keep_auth_and_idempotency(pool: PgPool) {
    let a = fixture(&pool).await;
    let value = json!({
        "schema_version": 1,
        "name": "complete-large-body",
        "description": "The fields after the old transport boundary must all survive.",
        "fork_from_project_id": null
    });
    let encoded = serde_json::to_vec(&value).unwrap();
    let mut bytes = vec![b' '; 2 * 1024 * 1024 + 1];
    // Every real field comes after the previous implicit Axum ceiling.
    bytes.extend_from_slice(&encoded);
    let url = format!("{}/api/v2/projects", a.origin);
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.projects")
        .fetch_one(&pool).await.unwrap();
    let no_auth = a.http.post(&url)
        .header("origin", &a.origin)
        .header("content-type", "application/json")
        .header("idempotency-key", "large-command-no-auth")
        .body(bytes.clone()).send().await.unwrap();
    assert_eq!(no_auth.status(), StatusCode::UNAUTHORIZED);

    let mut resource = Value::Null;
    for (body, replayed) in [(bytes.clone(), false), (encoded, true)] {
        let response = a.http.post(&url)
            .header("origin", &a.origin)
            .header("cookie", &a.browser_cookie)
            .header("content-type", "application/json")
            .header("idempotency-key", "large-command")
            .body(body).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let response: Value = response.json().await.unwrap();
        assert_eq!(response["replayed"], replayed);
        assert_eq!(response["resource"]["name"], value["name"]);
        assert_eq!(response["resource"]["description"], value["description"]);
        if replayed {
            assert_eq!(response["resource"], resource);
        } else {
            resource = response["resource"].clone();
        }
    }
    // Parsing cannot silently stop after a valid prefix or the previous byte cap.
    bytes.push(b'x');
    let invalid = a.http.post(&url)
        .header("origin", &a.origin)
        .header("cookie", &a.browser_cookie)
        .header("content-type", "application/json")
        .header("idempotency-key", "large-command-invalid-tail")
        .body(bytes).send().await.unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid.json::<Value>().await.unwrap()["code"], "VALIDATION_ERROR");
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM app.projects")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(after, before + 1, "only the one authenticated valid command can persist");
}
