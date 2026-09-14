//! Original qualified protocol fixture; never production science or Live promotion.
use super::*;
use contracts::{control::*, delivery::*, forward::ForwardEnvironmentV1, settings::*};

pub(super) async fn check(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    directory: &tempfile::TempDir,
    release: &ReleaseViewV1,
) {
    let mandate = store.mandate(operator, release.mandate_id).await.unwrap();
    let metrics = store
        .evaluation_policy(operator, mandate.content.required_evaluation_policy_id)
        .await
        .unwrap()
        .metric_requirements;
    assert_eq!(
        store
            .project(operator, release.project_id)
            .await
            .unwrap()
            .state,
        contracts::runs::ProjectState::Active
    );
    for (index, revoke_first) in [false, true].into_iter().enumerate() {
        let key = format!("auto-{index}");
        let down = store
            .create_downstream(
                operator,
                &format!("{key}-down"),
                &DownstreamCreate {
                    schema_version: SchemaV1,
                    credential_ref: Id::new(),
                    configuration: DownstreamConfigurationV1 {
                        name: key.clone(),
                        endpoint: "https://auto-paper.example".into(),
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
        let request = AutomationAuthorizeV1 {
            schema_version: SchemaV1,
            expected_project_revision: store
                .project(operator, release.project_id)
                .await
                .unwrap()
                .revision,
            content: AutomationPolicyContentV1 {
                mode: if revoke_first {
                    AutomationModeV1::AutoHandoff
                } else {
                    AutomationModeV1::AutoPaper
                },
                mandate_id: release.mandate_id,
                downstream_id: down.id,
                required_paper_observations: 20,
                minimum_paper_elapsed_seconds: contracts::DbCounter::new(60).unwrap(),
                max_feedback_age_seconds: contracts::DbCounter::new(60).unwrap(),
                promotion_metric_requirements: metrics.clone(),
                degradation_metric_requirements: metrics.clone(),
                valid_until: release.valid_until,
                enabled_for_new_rebalances: true,
                max_rebalances_per_day: 1,
            },
        };
        let policy = store
            .authorize_automation(
                operator,
                &format!("{key}-policy"),
                release.project_id,
                &request,
            )
            .await
            .unwrap()
            .resource;
        assert_eq!(
            store.automation_project_after(None).await.unwrap(),
            Some(release.project_id)
        );
        assert!(store
            .automation_project_after(Some(release.project_id))
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            Box::pin(store.automate_paper(release.project_id, |id, size| f.read(id, size))).await,
            Err(StoreError::Domain(
                domain::DomainError::CapabilityUnavailable("downstream_delivery_unavailable")
            ))
        ));
        let store::downstream::ProbePreparation::Pending(ticket) = store
            .prepare_downstream_probe(
                operator,
                &format!("{key}-probe"),
                down.id,
                &DownstreamProbeRequestV1 {
                    schema_version: SchemaV1,
                    expected_revision: down.revision,
                },
            )
            .await
            .unwrap()
        else {
            panic!("new native probe")
        };
        store.complete_downstream_probe(*ticket,DownstreamProbeOutcomeV1::Available{capabilities:DownstreamCapabilitiesV1{schema_version:SchemaV1,delivery_mode:DownstreamDeliveryModeV1::TargetOnly,accepted_package_versions:vec![PackageSchemaVersion::V1],environments:vec![ForwardEnvironmentV1::Paper,ForwardEnvironmentV1::Live],market_capability_versions:vec![release.market_capability_version.clone()],accepting_targets:true,checked_at:chrono::Utc::now()}},|id,bytes|async move{f.objects.put(id,&bytes).map_err(|_|StoreError::Integrity)}).await.unwrap();
        if index == 0 {
            let inputs: i64 = sqlx::query_scalar("SELECT count(*) FROM app.input_sets")
                .fetch_one(pool)
                .await
                .unwrap();
            sqlx::raw_sql("CREATE FUNCTION app.fail_auto_offer_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'auto_offer_fixture'; END $$; CREATE TRIGGER fail_auto_offer_fixture BEFORE INSERT ON app.handoff_offers FOR EACH ROW EXECUTE FUNCTION app.fail_auto_offer_fixture();").execute(pool).await.unwrap();
            assert!(Box::pin(
                store.automate_paper(release.project_id, |id, size| f.read(id, size))
            )
            .await
            .is_err());
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.approvals")
                    .fetch_one(pool)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_offers")
                    .fetch_one(pool)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.input_sets")
                    .fetch_one(pool)
                    .await
                    .unwrap(),
                inputs
            );
            sqlx::raw_sql("DROP TRIGGER fail_auto_offer_fixture ON app.handoff_offers; DROP FUNCTION app.fail_auto_offer_fixture();").execute(pool).await.unwrap();
            let web = support::fixture(pool.clone()).await;
            let vault = integrations::secrets::SecretVault::open(
                &web._state.path().join("secrets"),
                &web._state.path().join("master.key"),
            )
            .unwrap();
            let worker = server::worker::Worker::new(
                store.clone(),
                vault,
                integrations::artifacts::ArtifactStore::open(&directory.path().join("objects"))
                    .unwrap(),
                server::runtime_transport::RuntimeTargets::default(),
                1,
            )
            .unwrap();
            let (shutdown, observed) = tokio::sync::watch::channel(false);
            let first = tokio::spawn(worker.clone().run(observed.clone()));
            let second = tokio::spawn(worker.run(observed));
            tokio::time::timeout(std::time::Duration::from_secs(20), async {
                loop {
                    let count: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM app.handoff_offers WHERE downstream_id=$1",
                    )
                    .bind(down.id.as_uuid())
                    .fetch_one(pool)
                    .await
                    .unwrap();
                    if count == 1 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            })
            .await
            .expect("original policy must produce one Paper offer");
            shutdown.send(true).unwrap();
            first.await.unwrap().unwrap();
            second.await.unwrap().unwrap();
        } else {
            let (a, b) = tokio::join!(
                Box::pin(store.automate_paper(release.project_id, |id, size| f.read(id, size))),
                Box::pin(store.automate_paper(release.project_id, |id, size| f.read(id, size)))
            );
            assert_ne!(a.unwrap().is_some(), b.unwrap().is_some());
        }
        let offer_id: uuid::Uuid =
            sqlx::query_scalar("SELECT id FROM app.handoff_offers WHERE downstream_id=$1")
                .bind(down.id.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
        let offer = store
            .handoff(operator, offer_id.to_string().try_into().unwrap())
            .await
            .unwrap();
        assert_eq!(offer.release_id, release.id);
        assert_eq!(offer.environment, ForwardEnvironmentV1::Paper);
        assert_eq!(offer.state, HandoffStateV1::Offered);
        let approval = store.approval(operator, offer.approval_id).await.unwrap();
        assert_eq!(approval.authority_kind, "FROZEN_POLICY");
        assert_eq!(approval.automation_policy_id, Some(policy.id));
        assert!(offer.expires_at <= policy.content.valid_until);
        assert!(
            Box::pin(store.automate_paper(release.project_id, |_, _| async {
                panic!("original Candidate already has a Paper offer")
            }))
            .await
            .unwrap()
            .is_none()
        );
        let principal = store
            .create_principal(
                operator,
                &format!("{key}-principal"),
                &PrincipalCreate {
                    schema_version: SchemaV1,
                    name: key.clone(),
                    kind: AssignablePrincipalKind::Downstream,
                    project_id: Some(release.project_id),
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
                &format!("{key}-credential"),
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
            panic!("new machine credential")
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
        let page = ListQuery {
            cursor: None,
            limit: 1,
        };
        let listed = store
            .handoffs(&machine, release.project_id, &page)
            .await
            .unwrap();
        assert_eq!(listed.items.len(), 1);
        assert_eq!(listed.items[0].id, offer.id);
        assert!(listed.next_cursor.is_none());
        assert!(store
            .handoffs(
                &machine,
                release.project_id,
                &ListQuery {
                    cursor: Some(offer.id),
                    limit: 1
                }
            )
            .await
            .unwrap()
            .items
            .is_empty());
        assert!(matches!(
            store.handoffs(&machine, Id::new(), &page).await,
            Err(store::StoreError::NotFound)
        ));
        let all = store
            .handoffs(
                operator,
                release.project_id,
                &ListQuery {
                    cursor: None,
                    limit: 100,
                },
            )
            .await
            .unwrap();
        assert_eq!(all.items.len(), index + 1);
        if let Some(other) = all.items.iter().find(|item| item.downstream_id != down.id) {
            assert!(matches!(
                store.handoff(&machine, other.id).await,
                Err(store::StoreError::Forbidden)
            ));
        }
        let revoke = PolicyRevokeV1 {
            schema_version: SchemaV1,
            expected_latest_revocation_id: None,
            effective_at: None,
            reason: "Withdraw future automatic claims".into(),
        };
        if revoke_first {
            store
                .revoke_automation(operator, &format!("{key}-revoke"), policy.id, &revoke)
                .await
                .unwrap();
        }
        let claim = HandoffClaimV1 {
            schema_version: SchemaV1,
            external_claim_id: key.clone(),
            package_schema_version: PackageSchemaVersion::V1,
        };
        let claimed =
            Box::pin(
                store.claim_handoff(&machine, &key, offer.id, &claim, |id, size| {
                    f.read(id, size)
                }),
            )
            .await;
        if revoke_first {
            assert!(matches!(
                claimed,
                Err(StoreError::Invalid("automation_expiry"))
            ));
            assert_eq!(store.reconcile_handoffs().await.unwrap(), 1);
            assert_eq!(
                store.handoff(operator, offer.id).await.unwrap().state,
                HandoffStateV1::Revoked
            );
        } else {
            let claimed = claimed.unwrap();
            assert_eq!(claimed.resource.package.release_id, release.id);
            store
                .revoke_automation(operator, &format!("{key}-revoke"), policy.id, &revoke)
                .await
                .unwrap();
            assert_eq!(store.reconcile_handoffs().await.unwrap(), 0);
            assert_eq!(
                store.handoff(operator, offer.id).await.unwrap().state,
                HandoffStateV1::Claimed
            );
            let replay =
                Box::pin(
                    store.claim_handoff(&machine, &key, offer.id, &claim, |_, _| async {
                        panic!("old transfer must remain replayable")
                    }),
                )
                .await
                .unwrap();
            assert!(replay.replayed);
            let mut replacement = request.clone();
            replacement.expected_project_revision = store
                .project(operator, release.project_id)
                .await
                .unwrap()
                .revision;
            store
                .authorize_automation(
                    operator,
                    "replacement-does-not-rebalance",
                    release.project_id,
                    &replacement,
                )
                .await
                .unwrap();
            assert!(
                Box::pin(store.automate_paper(release.project_id, |_, _| async {
                    panic!("copying a policy does not reset original delivery")
                }))
                .await
                .unwrap()
                .is_none()
            );
        }
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.handoff_offers WHERE environment='LIVE'"
        )
        .fetch_one(pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_transfers")
            .fetch_one(pool)
            .await
            .unwrap(),
        1
    );
}

// Distinct original build/study/publication, never a copied qualification or SQL Release.
pub(super) async fn quota(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    first: &ReleaseViewV1,
) {
    let candidate: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.portfolio_candidates WHERE project_id=$1 AND mandate_id=$2 AND id<>$3 ORDER BY id DESC LIMIT 1")
        .bind(first.project_id.as_uuid()).bind(first.mandate_id.as_uuid()).bind(first.candidate_id.as_uuid()).fetch_one(pool).await.unwrap();
    let candidate: Id = candidate.to_string().try_into().unwrap();
    let intent = Box::pin(qualified_portfolio::original_release_intent(
        pool, store, operator, f, build, candidate,
    ))
    .await;
    let second = Box::pin(store.create_release(
        operator,
        "quota-second-release",
        &intent,
        |id, size| f.read(id, size),
        |object| {
            std::future::ready(
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity),
            )
        },
    ))
    .await
    .unwrap()
    .resource;
    let project = store.project(operator, first.project_id).await.unwrap();
    let policy = store
        .automation_policy(operator, project.current_automation_policy_id.unwrap())
        .await
        .unwrap();
    let down = store
        .downstream(operator, policy.content.downstream_id)
        .await
        .unwrap();
    let store::downstream::ProbePreparation::Pending(ticket) = store
        .prepare_downstream_probe(
            operator,
            "quota-probe",
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
                    environments: vec![ForwardEnvironmentV1::Paper, ForwardEnvironmentV1::Live],
                    market_capability_versions: vec![second.market_capability_version.clone()],
                    accepting_targets: true,
                    checked_at: chrono::Utc::now(),
                },
            },
            |id, bytes| async move { f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity) },
        )
        .await
        .unwrap();
    for limit in [1, 2] {
        let mut content = policy.content.clone();
        content.max_rebalances_per_day = limit;
        store
            .authorize_automation(
                operator,
                &format!("quota-policy-{limit}"),
                project.id,
                &AutomationAuthorizeV1 {
                    schema_version: SchemaV1,
                    expected_project_revision: store
                        .project(operator, project.id)
                        .await
                        .unwrap()
                        .revision,
                    content,
                },
            )
            .await
            .unwrap();
        let result = Box::pin(store.automate_paper(project.id, |id, size| f.read(id, size))).await;
        if limit == 1 {
            assert!(
                matches!(result, Err(StoreError::Invalid("automation_daily_quota"))),
                "{result:?}"
            );
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM app.approvals WHERE release_id=$1")
                    .bind(second.id.as_uuid())
                    .fetch_one(pool)
                    .await
                    .unwrap();
            assert_eq!(count, 0, "quota failure leaves no partial approval");
        } else {
            let offer = result.unwrap().unwrap();
            assert_eq!(offer.candidate_id, candidate);
            assert_eq!(offer.release_id, second.id);
            assert_eq!(offer.downstream_id, down.id);
        }
    }
}
