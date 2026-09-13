//! Original Store admissions and files with controlled runtime/model declarations.
//! No SQL-authored qualifications, actual model inference or REAL market claim.
use super::*;
use contracts::{experiments::ExperimentProposalV1, research::DataOrigin};
use store::turns::{NativePublicSummary, TurnOutcome, UsageReceipt};

#[path = "portfolio_inputs.rs"]
mod inputs;
#[path = "portfolio_result.rs"]
mod result;
#[path = "candidate_simulation_result.rs"]
mod simulation_result;

async fn begin(store: &Store, lease: &RunLease) {
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(
            lease.run.id,
            &lease.fence,
            &store::lifecycle::mission::NativeSessionReceipt {
                thread_id: Id::new().to_string(),
                codex_version: "0.144.4".into(),
                protocol_schema_version: "v2".into(),
                requested_service_tier: None,
                effective: contracts::codex::CodexEffectiveSettingsV1 {
                    model: "controlled-native-model".into(),
                    provider: "controlled-native-provider".into(),
                    reasoning_effort: Some("medium".into()),
                    service_tier: None,
                },
            },
        )
        .await
        .unwrap();
}

async fn answer(store: &Store, f: &cycle_support::Fixture, lease: &RunLease, text: String) {
    let turn = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap()
        .reservation;
    let native_turn = Id::new().to_string();
    store
        .claim_turn_dispatch(turn.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(turn.id, &lease.fence, &native_turn)
        .await
        .unwrap();
    store
        .observe_mission_turn_terminal(
            turn.id,
            &lease.fence,
            TurnOutcome::Succeeded,
            "NATIVE_TURN_COMPLETED",
        )
        .await
        .unwrap();
    store
        .settle_turn(
            turn.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(12).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "NATIVE_TURN_COMPLETED".into(),
            },
        )
        .await
        .unwrap();
    store
        .record_mission_summary(
            turn.id,
            &lease.fence,
            &NativePublicSummary {
                schema_version: SchemaV1,
                native_turn_id: native_turn,
                native_item_id: Id::new().to_string(),
                phase: Some("final_answer".into()),
                text,
            },
            |id, size| f.read(id, size),
            |object| async move {
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_reviewed_alphas_publish_candidates_and_retry_last_target(pool: PgPool) {
    Box::pin(qualified_chain(pool, false)).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_liquidity_remains_bound_through_original_qualified_chain(pool: PgPool) {
    Box::pin(qualified_chain(pool, true)).await;
}

async fn qualified_chain(pool: PgPool, with_liquidity: bool) {
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup_with_policy(
        &pool,
        &store,
        &actor,
        objects,
        DataOrigin::Real,
        with_liquidity,
        |policy| {
            policy.selection.candidate_count = 2;
            policy.maximum_sealed_uses_per_lineage = 2;
            policy.sealed_metric_requirements[0].threshold_low = Some("0.1".parse().unwrap());
            let mut portfolio = policy.metric_requirements[0].clone();
            portfolio.metric_code = "PORTFOLIO_DAILY_RETURN_MEAN".into();
            portfolio.scope = "portfolio".into();
            portfolio.method_allowlist = vec!["nautilus-analysis.ReturnsAverage".into()];
            policy.portfolio_metric_requirements = Some(vec![portfolio]);
        },
    )
    .await;
    let (store, actor, f, cycle, preparation) =
        mission_support::start(store, actor, f, false).await;
    mission_support::complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(parent)) = store
        .claim_mission(&message, "two-source-research", 120)
        .await
        .unwrap()
    else {
        panic!("original Research admission");
    };
    begin(&store, &parent).await;
    store
        .prepare_initial_mission_turn(
            parent.run.id,
            &parent.fence,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap();
    answer(
        &store,
        &f,
        &parent,
        "Controlled research plan, not native science.".into(),
    )
    .await;
    let first = experiment_support::propose(&pool, &store, &actor, &f, cycle).await;
    let body: serde_json::Value = sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='EXPERIMENT_PROPOSE' AND resource_id=$1")
        .bind(first.as_uuid()).fetch_one(&pool).await.unwrap();
    let mut proposal: ExperimentProposalV1 = serde_json::from_value(body).unwrap();
    proposal.hypothesis =
        "Second preregistered controlled trial with the same original input contract".into();
    let second = store
        .propose_experiment(&actor, "second-original-trial", &proposal)
        .await
        .unwrap()
        .resource
        .id;
    let experiments = [first, second];
    for experiment in experiments {
        let compiler = start(&store, &f, &parent, experiment)
            .await
            .unwrap()
            .resource
            .id;
        complete_compilation(&pool, &store, &f, compiler).await;
        let predicted = forecast(&store, &f, &parent, experiment)
            .await
            .unwrap()
            .resource
            .id;
        experiment_support::complete_forecast(&pool, &store, &f, predicted).await;
        store
            .prepare_research_alpha(parent.run.id, &parent.fence, experiment)
            .await
            .unwrap();
        let validated = validation(&store, &f, &parent, experiment)
            .await
            .unwrap()
            .resource
            .id;
        experiment_support::complete_validation(&pool, &store, &f, validated, 1000, 0.8).await;
        validation_publication::publish(&store, &f, validated)
            .await
            .unwrap();
        assert!(result_turn(&store, &f, &parent).await.unwrap());
        answer(
            &store,
            &f,
            &parent,
            "Controlled acknowledgement of original formal Validation.".into(),
        )
        .await;
    }
    assert!(store
        .complete_mission(parent.run.id, &parent.fence)
        .await
        .unwrap());
    store.acknowledge_run(&message).await.unwrap();
    let message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(review)) = store
        .claim_mission(&message, "two-source-review", 120)
        .await
        .unwrap()
    else {
        panic!("original independent Reviewer admission");
    };
    begin(&store, &review).await;
    for _ in experiments {
        let work = store
            .mission_review_work(review.run.id, &review.fence)
            .await
            .unwrap()
            .unwrap();
        store
            .prepare_mission_review_turn(
                review.run.id,
                &review.fence,
                work.experiment_id,
                |id, size| f.read(id, size),
                |object| {
                    std::future::ready(
                        f.objects
                            .put(object.id, &object.bytes)
                            .map_err(|_| StoreError::Integrity),
                    )
                },
            )
            .await
            .unwrap();
        answer(
            &store,
            &f,
            &review,
            serde_json::json!({"schema_version":1,
            "alpha_version_id":work.alpha_version_id,"decision":"PASS",
            "reasons":["Controlled independent protocol assessment, not actual scientific review"]})
            .to_string(),
        )
        .await;
    }
    assert!(store
        .mission_review_work(review.run.id, &review.fence)
        .await
        .unwrap()
        .is_none());
    for _ in experiments {
        store
            .prepare_review_sealed(
                review.run.id,
                &review.fence,
                |id, size| f.read(id, size),
                |object| {
                    std::future::ready(
                        f.objects
                            .put(object.id, &object.bytes)
                            .map_err(|_| StoreError::Integrity),
                    )
                },
            )
            .await
            .unwrap();
    }
    // Match the Worker: admit every reviewed target before ending the Reviewer,
    // with both native jobs queued under the frozen parallel-run limit of two.
    assert!(store
        .complete_mission(review.run.id, &review.fence)
        .await
        .unwrap());
    store.acknowledge_run(&message).await.unwrap();
    let sealed_runs: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT held.run_id FROM app.mission_sealed_evaluations held JOIN app.mission_review_turns t ON t.reservation_id=held.review_reservation_id JOIN app.runs r ON r.id=held.run_id WHERE t.run_id=$1 AND r.state='QUEUED' ORDER BY held.run_id")
        .bind(review.run.id.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(sealed_runs.len(), 2);
    for sealed in sealed_runs {
        let sealed: Id = sealed.to_string().try_into().unwrap();
        let message = validation_publication::message(&pool, sealed).await;
        let Some(ClaimResult::Leased(lease)) = store
            .claim_native_run(&message, "two-source-sealed", 60)
            .await
            .unwrap()
        else {
            panic!("original Sealed admission");
        };
        experiment_support::complete_sealed(&pool, &store, &f, *lease).await;
        validation_publication::publish(&store, &f, sealed)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            store.acknowledge_run(&message),
            store.acknowledge_run(&message)
        );
        a.unwrap();
        b.unwrap();
    }
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT count(*),count(DISTINCT q.alpha_version_id),count(DISTINCT q.qualifying_evaluation_id) FROM app.qualifications q JOIN app.alpha_versions v ON v.id=q.alpha_version_id JOIN app.alphas a ON a.id=v.alpha_id AND a.active_version_id=v.id AND a.lifecycle='QUALIFIED' WHERE v.project_id=$1")
        .bind(f.data.project.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (2, 2, 2));
    let request = inputs::request(&pool, &store, &actor, &f, cycle).await;
    let origin:String=sqlx::query_scalar("SELECT a.origin FROM app.execution_assumptions e JOIN app.artifacts a ON a.id=e.fee_schedule_artifact_id WHERE e.id=$1")
        .bind(f.data.assumptions.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        origin, "SYNTHETIC",
        "declared parameters must not masquerade as market data"
    );
    let metadata_id: uuid::Uuid = sqlx::query_scalar("SELECT e.native_metadata_artifact_id FROM app.input_set_items i JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=i.dataset_revision_id WHERE i.input_set_id=$1")
        .bind(request.input_set_id.as_uuid()).fetch_one(&pool).await.unwrap();
    // Same-size corrupted reads must fail at admission and publication.
    // The original immutable files are never changed.
    let fixture = &f;
    let changed_forward_fees = |id: Id, size: DbCounter| async move {
        let bytes = fixture.read(id, size).await?;
        if id.as_uuid() != metadata_id {
            return Ok(bytes);
        }
        let text = String::from_utf8(bytes).unwrap();
        let from = "\"taker_fee\":\"0.002\"";
        assert!(text.contains(from));
        let changed = text.replace(from, "\"taker_fee\":\"0.003\"").into_bytes();
        assert_eq!(changed.len() as u64, size.get());
        Ok(changed)
    };
    assert!(store
        .start_portfolio_build(
            &actor,
            "changed-forward-fees",
            &request,
            changed_forward_fees,
            |_| async { panic!("wrong Forward fees publish nothing") },
        )
        .await
        .is_err());
    let assumption = store
        .execution_assumption(&actor, f.data.assumptions)
        .await
        .unwrap();
    let costs_id = assumption.fee_schedule_artifact_id;
    let liquidity_id = assumption.bar_liquidity.map(|s| s.report_artifact_id);
    let changed_costs = |id: Id, size: DbCounter| async move {
        let bytes = fixture.read(id, size).await?;
        if id != costs_id {
            return Ok(bytes);
        }
        let text = String::from_utf8(bytes).unwrap();
        let from = "\"random_seed\":\"7\"";
        assert!(text.contains(from));
        let changed = text.replace(from, "\"random_seed\":\"8\"").into_bytes();
        assert_eq!(changed.len() as u64, size.get());
        Ok(changed)
    };
    assert!(matches!(
        store
            .start_portfolio_build(
                &actor,
                "changed-cost-source",
                &request,
                changed_costs,
                |_| async { panic!("corrupt cost source publishes nothing") },
            )
            .await,
        Err(StoreError::Integrity)
    ));
    assert_eq!(liquidity_id.is_some(), with_liquidity);
    let changed_liquidity = |currency: bool| {
        move |id: Id, size: DbCounter| async move {
            let bytes = fixture.read(id, size).await?;
            if Some(id) != liquidity_id {
                return Ok(bytes);
            }
            let text = String::from_utf8(bytes).unwrap();
            let (from, to) = if currency {
                ("\"currency\":\"USD\"", "\"currency\":\"EUR\"")
            } else {
                ("\"notional_value\":\"1000\"", "\"notional_value\":\"1001\"")
            };
            assert!(text.contains(from));
            let changed = text.replace(from, to).into_bytes();
            assert_eq!(changed.len() as u64, size.get());
            Ok(changed)
        }
    };
    if with_liquidity {
        assert!(matches!(
            store
                .start_portfolio_build(
                    &actor,
                    "changed-liquidity-source",
                    &request,
                    changed_liquidity(true),
                    |_| async { panic!("corrupt liquidity publishes nothing") },
                )
                .await,
            Err(StoreError::Integrity)
        ));
    }
    let changed_groups = |replacement: &'static str| {
        move |id: Id, size: DbCounter| async move {
            let bytes = fixture.read(id, size).await?;
            if id.as_uuid() == metadata_id {
                let text = String::from_utf8(bytes).unwrap();
                assert!(text.contains("fixture-group"));
                let changed = text.replace("fixture-group", replacement).into_bytes();
                assert_eq!(changed.len() as u64, size.get());
                Ok(changed)
            } else {
                Ok(bytes)
            }
        }
    };
    if with_liquidity {
        assert!(store
            .start_portfolio_build(
                &actor,
                "changed-group-source",
                &request,
                changed_groups("foreign-group"),
                |_| async { panic!("invalid group source publishes nothing") },
            )
            .await
            .is_err());
    }
    let admitted = store
        .start_portfolio_build(
            &actor,
            "original-qualified-build",
            &request,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    let replay = store
        .start_portfolio_build(
            &actor,
            "original-qualified-build",
            &request,
            |_, _| async { panic!("replay reads no source") },
            |_| async { panic!("replay publishes no object") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, admitted.id);
    let message = validation_publication::message(&pool, admitted.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "original-qualified-build", 60)
        .await
        .unwrap()
    else {
        panic!("original Build admission");
    };
    let job = store.native_job(admitted.id, &lease.fence).await.unwrap();
    assert_eq!(job.spec.job_kind, contracts::runs::RunKind::PortfolioBuild);
    for input in &job.spec.inputs {
        if let RuntimeInputV1::Artifact {
            artifact_id,
            byte_count,
            ..
        } = input
        {
            assert_eq!(
                f.read(*artifact_id, *byte_count).await.unwrap().len() as u64,
                byte_count.get()
            );
        }
    }
    result::complete(&pool, &store, &f, &lease, &job).await;
    if with_liquidity {
        assert!(matches!(
            store
                .publish_scientific_result(
                    admitted.id,
                    |id: Id, size: DbCounter| async move {
                        let bytes = fixture.read(id, size).await?;
                        if id.as_uuid() != metadata_id {
                            return Ok(bytes);
                        }
                        let text = String::from_utf8(bytes).unwrap();
                        let from = "\"price_increment\":\"0.00001\"";
                        assert!(text.contains(from));
                        let changed = text
                            .replace(from, "\"price_increment\":\"0.00002\"")
                            .into_bytes();
                        assert_eq!(changed.len() as u64, size.get());
                        Ok(changed)
                    },
                    |_| async { panic!("changed native tick publishes no Candidate") },
                )
                .await,
            Err(StoreError::Integrity)
        ));
    }
    assert!(matches!(
        store
            .publish_scientific_result(admitted.id, changed_forward_fees, |_| async {
                panic!("wrong Forward fees publish no Candidate")
            },)
            .await,
        Err(StoreError::Integrity)
    ));
    assert!(matches!(
        store
            .publish_scientific_result(admitted.id, changed_costs, |_| async {
                panic!("changed costs publish no Candidate")
            },)
            .await,
        Err(StoreError::Integrity)
    ));
    if with_liquidity {
        for currency in [false, true] {
            assert!(matches!(
                store
                    .publish_scientific_result(
                        admitted.id,
                        changed_liquidity(currency),
                        |_| async { panic!("changed liquidity publishes no Candidate") },
                    )
                    .await,
                Err(StoreError::Integrity)
            ));
        }
    }
    for replacement in if with_liquidity {
        vec!["foreign-group", "             "]
    } else {
        Vec::new()
    } {
        assert!(matches!(
            store
                .publish_scientific_result(admitted.id, changed_groups(replacement), |_| async {
                    panic!("changed group source publishes no Candidate")
                })
                .await,
            Err(StoreError::Integrity)
        ));
    }
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let candidate = validation_publication::publish(&store, &f, admitted.id)
        .await
        .unwrap()
        .resource;
    let facts:(String,String,String,i64,i64)=sqlx::query_as("SELECT c.solver_status,c.evidence_status,a.origin,(SELECT count(*) FROM app.candidate_alphas WHERE candidate_id=c.id),(SELECT count(*) FROM app.candidate_targets WHERE candidate_id=c.id) FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id JOIN app.artifacts a ON a.id=c.target_artifact_id WHERE c.id=$1")
        .bind(candidate.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts,
        ("OPTIMAL".into(), "VALID".into(), "SYNTHETIC".into(), 2, 1)
    );
    let replay = store
        .publish_scientific_result(
            admitted.id,
            |_, _| async { panic!("Candidate replay reads nothing") },
            |_| async { panic!("Candidate replay publishes nothing") },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, candidate);
    store.acknowledge_run(&message).await.unwrap();

    // A later decision consumes the original published target, not an account
    // snapshot and not a copied qualification. It needs its own frozen cutoff.
    use contracts::{
        portfolio::PortfolioBuildWeightsV1,
        research::{DataPartition, InputItemV1, InputPurpose, InputSetCreate},
    };
    let dataset: uuid::Uuid = sqlx::query_scalar(
        "SELECT dataset_revision_id FROM app.input_set_items WHERE input_set_id=$1",
    )
    .bind(request.input_set_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let cutoff = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let input = store
        .create_input_set(
            &actor,
            "last-target-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset.to_string().try_into().unwrap(),
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap()
        .resource
        .header
        .id;
    let mut next = request;
    next.input_set_id = input;
    next.current_weights_source = PortfolioBuildWeightsV1::LastTarget {
        candidate_id: candidate,
    };
    let stale = store
        .start_portfolio_build(
            &actor,
            "stale-last-target",
            &next,
            |id, size| f.read(id, size),
            |_| async { panic!("old catalog cutoff cannot admit a newer target") },
        )
        .await;
    assert!(
        matches!(stale,Err(StoreError::Domain(domain::DomainError::Fields(ref fields))) if fields.iter().any(|issue|issue.field=="portfolio.current_weights")),
        "{stale:?}"
    );
    let dataset = inputs::forward(&pool, &store, &actor, &f).await;
    let cutoff = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    next.input_set_id = store
        .create_input_set(
            &actor,
            "current-last-target-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset,
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap()
        .resource
        .header
        .id;
    let simulate = contracts::portfolio::CandidateSimulationRequestV1 {
        schema_version: SchemaV1,
        cycle_id: cycle,
        candidate_id: candidate,
        input_set_id: next.input_set_id,
        runtime_id: next.runtime_id,
        expected_runtime_revision: next.expected_runtime_revision,
        limits: next.limits.clone(),
    };
    let mut stale_simulation = simulate.clone();
    stale_simulation.input_set_id = input;
    assert!(store
        .start_candidate_simulation(
            &actor,
            "stale-candidate-simulate",
            &stale_simulation,
            |id, size| f.read(id, size),
            |_| async { panic!("old Forward window cannot publish") }
        )
        .await
        .is_err());
    assert!(store
        .start_candidate_simulation(
            &actor,
            "changed-candidate-costs",
            &simulate,
            changed_costs,
            |_| async { panic!("changed original settings cannot publish") }
        )
        .await
        .is_err());
    assert!(matches!(
        store
            .start_candidate_simulation(
                &actor,
                "original-candidate-simulate",
                &simulate,
                |id, size| f.read(id, size),
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.candidate_simulation_tasks WHERE candidate_id=$1",
    )
    .bind(candidate.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        pending, 0,
        "failed parameter publication leaves no simulation binding"
    );
    let simulated = store
        .start_candidate_simulation(
            &actor,
            "original-candidate-simulate",
            &simulate,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(simulated.kind, contracts::runs::RunKind::PortfolioSimulate);
    let replay = store
        .start_candidate_simulation(
            &actor,
            "original-candidate-simulate",
            &simulate,
            |_, _| async { panic!("replay must not reread mutable availability") },
            |_| async { panic!("replay must not republish") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, simulated.id);
    let bindings: Vec<(uuid::Uuid, uuid::Uuid, uuid::Uuid, serde_json::Value)> = sqlx::query_as("SELECT candidate_id,policy_id,dataset_revision_id,request FROM app.candidate_simulation_tasks WHERE run_id=$1")
        .bind(simulated.id.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(bindings.len(), 1, "replay preserves one original binding");
    assert_eq!(bindings[0].0, candidate.as_uuid());
    assert_eq!(bindings[0].2, dataset.as_uuid());
    assert_eq!(bindings[0].3, serde_json::to_value(&simulate).unwrap());
    let original_policy: uuid::Uuid = sqlx::query_scalar("SELECT m.required_evaluation_policy_id FROM app.portfolio_candidates c JOIN app.portfolio_mandates m ON m.id=c.mandate_id WHERE c.id=$1")
        .bind(candidate.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(bindings[0].1, original_policy);
    for statement in [
        "UPDATE app.candidate_simulation_tasks SET request=request WHERE run_id=$1",
        "DELETE FROM app.candidate_simulation_tasks WHERE run_id=$1",
    ] {
        let error = sqlx::query(statement)
            .bind(simulated.id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23000")
        );
    }
    let (parameter,size):(uuid::Uuid,i64)=sqlx::query_as("SELECT t.parameters_artifact_id,a.byte_count FROM app.run_native_tasks t JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE t.run_id=$1")
        .bind(simulated.id.as_uuid()).fetch_one(&pool).await.unwrap();
    let bytes = f
        .read(
            parameter.to_string().try_into().unwrap(),
            DbCounter::new(size as u64).unwrap(),
        )
        .await
        .unwrap();
    let contracts::execution::NativeTaskParametersV1::SimulateCandidate {
        candidate_id,
        candidate_available_ns,
        source_selection,
        request: frozen,
        ..
    } = serde_json::from_slice(&bytes).unwrap()
    else {
        panic!("original Candidate adapter");
    };
    assert_eq!(candidate_id, candidate);
    assert_eq!(frozen.selection.event_start_ns, candidate_available_ns);
    assert!(source_selection.event_start_ns < frozen.selection.event_start_ns);
    assert!(source_selection.event_end_ns >= frozen.selection.event_end_ns);
    assert_eq!(frozen.target_points.len(), 1);
    let evaluations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
            .bind(simulated.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(evaluations, 0, "admission never invents Evaluation");
    let message = store
        .read_run_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == simulated.id)
        .unwrap();
    store
        .cancel_run(
            &actor,
            "cancel-original-candidate-simulation",
            simulated.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: simulated.revision,
            },
        )
        .await
        .unwrap();
    assert!(
        matches!(
            store.acknowledge_run(&message).await,
            Err(StoreError::Conflict)
        ),
        "terminal receipt alone cannot ACK before Candidate Evaluation"
    );
    assert!(matches!(
        store
            .publish_scientific_result(
                simulated.id,
                |_, _| async { panic!("cancelled task has no native output") },
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
        .bind(simulated.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "failed report publication rolls back evaluation");
    let publish = || {
        store.publish_scientific_result(
            simulated.id,
            |_, _| async { panic!("cancelled task has no native output") },
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
    };
    let (left, right) = tokio::join!(publish(), publish());
    let left = left.unwrap().unwrap();
    let right = right.unwrap().unwrap();
    assert_eq!(left.resource, right.resource);
    assert_ne!(
        left.replayed, right.replayed,
        "only one original publication"
    );
    let facts:(uuid::Uuid,String,String,String,i64)=sqlx::query_as("SELECT e.subject_candidate_id,e.evaluation_kind,e.execution_status,e.decision,(SELECT count(*) FROM app.metric_values v WHERE v.evaluation_id=e.id) FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE e.id=$1")
        .bind(left.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts,
        (
            candidate.as_uuid(),
            "FORWARD".into(),
            "CANCELLED".into(),
            "INCONCLUSIVE".into(),
            0
        )
    );
    let replay = store
        .publish_scientific_result(
            simulated.id,
            |_, _| async { panic!("replay cannot reread native objects") },
            |_| async { panic!("replay cannot republish") },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, left.resource);
    store.acknowledge_run(&message).await.unwrap();
    let successful = store
        .start_candidate_simulation(
            &actor,
            "intraday-original-candidate-simulation",
            &simulate,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    let message = validation_publication::message(&pool, successful.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "original-candidate-hold", 60)
        .await
        .unwrap()
    else {
        panic!("native Candidate simulation lease");
    };
    let job = store.native_job(successful.id, &lease.fence).await.unwrap();
    simulation_result::complete(&pool, &store, &f, &lease, &job).await;
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store
            .publish_scientific_result(
                successful.id,
                |id, size| f.read(id, size),
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    assert!(matches!(
        store
            .publish_scientific_result(successful.id, changed_costs, |_| async {
                panic!("changed original settings cannot publish Evaluation")
            })
            .await,
        Err(StoreError::Integrity)
    ));
    let published = std::cell::Cell::new(false);
    let changed_during_publication = store
        .publish_scientific_result(
            successful.id,
            |id, size| {
                let published = &published;
                async move {
                    if published.get() {
                        changed_costs(id, size).await
                    } else {
                        fixture.read(id, size).await
                    }
                }
            },
            |object| {
                let result = f
                    .objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity);
                published.set(true);
                std::future::ready(result)
            },
        )
        .await;
    assert!(matches!(
        changed_during_publication,
        Err(StoreError::Integrity)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
        .bind(successful.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "post-publication source change rolls back Evaluation"
    );
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let evaluated = store
        .publish_scientific_result(
            successful.id,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .unwrap();
    let facts:(String,String,i64,bool)=sqlx::query_as("SELECT e.execution_status,e.decision,(SELECT count(*) FROM app.metric_values v WHERE v.evaluation_id=e.id AND v.status='INSUFFICIENT_DATA' AND v.value IS NULL AND v.reason_code='PORTFOLIO_DAILY_RETURNS_UNAVAILABLE'),e.valid_until IS NOT NULL FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE e.id=$1")
        .bind(evaluated.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, ("SUCCEEDED".into(), "INCONCLUSIVE".into(), 3, true));
    let first = store
        .candidate_evaluations(
            &actor,
            candidate,
            &contracts::control::ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    let second = store
        .candidate_evaluations(
            &actor,
            candidate,
            &contracts::control::ListQuery {
                cursor: first.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.next_cursor.is_none());
    let mut ids = vec![first.items[0].id, second.items[0].id];
    ids.sort();
    let mut expected = vec![left.resource, evaluated.resource];
    expected.sort();
    assert_eq!(
        ids, expected,
        "only original Candidate publications, not Alpha or Sealed"
    );
    for id in ids {
        let detail = store.evaluation(&actor, id).await.unwrap();
        assert_eq!(detail.subject_candidate_id, Some(candidate));
        assert!(detail.subject_alpha_version_id.is_none());
        let metrics = store
            .evaluation_metrics(&actor, id, &Default::default())
            .await
            .unwrap();
        assert_eq!(
            metrics.items.len(),
            if id == evaluated.resource { 3 } else { 0 }
        );
        assert!(metrics.items.iter().all(|metric| metric.evaluation_id == id
            && metric.value.is_none()
            && metric.reason_code.as_deref() == Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE")));
    }
    assert!(matches!(
        store
            .candidate_evaluations(&actor, Id::new(), &Default::default())
            .await,
        Err(StoreError::NotFound)
    ));
    store.acknowledge_run(&message).await.unwrap();
    let snapshot="SELECT (SELECT count(*) FROM app.portfolio_build_tasks),(SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),reserved_cpu_seconds FROM app.research_cycles WHERE id=$1";
    let before: (i64, i64, i64, i64) = sqlx::query_as(snapshot)
        .bind(cycle.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut allocated = Vec::new();
    let failed = store
        .start_portfolio_build(
            &actor,
            "last-target-build",
            &next,
            |id, size| f.read(id, size),
            |object| {
                allocated.push((
                    object.id,
                    DbCounter::new(object.bytes.len() as u64).unwrap(),
                ));
                std::future::ready(if allocated.len() == 2 {
                    Err(StoreError::Integrity)
                } else {
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                })
            },
        )
        .await;
    assert!(matches!(failed, Err(StoreError::Integrity)), "{failed:?}");
    assert_eq!(allocated.len(), 2, "derived weights then native parameters");
    let after: (i64, i64, i64, i64) = sqlx::query_as(snapshot)
        .bind(cycle.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "no artifact rows, Run or CPU charge after the second publication fails"
    );
    for (id, size) in allocated {
        store
            .discard_unpublished_operator_artifact(id, |id| {
                std::future::ready(
                    f.objects
                        .discard_unpublished(id)
                        .map_err(|_| StoreError::Integrity),
                )
            })
            .await
            .unwrap();
        assert!(f.read(id, size).await.is_err());
    }
    let mut published = 0;
    let next_run = store
        .start_portfolio_build(
            &actor,
            "last-target-build",
            &next,
            |id, size| f.read(id, size),
            |object| {
                published += 1;
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(published, 2);
    let message = validation_publication::message(&pool, next_run.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "last-target-build", 60)
        .await
        .unwrap()
    else {
        panic!("original LastTarget admission");
    };
    let job = store.native_job(next_run.id, &lease.fence).await.unwrap();
    result::complete(&pool, &store, &f, &lease, &job).await;
    let next_candidate = validation_publication::publish(&store, &f, next_run.id)
        .await
        .unwrap()
        .resource;
    assert_ne!(next_candidate, candidate);
    let facts:(String,String,String,uuid::Uuid,Option<uuid::Uuid>)=sqlx::query_as("SELECT c.evidence_status,c.current_weights_source,a.origin,t.last_target_candidate_id,t.snapshot_id FROM app.portfolio_candidates c JOIN app.artifacts a ON a.id=c.target_artifact_id JOIN app.portfolio_build_tasks t ON t.run_id=c.run_id WHERE c.id=$1")
        .bind(next_candidate.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts,
        (
            "VALID".into(),
            "LAST_TARGET".into(),
            "SYNTHETIC".into(),
            candidate.as_uuid(),
            None
        )
    );
    store.acknowledge_run(&message).await.unwrap();
}
