//! Native qualification/Forward/Live transactions; historical Paper clock and science are controlled.
use super::*;
use contracts::{control::*, delivery::*, forward::ForwardEnvironmentV1, settings::*};

pub(super) async fn check(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    release: &ReleaseViewV1,
    directory: &tempfile::TempDir,
) {
    let row = sqlx::query("SELECT r.id,r.cycle_id,r.input_set_id,r.active_attempt_id,r.deadline_at,e.report_artifact_id FROM app.portfolio_candidates c JOIN app.runs r ON r.id=c.run_id JOIN app.evaluations e ON e.id=$2 WHERE c.id=$1")
        .bind(release.candidate_id.as_uuid()).bind(release.evaluation_id.as_uuid()).fetch_one(pool).await.unwrap();
    let id = |column: &str| -> Id {
        row.get::<uuid::Uuid, _>(column)
            .to_string()
            .try_into()
            .unwrap()
    };
    for changed in [false, true] {
        let relation = forward_support::support::Fixture {
            project: f.data.project,
            cycle: id("cycle_id"),
            run: id("id"),
            session: Id::new(),
            profile: f.researcher_profile.profile_id,
            input_set: id("input_set_id"),
            artifact: f.data.artifact,
            report: id("report_artifact_id"),
            budget: f.brief.content.budget.clone(),
            fence: store::turns::WorkerFence {
                attempt_id: id("active_attempt_id"),
                worker_owner_id: "unused historical relation".into(),
                owner_epoch: contracts::Revision::INITIAL,
            },
            deadline: row.get("deadline_at"),
        };
        let feedback = forward_support::setup_with_release(
            pool,
            relation,
            store.clone(),
            operator.clone(),
            release.id,
        )
        .await;
        for (id, bytes) in feedback.objects.lock().unwrap().iter() {
            match f
                .objects
                .read(*id, DbCounter::new(bytes.len() as u64).unwrap())
            {
                Ok(existing) => assert_eq!(&existing, bytes),
                Err(_) => f.objects.put(*id, bytes).unwrap(),
            }
        }
        let read = |id, size| f.read(id, size);
        let publish = |v: store::lifecycle::native::NativeObjectPublication| {
            f.objects.put(v.id, &v.bytes).unwrap();
            feedback.objects.lock().unwrap().insert(v.id, v.bytes);
            async { Ok(()) }
        };
        let mut content = feedback.policy.content.clone();
        content.mode = AutomationModeV1::AutoHandoff;
        content.required_paper_observations = 2;
        content.minimum_paper_elapsed_seconds = DbCounter::new(172800).unwrap();
        content.max_rebalances_per_day = 1;
        content.valid_until = release.valid_until;
        let key = format!("live-{changed}");
        let policy = store
            .authorize_automation(
                operator,
                &key,
                f.data.project,
                &AutomationAuthorizeV1 {
                    schema_version: SchemaV1,
                    expected_project_revision: store
                        .project(operator, f.data.project)
                        .await
                        .unwrap()
                        .revision,
                    content,
                },
            )
            .await
            .unwrap()
            .resource;
        assert!(matches!(
            store.automate_live(f.data.project, read).await,
            Err(StoreError::Invalid("automation_live_unevaluated_stream"))
        ));
        let measured = store
            .enqueue_forward_evaluation(feedback.handoff, "daily", read, publish)
            .await
            .unwrap()
            .resource;
        forward_result::complete(
            pool,
            store,
            measured.id,
            &feedback.caps,
            &feedback.objects,
            0.1,
        )
        .await;
        for (id, bytes) in feedback.objects.lock().unwrap().iter() {
            match f
                .objects
                .read(*id, DbCounter::new(bytes.len() as u64).unwrap())
            {
                Ok(existing) => assert_eq!(&existing, bytes),
                Err(_) => f.objects.put(*id, bytes).unwrap(),
            }
        }
        store
            .publish_scientific_result(measured.id, read, publish)
            .await
            .unwrap()
            .unwrap();
        let observation = store
            .observe_forward(measured.id)
            .await
            .unwrap()
            .unwrap()
            .resource;
        let unavailable = store.automate_live(f.data.project, read).await;
        assert!(
            matches!(
                unavailable,
                Err(StoreError::Domain(
                    domain::DomainError::CapabilityUnavailable("downstream_delivery_unavailable")
                ))
            ),
            "{unavailable:?}"
        );
        let down = store
            .downstream(operator, policy.content.downstream_id)
            .await
            .unwrap();
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
            panic!("new probe")
        };
        store.complete_downstream_probe(*ticket, DownstreamProbeOutcomeV1::Available { capabilities: DownstreamCapabilitiesV1 {
            schema_version: SchemaV1, delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
            accepted_package_versions: vec![PackageSchemaVersion::V1],
            environments: vec![ForwardEnvironmentV1::Paper, ForwardEnvironmentV1::Live],
            market_capability_versions: vec![release.market_capability_version.clone()],
            accepting_targets: true, checked_at: chrono::Utc::now(),
        }}, |id, bytes| async move { f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity) }).await.unwrap();
        if !changed {
            let before: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.approvals),(SELECT count(*) FROM app.live_promotion_evidence),(SELECT count(*) FROM app.handoff_offers)").fetch_one(pool).await.unwrap();
            sqlx::raw_sql("CREATE FUNCTION app.fail_live_offer() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled Live offer failure'; END $$; CREATE TRIGGER fail_live_offer BEFORE INSERT ON app.handoff_offers FOR EACH ROW EXECUTE FUNCTION app.fail_live_offer();").execute(pool).await.unwrap();
            assert!(store.automate_live(f.data.project, read).await.is_err());
            let after: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.approvals),(SELECT count(*) FROM app.live_promotion_evidence),(SELECT count(*) FROM app.handoff_offers)").fetch_one(pool).await.unwrap();
            assert_eq!(before, after);
            sqlx::raw_sql("DROP TRIGGER fail_live_offer ON app.handoff_offers; DROP FUNCTION app.fail_live_offer();").execute(pool).await.unwrap();
        }
        let offer = if changed {
            let (a, b) = tokio::join!(
                Box::pin(store.automate_live(f.data.project, read)),
                Box::pin(store.automate_live(f.data.project, read))
            );
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_ne!(a.is_some(), b.is_some());
            a.or(b).unwrap()
        } else {
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
            let (a, b) = tokio::join!(
                worker.process_automation(None),
                worker.process_automation(None)
            );
            assert_eq!(a.0, Some(f.data.project));
            assert_eq!(b.0, Some(f.data.project));
            a.1.unwrap();
            b.1.unwrap();
            // SKIP LOCKED may defer both concurrent ticks across independent lanes.
            // The next ordinary tick must consume the still-current Live policy.
            worker.process_automation(None).await.1.unwrap();
            let ids: Vec<uuid::Uuid> = sqlx::query_scalar(
                "SELECT id FROM app.handoff_offers WHERE downstream_id=$1 AND environment='LIVE'",
            )
            .bind(down.id.as_uuid())
            .fetch_all(pool)
            .await
            .unwrap();
            assert_eq!(ids.len(), 1);
            let first = store
                .handoff(operator, ids[0].to_string().try_into().unwrap())
                .await
                .unwrap();
            worker.process_automation(None).await.1.unwrap();
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM app.handoff_offers WHERE downstream_id=$1 AND environment='LIVE'").bind(down.id.as_uuid()).fetch_one(pool).await.unwrap();
            assert_eq!(count, 1);
            first
        };
        assert_eq!(offer.environment, ForwardEnvironmentV1::Live);
        assert!(offer.expires_at <= policy.content.valid_until);
        assert!(offer.expires_at <= release.valid_until);
        let proof: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT observation_ids FROM app.live_promotion_evidence WHERE approval_id=$1",
        )
        .bind(offer.approval_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(proof, vec![observation.as_uuid()]);
        let principal = store
            .create_principal(
                operator,
                &format!("{key}-principal"),
                &PrincipalCreate {
                    schema_version: SchemaV1,
                    name: key.clone(),
                    kind: AssignablePrincipalKind::Downstream,
                    project_id: Some(f.data.project),
                    downstream_id: Some(down.id),
                    enabled: true,
                },
            )
            .await
            .unwrap()
            .resource;
        let store::control::CredentialPreparation::New(ticket) = store
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
            panic!("new credential")
        };
        let verifier = Id::new();
        let credential = ticket.publish(Id::new(), verifier).await.unwrap().resource;
        let machine = store::authority::Actor::Machine {
            credential_id: credential.id,
            verifier_ref: verifier,
            operator_grant: None,
        };
        let claim = HandoffClaimV1 {
            schema_version: SchemaV1,
            external_claim_id: key.clone(),
            package_schema_version: PackageSchemaVersion::V1,
        };
        if changed {
            for (stream, mean, expected_reason) in [
                ("other", 0.1, "automation_live_evidence_changed"),
                ("degraded", -0.1, "automation_live_paper_not_qualified"),
            ] {
                let mut message = feedback.message.clone();
                message.external_message_id = format!("{stream}-stream");
                message.report.stream_id = stream.into();
                store
                    .submit_forward_message(&feedback.actor, &message, read, publish)
                    .await
                    .unwrap();
                assert!(matches!(
                    store
                        .claim_handoff(&machine, &key, offer.id, &claim, read)
                        .await,
                    Err(StoreError::Invalid("automation_live_unevaluated_stream"))
                ));
                let run = store
                    .enqueue_forward_evaluation(feedback.handoff, stream, read, publish)
                    .await
                    .unwrap()
                    .resource;
                forward_result::complete(
                    pool,
                    store,
                    run.id,
                    &feedback.caps,
                    &feedback.objects,
                    mean,
                )
                .await;
                for (id, bytes) in feedback.objects.lock().unwrap().iter() {
                    match f
                        .objects
                        .read(*id, DbCounter::new(bytes.len() as u64).unwrap())
                    {
                        Ok(existing) => assert_eq!(&existing, bytes),
                        Err(_) => f.objects.put(*id, bytes).unwrap(),
                    }
                }
                store
                    .publish_scientific_result(run.id, read, publish)
                    .await
                    .unwrap()
                    .unwrap();
                store.observe_forward(run.id).await.unwrap().unwrap();
                assert!(matches!(
                    store
                        .claim_handoff(&machine, &key, offer.id, &claim, read)
                        .await,
                    Err(StoreError::Invalid(reason)) if reason == expected_reason
                ));
            }
            assert!(
                sqlx::query("DELETE FROM app.live_promotion_evidence WHERE approval_id=$1")
                    .bind(offer.approval_id.as_uuid())
                    .execute(pool)
                    .await
                    .is_err()
            );
            assert_eq!(
                store.handoff(operator, offer.id).await.unwrap().state,
                HandoffStateV1::Offered
            );
            quota(pool, store, operator, f, release, policy.content.clone()).await;
        } else {
            let claimed = store
                .claim_handoff(&machine, &key, offer.id, &claim, read)
                .await
                .unwrap();
            assert_eq!(claimed.resource.handoff.state, HandoffStateV1::Claimed);
            assert_eq!(
                Box::pin(graph_recovery::check(
                    pool,
                    &directory.path().join("objects"),
                    operator,
                ))
                .await,
                1,
                "must check one restored claimed Live project"
            );
            store
                .revoke_automation(
                    operator,
                    &format!("{key}-revoke"),
                    policy.id,
                    &PolicyRevokeV1 {
                        schema_version: SchemaV1,
                        expected_latest_revocation_id: None,
                        effective_at: None,
                        reason: "Original claimed fact survives policy revocation".into(),
                    },
                )
                .await
                .unwrap();
            let replay = store
                .claim_handoff(&machine, &key, offer.id, &claim, |_, _| async {
                    panic!("Claim replay reads no evidence")
                })
                .await
                .unwrap();
            assert!(replay.replayed);
        }
    }
}

