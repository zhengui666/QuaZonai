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
async fn projection(
    origin: &str,
    credential: &std::path::Path,
    handoff: Id,
    stream: &str,
) -> ForwardWindowViewV1 {
    let output = client::invoke(
        origin,
        credential,
        &[
            "forward",
            "window",
            &handoff.to_string(),
            "--stream",
            stream,
        ],
        serde_json::Value::Null,
    )
    .await;
    let problem: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap_or_default();
    assert!(
        output.status.success(),
        "window CLI failed: code={} status={}",
        problem["code"],
        problem["status"]
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value.get("returns").is_none());
    serde_json::from_value(value).unwrap()
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
            returns_frequency: None,
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
    let partial = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert!(!partial.is_contiguous);
    assert_eq!(partial.returns_frequency, None);
    assert_eq!(partial.complete_observations.get(), 0);
    assert_eq!(partial.latest_message_ids, vec![first.id]);
    assert!(partial
        .reason_codes
        .contains(&ForwardWindowReasonV1::Partial));
    let empty = projection(origin, credential, handoff.id, "missing-stream").await;
    assert_eq!(empty.reason_codes, vec![ForwardWindowReasonV1::NoMessages]);
    assert!(empty.window_start.is_none());

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
    let complete = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert!(complete.is_contiguous);
    assert_eq!(complete.complete_observations.get(), 1);
    assert_eq!(complete.latest_message_ids, vec![corrected.id]);
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
    let end2: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    failed.report.window_start = request.report.window_end;
    failed.report.window_end = end2;
    failed.report.issued_at = end2;
    failed.report.complete = true;
    failed.report.returns[0].timestamp_ns =
        DbCounter::new(end2.timestamp_nanos_opt().unwrap() as u64).unwrap();

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
    let second = accepted(submit(origin, credential, &failed).await).resource;
    assert_eq!(second.sequence.get(), 2);
    let complete = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert!(complete.is_contiguous);
    assert_eq!(complete.complete_observations.get(), 2);
    assert_eq!(complete.latest_message_ids, vec![corrected.id, second.id]);
    let end3: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let end4: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut fourth = failed.clone();
    fourth.external_message_id = "forward-fourth".into();
    fourth.report.sequence = DbCounter::new(4).unwrap();
    fourth.report.window_start = end3;
    fourth.report.window_end = end4;
    fourth.report.issued_at = end4;
    fourth.report.returns[0].timestamp_ns =
        DbCounter::new(end4.timestamp_nanos_opt().unwrap() as u64).unwrap();
    let fourth_id = accepted(submit(origin, credential, &fourth).await)
        .resource
        .id;
    let gap = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert!(!gap.is_contiguous);
    assert_eq!(gap.complete_observations.get(), 0);
    assert!(gap
        .reason_codes
        .contains(&ForwardWindowReasonV1::SequenceGap));
    assert!(gap.reason_codes.contains(&ForwardWindowReasonV1::WindowGap));
    let mut third = fourth.clone();
    third.external_message_id = "forward-third-late".into();
    third.report.sequence = DbCounter::new(3).unwrap();
    third.report.window_start = end2;
    third.report.window_end = end3;
    third.report.returns[0].timestamp_ns =
        DbCounter::new(end3.timestamp_nanos_opt().unwrap() as u64).unwrap();
    let third_id = accepted(submit(origin, credential, &third).await)
        .resource
        .id;
    let complete = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert!(complete.is_contiguous);
    assert_eq!(complete.complete_observations.get(), 4);
    assert_eq!(
        complete.latest_message_ids,
        vec![corrected.id, second.id, third_id, fourth_id]
    );
    let mut frequency = third.clone();
    frequency.external_message_id = "forward-frequency-correction".into();
    frequency.report.message_revision = 2;
    frequency.report.supersedes_message_id = Some(third_id);
    frequency.report.returns_frequency = Some(ForwardReturnsFrequencyV1::ReportedObservation);
    let frequency_id = accepted(submit(origin, credential, &frequency).await)
        .resource
        .id;
    let mixed = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert_eq!(mixed.complete_observations.get(), 0);
    assert_eq!(mixed.returns_frequency, None);
    assert_eq!(mixed.latest_message_ids[2], frequency_id);
    assert_eq!(
        mixed.reason_codes,
        vec![ForwardWindowReasonV1::FrequencyMismatch]
    );
    assert!(!mixed.is_contiguous);
    let mut daily = request.clone();
    daily.external_message_id = "forward-false-daily".into();
    daily.report.returns_frequency = Some(ForwardReturnsFrequencyV1::UtcDay);
    rejected(submit(origin, credential, &daily).await, 422);
    let mut partial = failed.clone();
    partial.external_message_id = "forward-partial-correction".into();
    partial.report.message_revision = 2;
    partial.report.supersedes_message_id = Some(second.id);
    partial.report.complete = false;
    partial.report.returns[0].value = None;
    partial.report.returns[0].reason_code = Some("NATIVE_RETURN_UNAVAILABLE".into());
    let partial_id = accepted(submit(origin, credential, &partial).await)
        .resource
        .id;
    let incomplete = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert!(!incomplete.is_contiguous);
    assert_eq!(incomplete.complete_observations.get(), 0);
    assert_eq!(incomplete.latest_message_ids[1], partial_id);
    assert!(incomplete
        .reason_codes
        .contains(&ForwardWindowReasonV1::Partial));
    assert!(incomplete
        .reason_codes
        .contains(&ForwardWindowReasonV1::MissingReturns));
    let mut overlap = fourth.clone();
    overlap.external_message_id = "forward-overlap".into();
    overlap.report.sequence = DbCounter::new(5).unwrap();
    accepted(submit(origin, credential, &overlap).await);
    let overlapping = projection(origin, credential, handoff.id, &request.report.stream_id).await;
    assert_eq!(overlapping.complete_observations.get(), 0);
    assert!(overlapping
        .reason_codes
        .contains(&ForwardWindowReasonV1::WindowOverlap));
    assert!(overlapping
        .reason_codes
        .contains(&ForwardWindowReasonV1::SampleOverlap));
    assert!(
        store
            .forward_window(
                operator,
                handoff.id,
                &ForwardWindowQueryV1 {
                    stream_id: request.report.stream_id.clone()
                },
                |id, size| async move {
                    let bytes = f.read(id, size).await?;
                    let mut changed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    changed["release_id"] = serde_json::json!(Id::new());
                    Ok(serde_json::to_vec(&changed).unwrap())
                }
            )
            .await
            .is_err(),
        "altered report bytes cannot borrow a native source binding"
    );
}
