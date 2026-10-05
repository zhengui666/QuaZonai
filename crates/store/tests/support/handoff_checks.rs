//! Actual original approval/offer transactions. No SQL-authored delivery authority.
#[path = "claim_checks.rs"]
mod claims;
use super::*;
use contracts::control::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn check(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    release: &ReleaseViewV1,
    sibling: &ReleaseViewV1,
    old: &ApprovalViewV1,
    current: &ApprovalViewV1,
    renewed: &ReleaseApproveV1,
) {
    let mut request = HandoffOfferV1 {
        schema_version: SchemaV1,
        release_id: release.id,
        approval_id: old.id,
        supersedes_handoff_id: None,
        expires_at: current.valid_until,
    };
    assert!(matches!(
        Box::pin(
            store.offer_handoff(actor, "stale-approval-offer", &request, |id, size| f
                .read(id, size))
        )
        .await,
        Err(StoreError::Invalid("approval_decision_changed"))
    ));
    request.approval_id = current.id;
    let mut expired = request.clone();
    expired.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);
    assert!(matches!(
        Box::pin(
            store.offer_handoff(actor, "expired-offer", &expired, |id, size| f
                .read(id, size))
        )
        .await,
        Err(StoreError::Invalid("offer_expiry"))
    ));
    let mut wrong = request.clone();
    wrong.release_id = sibling.id;
    assert!(matches!(
        Box::pin(
            store.offer_handoff(actor, "wrong-release-offer", &wrong, |_, _| async {
                panic!("wrong binding must read nothing")
            })
        )
        .await,
        Err(StoreError::NotFound)
    ));
    let mut stale = request.clone();
    stale.supersedes_handoff_id = Some(Id::new());
    assert!(matches!(
        Box::pin(
            store.offer_handoff(actor, "missing-predecessor", &stale, |id, size| f
                .read(id, size))
        )
        .await,
        Err(StoreError::Conflict)
    ));
    let original_probe = store
        .downstream_readiness(actor, current.downstream_id)
        .await
        .unwrap()
        .latest_observation
        .unwrap();
    let DownstreamProbeOutcomeV1::Available { mut capabilities } = original_probe.outcome else {
        panic!("original readiness")
    };
    probe(
        store,
        actor,
        f,
        current,
        "offer-failed-probe",
        DownstreamProbeOutcomeV1::Unavailable {
            reason: contracts::runtime::RuntimeProbeFailure::Unavailable,
        },
    )
    .await;
    assert!(Box::pin(store.offer_handoff(
        actor,
        "failed-readiness-offer",
        &request,
        |id, size| f.read(id, size)
    ))
    .await
    .is_err());
    capabilities.checked_at = chrono::Utc::now();
    let fresh = probe(
        store,
        actor,
        f,
        current,
        "offer-fresh-probe",
        DownstreamProbeOutcomeV1::Available { capabilities },
    )
    .await;
    assert_ne!(
        Some(fresh),
        current.readiness_observation_id,
        "a new same-revision probe may serve the original approval"
    );
    let (a, b) = tokio::join!(
        Box::pin(
            store.offer_handoff(actor, "original-offer", &request, |id, size| f
                .read(id, size))
        ),
        Box::pin(
            store.offer_handoff(actor, "original-offer", &request, |id, size| f
                .read(id, size))
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let first = a.resource;
    assert_eq!(first.state, HandoffStateV1::Offered);
    assert_eq!(first.delivery_sequence.get(), 1);
    assert_eq!(first.claimed_at, None);
    assert!(matches!(
        Box::pin(
            store.offer_handoff(actor, "new-key-same-version", &request, |id, size| f
                .read(id, size))
        )
        .await,
        Err(StoreError::Conflict)
    ));
    let same = Box::pin(store.approve_release(
        actor,
        "replacement-approval",
        release.id,
        renewed,
        |id, size| f.read(id, size),
    ))
    .await
    .unwrap()
    .resource;
    let mut replacement = request.clone();
    replacement.approval_id = same.id;
    assert!(matches!(
        Box::pin(store.offer_handoff(
            actor,
            "new-approval-same-version",
            &replacement,
            |id, size| f.read(id, size)
        ))
        .await,
        Err(StoreError::Conflict)
    ));
    let approval = Box::pin(store.approve_release(
        actor,
        "sibling-approval",
        sibling.id,
        renewed,
        |id, size| f.read(id, size),
    ))
    .await
    .unwrap()
    .resource;
    let mut next = HandoffOfferV1 {
        release_id: sibling.id,
        approval_id: approval.id,
        supersedes_handoff_id: None,
        ..request.clone()
    };
    assert!(matches!(
        Box::pin(
            store.offer_handoff(actor, "implicit-supersession", &next, |id, size| f
                .read(id, size))
        )
        .await,
        Err(StoreError::Conflict)
    ));
    next.supersedes_handoff_id = Some(first.id);
    sqlx::raw_sql("CREATE FUNCTION app.fail_offer_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'offer_insert_fixture'; END $$; CREATE TRIGGER fail_offer_fixture BEFORE INSERT ON app.handoff_offers FOR EACH ROW EXECUTE FUNCTION app.fail_offer_fixture();").execute(pool).await.unwrap();
    assert!(
        Box::pin(store.offer_handoff(actor, "next-offer", &next, |id, size| f.read(id, size)))
            .await
            .is_err()
    );
    assert_eq!(
        store.handoff(actor, first.id).await.unwrap().state,
        HandoffStateV1::Offered,
        "rollback must preserve original offer"
    );
    sqlx::raw_sql("DROP TRIGGER fail_offer_fixture ON app.handoff_offers; DROP FUNCTION app.fail_offer_fixture();").execute(pool).await.unwrap();
    let second =
        Box::pin(store.offer_handoff(actor, "next-offer", &next, |id, size| f.read(id, size)))
            .await
            .unwrap()
            .resource;
    assert_eq!(second.supersedes_handoff_id, Some(first.id));
    assert_eq!(second.delivery_sequence.get(), 2);
    let revoked = store.handoff(actor, first.id).await.unwrap();
    assert_eq!(revoked.state, HandoffStateV1::Revoked);
    assert_eq!(revoked.claimed_at, None);
    assert!(revoked.revision > first.revision);
    let replay = Box::pin(
        store.offer_handoff(actor, "original-offer", &request, |_, _| async {
            panic!("replay must not renew delivery")
        }),
    )
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.state, HandoffStateV1::Offered);
    assert_eq!(
        store.handoff(actor, first.id).await.unwrap().state,
        HandoffStateV1::Revoked
    );
    assert!(
        sqlx::query("UPDATE app.handoff_offers SET supersedes_handoff_id=NULL WHERE id=$1")
            .bind(second.id.as_uuid())
            .execute(pool)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_transfers")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    let other: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.downstream_integrations WHERE id<>$1 LIMIT 1")
            .bind(current.downstream_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    for (name, downstream, allowed) in [
        ("offer-reader", current.downstream_id, true),
        (
            "other-offer-reader",
            other.to_string().try_into().unwrap(),
            false,
        ),
    ] {
        let principal = store
            .create_principal(
                actor,
                name,
                &PrincipalCreate {
                    schema_version: SchemaV1,
                    name: name.into(),
                    kind: AssignablePrincipalKind::Downstream,
                    project_id: Some(release.project_id),
                    downstream_id: Some(downstream),
                    enabled: true,
                },
            )
            .await
            .unwrap()
            .resource;
        let store::control::CredentialPreparation::New(prepared) = store
            .prepare_credential_issuance(
                actor,
                name,
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
            panic!("new reader");
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
        let read = store.handoff(&machine, second.id).await;
        if allowed {
            assert_eq!(read.unwrap().id, second.id);
            Box::pin(claims::check(
                pool, store, actor, &machine, f, &first, &second,
            ))
            .await;
        } else {
            assert!(matches!(read, Err(StoreError::Forbidden)));
            let ack = HandoffAckV1 {
                schema_version: SchemaV1,
                external_ack_id: "foreign-ack".into(),
                external_claim_id: Some("claim-original".into()),
                outcome: HandoffAckOutcomeV1::Acknowledged,
                reason_code: "ACCEPTED".into(),
                reason: "Must not acknowledge another downstream".into(),
            };
            assert!(matches!(
                store
                    .acknowledge_handoff(&machine, "foreign-ack", second.id, &ack)
                    .await,
                Err(StoreError::Forbidden)
            ));

            let request = HandoffClaimV1 {
                schema_version: SchemaV1,
                external_claim_id: "claim-original".into(),
                package_schema_version: PackageSchemaVersion::V1,
            };
            assert!(matches!(
                Box::pin(store.claim_handoff(
                    &machine,
                    "claim-original",
                    second.id,
                    &request,
                    |_, _| async { panic!("other downstream cannot read package") }
                ))
                .await,
                Err(StoreError::Forbidden)
            ));
        }
        assert!(matches!(
            Box::pin(store.offer_handoff(
                &machine,
                "downstream-cannot-offer",
                &next,
                |_, _| async { panic!("no operator authority") }
            ))
            .await,
            Err(StoreError::Forbidden)
        ));
    }
}

async fn probe(
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    approval: &ApprovalViewV1,
    key: &str,
    outcome: DownstreamProbeOutcomeV1,
) -> Id {
    let store::downstream::ProbePreparation::Pending(ticket) = store
        .prepare_downstream_probe(
            actor,
            key,
            approval.downstream_id,
            &DownstreamProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: approval.downstream_revision.unwrap(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new probe")
    };
    store
        .complete_downstream_probe(*ticket, outcome, |id, bytes| async move {
            f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity)
        })
        .await
        .unwrap()
        .resource
        .id
}