// Actual current-day offers, without moving the clock or inventing old observations.
async fn quota(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    release: &ReleaseViewV1,
    mut content: AutomationPolicyContentV1,
) {
    let down = store
        .create_downstream(
            operator,
            "same-candidate-quota-downstream",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Same Candidate daily quota".into(),
                    endpoint: "https://quota.example".into(),
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
    let store::downstream::ProbePreparation::Pending(ticket) = store
        .prepare_downstream_probe(
            operator,
            "same-candidate-quota-probe",
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
                    market_capability_versions: vec![release.market_capability_version.clone()],
                    accepting_targets: true,
                    checked_at: chrono::Utc::now(),
                },
            },
            |id, bytes| async move { f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity) },
        )
        .await
        .unwrap();
    let mut without_paper = content.clone();
    without_paper.downstream_id = down.id;
    without_paper.mode = AutomationModeV1::AutoHandoff;
    let deadline: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT clock_timestamp()+interval '15 seconds'")
            .fetch_one(pool)
            .await
            .unwrap();
    without_paper.valid_until = deadline;
    store
        .authorize_automation(
            operator,
            "live-without-paper-policy",
            f.data.project,
            &AutomationAuthorizeV1 {
                schema_version: SchemaV1,
                expected_project_revision: store
                    .project(operator, f.data.project)
                    .await
                    .unwrap()
                    .revision,
                content: without_paper,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .automate_live(f.data.project, |_, _| async {
                panic!("missing Paper must block before reading Package")
            })
            .await,
        Err(StoreError::Invalid(
            "automation_live_complete_paper_required"
        ))
    ));
    // Wait on the gate's actual clock; never rewrite immutable policy timestamps.
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        sqlx::query("SELECT pg_sleep(GREATEST(EXTRACT(EPOCH FROM ($1::timestamptz-clock_timestamp())),0)::double precision+0.02)")
            .bind(deadline)
            .execute(pool)
            .await
            .unwrap();
    })
    .await
    .expect("policy expiry must arrive within its bounded validity");
    assert!(matches!(
        store
            .automate_live(f.data.project, |_, _| async {
                panic!("expired policy must block before reading Package")
            })
            .await,
        Err(StoreError::Invalid("automation_expiry"))
    ));
    let offers: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.handoff_offers WHERE downstream_id=$1")
            .bind(down.id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(offers, 0);
    let approval = store
        .approve_release(
            operator,
            "same-candidate-manual-live",
            release.id,
            &ReleaseApproveV1 {
                schema_version: SchemaV1,
                downstream_id: down.id,
                environment: ForwardEnvironmentV1::Live,
                expected_downstream_revision: down.revision,
                expected_latest_decision_id: None,
                valid_until: release.valid_until,
            },
            |id, size| f.read(id, size),
        )
        .await
        .unwrap()
        .resource;
    let live = store
        .offer_handoff(
            operator,
            "same-candidate-live-offer",
            &HandoffOfferV1 {
                schema_version: SchemaV1,
                release_id: release.id,
                approval_id: approval.id,
                supersedes_handoff_id: None,
                expires_at: approval.valid_until,
            },
            |id, size| f.read(id, size),
        )
        .await
        .unwrap()
        .resource;
    content.downstream_id = down.id;
    content.mode = AutomationModeV1::AutoPaper;
    content.max_rebalances_per_day = 1;
    store
        .authorize_automation(
            operator,
            "same-candidate-quota-policy",
            f.data.project,
            &AutomationAuthorizeV1 {
                schema_version: SchemaV1,
                expected_project_revision: store
                    .project(operator, f.data.project)
                    .await
                    .unwrap()
                    .revision,
                content,
            },
        )
        .await
        .unwrap();
    let paper = store
        .automate_paper(f.data.project, |id, size| f.read(id, size))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(paper.candidate_id, live.candidate_id);
    assert_eq!(paper.environment, ForwardEnvironmentV1::Paper);
    assert_eq!(live.environment, ForwardEnvironmentV1::Live);
    let counts:(i64,i64)=sqlx::query_as("SELECT count(*),count(DISTINCT r.candidate_id) FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id WHERE h.downstream_id=$1 AND h.offered_at>=(date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC') AND h.offered_at<(date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')+interval '1 day'").bind(down.id.as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(counts, (2, 1));
    assert!(store
        .automate_paper(f.data.project, |_, _| async {
            panic!("existing offer needs no source IO")
        })
        .await
        .unwrap()
        .is_none());
}
