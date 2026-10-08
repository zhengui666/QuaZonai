//! Original qualification/study/release transactions with a controlled numerical
//! result. No market acquisition, Paper engine or real account is fabricated.
use super::*;
use contracts::{control::*, delivery::*, forward::*, portfolio::*, science::*, settings::*};

#[sqlx::test(migrations = "../../migrations")]
async fn explicit_paper_initial_capital_reaches_claim_without_weakening_research(pool: PgPool) {
    Box::pin(exercise(pool)).await;
}

async fn exercise(pool: PgPool) {
    Box::pin(exercise_with_consumer(
        pool,
        |store, machine, claim, request| async move {
            let key = request.owner_instance_id.to_string();
            let (left, right) = tokio::join!(
                Box::pin(store.consume_paper_initial_execution(
                    &machine,
                    &key,
                    claim.handoff.id,
                    &request
                )),
                Box::pin(store.consume_paper_initial_execution(
                    &machine,
                    &key,
                    claim.handoff.id,
                    &request
                )),
            );
            let left = left.unwrap();
            let right = right.unwrap();
            assert_ne!(
                left.replayed, right.replayed,
                "one atomic fresh consumption"
            );
            if left.replayed {
                right
            } else {
                left
            }
        },
    ))
    .await;
}

/// Reuses the complete original qualified chain for Store and real HTTP entry
/// checks. Only the numerical worker result is controlled; no new market data.
pub(crate) async fn exercise_with_consumer<C, F>(pool: PgPool, consume: C)
where
    C: FnOnce(
        Store,
        store::authority::Actor,
        contracts::strategy_portfolio::HandoffClaimViewV2,
        PaperInitialExecutionConsumeV1,
    ) -> F,
    F: std::future::Future<Output = CommandResult<PaperInitialExecutionViewV1>>,
{
    let (store, actor, f, original_build, _, _directory) = Box::pin(qualified_chain_policy(
        pool.clone(),
        cycle_support::Liquidity::None,
        ForwardEnvironmentV1::Paper,
        contracts::research::DataUse::ResearchAndPaper,
        release_policy,
    ))
    .await
    .unwrap();
    let publish = |object: store::lifecycle::native::NativeObjectPublication| {
        std::future::ready(
            f.objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity),
        )
    };
    // Outer boxing alone leaves every await in one large debug-build poll frame.
    // These separate async state machines finish and unwind in business order;
    // the same real Store, PostgreSQL, objects and directory stay alive throughout.
    // Create the account and reject implicit resets before constructing a build.
    let (down, build) = Box::pin(async {
        let down = store
            .create_downstream(
                &actor,
                "initial-paper-downstream",
                &DownstreamCreate {
                    schema_version: SchemaV1,
                    credential_ref: Id::new(),
                    configuration: DownstreamConfigurationV1 {
                        name: "New model account".into(),
                        endpoint: "https://initial-paper.example".into(),
                        accepted_package_versions: vec![PackageSchemaVersion::V2],
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
        let mut build = original_build.clone();
        build.current_weights_source = PortfolioBuildWeightsV1::PaperInitialCapital {
            downstream_id: down.id,
            trader_id: "PAPER-001".into(),
            account_id: "BINANCE-001".into(),
        };
        let mut live = build.clone();
        live.environment = ForwardEnvironmentV1::Live;
        assert!(store
            .start_portfolio_build(
                &actor,
                "initial-not-live",
                &live,
                |_, _| async { panic!("Live initial cannot read") },
                |_| async { panic!("Live initial cannot publish") }
            )
            .await
            .is_err());
        let occupied: uuid::Uuid = sqlx::query_scalar(
            "SELECT downstream_id FROM app.forward_weight_snapshots WHERE project_id=$1 LIMIT 1",
        )
        .bind(f.data.project.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut reset = build.clone();
        reset.current_weights_source = PortfolioBuildWeightsV1::PaperInitialCapital {
            downstream_id: occupied.to_string().try_into().unwrap(),
            trader_id: "PAPER-001".into(),
            account_id: "BINANCE-001".into(),
        };
        assert!(matches!(
            store
                .start_portfolio_build(
                    &actor,
                    "cannot-erase-original-weights",
                    &reset,
                    |id, size| f.read(id, size),
                    publish
                )
                .await,
            Err(StoreError::Invalid("paper_account_already_has_state"))
        ));
        (down, build)
    })
    .await;

    // Complete the real build and keep its immutable initialization evidence.
    let (root, weights_size, original_weights, candidate) = Box::pin(async {
        let run = store
            .start_portfolio_build(
                &actor,
                "initial-build",
                &build,
                |id, size| f.read(id, size),
                publish,
            )
            .await
            .unwrap()
            .resource;
        let replay = store
            .start_portfolio_build(
                &actor,
                "initial-build",
                &build,
                |_, _| async { panic!("replay reads nothing") },
                |_| async { panic!("replay publishes nothing") },
            )
            .await
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.resource.id, run.id);
        let message = validation_publication::message(&pool, run.id).await;
        let Some(ClaimResult::Leased(lease)) = store
            .claim_native_run(&message, "initial-build", 60)
            .await
            .unwrap()
        else {
            panic!("native build lease")
        };
        let job = store.native_job(run.id, &lease.fence).await.unwrap();
        let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
            .bind(job.spec.parameters_artifact_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
        let NativeTaskParametersV1::BuildPortfolio {
            request: frozen, ..
        } = serde_json::from_slice(
            &f.read(
                job.spec.parameters_artifact_id,
                DbCounter::new(size as u64).unwrap(),
            )
            .await
            .unwrap(),
        )
        .unwrap()
        else {
            panic!("original build")
        };
        let root = frozen.current_weights.paper_initialization.clone().unwrap();
        assert_eq!(root.artifact_id, frozen.current_weights_artifact_id);
        assert_eq!(root.downstream_id, down.id);
        assert_eq!(frozen.current_weights.cash_weight, "1".parse().unwrap());
        assert!(frozen
            .current_weights
            .weights
            .iter()
            .all(|w| w.weight == "0".parse().unwrap()));
        assert_eq!(
            frozen.current_weights.asof_ns,
            frozen.selection.decision_cutoff_ns
        );
        let PortfolioWeightsSourceV1::PaperInitialCapital { account_start } =
            &frozen.current_weights.source
        else {
            panic!("explicit model condition")
        };
        assert_eq!(
            account_start.starting_capital,
            frozen.mandate.capital_assumption
        );
        assert_eq!(
            account_start.starting_capital,
            frozen.execution_settings.starting_capital
        );
        let weights_size: i64 =
            sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
                .bind(root.artifact_id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        let original_weights = f
            .read(
                root.artifact_id,
                DbCounter::new(weights_size as u64).unwrap(),
            )
            .await
            .unwrap();
        let created: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT created_at FROM app.paper_initial_capital_sources WHERE weights_artifact_id=$1",
        )
        .bind(root.artifact_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            created.timestamp_nanos_opt().unwrap() as u64
                >= frozen.current_weights.available_ns.get(),
            "receipt retains actual creation time"
        );
        let changed_mandate = store
            .create_mandate(
                &actor,
                "renamed-paper-mandate",
                &MandateCreateV1 {
                    schema_version: SchemaV1,
                    project_id: f.data.project,
                    runtime_id: build.runtime_id,
                    expected_runtime_revision: build.expected_runtime_revision,
                    content: frozen.mandate.clone(),
                },
            )
            .await
            .unwrap()
            .resource;
        let mut changed = build.clone();
        changed.mandate_id = changed_mandate.id;
        assert!(matches!(
            store
                .start_portfolio_build(
                    &actor,
                    "cannot-fund-new-mandate",
                    &changed,
                    |id, size| f.read(id, size),
                    publish
                )
                .await,
            Err(StoreError::Invalid(
                "paper_initialization_scope_already_bound"
            ))
        ));
        Box::pin(result::complete(&pool, &store, &f, &lease, &job)).await;
        let candidate = validation_publication::publish(&store, &f, run.id)
            .await
            .unwrap()
            .resource;
        store.acknowledge_run(&message).await.unwrap();
        assert_eq!(
            store
                .candidate(&actor, candidate)
                .await
                .unwrap()
                .header
                .origin,
            DataOrigin::Synthetic
        );
        (root, weights_size, original_weights, candidate)
    })
    .await;

    // Unwind build polling before the independent study and original release.
    let (release, offered) = Box::pin(async {
        let intent = Box::pin(original_release_intent(
            &pool, &store, &actor, &f, &build, candidate,
        ))
        .await;
        let evaluation = store
            .evaluation(&actor, intent.evaluation_id)
            .await
            .unwrap();
        assert_eq!(evaluation.decision, contracts::evidence::Decision::Pass);
        let study_origin:String=sqlx::query_scalar("SELECT a.origin FROM app.evaluations e JOIN app.artifacts a ON a.id=e.report_artifact_id WHERE e.id=$1")
            .bind(intent.evaluation_id.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(
            study_origin, "REAL",
            "independent market study stays real evidence"
        );
        let release = Box::pin(store.create_release(
            &actor,
            "initial-release",
            &intent,
            |id, size| f.read(id, size),
            publish,
        ))
        .await
        .unwrap()
        .resource;
        assert_eq!(release.environment, PackageOriginV1::Synthetic);
        let store::downstream::ProbePreparation::Pending(ticket) = store
            .prepare_downstream_probe(
                &actor,
                "initial-probe",
                down.id,
                &DownstreamProbeRequestV1 {
                    schema_version: SchemaV1,
                    expected_revision: down.revision,
                },
            )
            .await
            .unwrap()
        else {
            panic!("probe")
        };
        store
            .complete_downstream_probe(
                *ticket,
                DownstreamProbeOutcomeV1::Available {
                    capabilities: DownstreamCapabilitiesV1 {
                        schema_version: SchemaV1,
                        delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
                        accepted_package_versions: vec![PackageSchemaVersion::V2],
                        environments: vec![ForwardEnvironmentV1::Paper, ForwardEnvironmentV1::Live],
                        market_capability_versions: vec![release.market_capability_version.clone()],
                        accepting_targets: true,
                        checked_at: chrono::Utc::now(),
                    },
                },
                |id, bytes| {
                    std::future::ready(f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity))
                },
            )
            .await
            .unwrap();
        let request = ReleaseApproveV1 {
            schema_version: SchemaV1,
            downstream_id: down.id,
            environment: ForwardEnvironmentV1::Paper,
            expected_downstream_revision: down.revision,
            expected_latest_decision_id: None,
            valid_until: release.valid_until,
        };
        let mut request_live = request.clone();
        request_live.environment = ForwardEnvironmentV1::Live;
        assert!(matches!(
            Box::pin(store.approve_release(
                &actor,
                "paper-never-live",
                release.id,
                &request_live,
                |id, size| f.read(id, size)
            ))
            .await,
            Err(StoreError::Invalid("paper_initialization_paper_only"))
        ));
        let approved = Box::pin(store.approve_release(
            &actor,
            "initial-approval",
            release.id,
            &request,
            |id, size| f.read(id, size),
        ))
        .await
        .unwrap()
        .resource;
        let offered = Box::pin(store.offer_handoff(
            &actor,
            "initial-offer",
            &HandoffOfferV1 {
                schema_version: SchemaV1,
                release_id: release.id,
                approval_id: approved.id,
                supersedes_handoff_id: None,
                expires_at: release.valid_until,
            },
            |id, size| f.read(id, size),
        ))
        .await
        .unwrap()
        .resource;
        (release, offered)
    })
    .await;

    // Bind the original credential and claim without retaining release poll frames.
    let (principal, credential, machine, claimed, canonical, consumption, key) = Box::pin(async {
        let principal = store
            .create_principal(
                &actor,
                "initial-consumer",
                &PrincipalCreate {
                    schema_version: SchemaV1,
                    name: "Model consumer".into(),
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
                &actor,
                "initial-credential",
                principal.id,
                &CredentialIssue {
                    schema_version: SchemaV1,
                    scope_codes: vec![MachineScope::DownstreamClaim, MachineScope::ForwardSubmit],
                    expires_at: release.valid_until,
                },
            )
            .await
            .unwrap()
        else {
            panic!("credential")
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
            external_claim_id: "initial-claim".into(),
            package_schema_version: PackageSchemaVersion::V2,
        };
        let claimed = Box::pin(store.claim_handoff(
            &machine,
            "initial-claim",
            offered.id,
            &claim,
            |id, size| f.read(id, size),
        ))
        .await
        .unwrap()
        .resource;
        assert_eq!(
            claimed.package.current_weights.paper_initialization,
            Some(root.clone())
        );
        assert_eq!(claimed.package.forward_dataset.origin, DataOrigin::Real);
        assert_eq!(
            claimed.package.environment_origin,
            PackageOriginV1::Synthetic
        );
        assert!(
            Box::pin(store.claim_handoff(
                &machine,
                "initial-claim",
                offered.id,
                &claim,
                |_, _| async { panic!("original receipt only") }
            ))
            .await
            .unwrap()
            .replayed
        );
        let canonical = contracts::strategy_portfolio::HandoffClaimViewV2 {
            handoff: claimed.handoff.clone(),
            package: contracts::strategy_portfolio::TargetPackageEnvelopeV2::Forecast(Box::new(
                claimed.package.clone(),
            )),
        };
        let consumption = PaperInitialExecutionConsumeV1 {
            schema_version: SchemaV1,
            paper_initialization: root.clone(),
            release_id: claimed.handoff.release_id,
            external_claim_id: "initial-claim".into(),
            owner_instance_id: Id::new(),
        };
        let key = consumption.owner_instance_id.to_string();
        (
            principal,
            credential,
            machine,
            claimed,
            canonical,
            consumption,
            key,
        )
    })
    .await;

    // Check authority and database binding before entering the consumer.
    let initial_before = Box::pin(async {
        let initial_before: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(s) FROM app.paper_initial_capital_sources s WHERE weights_artifact_id=$1",
        )
        .bind(root.artifact_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT paper_claim_credential_id FROM app.handoff_offers WHERE id=$1"
            )
            .bind(offered.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
            credential.id.as_uuid()
        );
        let mut wrong = consumption.clone();
        wrong.paper_initialization.account_id = "OTHER-ACCOUNT".into();
        assert!(store
            .consume_paper_initial_execution(&machine, "wrong-root-scope", offered.id, &wrong)
            .await
            .is_err());
        assert!(store
            .consume_paper_initial_execution(
                &actor,
                "operator-is-not-claim-owner",
                offered.id,
                &consumption
            )
            .await
            .is_err());
        // A second valid credential for exactly the same principal is not the
        // original claimed owner. It must not borrow the first credential's claim.
        let store::control::CredentialPreparation::New(second_ticket) = store
            .prepare_credential_issuance(
                &actor,
                "initial-second-credential",
                principal.id,
                &CredentialIssue {
                    schema_version: SchemaV1,
                    scope_codes: vec![MachineScope::DownstreamClaim],
                    expires_at: release.valid_until,
                },
            )
            .await
            .unwrap()
        else {
            panic!("second credential")
        };
        let second_verifier = Id::new();
        let second_credential = second_ticket
            .publish(Id::new(), second_verifier)
            .await
            .unwrap()
            .resource;
        let second_machine = store::authority::Actor::Machine {
            credential_id: second_credential.id,
            verifier_ref: second_verifier,
            operator_grant: None,
        };
        assert!(matches!(
            store
                .consume_paper_initial_execution(
                    &second_machine,
                    "wrong-claim-credential",
                    offered.id,
                    &consumption
                )
                .await,
            Err(StoreError::Forbidden)
        ));
        assert!(sqlx::query_scalar::<_, bool>(
            "SELECT app.paper_initial_execution_claim_valid($1,$2,$3,clock_timestamp())"
        )
        .bind(root.artifact_id.as_uuid())
        .bind(offered.id.as_uuid())
        .bind(credential.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap());
        assert!(!sqlx::query_scalar::<_, bool>(
            "SELECT app.paper_initial_execution_claim_valid($1,$2,$3,$4)"
        )
        .bind(root.artifact_id.as_uuid())
        .bind(offered.id.as_uuid())
        .bind(credential.id.as_uuid())
        .bind(release.valid_until)
        .fetch_one(&pool)
        .await
        .unwrap());
        initial_before
    })
    .await;

    // The Store race or authenticated HTTP/crypto consumer has its own poll frame.
    let consumed = Box::pin(consume(
        store.clone(),
        machine.clone(),
        canonical.clone(),
        consumption.clone(),
    ))
    .await;

    // Read back the sole consumption and exercise all replay and reset guards.
    Box::pin(async {
        assert!(!consumed.replayed);
        assert_eq!(
            consumed.resource.state,
            PaperInitialExecutionStateV1::Consumed
        );
        assert_eq!(
            serde_json::to_value(&consumed.resource.claim).unwrap(),
            serde_json::to_value(&canonical).unwrap()
        );
        assert_eq!(
            consumed.resource.package_artifact_id,
            release.package_artifact_id
        );
        assert_eq!(consumed.resource.consuming_credential_id, credential.id);
        let replay = store
            .consume_paper_initial_execution(&machine, &key, offered.id, &consumption)
            .await
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(
            serde_json::to_value(replay.resource).unwrap(),
            serde_json::to_value(&consumed.resource).unwrap()
        );
        assert!(matches!(
            store
                .consume_paper_initial_execution(
                    &machine,
                    "new-key-same-root",
                    offered.id,
                    &consumption
                )
                .await,
            Err(StoreError::Invalid(
                "paper_initial_execution_already_consumed"
            ))
        ));
        let mut restarted = consumption.clone();
        restarted.owner_instance_id = Id::new();
        assert!(matches!(
            store
                .consume_paper_initial_execution(
                    &machine,
                    "new-process-new-journal",
                    offered.id,
                    &restarted
                )
                .await,
            Err(StoreError::Invalid(
                "paper_initial_execution_already_consumed"
            ))
        ));
        assert!(matches!(
            store
                .consume_paper_initial_execution(&machine, &key, offered.id, &restarted)
                .await,
            Err(StoreError::IdempotencyConflict)
        ));
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM app.command_receipts WHERE operation='PAPER_INITIAL_EXECUTION_CONSUME' AND resource_id=$1")
            .bind(root.artifact_id.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1);
        let duplicate=sqlx::query("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) SELECT principal_scope,operation,'sql-new-key',normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body FROM app.command_receipts WHERE operation='PAPER_INITIAL_EXECUTION_CONSUME' AND resource_id=$1")
            .bind(root.artifact_id.as_uuid()).execute(&pool).await.unwrap_err();
        assert_eq!(
            duplicate.as_database_error().unwrap().constraint(),
            Some("paper_initial_execution_once")
        );
        let changed=sqlx::query("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) SELECT principal_scope,operation,'sql-tampered-package',normalized_nonsecret_request,resource_id,response_status,jsonb_set(response_nonsecret_body,'{resource,claim,package,cash_weight}','\"0.5\"'::jsonb) FROM app.command_receipts WHERE operation='PAPER_INITIAL_EXECUTION_CONSUME' AND resource_id=$1")
            .bind(root.artifact_id.as_uuid()).execute(&pool).await.unwrap_err();
        assert_eq!(
            changed.as_database_error().unwrap().message(),
            "PAPER_INITIAL_EXECUTION_BINDING"
        );
        assert_eq!(initial_before,sqlx::query_scalar::<_,serde_json::Value>("SELECT to_jsonb(s) FROM app.paper_initial_capital_sources s WHERE weights_artifact_id=$1")
            .bind(root.artifact_id.as_uuid()).fetch_one(&pool).await.unwrap());
        assert!(matches!(
            store
                .start_portfolio_build(
                    &actor,
                    "cannot-refund-after-claim",
                    &build,
                    |id, size| f.read(id, size),
                    publish
                )
                .await,
            Err(StoreError::Invalid("paper_account_already_has_state"))
        ));
    })
    .await;

    // Keep reported observations separate from unchanged daily research evidence.
    let end = Box::pin(async {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let end: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&pool)
            .await
            .unwrap();
        let report = ForwardMessageSubmitV1 {
            schema_version: SchemaV1,
            external_message_id: "simulated-return".into(),
            report: ForwardReportContentV1 {
                schema_version: SchemaV1,
                project_id: f.data.project,
                handoff_id: offered.id,
                external_claim_id: "initial-claim".into(),
                issuer_version: "controlled-paper/1".into(),
                stream_id: "observed".into(),
                sequence: DbCounter::new(1).unwrap(),
                message_revision: 1,
                supersedes_message_id: None,
                window_start: claimed.handoff.claimed_at.unwrap(),
                window_end: end,
                issued_at: end,
                complete: true,
                returns_frequency: Some(ForwardReturnsFrequencyV1::ReportedObservation),
                returns: vec![NativeReturnV1 {
                    timestamp_ns: DbCounter::new(end.timestamp_nanos_opt().unwrap() as u64).unwrap(),
                    value: Some(0.01),
                    reason_code: None,
                }],
            },
        };
        let report_view = store
            .submit_forward_message(&machine, &report, |id, size| f.read(id, size), publish)
            .await
            .unwrap()
            .resource;
        let report_origin: String = sqlx::query_scalar("SELECT origin FROM app.artifacts WHERE id=$1")
            .bind(report_view.report_artifact_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(report_origin, "SYNTHETIC");
        let window = store
            .forward_window(
                &actor,
                offered.id,
                &ForwardWindowQueryV1 {
                    stream_id: "observed".into(),
                },
                |id, size| f.read(id, size),
            )
            .await
            .unwrap();
        assert_eq!(
            window.returns_frequency,
            Some(ForwardReturnsFrequencyV1::ReportedObservation)
        );
        assert!(
            domain::forward::evaluation::request(&NativeForwardRequestV1 {
                window: window.clone(),
                sources: vec![report_view.clone()],
            })
            .is_err(),
            "server-retrieved reported observations do not satisfy the unchanged daily metric contract"
        );
        assert!(
            store
                .enqueue_forward_evaluation(
                    offered.id,
                    "observed",
                    |id, size| f.read(id, size),
                    publish
                )
                .await
                .is_err(),
            "short observations cannot impersonate daily evaluation evidence"
        );
        end
    })
    .await;

    // Persist rooted follow-on weights and retain historical replay semantics.
    Box::pin(async {
        let mut snapshot = DownstreamWeightsSubmitV1 {
            schema_version: SchemaV1,
            project_id: f.data.project,
            environment: ForwardEnvironmentV1::Paper,
            external_message_id: "paper-follow-on".into(),
            paper_initialization: Some(root.clone()),
            asof_ns: DbCounter::new(end.timestamp_nanos_opt().unwrap() as u64).unwrap(),
            available_ns: DbCounter::new(end.timestamp_nanos_opt().unwrap() as u64).unwrap(),
            valid_until_ns: DbCounter::new(release.valid_until.timestamp_nanos_opt().unwrap() as u64)
                .unwrap(),
            base_currency: claimed.package.base_currency.clone(),
            cash_weight: claimed.package.cash_weight.clone(),
            weights: claimed
                .package
                .targets
                .iter()
                .map(|t| AllocationTargetV1 {
                    instrument_id: t.instrument_id.clone(),
                    currency: t.currency.clone(),
                    weight: t.target_weight.clone(),
                })
                .collect(),
        };
        let mut wrong = snapshot.clone();
        wrong.paper_initialization.as_mut().unwrap().artifact_id = Id::new();
        assert!(matches!(
            store
                .submit_downstream_weights(&machine, &wrong, publish)
                .await,
            Err(StoreError::Invalid("paper_initialization_claim_binding"))
        ));
        for environment in [ForwardEnvironmentV1::Paper, ForwardEnvironmentV1::Live] {
            let mut missing = snapshot.clone();
            missing.paper_initialization = None;
            missing.environment = environment;
            missing.external_message_id = format!("missing-root-{environment:?}");
            assert!(matches!(
                store
                    .submit_downstream_weights(&machine, &missing, |_| async {
                        panic!("missing root cannot publish an observation")
                    })
                    .await,
                Err(StoreError::Invalid(
                    "paper_initialization_reference_required"
                ))
            ));
        }
        let stored = store
            .submit_downstream_weights(&machine, &snapshot, publish)
            .await
            .unwrap()
            .resource;
        assert_eq!(stored.content.paper_initialization, Some(root.clone()));
        let stored_row: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(w) FROM app.forward_weight_snapshots w WHERE id=$1")
                .bind(stored.id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        for environment in ["PAPER", "LIVE"] {
            let mut attempted = stored_row.clone();
            attempted["id"] = serde_json::json!(Id::new());
            attempted["external_message_id"] =
                serde_json::json!(format!("sql-missing-root-{environment}"));
            attempted["environment"] = serde_json::json!(environment);
            attempted["content"]
                .as_object_mut()
                .unwrap()
                .remove("paper_initialization");
            let error=sqlx::query("INSERT INTO app.forward_weight_snapshots SELECT (jsonb_populate_record(NULL::app.forward_weight_snapshots,$1)).*")
                .bind(attempted).execute(&pool).await.unwrap_err();
            assert_eq!(
                error.as_database_error().unwrap().message(),
                "PAPER_WEIGHTS_ROOT_REQUIRED"
            );
        }
        // Controlled pre-fix historical receipt only: reading an exact immutable
        // result must not create an observation under the new write rules.
        let mut historical = snapshot.clone();
        historical.paper_initialization = None;
        historical.environment = ForwardEnvironmentV1::Live;
        historical.external_message_id = "historical-rootless-receipt".into();
        let mut content = stored.content.clone();
        content.paper_initialization = None;
        content.source = PortfolioWeightsSourceV1::ForwardSnapshot {
            downstream_id: down.id,
            external_message_id: historical.external_message_id.clone(),
        };
        let historical_view = DownstreamWeightsViewV1 {
            id: Id::new(),
            project_id: f.data.project,
            downstream_id: down.id,
            environment: ForwardEnvironmentV1::Live,
            report_artifact_id: Id::new(),
            content,
            received_at: end,
        };
        let body = serde_json::json!({"schema_version":1,"replayed":false,"resource":historical_view});
        let receipt = Id::new();
        sqlx::query("INSERT INTO app.command_receipts(id,principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) VALUES($1,$2,'FORWARD_WEIGHTS',$3,$4,$5,201,$6)")
            .bind(receipt.as_uuid()).bind(format!("DOWNSTREAM:{}:PROJECT:{}:LIVE",down.id,f.data.project))
            .bind(&historical.external_message_id).bind(serde_json::to_value(&historical).unwrap())
            .bind(historical_view.id.as_uuid()).bind(&body).execute(&pool).await.unwrap();
        let count_before: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app.forward_weight_snapshots WHERE downstream_id=$1",
        )
        .bind(down.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        let replay = store
            .submit_downstream_weights(&machine, &historical, |_| async {
                panic!("historical replay cannot publish")
            })
            .await
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(
            serde_json::to_value(replay.resource).unwrap(),
            serde_json::to_value(&historical_view).unwrap()
        );
        let unchanged: serde_json::Value =
            sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE id=$1")
                .bind(receipt.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, body);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM app.forward_weight_snapshots WHERE downstream_id=$1"
            )
            .bind(down.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
            count_before
        );
        historical.valid_until_ns = DbCounter::new(historical.valid_until_ns.get() - 1).unwrap();
        assert!(matches!(
            store
                .submit_downstream_weights(&machine, &historical, |_| async {
                    panic!("changed replay cannot publish")
                })
                .await,
            Err(StoreError::IdempotencyConflict)
        ));
        snapshot.environment = ForwardEnvironmentV1::Live;
        snapshot.external_message_id = "not-real-observation".into();
        assert!(store
            .submit_downstream_weights(&machine, &snapshot, publish)
            .await
            .is_err());
    })
    .await;

    // Verify immutable funding and retire authority only after the complete flow.
    Box::pin(async {
        let mut live_follow = build.clone();
        live_follow.environment = ForwardEnvironmentV1::Live;
        live_follow.current_weights_source = PortfolioBuildWeightsV1::LastTarget {
            candidate_id: candidate,
        };
        assert!(store
            .start_portfolio_build(
                &actor,
                "paper-root-cannot-be-live-target",
                &live_follow,
                |id, size| f.read(id, size),
                publish
            )
            .await
            .is_err());
        assert_eq!(
            f.read(
                root.artifact_id,
                DbCounter::new(weights_size as u64).unwrap()
            )
            .await
            .unwrap(),
            original_weights
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app.paper_initial_capital_sources WHERE downstream_id=$1",
        )
        .bind(down.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, 1,
            "all retries retain one immutable funding condition"
        );
        // Retiring an already consumed approval preserves inert original status;
        // revoking its credential then removes even read-through machine authority.
        store
            .revoke_approval(
                &actor,
                "consume-retire-approval",
                offered.approval_id,
                &ApprovalRevokeV1 {
                    schema_version: SchemaV1,
                    expected_latest_revocation_id: None,
                    effective_at: None,
                    reason_code: "PAPER_RETIRED".into(),
                    reason: "Controlled initialization retirement".into(),
                },
            )
            .await
            .unwrap();
        assert!(
            store
                .consume_paper_initial_execution(&machine, &key, offered.id, &consumption)
                .await
                .unwrap()
                .replayed
        );
        store
            .revoke_credential(
                &actor,
                "consume-retire-credential",
                credential.id,
                &CredentialRevoke {
                    schema_version: SchemaV1,
                    reason: "Controlled initialization credential retirement".into(),
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            store
                .consume_paper_initial_execution(&machine, &key, offered.id, &consumption)
                .await,
            Err(StoreError::InvalidCredentials)
        ));
    })
    .await;
}
