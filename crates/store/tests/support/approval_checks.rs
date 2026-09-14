//! Extend the original qualified Release chain, without SQL-authored PASS/approval.
#[path = "handoff_checks.rs"]
mod handoffs;
use super::*;
use contracts::research::{InputPurpose, InputSetCreate};
use contracts::{delivery::*, forward::ForwardEnvironmentV1, settings::*};

pub(super) async fn check(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    release: &ReleaseViewV1,
    sibling: &ReleaseViewV1,
) {
    let down = store
        .create_downstream(
            actor,
            "approval-downstream",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Approval protocol fixture".into(),
                    endpoint: "https://approval.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: DownstreamEnvironments::Both,
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    let request = ReleaseApproveV1 {
        schema_version: SchemaV1,
        downstream_id: down.id,
        environment: ForwardEnvironmentV1::Paper,
        expected_downstream_revision: down.revision,
        expected_latest_decision_id: None,
        valid_until: release.valid_until,
    };
    assert!(Box::pin(store.approve_release(
        actor,
        "no-readiness",
        release.id,
        &request,
        |id, size| f.read(id, size)
    ))
    .await
    .is_err());
    let store::downstream::ProbePreparation::Pending(ticket) = store
        .prepare_downstream_probe(
            actor,
            "approval-probe",
            down.id,
            &DownstreamProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: down.revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("new probe")
    };
    let probe = store
        .complete_downstream_probe(
            *ticket,
            DownstreamProbeOutcomeV1::Available {
                capabilities: DownstreamCapabilitiesV1 {
                    schema_version: SchemaV1,
                    delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: vec![ForwardEnvironmentV1::Paper, ForwardEnvironmentV1::Live],
                    market_capability_versions: vec![release.market_capability_version.clone()],
                    accepting_targets: true,
                    checked_at: chrono::Utc::now(),
                },
            },
            |id, bytes| async move { f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity) },
        )
        .await
        .unwrap()
        .resource;
    let mut live = request.clone();
    live.environment = ForwardEnvironmentV1::Live;
    let rejected = Box::pin(store.approve_release(
        actor,
        "paper-license-not-live",
        release.id,
        &live,
        |id, size| f.read(id, size),
    ))
    .await;
    assert!(
        matches!(rejected, Err(StoreError::Invalid("approval_data_use"))),
        "LIVE license rejection: {}",
        rejected
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_else(|| "unexpected success".into())
    );
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.input_sets")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION app.fail_approval_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'approval_insert_fixture'; END $$; CREATE TRIGGER fail_approval_fixture BEFORE INSERT ON app.approvals FOR EACH ROW EXECUTE FUNCTION app.fail_approval_fixture();").execute(pool).await.unwrap();
    assert!(Box::pin(store.approve_release(
        actor,
        "approval-original",
        release.id,
        &request,
        |id, size| f.read(id, size)
    ))
    .await
    .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.input_sets")
            .fetch_one(pool)
            .await
            .unwrap(),
        before,
        "failed approval must roll back frozen evidence too"
    );
    sqlx::raw_sql("DROP TRIGGER fail_approval_fixture ON app.approvals; DROP FUNCTION app.fail_approval_fixture();").execute(pool).await.unwrap();
    let (a, b) = tokio::join!(
        Box::pin(store.approve_release(
            actor,
            "approval-original",
            release.id,
            &request,
            |id, size| f.read(id, size)
        )),
        Box::pin(store.approve_release(
            actor,
            "approval-original",
            release.id,
            &request,
            |id, size| f.read(id, size)
        ))
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let approval = a.resource;
    assert_eq!(approval.decision_ordinal, Some(0));
    assert_eq!(approval.downstream_revision, Some(down.revision));
    assert_eq!(approval.readiness_observation_id, Some(probe.id));
    assert_eq!(
        store
            .approval(actor, approval.id)
            .await
            .unwrap()
            .evidence_set_id,
        approval.evidence_set_id
    );
    let valid: bool = sqlx::query_scalar("SELECT app.approval_evidence_valid($1,$2)")
        .bind(release.id.as_uuid())
        .bind(approval.evidence_set_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(valid);
    // Restricted report references are not ordinary executable research input.
    let view = store
        .input_set(actor, approval.evidence_set_id)
        .await
        .unwrap();
    assert!(store
        .create_input_set(
            actor,
            "no-private-execution-copy",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: release.project_id,
                purpose: InputPurpose::Portfolio,
                decision_cutoff: view.header.decision_cutoff,
                items: view.items.into_iter().map(|v| v.item).collect()
            }
        )
        .await
        .is_err());
    let reject = store
        .reject_release(
            actor,
            "reject-approved",
            sibling.id,
            &ReleaseRejectV1 {
                schema_version: SchemaV1,
                downstream_id: down.id,
                environment: request.environment,
                expected_latest_decision_id: None,
                reason_code: "DECLINED".into(),
                reason: "Original Candidate decision".into(),
            },
        )
        .await
        .unwrap()
        .resource;
    let mut rejected = request.clone();
    rejected.expected_latest_decision_id = Some(reject.id);
    assert!(matches!(
        Box::pin(store.approve_release(
            actor,
            "rejected-candidate",
            release.id,
            &rejected,
            |id, size| f.read(id, size)
        ))
        .await,
        Err(StoreError::Invalid("candidate_rejected"))
    ));
    let reopened = store
        .reopen_release(
            actor,
            "reopen-approved",
            reject.id,
            &ReleaseReopenV1 {
                schema_version: SchemaV1,
                expected_latest_decision_id: reject.id,
                reason_code: "RECONSIDER".into(),
                reason: "Reconsideration requires a new approval".into(),
            },
        )
        .await
        .unwrap()
        .resource;
    assert!(matches!(
        Box::pin(store.approve_release(
            actor,
            "stale-decision",
            release.id,
            &request,
            |id, size| f.read(id, size)
        ))
        .await,
        Err(StoreError::Conflict)
    ));
    let mut renewed = request.clone();
    renewed.expected_latest_decision_id = Some(reopened.id);
    let second =
        Box::pin(
            store.approve_release(actor, "new-approval", release.id, &renewed, |id, size| {
                f.read(id, size)
            }),
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(second.decision_ordinal, Some(reopened.ordinal));
    assert_ne!(second.id, approval.id);
    assert_eq!(
        store
            .approval(actor, approval.id)
            .await
            .unwrap()
            .decision_ordinal,
        Some(0)
    );
    assert!(
        sqlx::query("UPDATE app.approvals SET decision_ordinal=2 WHERE id=$1")
            .bind(approval.id.as_uuid())
            .execute(pool)
            .await
            .is_err()
    );
    let replay = Box::pin(store.approve_release(
        actor,
        "approval-original",
        release.id,
        &request,
        |_, _| async { panic!("original receipt doesn't reapprove") },
    ))
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.decision_ordinal, Some(0));
    Box::pin(handoffs::check(
        pool, store, actor, f, release, sibling, &approval, &second, &renewed,
    ))
    .await;
    let mut expired = renewed.clone();
    expired.valid_until = chrono::Utc::now() - chrono::Duration::seconds(1);
    assert!(Box::pin(store.approve_release(
        actor,
        "expired-approval",
        release.id,
        &expired,
        |id, size| f.read(id, size)
    ))
    .await
    .is_err());
    let mut changed = down.configuration.clone();
    changed.name = "Updated downstream".into();
    store
        .update_downstream(
            actor,
            "change-approved-downstream",
            down.id,
            &DownstreamUpdate {
                schema_version: SchemaV1,
                expected_revision: down.revision,
                configuration: changed,
                credential_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
    assert!(matches!(
        Box::pin(store.approve_release(
            actor,
            "old-downstream",
            release.id,
            &renewed,
            |id, size| f.read(id, size)
        ))
        .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_offers")
            .fetch_one(pool)
            .await
            .unwrap(),
        3
    );
}
