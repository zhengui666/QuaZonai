//! Native transfer/receipt/expiry transactions on the original qualified Package.
use super::*;

pub(super) async fn check(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    machine: &store::authority::Actor,
    f: &cycle_support::Fixture,
    revoked: &HandoffViewV1,
    offer: &HandoffViewV1,
) {
    let request = HandoffClaimV1 {
        schema_version: SchemaV1,
        external_claim_id: "claim-original".into(),
        package_schema_version: PackageSchemaVersion::V1,
    };
    assert!(matches!(
        Box::pin(store.claim_handoff(
            operator,
            "claim-original",
            offer.id,
            &request,
            |_, _| async { panic!("Operator cannot claim") }
        ))
        .await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        Box::pin(
            store.claim_handoff(machine, "wrong-key", offer.id, &request, |_, _| async {
                panic!("key must bind claim identity")
            })
        )
        .await,
        Err(StoreError::Invalid("claim_idempotency_key"))
    ));
    assert!(matches!(
        Box::pin(store.claim_handoff(
            machine,
            "claim-original",
            revoked.id,
            &request,
            |_, _| async { panic!("revoked offer cannot transfer") }
        ))
        .await,
        Err(StoreError::Conflict)
    ));
    sqlx::raw_sql("CREATE FUNCTION app.fail_transfer_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'transfer_fixture'; END $$; CREATE TRIGGER fail_transfer_fixture BEFORE INSERT ON app.handoff_transfers FOR EACH ROW EXECUTE FUNCTION app.fail_transfer_fixture();").execute(pool).await.unwrap();
    assert!(Box::pin(store.claim_handoff(
        machine,
        "claim-original",
        offer.id,
        &request,
        |id, size| f.read(id, size)
    ))
    .await
    .is_err());
    assert_eq!(
        store.handoff(operator, offer.id).await.unwrap().state,
        HandoffStateV1::Offered
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_transfers")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    sqlx::raw_sql("DROP TRIGGER fail_transfer_fixture ON app.handoff_transfers; DROP FUNCTION app.fail_transfer_fixture();").execute(pool).await.unwrap();
    let (a, b) = tokio::join!(
        Box::pin(
            store.claim_handoff(machine, "claim-original", offer.id, &request, |id, size| f
                .read(id, size))
        ),
        Box::pin(
            store.claim_handoff(machine, "claim-original", offer.id, &request, |id, size| f
                .read(id, size))
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        serde_json::to_value(&a.resource).unwrap(),
        serde_json::to_value(&b.resource).unwrap()
    );
    assert_eq!(a.resource.handoff.state, HandoffStateV1::Claimed);
    assert_eq!(a.resource.package.release_id, offer.release_id);
    assert_eq!(
        a.resource.handoff.external_claim_id.as_deref(),
        Some("claim-original")
    );
    assert!(a.resource.handoff.claimed_at.unwrap() >= offer.offered_at);
    let transfer: (String, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT provenance,claimed_at FROM app.handoff_transfers WHERE handoff_id=$1",
    )
    .bind(offer.id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(transfer.0, "RECORDED_TRANSITION");
    assert_eq!(Some(transfer.1), a.resource.handoff.claimed_at);
    let different = HandoffClaimV1 {
        external_claim_id: "claim-different".into(),
        ..request.clone()
    };
    assert!(matches!(
        Box::pin(store.claim_handoff(
            machine,
            "claim-different",
            offer.id,
            &different,
            |_, _| async { panic!("already transferred") }
        ))
        .await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        Box::pin(store.claim_handoff(
            machine,
            "claim-original",
            revoked.id,
            &request,
            |_, _| async { panic!("native claim id cannot bind another offer") }
        ))
        .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert_eq!(store.reconcile_handoffs().await.unwrap(), 0);
    let replay = Box::pin(store.claim_handoff(
        machine,
        "claim-original",
        offer.id,
        &request,
        |_, _| async { panic!("replay returns original target-only package") },
    ))
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        replay.resource.handoff.claimed_at,
        a.resource.handoff.claimed_at
    );
    Box::pin(expiry(pool, store, operator, f, offer, "expiry")).await;
    assert_eq!(
        store.handoff(operator, offer.id).await.unwrap().state,
        HandoffStateV1::Claimed
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_transfers")
            .fetch_one(pool)
            .await
            .unwrap(),
        1
    );
    let revoke = ApprovalRevokeV1 {
        schema_version: SchemaV1,
        expected_latest_revocation_id: None,
        effective_at: None,
        reason_code: "OPERATOR_WITHDRAWAL".into(),
        reason: "Stop future claims; preserve the completed transfer".into(),
    };
    assert!(matches!(
        store
            .revoke_approval(machine, "revoke-machine", offer.approval_id, &revoke)
            .await,
        Err(StoreError::Forbidden)
    ));
    let (a, b) = tokio::join!(
        store.revoke_approval(operator, "revoke-claimed", offer.approval_id, &revoke),
        store.revoke_approval(operator, "revoke-claimed", offer.approval_id, &revoke)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.id, b.resource.id);
    assert!(matches!(
        store
            .revoke_approval(operator, "stale-revoke", offer.approval_id, &revoke)
            .await,
        Err(StoreError::Conflict)
    ));
    assert_eq!(store.reconcile_handoffs().await.unwrap(), 0);
    assert_eq!(
        store.handoff(operator, offer.id).await.unwrap().state,
        HandoffStateV1::Claimed
    );
    let history = store
        .approval_revocations(operator, offer.approval_id, &ListQuery::default())
        .await
        .unwrap();
    assert_eq!(history.items.len(), 1);
    assert_eq!(history.items[0].id, a.resource.id);
    let ack = HandoffAckV1 {
        schema_version: SchemaV1,
        external_ack_id: "ack-original".into(),
        external_claim_id: Some("claim-original".into()),
        outcome: HandoffAckOutcomeV1::Acknowledged,
        reason_code: "ACCEPTED".into(),
        reason: "Original target package accepted".into(),
    };
    assert!(matches!(
        store
            .acknowledge_handoff(operator, "ack-original", offer.id, &ack)
            .await,
        Err(StoreError::Forbidden)
    ));
    let mut wrong = ack.clone();
    wrong.external_claim_id = Some("another-claim".into());
    assert!(matches!(
        store
            .acknowledge_handoff(machine, "ack-original", offer.id, &wrong)
            .await,
        Err(StoreError::Conflict)
    ));
    let (a, b) = tokio::join!(
        store.acknowledge_handoff(machine, "ack-original", offer.id, &ack),
        store.acknowledge_handoff(machine, "ack-original", offer.id, &ack)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.state, HandoffStateV1::Acknowledged);
    assert_eq!(a.resource.acknowledged_at, b.resource.acknowledged_at);
    assert_eq!(a.resource.claimed_at, Some(transfer.1));
    assert!(a.resource.acknowledged_at.unwrap() >= transfer.1);
    wrong = ack.clone();
    wrong.reason = "Changed acknowledgement".into();
    assert!(matches!(
        store
            .acknowledge_handoff(machine, "ack-original", offer.id, &wrong)
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    wrong = ack.clone();
    wrong.external_ack_id = "second-ack".into();
    wrong.outcome = HandoffAckOutcomeV1::Rejected;
    assert!(matches!(
        store
            .acknowledge_handoff(machine, "second-ack", offer.id, &wrong)
            .await,
        Err(StoreError::Conflict)
    ));
    let original = Box::pin(store.claim_handoff(
        machine,
        "claim-original",
        offer.id,
        &request,
        |_, _| async { panic!("historical claim replay needs no new package read") },
    ))
    .await
    .unwrap();
    assert!(original.replayed);
    assert_eq!(original.resource.handoff.state, HandoffStateV1::Claimed);
    assert_eq!(
        store.handoff(operator, offer.id).await.unwrap().state,
        HandoffStateV1::Acknowledged
    );
    for scenario in ["scheduled", "revoke-race", "reject"] {
        Box::pin(expiry(pool, store, operator, f, offer, scenario)).await;
    }
}

async fn expiry(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    claimed: &HandoffViewV1,
    scenario: &str,
) {
    let original = store.release(operator, claimed.release_id).await.unwrap();
    let down = store
        .create_downstream(
            operator,
            &format!("{scenario}-downstream"),
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Expiry protocol fixture".into(),
                    endpoint: "https://expiry.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: DownstreamEnvironments::Paper,
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    let store::downstream::ProbePreparation::Pending(ticket) = store
        .prepare_downstream_probe(
            operator,
            &format!("{scenario}-probe"),
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
    store
        .complete_downstream_probe(
            *ticket,
            DownstreamProbeOutcomeV1::Available {
                capabilities: DownstreamCapabilitiesV1 {
                    schema_version: SchemaV1,
                    delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: vec![ForwardEnvironmentV1::Paper],
                    market_capability_versions: vec![original.market_capability_version.clone()],
                    accepting_targets: true,
                    checked_at: chrono::Utc::now(),
                },
            },
            |id, bytes| async move { f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity) },
        )
        .await
        .unwrap();
    let approval = Box::pin(store.approve_release(
        operator,
        &format!("{scenario}-approval"),
        original.id,
        &ReleaseApproveV1 {
            schema_version: SchemaV1,
            downstream_id: down.id,
            environment: ForwardEnvironmentV1::Paper,
            expected_downstream_revision: down.revision,
            expected_latest_decision_id: None,
            valid_until: original.valid_until,
        },
        |id, size| f.read(id, size),
    ))
    .await
    .unwrap()
    .resource;
    let principal = store
        .create_principal(
            operator,
            &format!("{scenario}-principal"),
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Expiry consumer".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(original.project_id),
                downstream_id: Some(down.id),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let store::control::CredentialPreparation::New(prepared) = store
        .prepare_credential_issuance(
            operator,
            &format!("{scenario}-credential"),
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::DownstreamClaim, MachineScope::DownstreamAck],
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new credential")
    };
    let verifier = Id::new();
    let credential = prepared
        .publish(Id::new(), verifier)
        .await
        .unwrap()
        .resource;
    let machine = store::authority::Actor::Machine {
        credential_id: credential.id,
        verifier_ref: verifier,
        operator_grant: None,
    };
    let expires_at =
        chrono::Utc::now() + chrono::Duration::seconds(if scenario == "expiry" { 4 } else { 60 });
    let offer = Box::pin(store.offer_handoff(
        operator,
        &format!("{scenario}-offer"),
        &HandoffOfferV1 {
            schema_version: SchemaV1,
            release_id: original.id,
            approval_id: approval.id,
            supersedes_handoff_id: None,
            expires_at,
        },
        |id, size| f.read(id, size),
    ))
    .await
    .unwrap()
    .resource;
    assert_eq!(store.reconcile_handoffs().await.unwrap(), 0);
    if scenario == "reject" {
        let mut ack = HandoffAckV1 {
            schema_version: SchemaV1,
            external_ack_id: "preclaim-reject".into(),
            external_claim_id: None,
            outcome: HandoffAckOutcomeV1::Acknowledged,
            reason_code: "DECLINED".into(),
            reason: "Downstream cannot accept this offer".into(),
        };
        assert!(matches!(
            store
                .acknowledge_handoff(&machine, "preclaim-reject", offer.id, &ack)
                .await,
            Err(StoreError::Conflict)
        ));
        ack.outcome = HandoffAckOutcomeV1::Rejected;
        for replayed in [false, true] {
            let result = store
                .acknowledge_handoff(&machine, "preclaim-reject", offer.id, &ack)
                .await
                .unwrap();
            assert_eq!(result.replayed, replayed);
            assert_eq!(result.resource.state, HandoffStateV1::Rejected);
            assert!(result.resource.claimed_at.is_none());
            assert!(result.resource.acknowledged_at.is_none());
        }
    }
    let revoke = ApprovalRevokeV1 {
        schema_version: SchemaV1,
        expected_latest_revocation_id: None,
        effective_at: if scenario == "scheduled" {
            Some(chrono::Utc::now() + chrono::Duration::seconds(2))
        } else {
            None
        },
        reason_code: "WITHDRAWN".into(),
        reason: "Future claims withdrawn".into(),
    };
    if scenario == "scheduled" {
        let first = store
            .revoke_approval(operator, "scheduled-revoke", approval.id, &revoke)
            .await
            .unwrap()
            .resource;
        assert_eq!(
            store.handoff(operator, offer.id).await.unwrap().state,
            HandoffStateV1::Offered
        );
        let later = ApprovalRevokeV1 {
            expected_latest_revocation_id: Some(first.id),
            effective_at: Some(chrono::Utc::now() + chrono::Duration::minutes(5)),
            ..revoke.clone()
        };
        store
            .revoke_approval(operator, "later-revoke", approval.id, &later)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    } else if scenario == "expiry" {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    let request = HandoffClaimV1 {
        schema_version: SchemaV1,
        external_claim_id: "expiry-claim".into(),
        package_schema_version: PackageSchemaVersion::V1,
    };
    if scenario == "revoke-race" {
        let (claim, revoke) = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            tokio::join!(
                Box::pin(store.claim_handoff(
                    &machine,
                    "expiry-claim",
                    offer.id,
                    &request,
                    |id, size| f.read(id, size)
                )),
                store.revoke_approval(operator, "race-revoke", approval.id, &revoke)
            )
        })
        .await
        .expect("claim/revoke lock order must not deadlock");
        revoke.unwrap();
        let transferred = match claim {
            Ok(result) => {
                assert_eq!(result.resource.handoff.state, HandoffStateV1::Claimed);
                true
            }
            Err(StoreError::Conflict | StoreError::Invalid("claim_expiry")) => false,
            Err(error) => panic!("unexpected claim/revoke result: {error}"),
        };
        assert_eq!(
            store.handoff(operator, offer.id).await.unwrap().state,
            if transferred {
                HandoffStateV1::Claimed
            } else {
                HandoffStateV1::Revoked
            }
        );
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app.handoff_transfers WHERE handoff_id=$1")
                .bind(offer.id.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(count, i64::from(transferred));
        return;
    }
    let (claim, expired) = tokio::join!(
        Box::pin(
            store.claim_handoff(&machine, "expiry-claim", offer.id, &request, |id, size| f
                .read(id, size))
        ),
        store.reconcile_handoffs()
    );
    assert!(matches!(
        claim,
        Err(StoreError::Conflict | StoreError::Invalid("claim_expiry"))
    ));
    assert_eq!(
        expired.unwrap() + store.reconcile_handoffs().await.unwrap(),
        if scenario == "reject" { 0 } else { 1 }
    );
    assert_eq!(
        store.handoff(operator, offer.id).await.unwrap().state,
        match scenario {
            "expiry" => HandoffStateV1::Expired,
            "scheduled" => HandoffStateV1::Revoked,
            _ => HandoffStateV1::Rejected,
        }
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.handoff_transfers WHERE handoff_id=$1"
        )
        .bind(offer.id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap(),
        0
    );
}
