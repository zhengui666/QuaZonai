//! Native qualification/Forward/Live transactions; historical Paper clock and science are controlled.
use super::*;
use contracts::{control::*, delivery::*, forward::ForwardEnvironmentV1, settings::*};

pub(super) async fn check(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    release: &ReleaseViewV1,
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
        let (a, b) = tokio::join!(
            Box::pin(store.automate_live(f.data.project, read)),
            Box::pin(store.automate_live(f.data.project, read))
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_ne!(a.is_some(), b.is_some());
        let offer = a.or(b).unwrap();
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
            let mut message = feedback.message.clone();
            message.external_message_id = "other-stream".into();
            message.report.stream_id = "other".into();
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
                .enqueue_forward_evaluation(feedback.handoff, "other", read, publish)
                .await
                .unwrap()
                .resource;
            forward_result::complete(pool, store, run.id, &feedback.caps, &feedback.objects, 0.1)
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
                Err(StoreError::Invalid("automation_live_evidence_changed"))
            ));
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
        } else {
            let claimed = store
                .claim_handoff(&machine, &key, offer.id, &claim, read)
                .await
                .unwrap();
            assert_eq!(claimed.resource.handoff.state, HandoffStateV1::Claimed);
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
