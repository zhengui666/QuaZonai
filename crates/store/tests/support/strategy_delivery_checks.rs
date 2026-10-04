//! Real Store/PGMQ delivery transactions around controlled accepted producer data.
//! This fixture does not attest actual model execution or real market observations.
use super::*;
use chrono::{DateTime, Duration, Utc};
use contracts::{
    control::{AssignablePrincipalKind, CredentialIssue, MachineScope, PrincipalCreate},
    data::DataGrantRevoke,
    delivery::*,
    forward::ForwardEnvironmentV1,
    settings::PackageSchemaVersion,
};

async fn current(pool: &PgPool) -> (Fixture, Id) {
    let mut f = setup_for_delivery(pool).await;
    current_inputs(pool, &mut f, 1).await;
    let run = start(&f, "delivery-current-build").await.resource.id;
    accept(pool, &f, run).await;
    let candidate = publish(&f, run).await.resource;
    let (allowed, until): (String, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT allowed_uses,valid_until FROM app.data_use_grants WHERE id=$1")
            .bind(f.data.data.grant.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(allowed, "RESEARCH_AND_PAPER");
    assert!(
        until.is_none(),
        "original source grant is genuinely permanent"
    );
    (f, candidate)
}

async fn release(f: &Fixture, candidate: Id, key: &str) -> StrategyReleaseViewV1 {
    f.store
        .create_strategy_release(
            &f.actor,
            key,
            &StrategyReleaseCreateV1 {
                schema_version: SchemaV1,
                source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
                candidate_id: candidate,
            },
            |id, size| f.data.read(id, size),
            |object| async move {
                f.data
                    .objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap()
        .resource
}

async fn ready(pool: &PgPool, f: &Fixture) -> contracts::settings::DownstreamView {
    let StrategyPortfolioPurposeV1::CurrentDecision { account_start, .. } = &f.build.purpose else {
        panic!("current account");
    };
    let downstream = f
        .store
        .downstream(&f.actor, account_start.downstream_id)
        .await
        .unwrap();
    let market: String = sqlx::query_scalar(
        "SELECT venue_capability_ref FROM app.execution_assumptions WHERE id=$1",
    )
    .bind(account_start.execution_assumptions_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let store::downstream::ProbePreparation::Pending(ticket) = f
        .store
        .prepare_downstream_probe(
            &f.actor,
            "delivery-ready",
            downstream.id,
            &DownstreamProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: downstream.revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("new probe");
    };
    f.store
        .complete_downstream_probe(
            *ticket,
            DownstreamProbeOutcomeV1::Available {
                capabilities: DownstreamCapabilitiesV1 {
                    schema_version: SchemaV1,
                    delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
                    accepted_package_versions: vec![
                        PackageSchemaVersion::V1,
                        PackageSchemaVersion::V2,
                    ],
                    environments: vec![ForwardEnvironmentV1::Paper],
                    market_capability_versions: vec![market],
                    accepting_targets: true,
                    checked_at: Utc::now(),
                },
            },
            |id, bytes| async move {
                f.data
                    .objects
                    .put(id, &bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap();
    downstream
}

fn approval_request(
    downstream: &contracts::settings::DownstreamView,
    until: DateTime<Utc>,
) -> ReleaseApproveV1 {
    ReleaseApproveV1 {
        schema_version: SchemaV1,
        downstream_id: downstream.id,
        environment: ForwardEnvironmentV1::Paper,
        expected_downstream_revision: downstream.revision,
        expected_latest_decision_id: None,
        valid_until: until,
    }
}

async fn approve_offer(
    f: &Fixture,
    release: &StrategyReleaseViewV1,
    downstream: &contracts::settings::DownstreamView,
) -> HandoffViewV1 {
    let approval = f
        .store
        .approve_release(
            &f.actor,
            "delivery-approve",
            release.id,
            &approval_request(downstream, release.valid_until),
            |id, size| f.data.read(id, size),
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(approval.valid_until, release.valid_until);
    let offer = f
        .store
        .offer_handoff(
            &f.actor,
            "delivery-offer",
            &HandoffOfferV1 {
                schema_version: SchemaV1,
                release_id: release.id,
                approval_id: approval.id,
                supersedes_handoff_id: None,
                expires_at: release.valid_until,
            },
            |id, size| f.data.read(id, size),
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(offer.expires_at, release.valid_until);
    offer
}

async fn machine(f: &Fixture, downstream: Id) -> Actor {
    let principal = f
        .store
        .create_principal(
            &f.actor,
            "delivery-principal",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Controlled downstream".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(f.data.data.project),
                downstream_id: Some(downstream),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let store::control::CredentialPreparation::New(ticket) = f
        .store
        .prepare_credential_issuance(
            &f.actor,
            "delivery-credential",
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::DownstreamClaim, MachineScope::DownstreamAck],
                expires_at: Utc::now() + Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new credential");
    };
    let verifier = Id::new();
    let credential = ticket.publish(Id::new(), verifier).await.unwrap().resource;
    Actor::Machine {
        credential_id: credential.id,
        verifier_ref: verifier,
        operator_grant: None,
    }
}

fn claim_request() -> HandoffClaimV1 {
    HandoffClaimV1 {
        schema_version: SchemaV1,
        external_claim_id: "delivery-claim".into(),
        package_schema_version: PackageSchemaVersion::V2,
    }
}

async fn revoke_at(f: &Fixture, at: DateTime<Utc>) {
    f.store
        .revoke_data_grant(
            &f.actor,
            "delivery-source-revoke",
            f.data.data.grant,
            &DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: Some(at),
                reason_code: "CONTROLLED_EXPIRY".into(),
                reason: "Scheduled source lifetime regression".into(),
            },
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn permanent_source_grant_reaches_release_approval_claim_and_receipt_replay(pool: PgPool) {
    let (f, candidate) = current(&pool).await;
    let downstream = ready(&pool, &f).await;
    let release = release(&f, candidate, "delivery-release").await;
    let offer = approve_offer(&f, &release, &downstream).await;
    let machine = machine(&f, downstream.id).await;
    let first = f
        .store
        .claim_handoff_envelope(
            &machine,
            "delivery-claim",
            offer.id,
            &claim_request(),
            |id, size| f.data.read(id, size),
        )
        .await
        .unwrap();
    let TargetPackageEnvelopeV2::TargetDecision(package) = &first.resource.package else {
        panic!("V2 package");
    };
    assert_eq!(package.valid_until, release.valid_until);
    assert_eq!(first.resource.handoff.state, HandoffStateV1::Claimed);
    revoke_at(&f, release.valid_until - Duration::seconds(1)).await;
    let replay = f
        .store
        .claim_handoff_envelope(
            &machine,
            "delivery-claim",
            offer.id,
            &claim_request(),
            |_, _| async { Err(StoreError::Integrity) },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(&replay.resource).unwrap(),
        serde_json::to_value(&first.resource).unwrap()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn scheduled_source_revocation_bounds_frozen_release_approval_and_claim(pool: PgPool) {
    let (f, candidate) = current(&pool).await;
    let downstream = ready(&pool, &f).await;
    let target_until: DateTime<Utc> = sqlx::query_scalar(
        "SELECT min(valid_until) FROM app.candidate_targets WHERE candidate_id=$1",
    )
    .bind(candidate.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let revoked_at = target_until - Duration::seconds(1);
    revoke_at(&f, revoked_at).await;
    let release = release(&f, candidate, "delivery-release").await;
    assert_eq!(release.valid_until, revoked_at);
    let too_long = f
        .store
        .approve_release(
            &f.actor,
            "approval-exceeds-source",
            release.id,
            &approval_request(&downstream, target_until),
            |id, size| f.data.read(id, size),
        )
        .await
        .unwrap_err();
    assert!(matches!(too_long, StoreError::Invalid("approval_expiry")));
    let offer = approve_offer(&f, &release, &downstream).await;
    let machine = machine(&f, downstream.id).await;
    let claimed = f
        .store
        .claim_handoff_envelope(
            &machine,
            "delivery-claim",
            offer.id,
            &claim_request(),
            |id, size| f.data.read(id, size),
        )
        .await
        .unwrap()
        .resource;
    let TargetPackageEnvelopeV2::TargetDecision(package) = claimed.package else {
        panic!("V2 package");
    };
    assert_eq!(package.valid_until, revoked_at);
    assert_eq!(claimed.handoff.expires_at, revoked_at);
}

#[sqlx::test(migrations = "../../migrations")]
async fn earlier_revocation_after_freeze_refuses_new_authority_and_unclaimed_transfer(
    pool: PgPool,
) {
    let (f, candidate) = current(&pool).await;
    let downstream = ready(&pool, &f).await;
    let release = release(&f, candidate, "delivery-release").await;
    let offer = approve_offer(&f, &release, &downstream).await;
    let machine = machine(&f, downstream.id).await;
    revoke_at(&f, release.valid_until - Duration::seconds(1)).await;
    let approval = f
        .store
        .approve_release(
            &f.actor,
            "approval-after-source-change",
            release.id,
            &approval_request(&downstream, release.valid_until - Duration::seconds(1)),
            |id, size| f.data.read(id, size),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        approval,
        StoreError::Invalid("strategy_release_source_window_changed")
    ));
    let claim = f
        .store
        .claim_handoff_envelope(
            &machine,
            "delivery-claim",
            offer.id,
            &claim_request(),
            |id, size| f.data.read(id, size),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        claim,
        StoreError::Invalid("strategy_release_source_window_changed")
    ));
    assert_eq!(
        f.store.handoff(&f.actor, offer.id).await.unwrap().state,
        HandoffStateV1::Offered
    );
}
