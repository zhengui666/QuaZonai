//! Original Store admissions and files with controlled runtime/model declarations.
//! No SQL-authored qualifications, actual model inference or REAL market claim.
use super::*;
use contracts::{experiments::ExperimentProposalV1, research::DataOrigin};
use store::turns::{NativePublicSummary, TurnOutcome, UsageReceipt};

#[path = "equity_curve_checks.rs"]
pub(super) mod equity_curve_checks;

#[path = "approval_checks.rs"]
mod approvals;

#[path = "automatic_rebalance.rs"]
pub(super) mod automatic_rebalance;

#[path = "portfolio_inputs.rs"]
mod inputs;
#[path = "portfolio_result.rs"]
mod result;
#[path = "candidate_simulation_result.rs"]
mod simulation_result;
#[path = "portfolio_study_result.rs"]
mod study_result;

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
    Box::pin(check_chain(pool, cycle_support::Liquidity::None)).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_liquidity_remains_bound_through_original_qualified_chain(pool: PgPool) {
    Box::pin(check_chain(pool, cycle_support::Liquidity::Snapshot)).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn rolling_liquidity_remains_bound_through_original_qualified_chain(pool: PgPool) {
    Box::pin(check_chain(
        pool,
        cycle_support::Liquidity::Rolling(u32::MAX),
    ))
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn rolling_expiry_during_publication_rolls_back_targets_and_retries_invalid(pool: PgPool) {
    Box::pin(check_chain(pool, cycle_support::Liquidity::Rolling(10))).await;
}

async fn check_chain(pool: PgPool, liquidity: cycle_support::Liquidity) {
    // Unwind the large debug-build qualification poll frame before Study;
    // retain the same original DB/files, without increasing the thread stack.
    if let Some((store, actor, f, request, candidate, _directory)) =
        Box::pin(qualified_chain(pool.clone(), liquidity)).await
    {
        Box::pin(study_admission(
            &pool, &store, &actor, &f, &request, candidate, liquidity,
        ))
        .await;
    }
}

pub(super) async fn qualified_chain(
    pool: PgPool,
    liquidity: cycle_support::Liquidity,
) -> Option<(
    Store,
    store::authority::Actor,
    cycle_support::Fixture,
    contracts::portfolio::PortfolioBuildRequestV1,
    Id,
    tempfile::TempDir,
)> {
    Box::pin(qualified_chain_policy(
        pool,
        liquidity,
        contracts::forward::ForwardEnvironmentV1::Paper,
        contracts::research::DataUse::ResearchAndPaper,
        |_| {},
    ))
    .await
}

pub(super) async fn qualified_chain_policy(
    pool: PgPool,
    liquidity: cycle_support::Liquidity,
    environment: contracts::forward::ForwardEnvironmentV1,
    allowed_uses: contracts::research::DataUse,
    customize: fn(&mut contracts::research::EvaluationPolicyCreate),
) -> Option<(
    Store,
    store::authority::Actor,
    cycle_support::Fixture,
    contracts::portfolio::PortfolioBuildRequestV1,
    Id,
    tempfile::TempDir,
)> {
    Box::pin(qualified_chain_scheduled(
        pool,
        liquidity,
        environment,
        allowed_uses,
        customize,
        None,
    ))
    .await
}

pub(super) async fn qualified_chain_scheduled(
    pool: PgPool,
    liquidity: cycle_support::Liquidity,
    environment: contracts::forward::ForwardEnvironmentV1,
    allowed_uses: contracts::research::DataUse,
    customize: fn(&mut contracts::research::EvaluationPolicyCreate),
    interval: Option<u32>,
) -> Option<(
    Store,
    store::authority::Actor,
    cycle_support::Fixture,
    contracts::portfolio::PortfolioBuildRequestV1,
    Id,
    tempfile::TempDir,
)> {
    Box::pin(qualified_chain_calendar(
        pool,
        liquidity,
        environment,
        allowed_uses,
        customize,
        interval,
        None,
    ))
    .await
}

pub(super) async fn qualified_chain_calendar(
    pool: PgPool,
    liquidity: cycle_support::Liquidity,
    environment: contracts::forward::ForwardEnvironmentV1,
    allowed_uses: contracts::research::DataUse,
    customize: fn(&mut contracts::research::EvaluationPolicyCreate),
    interval: Option<u32>,
    calendar: Option<(contracts::science::NativeCalendarSessionsV1, i32)>,
) -> Option<(
    Store,
    store::authority::Actor,
    cycle_support::Fixture,
    contracts::portfolio::PortfolioBuildRequestV1,
    Id,
    tempfile::TempDir,
)> {
    let qualified = Box::pin(qualified_members(
        pool.clone(),
        liquidity,
        allowed_uses,
        customize,
        interval,
        calendar.as_ref(),
    ))
    .await;
    Box::pin(build_qualified_chain(
        pool,
        qualified,
        liquidity,
        environment,
        interval,
        calendar,
    ))
    .await
}

type QualifiedMembers = (
    Store,
    store::authority::Actor,
    cycle_support::Fixture,
    Id,
    tempfile::TempDir,
);

async fn qualified_members(
    pool: PgPool,
    liquidity: cycle_support::Liquidity,
    allowed_uses: contracts::research::DataUse,
    customize: fn(&mut contracts::research::EvaluationPolicyCreate),
    interval: Option<u32>,
    calendar: Option<&(contracts::science::NativeCalendarSessionsV1, i32)>,
) -> QualifiedMembers {
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup_with_policy_plan(
        &pool,
        &store,
        &actor,
        objects,
        (DataOrigin::Real, allowed_uses),
        liquidity,
        (
            |policy| {
                policy.selection.candidate_count = 2;
                policy.maximum_sealed_uses_per_lineage = 2;
                policy.sealed_metric_requirements[0].threshold_low = Some("0.1".parse().unwrap());
                let mut portfolio = policy.metric_requirements[0].clone();
                portfolio.metric_code = "PORTFOLIO_DAILY_RETURN_MEAN".into();
                portfolio.scope = "portfolio".into();
                portfolio.method_allowlist = vec!["nautilus-analysis.ReturnsAverage".into()];
                policy.portfolio_metric_requirements = Some(vec![portfolio]);
                customize(policy);
            },
            |policy| {
                if interval.is_some() || calendar.is_some() {
                    policy.portfolio_study_plan.as_mut().unwrap().manual_cutoffs = None;
                }
            },
            calendar.as_ref().map(|(document, _)| document.clone()),
        ),
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
        assert!(lease.run.deadline_at <= review.run.deadline_at);
        experiment_support::complete_sealed(&pool, &store, &f, *lease).await;
        // Result-before-ACK redelivery must not issue a second qualification.
        let (first, replay) = tokio::join!(
            validation_publication::publish(&store, &f, sealed),
            validation_publication::publish(&store, &f, sealed)
        );
        let (first, replay) = (first.unwrap(), replay.unwrap());
        assert_eq!(first.resource, replay.resource);
        assert_ne!(first.replayed, replay.replayed);
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
    (store, actor, f, cycle, directory)
}

async fn build_qualified_chain(
    pool: PgPool,
    (store, actor, f, cycle, directory): QualifiedMembers,
    liquidity: cycle_support::Liquidity,
    environment: contracts::forward::ForwardEnvironmentV1,
    interval: Option<u32>,
    calendar: Option<(contracts::science::NativeCalendarSessionsV1, i32)>,
) -> Option<(
    Store,
    store::authority::Actor,
    cycle_support::Fixture,
    contracts::portfolio::PortfolioBuildRequestV1,
    Id,
    tempfile::TempDir,
)> {
    let expected_origin = if environment == contracts::forward::ForwardEnvironmentV1::Paper {
        "SYNTHETIC"
    } else {
        "REAL"
    };
    let with_liquidity = liquidity == cycle_support::Liquidity::Snapshot;
    let request = inputs::request(
        &pool,
        &store,
        &actor,
        &f,
        cycle,
        environment,
        (interval, calendar.as_ref()),
    )
    .await;
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
    let liquidity_id = assumption
        .bar_liquidity
        .map(|s| s.report_artifact_id)
        .or(assumption.rolling_liquidity_artifact_id);
    let changed_policy = |id: Id, size: DbCounter| async move {
        let bytes = fixture.read(id, size).await?;
        if Some(id) != assumption.rolling_liquidity_artifact_id {
            return Ok(bytes);
        }
        let text = String::from_utf8(bytes).unwrap();
        let from = "\"participation_limit\":\"1\"";
        assert!(text.contains(from));
        let changed = text
            .replace(from, "\"participation_limit\":\"0\"")
            .into_bytes();
        assert_eq!(changed.len() as u64, size.get());
        Ok(changed)
    };
    if assumption.rolling_liquidity_artifact_id.is_some() {
        assert!(matches!(
            store
                .start_portfolio_build(
                    &actor,
                    "changed-rolling-policy",
                    &request,
                    changed_policy,
                    |_| async { panic!("changed policy admits no Build") },
                )
                .await,
            Err(StoreError::Integrity)
        ));
    }
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
    assert_eq!(
        liquidity_id.is_some(),
        liquidity != cycle_support::Liquidity::None
    );
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
    if assumption.rolling_liquidity_artifact_id.is_some() {
        assert!(matches!(
            store
                .publish_scientific_result(admitted.id, changed_policy, |_| async {
                    panic!("changed policy publishes no Candidate")
                },)
                .await,
            Err(StoreError::Integrity)
        ));
    }
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
    if liquidity == cycle_support::Liquidity::Rolling(10) {
        let written = std::sync::Mutex::new(Vec::new());
        let recorded = &written;
        let publication = store
            .publish_scientific_result(
                admitted.id,
                |id, size| f.read(id, size),
                |object| async move {
                    let first = {
                        let mut ids = recorded.lock().unwrap();
                        ids.push(object.id);
                        ids.len() == 1
                    };
                    if first {
                        let value: serde_json::Value =
                            serde_json::from_slice(&object.bytes).unwrap();
                        assert!(
                            value.get("targets").is_some(),
                            "source must be current before writing targets"
                        );
                    }
                    fixture
                        .objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)?;
                    if first {
                        tokio::time::sleep(std::time::Duration::from_secs(11)).await;
                    }
                    Ok(())
                },
            )
            .await;
        assert!(matches!(publication, Err(StoreError::Conflict)));
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app.portfolio_candidates WHERE run_id=$1")
                .bind(admitted.id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let written = written.into_inner().unwrap();
        assert_eq!(written.len(), 2);
        for id in written {
            assert!(store
                .discard_unpublished_operator_artifact(id, |id| async move {
                    fixture
                        .objects
                        .discard_unpublished(id)
                        .map_err(|_| StoreError::Integrity)
                })
                .await
                .unwrap());
        }
        let candidate = validation_publication::publish(&store, &f, admitted.id)
            .await
            .unwrap()
            .resource;
        let facts: (String, String, bool, bool, i64) = sqlx::query_as("SELECT solver_status,evidence_status,target_artifact_id IS NULL,cash_weight IS NULL,(SELECT count(*) FROM app.candidate_targets WHERE candidate_id=c.id) FROM app.portfolio_candidates c WHERE id=$1")
            .bind(candidate.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(facts, ("OPTIMAL".into(), "INVALID".into(), true, true, 0));
        let replay = store
            .publish_scientific_result(
                admitted.id,
                |_, _| async { panic!("expired replay reads nothing") },
                |_| async { panic!("expired replay writes nothing") },
            )
            .await
            .unwrap()
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.resource, candidate);
        store.acknowledge_run(&message).await.unwrap();
        return None;
    }
    let candidate = validation_publication::publish(&store, &f, admitted.id)
        .await
        .unwrap()
        .resource;
    let facts:(String,String,String,i64,i64)=sqlx::query_as("SELECT c.solver_status,c.evidence_status,a.origin,(SELECT count(*) FROM app.candidate_alphas WHERE candidate_id=c.id),(SELECT count(*) FROM app.candidate_targets WHERE candidate_id=c.id) FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id JOIN app.artifacts a ON a.id=c.target_artifact_id WHERE c.id=$1")
        .bind(candidate.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts,
        (
            "OPTIMAL".into(),
            "VALID".into(),
            expected_origin.into(),
            2,
            1
        )
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
            expected_origin.into(),
            candidate.as_uuid(),
            None
        )
    );
    store.acknowledge_run(&message).await.unwrap();
    Some((store, actor, f, next, candidate, directory))
}

async fn study_admission(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
    liquidity: cycle_support::Liquidity,
) {
    let request = contracts::portfolio::PortfolioStudyRequestV1 {
        schema_version: SchemaV1,
        cycle_id: build.cycle_id,
        candidate_id: candidate,
        runtime_id: build.runtime_id,
        expected_runtime_revision: build.expected_runtime_revision,
        limits: build.limits.clone(),
    };
    for field in [
        "input_set_id",
        "evaluation_start",
        "manual_cutoffs",
        "targets",
        "settings",
    ] {
        let mut value = serde_json::to_value(&request).unwrap();
        value[field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<contracts::portfolio::PortfolioStudyRequestV1>(value).is_err()
        );
    }
    let counts = "SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.portfolio_study_tasks),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs)";
    let before: (i64, i64, i64, i64) = sqlx::query_as(counts).fetch_one(pool).await.unwrap();
    let shortened = Box::pin(store.start_portfolio_study(
        actor,
        "study-shortened-window",
        &request,
        |id, size| async move {
            let bytes = f.read(id, size).await?;
            let Ok(mut metadata) =
                serde_json::from_slice::<contracts::catalogs::RuntimeCatalogMetadataV1>(&bytes)
            else {
                return Ok(bytes);
            };
            if !metadata
                .native_snapshot_ref
                .starts_with("controlled-study/")
            {
                return Ok(bytes);
            }
            let end = &mut metadata.quality.datasets[0].selection.event_end_ns;
            *end = DbCounter::new(end.get() - 30_000_000_000).unwrap();
            let bytes = serde_json::to_vec(&metadata).unwrap();
            assert_eq!(
                bytes.len() as u64,
                size.get(),
                "only the attested selection changed"
            );
            Ok(bytes)
        },
        |_| async { panic!("shortened report cannot publish Study") },
    ))
    .await
    .unwrap_err();
    assert!(
        format!("{shortened:?}").contains("portfolio_study_source_window"),
        "{shortened:?}"
    );
    assert_eq!(
        sqlx::query_as::<_, (i64, i64, i64, i64)>(counts)
            .fetch_one(pool)
            .await
            .unwrap(),
        before
    );
    if liquidity == cycle_support::Liquidity::Snapshot {
        let error = Box::pin(store.start_portfolio_study(
            actor,
            "snapshot-is-not-rolling-study",
            &request,
            |id, size| f.read(id, size),
            |_| async {
                panic!("snapshot cannot become rolling Study");
            },
        ))
        .await
        .unwrap_err();
        assert!(
            format!("{error:?}").contains("portfolio_study.liquidity_policy"),
            "{error:?}"
        );
        assert_eq!(
            sqlx::query_as::<_, (i64, i64, i64, i64)>(counts)
                .fetch_one(pool)
                .await
                .unwrap(),
            before
        );
        return;
    }
    assert!(matches!(
        Box::pin(store.start_portfolio_study(
            actor,
            "study-original",
            &request,
            |id, size| f.read(id, size),
            |_| async { Err(StoreError::Integrity) },
        ))
        .await,
        Err(StoreError::Integrity)
    ));
    assert_eq!(
        sqlx::query_as::<_, (i64, i64, i64, i64)>(counts)
            .fetch_one(pool)
            .await
            .unwrap(),
        before
    );
    let changed_source = std::sync::atomic::AtomicBool::new(false);
    let changed_source = &changed_source;
    let written = std::sync::Mutex::new(None);
    let failure = Box::pin(store.start_portfolio_study(
        actor,
        "study-original",
        &request,
        |id, size| async move {
            if changed_source.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(StoreError::Integrity);
            }
            f.read(id, size).await
        },
        |object| {
            f.objects.put(object.id, &object.bytes).unwrap();
            *written.lock().unwrap() = Some(object.id);
            changed_source.store(true, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(()))
        },
    ))
    .await;
    assert!(matches!(failure, Err(StoreError::Integrity)));
    assert_eq!(
        sqlx::query_as::<_, (i64, i64, i64, i64)>(counts)
            .fetch_one(pool)
            .await
            .unwrap(),
        before
    );
    let written = written.into_inner().unwrap().unwrap();
    assert!(store
        .discard_unpublished_operator_artifact(written, |id| std::future::ready(
            f.objects
                .discard_unpublished(id)
                .map_err(|_| StoreError::Integrity)
        ))
        .await
        .unwrap());
    let first = Box::pin(store.start_portfolio_study(
        actor,
        "study-original",
        &request,
        |id, size| f.read(id, size),
        |object| {
            std::future::ready(
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity),
            )
        },
    ));
    let second = Box::pin(store.start_portfolio_study(
        actor,
        "study-original",
        &request,
        |id, size| f.read(id, size),
        |object| {
            std::future::ready(
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity),
            )
        },
    ));
    let (first, second) = tokio::join!(first, second);
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_ne!(first.replayed, second.replayed);
    assert_eq!(first.resource.id, second.resource.id);
    let run = first.resource;
    let replay = Box::pin(store.start_portfolio_study(
        actor,
        "study-original",
        &request,
        |_, _| async { panic!("exact receipt reads no new sources") },
        |_| async { panic!("exact receipt writes nothing") },
    ))
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, run.id);
    let mut changed = request.clone();
    changed.limits.cpu_seconds = DbCounter::new(changed.limits.cpu_seconds.get() + 1).unwrap();
    assert!(matches!(
        Box::pin(store.start_portfolio_study(
            actor,
            "study-original",
            &changed,
            |_, _| async { panic!("changed intent reads nothing") },
            |_| async { panic!("changed intent writes nothing") },
        ))
        .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let (parameter,size,bindings,policy): (uuid::Uuid,i64,serde_json::Value,uuid::Uuid) = sqlx::query_as("SELECT t.parameters_artifact_id,a.byte_count,t.input_bindings,s.policy_id FROM app.portfolio_study_tasks s JOIN app.run_native_tasks t ON t.run_id=s.run_id JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE s.run_id=$1")
        .bind(run.id.as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(policy, f.brief.content.evaluation_policy_id.as_uuid());
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &f.read(
            parameter.to_string().try_into().unwrap(),
            DbCounter::new(size as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::StudyPortfolio {
        request: frozen, ..
    } = task
    else {
        panic!("original Study task");
    };
    assert_eq!(frozen.members.len(), 2);
    assert_eq!(frozen.manual_cutoffs_ns.as_ref().unwrap().len(), 2);
    assert!(frozen.research_available_through_ns < frozen.evaluation_start_ns);
    let sealed_available: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT event_end - interval '59 seconds' FROM app.dataset_revisions WHERE id=$1",
    )
    .bind(f.data.sealed.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        frozen.research_available_through_ns.get(),
        sealed_available.timestamp_nanos_opt().unwrap() as u64
    );
    assert!(frozen
        .assets
        .iter()
        .all(|a| a.current_weight == "0".parse().unwrap() && a.available_notional.is_none()));
    assert_eq!(
        frozen.rolling_liquidity.is_some(),
        matches!(liquidity, cycle_support::Liquidity::Rolling(_))
    );
    let inputs: Vec<RuntimeInputV1> = serde_json::from_value(bindings).unwrap();
    assert_eq!(
        inputs
            .iter()
            .filter(|i| matches!(i, RuntimeInputV1::Dataset { .. }))
            .count(),
        1
    );
    assert!(inputs.iter().all(|i| !matches!(
        i,
        RuntimeInputV1::Dataset {
            role: contracts::research::DataPartition::Sealed
                | contracts::research::DataPartition::Forward,
            ..
        }
    )));
    let message = validation_publication::message(pool, run.id).await;
    store
        .cancel_run(
            actor,
            "cancel-study",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    assert!(
        matches!(
            store.acknowledge_run(&message).await,
            Err(StoreError::Conflict)
        ),
        "terminal Study cannot ACK without independent PORTFOLIO publication"
    );
    assert!(matches!(
        store
            .publish_scientific_result(
                run.id,
                |_, _| async { panic!("cancelled Study has no native output") },
                |_| async { Err(StoreError::Integrity) },
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let evaluations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        evaluations, 0,
        "failed publication cannot seal an Evaluation"
    );
    let cancelled = validation_publication::publish(store, f, run.id)
        .await
        .unwrap()
        .resource;
    let detail = store.evaluation(actor, cancelled).await.unwrap();
    assert_eq!(
        detail.evaluation_kind,
        contracts::evidence::EvaluationKind::Portfolio
    );
    assert_eq!(detail.decision, contracts::evidence::Decision::Inconclusive);
    assert!(store
        .evaluation_metrics(actor, cancelled, &Default::default())
        .await
        .unwrap()
        .items
        .is_empty());
    store.acknowledge_run(&message).await.unwrap();
    let replay = store
        .publish_scientific_result(
            run.id,
            |_, _| async { panic!("receipt replay reads nothing") },
            |_| async { panic!("receipt replay writes nothing") },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, cancelled);

    for infeasible in [false, true] {
        let run = Box::pin(store.start_portfolio_study(
            actor,
            &format!("study-result-{infeasible}"),
            &request,
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
        let message = validation_publication::message(pool, run.id).await;
        let Some(ClaimResult::Leased(lease)) = store
            .claim_native_run(&message, "study-publication", 60)
            .await
            .unwrap()
        else {
            panic!("native Study admission");
        };
        let job = store.native_job(run.id, &lease.fence).await.unwrap();
        Box::pin(study_result::complete(
            pool, store, f, &lease, &job, infeasible, false,
        ))
        .await;
        assert!(matches!(
            store.acknowledge_run(&message).await,
            Err(StoreError::Conflict)
        ));
        let history: uuid::Uuid=sqlx::query_scalar("SELECT id FROM app.artifacts WHERE producer_run_id=$1 AND schema_name='qz.portfolio_history'").bind(run.id.as_uuid()).fetch_one(pool).await.unwrap();
        let history: Id = history.to_string().try_into().unwrap();
        let corrupted = store
            .publish_scientific_result(
                run.id,
                |id, size| async move {
                    let mut bytes = f.read(id, size).await?;
                    if id == history {
                        *bytes.last_mut().unwrap() ^= 1;
                    }
                    Ok(bytes)
                },
                |_| async { panic!("corrupt Arrow must not publish") },
            )
            .await;
        assert!(
            matches!(corrupted, Err(StoreError::Integrity)),
            "{corrupted:?}"
        );
        assert!(matches!(
            store
                .publish_scientific_result(
                    run.id,
                    |id, size| f.read(id, size),
                    |_| async { Err(StoreError::Integrity) }
                )
                .await,
            Err(StoreError::Integrity)
        ));
        if infeasible {
            let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
                .fetch_one(pool)
                .await
                .unwrap();
            let revoked_at = now + chrono::Duration::seconds(3);
            store
                .revoke_data_grant(
                    actor,
                    "study-future-revocation",
                    f.data.grant,
                    &contracts::data::DataGrantRevoke {
                        schema_version: SchemaV1,
                        effective_at: Some(revoked_at),
                        reason_code: "CONTROLLED_WITHDRAWAL".into(),
                        reason: "Controlled test grant expires during publication".into(),
                    },
                )
                .await
                .unwrap();
            let written = std::sync::Mutex::new(None);
            let recorded = &written;
            let failed = store
                .publish_scientific_result(
                    run.id,
                    |id, size| f.read(id, size),
                    |object| async move {
                        let document: serde_json::Value =
                            serde_json::from_slice(&object.bytes).unwrap();
                        let until: chrono::DateTime<chrono::Utc> =
                            serde_json::from_value(document["valid_until"].clone()).unwrap();
                        assert!(
                            until <= revoked_at,
                            "known revocation caps the first publication window"
                        );
                        f.objects.put(object.id, &object.bytes).unwrap();
                        *recorded.lock().unwrap() = Some(object.id);
                        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                        Ok(())
                    },
                )
                .await;
            assert!(matches!(failed, Err(StoreError::Conflict)), "{failed:?}");
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
                    .bind(run.id.as_uuid())
                    .fetch_one(pool)
                    .await
                    .unwrap();
            assert_eq!(count, 0, "expiry after write rolls back the Evaluation");
            let object = written.into_inner().unwrap().unwrap();
            assert!(store
                .discard_unpublished_operator_artifact(object, |id| std::future::ready(
                    f.objects
                        .discard_unpublished(id)
                        .map_err(|_| StoreError::Integrity)
                ))
                .await
                .unwrap());
        }
        let left = Box::pin(validation_publication::publish(store, f, run.id));
        let right = Box::pin(validation_publication::publish(store, f, run.id));
        let (left, right) = tokio::join!(left, right);
        let (left, right) = (left.unwrap(), right.unwrap());
        assert_ne!(left.replayed, right.replayed);
        assert_eq!(left.resource, right.resource);
        let facts:(String,String,String,i64,bool)=sqlx::query_as("SELECT e.evaluation_kind,e.execution_status,e.decision,(SELECT count(*) FROM app.metric_values v WHERE v.evaluation_id=e.id),coalesce(e.valid_until>clock_timestamp(),false) FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE e.id=$1").bind(left.resource.as_uuid()).fetch_one(pool).await.unwrap();
        assert_eq!(
            facts,
            (
                "PORTFOLIO".into(),
                "SUCCEEDED".into(),
                "INCONCLUSIVE".into(),
                if infeasible { 0 } else { 3 },
                !infeasible
            )
        );
        let view = store.evaluation(actor, left.resource).await.unwrap();
        Box::pin(equity_curve_checks::check_projection(
            store,
            actor,
            f,
            left.resource,
            candidate,
            infeasible,
        ))
        .await;
        let releases: i64 = sqlx::query_scalar("SELECT count(*) FROM app.releases")
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(matches!(
            Box::pin(store.create_release(
                actor,
                &format!("reject-inconclusive-release-{infeasible}"),
                &contracts::delivery::ReleaseCreateV1 {
                    schema_version: SchemaV1,
                    candidate_id: candidate,
                    evaluation_id: left.resource
                },
                |id, size| f.read(id, size),
                |_| async { panic!("INCONCLUSIVE cannot publish a Package") },
            ))
            .await,
            Err(StoreError::Invalid("release_portfolio_evaluation"))
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.releases")
                .fetch_one(pool)
                .await
                .unwrap(),
            releases
        );
        assert_eq!(view.subject_candidate_id, Some(candidate));
        assert_eq!(
            view.evaluation_kind,
            contracts::evidence::EvaluationKind::Portfolio
        );
        let report = f
            .read(view.report_artifact_id, {
                let size: i64 =
                    sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
                        .bind(view.report_artifact_id.as_uuid())
                        .fetch_one(pool)
                        .await
                        .unwrap();
                DbCounter::new(size as u64).unwrap()
            })
            .await
            .unwrap();
        let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
        assert_eq!(report["mode"], "STUDY");
        assert_eq!(report["native_reports"].as_array().unwrap().len(), 3);
        if infeasible {
            assert!(report["valid_until"].is_null());
        }
        assert!(store
            .candidate_evaluations(actor, candidate, &Default::default())
            .await
            .unwrap()
            .items
            .iter()
            .any(|v| v.id == left.resource));
        store.acknowledge_run(&message).await.unwrap();
        let replay = store
            .publish_scientific_result(
                run.id,
                |_, _| async { panic!("published Study replay reads no expired source") },
                |_| async { panic!("published Study replay writes nothing") },
            )
            .await
            .unwrap()
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.resource, left.resource);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_freezes_original_package_and_replays_without_republishing(pool: PgPool) {
    Box::pin(release_scenario(
        pool,
        contracts::forward::ForwardEnvironmentV1::Live,
    ))
    .await;
}

// T33 transaction evidence only: upstream model responses remain controlled.
#[sqlx::test(migrations = "../../migrations")]
async fn rebalance_new_cutoff_requires_new_evaluation_and_preserves_original_package(pool: PgPool) {
    Box::pin(rebalance_release_check(pool)).await;
}

async fn rebalance_release_check(pool: PgPool) {
    let (store, actor, f, build, first, _directory) = Box::pin(qualified_chain_policy(
        pool.clone(),
        cycle_support::Liquidity::None,
        contracts::forward::ForwardEnvironmentV1::Live,
        contracts::research::DataUse::ResearchAndPaper,
        release_policy,
    ))
    .await
    .unwrap();
    Box::pin(rebalance_packages(&pool, &store, &actor, &f, &build, first)).await;
}

async fn rebalance_packages(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    first: Id,
) {
    let second: uuid::Uuid = sqlx::query_scalar("SELECT c.id FROM app.portfolio_candidates c JOIN app.portfolio_build_tasks t ON t.run_id=c.run_id WHERE t.last_target_candidate_id=$1")
        .bind(first.as_uuid()).fetch_one(pool).await.unwrap();
    let second = second.to_string().try_into().unwrap();
    let prior = store.candidate(actor, first).await.unwrap();
    let next = store.candidate(actor, second).await.unwrap();
    assert_ne!(first, second);
    assert!(next.header.decision_asof > prior.header.decision_asof);
    assert_ne!(next.header.input_set_id, prior.header.input_set_id);
    assert_ne!(
        next.header.target_artifact_id,
        prior.header.target_artifact_id
    );
    assert_eq!(next.header.mandate_id, prior.header.mandate_id);
    assert_eq!(
        next.header.current_weights_source,
        contracts::portfolio::CandidateWeightsSourceV1::LastTarget
    );
    let cohort = |candidate: &contracts::portfolio::CandidateDetailV1| {
        candidate
            .members
            .iter()
            .map(|member| {
                (
                    member.alpha_version_id,
                    member.qualification_id,
                    member.ensemble_weight.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(cohort(&prior), cohort(&next));
    let original = Box::pin(original_release_intent(pool, store, actor, f, build, first)).await;
    let release = Box::pin(store.create_release(
        actor,
        "rebalance-first-release",
        &original,
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
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(release.package_artifact_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    let size = DbCounter::new(size as u64).unwrap();
    let original_bytes = f.read(release.package_artifact_id, size).await.unwrap();
    let wrong = contracts::delivery::ReleaseCreateV1 {
        candidate_id: second,
        ..original.clone()
    };
    assert!(matches!(
        Box::pin(store.create_release(
            actor,
            "rebalance-old-evaluation",
            &wrong,
            |id, size| f.read(id, size),
            |_| async { panic!("old Candidate evaluation cannot publish a new package") },
        ))
        .await,
        Err(StoreError::Invalid("release_portfolio_evaluation"))
    ));
    let missions: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE kind='MISSION'")
        .fetch_one(pool)
        .await
        .unwrap();
    let independent = Box::pin(original_release_intent(
        pool, store, actor, f, build, second,
    ))
    .await;
    assert_ne!(independent.evaluation_id, original.evaluation_id);
    let newer = Box::pin(store.create_release(
        actor,
        "rebalance-next-release",
        &independent,
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
    assert_ne!(newer.id, release.id);
    assert_ne!(newer.package_artifact_id, release.package_artifact_id);
    assert_eq!(newer.candidate_id, second);
    assert!(newer.asof > release.asof);
    assert_eq!(
        f.read(release.package_artifact_id, size).await.unwrap(),
        original_bytes
    );
    assert_eq!(
        serde_json::to_value(store.release(actor, release.id).await.unwrap()).unwrap(),
        serde_json::to_value(&release).unwrap()
    );
    assert_eq!(
        serde_json::to_value(store.candidate(actor, first).await.unwrap()).unwrap(),
        serde_json::to_value(&prior).unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.runs WHERE kind='MISSION'")
            .fetch_one(pool)
            .await
            .unwrap(),
        missions
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.approvals")
            .fetch_one(pool)
            .await
            .unwrap(),
        0,
        "a new Release does not inherit delivery authority"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_offers")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn synthetic_candidate_cannot_be_upgraded_to_real_release(pool: PgPool) {
    Box::pin(release_scenario(
        pool,
        contracts::forward::ForwardEnvironmentV1::Paper,
    ))
    .await;
}

// Small, explicitly registered protocol fixture; never production evidence thresholds.
pub(super) fn release_policy(policy: &mut contracts::research::EvaluationPolicyCreate) {
    policy.minimum_observations = 1;
    let criterion = &mut policy.portfolio_metric_requirements.as_mut().unwrap()[0];
    criterion.minimum_observations = DbCounter::new(1).unwrap();
    criterion.threshold_low = Some("0".parse().unwrap());
}

async fn release_scenario(pool: PgPool, environment: contracts::forward::ForwardEnvironmentV1) {
    // Controlled protocol evidence tests the transaction, not real-market acceptance.
    let (store, actor, f, build, candidate, _directory) = Box::pin(qualified_chain_policy(
        pool.clone(),
        cycle_support::Liquidity::None,
        environment,
        contracts::research::DataUse::ResearchAndPaper,
        release_policy,
    ))
    .await
    .unwrap();
    Box::pin(release_check(&pool, &store, &actor, &f, &build, candidate)).await;
}

pub(super) async fn original_release_intent(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
) -> contracts::delivery::ReleaseCreateV1 {
    let request = contracts::portfolio::PortfolioStudyRequestV1 {
        schema_version: SchemaV1,
        candidate_id: candidate,
        cycle_id: build.cycle_id,
        runtime_id: build.runtime_id,
        expected_runtime_revision: build.expected_runtime_revision,
        limits: build.limits.clone(),
    };
    let run = Box::pin(store.start_portfolio_study(
        actor,
        &format!("release-study-{candidate}"),
        &request,
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
    let message = validation_publication::message(pool, run.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, &format!("release-study-{candidate}"), 60)
        .await
        .unwrap()
    else {
        panic!("Study lease")
    };
    let job = store.native_job(run.id, &lease.fence).await.unwrap();
    Box::pin(study_result::complete(
        pool, store, f, &lease, &job, false, true,
    ))
    .await;
    let evaluation = validation_publication::publish(store, f, run.id)
        .await
        .unwrap()
        .resource;
    assert_eq!(
        store.evaluation(actor, evaluation).await.unwrap().decision,
        contracts::evidence::Decision::Pass
    );
    contracts::delivery::ReleaseCreateV1 {
        schema_version: SchemaV1,
        candidate_id: candidate,
        evaluation_id: evaluation,
    }
}

pub(super) async fn original_releases(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
) -> Option<(
    contracts::delivery::ReleaseViewV1,
    contracts::delivery::ReleaseViewV1,
    contracts::delivery::ReleaseCreateV1,
)> {
    let intent = Box::pin(original_release_intent(
        pool, store, actor, f, build, candidate,
    ))
    .await;
    if store
        .candidate(actor, candidate)
        .await
        .unwrap()
        .header
        .origin
        == DataOrigin::Synthetic
    {
        let rejected = Box::pin(store.create_release(
            actor,
            "synthetic-release",
            &intent,
            |id, size| f.read(id, size),
            |_| async { panic!("synthetic Candidate never publishes a REAL Package") },
        ))
        .await;
        assert!(
            matches!(rejected, Err(StoreError::Invalid("release_real_candidate"))),
            "{rejected:?}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.releases")
                .fetch_one(pool)
                .await
                .unwrap(),
            0
        );
        return None;
    }
    let failed = Box::pin(store.create_release(
        actor,
        "release-failed",
        &intent,
        |id, size| f.read(id, size),
        |_| async { Err(StoreError::Integrity) },
    ))
    .await;
    assert!(matches!(failed, Err(StoreError::Integrity)), "{failed:?}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.releases")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    let publish = |object: store::lifecycle::native::NativeObjectPublication| {
        std::future::ready(
            f.objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity),
        )
    };
    let (left, right) = tokio::join!(
        Box::pin(store.create_release(
            actor,
            "release-original",
            &intent,
            |id, size| f.read(id, size),
            publish
        )),
        Box::pin(store.create_release(
            actor,
            "release-original",
            &intent,
            |id, size| f.read(id, size),
            publish
        ))
    );
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_ne!(left.replayed, right.replayed);
    assert_eq!(left.resource.id, right.resource.id);
    let view = store.release(actor, left.resource.id).await.unwrap();
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        serde_json::to_value(&left.resource).unwrap()
    );
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(view.package_artifact_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    let package: contracts::delivery::TargetPackageV1 = serde_json::from_slice(
        &f.read(
            view.package_artifact_id,
            DbCounter::new(size as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(package.release_id, view.id);
    assert_eq!(package.evaluation_refs, vec![intent.evaluation_id]);
    assert_eq!(package.valid_until, view.valid_until);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.releases")
            .fetch_one(pool)
            .await
            .unwrap(),
        1
    );
    let replay = Box::pin(store.create_release(
        actor,
        "release-original",
        &intent,
        |_, _| async { panic!("replay reads no file") },
        |_| async { panic!("replay publishes no file") },
    ))
    .await
    .unwrap();
    assert!(replay.replayed);
    let changed = contracts::delivery::ReleaseCreateV1 {
        evaluation_id: Id::new(),
        ..intent.clone()
    };
    assert!(matches!(
        Box::pin(store.create_release(
            actor,
            "release-original",
            &changed,
            |_, _| async { panic!("changed intent") },
            |_| async { panic!("changed intent") }
        ))
        .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(!store
        .discard_unpublished_operator_artifact(view.package_artifact_id, |_| async {
            panic!("referenced Package cannot be discarded")
        })
        .await
        .unwrap());
    let sibling = Box::pin(store.create_release(
        actor,
        "release-sibling",
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
    let first = store
        .releases(
            actor,
            view.project_id,
            &contracts::control::ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].id, sibling.id);
    assert_eq!(first.next_cursor, Some(sibling.id));
    let last = store
        .releases(
            actor,
            view.project_id,
            &contracts::control::ListQuery {
                cursor: first.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].id, view.id);
    assert!(last.next_cursor.is_none());
    assert!(matches!(
        store.releases(actor, Id::new(), &Default::default()).await,
        Err(StoreError::NotFound)
    ));
    Some((view, sibling, intent))
}

async fn release_check(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
) {
    let Some((view, sibling, intent)) =
        Box::pin(original_releases(pool, store, actor, f, build, candidate)).await
    else {
        return;
    };
    release_decision_checks(pool, store, actor, &view, &sibling).await;
    Box::pin(approvals::check(pool, store, actor, f, &view, &sibling)).await;
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let deadline = now + chrono::Duration::seconds(3);
    store
        .revoke_data_grant(
            actor,
            "release-future-revocation",
            f.data.grant,
            &contracts::data::DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: Some(deadline),
                reason_code: "CONTROLLED_WITHDRAWAL".into(),
                reason: "Controlled Release publication expiry".into(),
            },
        )
        .await
        .unwrap();
    let written = std::sync::Mutex::new(None);
    let recorded = &written;
    let expired = Box::pin(store.create_release(
        actor,
        "release-expires-during-write",
        &intent,
        |id, size| f.read(id, size),
        |object| async move {
            let package: contracts::delivery::TargetPackageV1 =
                serde_json::from_slice(&object.bytes).unwrap();
            assert!(package.valid_until <= deadline);
            f.objects.put(object.id, &object.bytes).unwrap();
            *recorded.lock().unwrap() = Some(object.id);
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            Ok(())
        },
    ))
    .await;
    assert!(
        matches!(
            expired,
            Err(StoreError::Invalid(_) | StoreError::Conflict | StoreError::Domain(_))
        ),
        "{expired:?}"
    );
    let orphan = written.into_inner().unwrap().unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.releases")
            .fetch_one(pool)
            .await
            .unwrap(),
        2
    );
    assert!(store
        .discard_unpublished_operator_artifact(orphan, |id| std::future::ready(
            f.objects
                .discard_unpublished(id)
                .map_err(|_| StoreError::Integrity)
        ))
        .await
        .unwrap());
    let replay = Box::pin(store.create_release(
        actor,
        "release-original",
        &intent,
        |_, _| async { panic!("expired replay reads no source") },
        |_| async { panic!("expired replay writes no source") },
    ))
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.valid_until, view.valid_until);
}

async fn release_decision_checks(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    release: &contracts::delivery::ReleaseViewV1,
    sibling: &contracts::delivery::ReleaseViewV1,
) {
    use contracts::{control::ListQuery, delivery::*, forward::ForwardEnvironmentV1};
    let downstream: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM app.downstream_integrations WHERE name='Controlled weights source'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let downstream = downstream.to_string().try_into().unwrap();
    let request = ReleaseRejectV1 {
        schema_version: SchemaV1,
        downstream_id: downstream,
        environment: ForwardEnvironmentV1::Live,
        expected_latest_decision_id: None,
        reason_code: "OPERATOR_DECLINED".into(),
        reason: "Controlled original rejection".into(),
    };
    let (first, replay) = tokio::join!(
        store.reject_release(actor, "reject-original", release.id, &request),
        store.reject_release(actor, "reject-original", release.id, &request)
    );
    let (first, replay) = (first.unwrap(), replay.unwrap());
    assert_ne!(first.replayed, replay.replayed);
    assert_eq!(first.resource.id, replay.resource.id);
    assert_eq!(first.resource.ordinal, 1);
    assert!(matches!(
        store
            .reject_release(actor, "reject-stale", sibling.id, &request)
            .await,
        Err(StoreError::Conflict)
    ));
    let paper = ReleaseRejectV1 {
        environment: ForwardEnvironmentV1::Paper,
        ..request.clone()
    };
    assert_eq!(
        store
            .reject_release(actor, "reject-paper", release.id, &paper)
            .await
            .unwrap()
            .resource
            .ordinal,
        1
    );
    let reopen = ReleaseReopenV1 {
        schema_version: SchemaV1,
        expected_latest_decision_id: first.resource.id,
        reason_code: "OPERATOR_RECONSIDERED".into(),
        reason: "Controlled reconsideration, not approval".into(),
    };
    let next = store
        .reopen_release(actor, "reopen-original", first.resource.id, &reopen)
        .await
        .unwrap()
        .resource;
    assert_eq!(next.decision, ReleaseDecisionV1::Reopen);
    assert_eq!(next.ordinal, 2);
    assert_eq!(next.supersedes_decision_id, Some(first.resource.id));
    assert!(
        store
            .reopen_release(actor, "reopen-original", first.resource.id, &reopen)
            .await
            .unwrap()
            .replayed
    );
    assert!(matches!(
        store
            .reopen_release(actor, "reopen-stale", first.resource.id, &reopen)
            .await,
        Err(StoreError::Conflict)
    ));
    let current = ReleaseRejectV1 {
        expected_latest_decision_id: Some(next.id),
        ..request
    };
    let (left, right) = tokio::join!(
        store.reject_release(actor, "race-a", release.id, &current),
        store.reject_release(actor, "race-b", sibling.id, &current)
    );
    let winner = match (left, right) {
        (Ok(a), Err(StoreError::Conflict)) | (Err(StoreError::Conflict), Ok(a)) => a.resource,
        other => panic!("one Candidate-wide winner: {other:?}"),
    };
    assert_eq!(winner.ordinal, 3);
    let page = store
        .release_decisions(
            actor,
            release.id,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].id, winner.id);
    assert!(page.next_cursor.is_some());
    let older = store
        .release_decisions(
            actor,
            sibling.id,
            &ListQuery {
                cursor: page.next_cursor,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(older.items.len(), 3);
    assert!(older
        .items
        .iter()
        .all(|r| r.candidate_id == release.candidate_id));
    assert!(
        sqlx::query("UPDATE app.release_decisions SET reason='overwrite' WHERE id=$1")
            .bind(first.resource.id.as_uuid())
            .execute(pool)
            .await
            .is_err()
    );
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
}
