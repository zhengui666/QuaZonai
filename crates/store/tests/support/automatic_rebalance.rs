//! Original commands and real PG/PGMQ; upstream numerical declarations are controlled.
use super::*;
use contracts::{delivery::*, forward::ForwardEnvironmentV1, research::*};

#[sqlx::test(migrations = "../../migrations")]
async fn frozen_policy_rebalance_queues_one_original_bounded_build(pool: PgPool) {
    Box::pin(scenario(pool)).await;
}

async fn scenario(pool: PgPool) {
    let (store, actor, f, build, candidate, _directory) = Box::pin(qualified_chain_scheduled(
        pool.clone(),
        cycle_support::Liquidity::None,
        ForwardEnvironmentV1::Live,
        DataUse::ResearchAndPaper,
        release_policy,
        Some(5),
    ))
    .await
    .unwrap();
    Box::pin(check(&pool, &store, &actor, &f, &build, candidate)).await;
}

async fn check(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
) {
    let intent = Box::pin(original_release_intent(
        pool, store, actor, f, build, candidate,
    ))
    .await;
    let release = Box::pin(store.create_release(
        actor,
        "automatic-seed-release",
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
    let downstream: uuid::Uuid = sqlx::query_scalar("SELECT w.downstream_id FROM app.portfolio_candidates c JOIN app.portfolio_build_tasks t ON t.run_id=c.run_id JOIN app.forward_weight_snapshots w ON w.id=t.snapshot_id WHERE c.id=$1")
        .bind(candidate.as_uuid()).fetch_one(pool).await.unwrap();
    let metric = contracts::evidence::MetricRequirementV1 {
        schema_version: SchemaV1,
        metric_code: "PORTFOLIO_DAILY_RETURN_MEAN".into(),
        scope: "portfolio".into(),
        comparator: contracts::evidence::Comparator::Ge,
        threshold_low: Some("0".parse().unwrap()),
        threshold_high: None,
        minimum_observations: DbCounter::new(1).unwrap(),
        required: true,
        method_allowlist: vec!["nautilus-analysis.ReturnsAverage".into()],
    };
    let policy = store
        .authorize_automation(
            actor,
            "original-rebalance-policy",
            release.project_id,
            &AutomationAuthorizeV1 {
                schema_version: SchemaV1,
                expected_project_revision: store
                    .project(actor, release.project_id)
                    .await
                    .unwrap()
                    .revision,
                content: AutomationPolicyContentV1 {
                    mode: AutomationModeV1::AutoPaper,
                    mandate_id: release.mandate_id,
                    downstream_id: downstream.to_string().try_into().unwrap(),
                    required_paper_observations: 1,
                    minimum_paper_elapsed_seconds: DbCounter::new(1).unwrap(),
                    max_feedback_age_seconds: DbCounter::new(60).unwrap(),
                    promotion_metric_requirements: vec![metric.clone()],
                    degradation_metric_requirements: vec![metric],
                    valid_until: release.valid_until,
                    enabled_for_new_rebalances: true,
                    max_rebalances_per_day: 2,
                },
            },
        )
        .await
        .unwrap()
        .resource;
    // Let the real clock cross the original five-second scheduling interval.
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    let old_dataset: uuid::Uuid = sqlx::query_scalar("SELECT i.dataset_revision_id FROM app.input_set_items i JOIN app.portfolio_candidates c ON c.input_set_id=i.input_set_id WHERE c.id=$1 AND i.dataset_revision_id IS NOT NULL")
        .bind(candidate.as_uuid()).fetch_one(pool).await.unwrap();
    let old_cutoff = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    store
        .create_input_set(
            actor,
            "new-id-old-native-cutoff",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: release.project_id,
                purpose: InputPurpose::Forward,
                decision_cutoff: old_cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: old_dataset.to_string().try_into().unwrap(),
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap();
    assert!(Box::pin(store.automate_rebalance_build(
        release.project_id,
        |id, size| f.read(id, size),
        |_| async { panic!("old native cutoff cannot publish a rebalance") }
    ))
    .await
    .unwrap()
    .is_none());
    let dataset = inputs::forward(pool, store, actor, f).await;
    let cutoff: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let input = store
        .create_input_set(
            actor,
            "automatic-next-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: release.project_id,
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
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.command_receipts")
        .fetch_one(pool)
        .await
        .unwrap();
    let failed = Box::pin(store.automate_rebalance_build(
        release.project_id,
        |id, size| f.read(id, size),
        |_| async { Err(StoreError::Integrity) },
    ))
    .await;
    assert!(matches!(failed, Err(StoreError::Integrity)), "{failed:?}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.portfolio_rebalances")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    let (left, right) = tokio::join!(
        Box::pin(store.automate_rebalance_build(
            release.project_id,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            }
        )),
        Box::pin(store.automate_rebalance_build(
            release.project_id,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            }
        )),
    );
    let runs: Vec<_> = [left.unwrap(), right.unwrap()]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.kind, contracts::runs::RunKind::PortfolioBuild);
    assert_eq!(run.input_set_id, input);
    assert_eq!(run.cycle_id, Some(build.cycle_id));
    let (original, creator, bound_policy, seed): (serde_json::Value, String, uuid::Uuid, uuid::Uuid) = sqlx::query_as("SELECT t.request,a.created_by,b.policy_id,b.source_candidate_id FROM app.portfolio_build_tasks t JOIN app.run_native_tasks n ON n.run_id=t.run_id JOIN app.artifacts a ON a.id=n.parameters_artifact_id JOIN app.portfolio_rebalances b ON b.run_id=t.run_id WHERE t.run_id=$1")
        .bind(run.id.as_uuid()).fetch_one(pool).await.unwrap();
    let original: contracts::portfolio::PortfolioBuildRequestV1 =
        serde_json::from_value(original).unwrap();
    assert_eq!(original.input_set_id, input);
    assert_eq!(
        original.expected_runtime_revision,
        build.expected_runtime_revision
    );
    assert_eq!(
        serde_json::to_value(&original.members).unwrap(),
        serde_json::to_value(&build.members).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&original.limits).unwrap(),
        serde_json::to_value(&build.limits).unwrap()
    );
    assert_eq!(
        (creator.as_str(), bound_policy, seed),
        ("RUNTIME", policy.id.as_uuid(), candidate.as_uuid())
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.command_receipts")
            .fetch_one(pool)
            .await
            .unwrap(),
        before,
        "Worker does not mint an Operator receipt"
    );
    assert!(Box::pin(store.automate_rebalance_build(
        release.project_id,
        |_, _| async { panic!("pending successor reads no file") },
        |_| async { panic!("pending successor publishes nothing") }
    ))
    .await
    .unwrap()
    .is_none());
    assert!(sqlx::query(
        "UPDATE app.portfolio_rebalances SET decision_cutoff=clock_timestamp() WHERE run_id=$1"
    )
    .bind(run.id.as_uuid())
    .execute(pool)
    .await
    .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.releases")
            .fetch_one(pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.approvals")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    let message = validation_publication::message(pool, run.id).await;
    assert!(matches!(
        store
            .claim_native_run(&message, "automatic-rebalance-native", 60)
            .await
            .unwrap(),
        Some(ClaimResult::Leased(_))
    ));
}
