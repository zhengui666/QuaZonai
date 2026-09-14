//! Actual CLI/TCP/PG original transfer and report publication. Controlled observations only.
use super::*;
use contracts::{
    control::{CommandResult, Page},
    forward::*,
    science::NativeReturnV1,
};

async fn submit(
    origin: &str,
    credential: &std::path::Path,
    request: &ForwardMessageSubmitV1,
) -> std::process::Output {
    client::invoke(
        origin,
        credential,
        &[
            "--idempotency-key",
            &request.external_message_id,
            "forward",
            "submit",
        ],
        serde_json::to_value(request).unwrap(),
    )
    .await
}
fn accepted(output: std::process::Output) -> CommandResult<ForwardMessageViewV1> {
    let problem: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap_or_default();
    assert!(
        output.status.success(),
        "forward CLI failed: code={} status={}",
        problem["code"],
        problem["status"]
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn rejected(output: std::process::Output, status: u16) {
    assert!(!output.status.success());
    let problem: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(problem["status"], status, "{}", problem["code"]);
}
pub(super) async fn check(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    connection: (&str, &std::path::Path),
    handoff: Id,
) {
    let (origin, credential) = connection;
    let handoff = store.handoff(operator, handoff).await.unwrap();
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let request = ForwardMessageSubmitV1 {
        schema_version: SchemaV1,
        external_message_id: "original-forward".into(),
        report: ForwardReportContentV1 {
            schema_version: SchemaV1,
            project_id: handoff.project_id,
            handoff_id: handoff.id,
            external_claim_id: handoff.external_claim_id.clone().unwrap(),
            issuer_version: "controlled-protocol/1".into(),
            stream_id: "target-returns".into(),
            sequence: DbCounter::new(1).unwrap(),
            message_revision: 1,
            supersedes_message_id: None,
            window_start: handoff.claimed_at.unwrap(),
            window_end: now,
            issued_at: now,
            complete: false,
            returns: vec![NativeReturnV1 {
                timestamp_ns: DbCounter::new(now.timestamp_nanos_opt().unwrap() as u64).unwrap(),
                value: Some(0.001),
                reason_code: None,
            }],
        },
    };
    assert!(matches!(
        store
            .submit_forward_message(
                operator,
                &request,
                |_, _| async { panic!("human cannot submit downstream source") },
                |_| async { panic!("human cannot publish downstream source") }
            )
            .await,
        Err(StoreError::Forbidden)
    ));
    rejected(
        client::invoke(
            origin,
            credential,
            &["--idempotency-key", "wrong-header", "forward", "submit"],
            serde_json::to_value(&request).unwrap(),
        )
        .await,
        422,
    );
    let (left, right) = tokio::join!(
        submit(origin, credential, &request),
        submit(origin, credential, &request)
    );
    let (left, right) = (accepted(left), accepted(right));
    assert_eq!(left.resource.id, right.resource.id);
    assert_ne!(left.replayed, right.replayed);
    let first = left.resource;
    assert_eq!(first.observation_count.get(), 1);
    assert_eq!(first.coverage_status, ForwardCoverageV1::Partial);
    let row: (String, String, String, i64) = sqlx::query_as(
        "SELECT schema_name,access_class,created_by,byte_count FROM app.artifacts WHERE id=$1",
    )
    .bind(first.report_artifact_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        (&*row.0, &*row.1, &*row.2),
        ("qz.forward_report", "EVALUATOR_ONLY", "IMPORT")
    );
    let bytes = f
        .read(
            first.report_artifact_id,
            DbCounter::new(row.3 as u64).unwrap(),
        )
        .await
        .unwrap();
    let report: ForwardReportV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report.downstream_id, handoff.downstream_id);
    assert_eq!(report.release_id, handoff.release_id);
    assert_eq!(
        serde_json::to_value(report.content).unwrap(),
        serde_json::to_value(&request.report).unwrap()
    );
    let receipt:serde_json::Value=sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='FORWARD_SUBMIT' AND resource_id=$1").bind(first.id.as_uuid()).fetch_one(pool).await.unwrap();
    assert!(receipt.get("returns").is_none());
    assert!(receipt.get("report").is_none());
    assert_eq!(
        receipt["report_artifact_id"],
        first.report_artifact_id.to_string()
    );
    let mut alias = request.clone();
    alias.external_message_id = "original-forward-alias".into();
    let replay = accepted(submit(origin, credential, &alias).await);
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, first.id);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.forward_messages")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let mut changed = alias.clone();
    changed.report.returns[0].value = Some(0.5);
    rejected(submit(origin, credential, &changed).await, 409);
    let mut foreign = request.clone();
    foreign.external_message_id = "wrong-claim".into();
    foreign.report.external_claim_id = "another-claim".into();
    rejected(submit(origin, credential, &foreign).await, 422);
    foreign.report = request.report.clone();
    foreign.report.project_id = Id::new();
    rejected(submit(origin, credential, &foreign).await, 404);
    let mut precision = request.clone();
    precision.report.issued_at += chrono::Duration::nanoseconds(1);
    rejected(submit(origin, credential, &precision).await, 422);
    let mut correction = request.clone();
    correction.external_message_id = "forward-correction".into();
    correction.report.message_revision = 2;
    correction.report.supersedes_message_id = Some(first.id);
    correction.report.complete = true;
    correction.report.returns[0].value = Some(0.002);
    let corrected = accepted(submit(origin, credential, &correction).await).resource;
    assert_eq!(corrected.coverage_status, ForwardCoverageV1::Correction);
    assert_eq!(corrected.supersedes_message_id, Some(first.id));
    assert_ne!(corrected.report_artifact_id, first.report_artifact_id);
    assert_eq!(
        f.read(
            first.report_artifact_id,
            DbCounter::new(row.3 as u64).unwrap()
        )
        .await
        .unwrap(),
        bytes
    );
    let mut reused_external = correction.clone();
    reused_external.external_message_id = request.external_message_id.clone();
    rejected(submit(origin, credential, &reused_external).await, 409);
    let mut fork = correction.clone();
    fork.external_message_id = "forward-fork".into();
    fork.report.message_revision = 3;
    rejected(submit(origin, credential, &fork).await, 409);
    let mut missing = correction.clone();
    missing.external_message_id = "forward-missing".into();
    missing.report.stream_id = "missing-root".into();
    rejected(submit(origin, credential, &missing).await, 409);
    assert!(accepted(submit(origin, credential, &request).await).replayed);
    let listed = client::invoke(
        origin,
        credential,
        &[
            "forward",
            "list",
            &handoff.project_id.to_string(),
            "--limit",
            "1",
        ],
        serde_json::Value::Null,
    )
    .await;
    assert!(listed.status.success());
    let page: Page<ForwardMessageViewV1> = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(page.items[0].id, corrected.id);
    assert_eq!(page.next_cursor, Some(corrected.id));
    let metadata: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert!(metadata["items"][0].get("returns").is_none());
    assert!(metadata["items"][0].get("report").is_none());
    assert!(matches!(
        store
            .artifact_content(operator, first.report_artifact_id)
            .await,
        Err(StoreError::NotFound)
    ));
    let all = store
        .forward_messages(
            operator,
            handoff.project_id,
            &contracts::control::ListQuery::default(),
        )
        .await
        .unwrap();
    assert_eq!(all.items.len(), 2);
    // Fault injection affects only disposable transaction publication, not original source facts.
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.artifacts")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION app.fail_forward_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forward_fixture'; END $$; CREATE TRIGGER fail_forward_fixture BEFORE INSERT ON app.forward_messages FOR EACH ROW EXECUTE FUNCTION app.fail_forward_fixture();").execute(pool).await.unwrap();
    let mut failed = request.clone();
    failed.external_message_id = "forward-rollback".into();
    failed.report.sequence = DbCounter::new(2).unwrap();
    assert!(!submit(origin, credential, &failed).await.status.success());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.artifacts")
            .fetch_one(pool)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.forward_messages")
            .fetch_one(pool)
            .await
            .unwrap(),
        2
    );
    sqlx::raw_sql("DROP TRIGGER fail_forward_fixture ON app.forward_messages; DROP FUNCTION app.fail_forward_fixture();").execute(pool).await.unwrap();
    assert_eq!(
        accepted(submit(origin, credential, &failed).await)
            .resource
            .sequence
            .get(),
        2
    );
}
