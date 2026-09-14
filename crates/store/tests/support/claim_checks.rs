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
    assert_eq!(store.expire_handoffs().await.unwrap(), 0);
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
    Box::pin(expiry(pool, store, operator, f, offer)).await;
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
}

async fn expiry(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    claimed: &HandoffViewV1,
) {
    let original = store.release(operator, claimed.release_id).await.unwrap();
    let down = store
        .create_downstream(
            operator,
            "expiry-downstream",
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
            "expiry-probe",
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
        "expiry-approval",
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
            "expiry-principal",
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
            "expiry-credential",
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::DownstreamClaim],
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
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(4);
    let offer = Box::pin(store.offer_handoff(
        operator,
        "expiry-offer",
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
    assert_eq!(store.expire_handoffs().await.unwrap(), 0);
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let request = HandoffClaimV1 {
        schema_version: SchemaV1,
        external_claim_id: "expiry-claim".into(),
        package_schema_version: PackageSchemaVersion::V1,
    };
    let (claim, expired) = tokio::join!(
        Box::pin(
            store.claim_handoff(&machine, "expiry-claim", offer.id, &request, |id, size| f
                .read(id, size))
        ),
        store.expire_handoffs()
    );
    assert!(matches!(
        claim,
        Err(StoreError::Conflict | StoreError::Invalid("claim_expiry"))
    ));
    assert_eq!(expired.unwrap() + store.expire_handoffs().await.unwrap(), 1);
    assert_eq!(
        store.handoff(operator, offer.id).await.unwrap().state,
        HandoffStateV1::Expired
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
