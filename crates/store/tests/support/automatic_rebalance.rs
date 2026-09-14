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
    Box::pin(continue_study(pool, store, actor, f, run, &release, before)).await;
}

async fn continue_study(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::runs::RunSnapshotV1,
    seed: &ReleaseViewV1,
    receipts: i64,
) {
    assert!(Box::pin(store.automate_rebalance_study(
        seed.project_id,
        |_, _| async { panic!("unfinished Build reads nothing") },
        |_| async { panic!("unfinished Build publishes nothing") }
    ))
    .await
    .unwrap()
    .is_none());
    let message = validation_publication::message(pool, build.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "automatic-rebalance-native", 60)
        .await
        .unwrap()
    else {
        panic!("Build lease")
    };
    let job = store.native_job(build.id, &lease.fence).await.unwrap();
    Box::pin(result::complete(pool, store, f, &lease, &job)).await;
    let candidate = validation_publication::publish(store, f, build.id)
        .await
        .unwrap()
        .resource;
    store.acknowledge_run(&message).await.unwrap();
    let publish = |object: store::lifecycle::native::NativeObjectPublication| {
        std::future::ready(
            f.objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity),
        )
    };
    let (left, right) = tokio::join!(
        Box::pin(store.automate_rebalance_study(
            seed.project_id,
            |id, size| f.read(id, size),
            publish
        )),
        Box::pin(store.automate_rebalance_study(
            seed.project_id,
            |id, size| f.read(id, size),
            publish
        ))
    );
    let runs: Vec<_> = [left.unwrap(), right.unwrap()]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(runs.len(), 1);
    let study = &runs[0];
    assert_eq!(study.kind, contracts::runs::RunKind::PortfolioSimulate);
    assert_eq!(study.cycle_id, build.cycle_id);
    assert_ne!(
        study.input_set_id, build.input_set_id,
        "Study uses its frozen independent historical plan"
    );
    let bound: (uuid::Uuid,String) = sqlx::query_as("SELECT s.candidate_id,a.created_by FROM app.portfolio_rebalance_studies child JOIN app.portfolio_study_tasks s ON s.run_id=child.study_run_id JOIN app.run_native_tasks t ON t.run_id=s.run_id JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE child.build_run_id=$1 AND child.study_run_id=$2")
        .bind(build.id.as_uuid()).bind(study.id.as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(bound, (candidate.as_uuid(), "RUNTIME".into()));
    assert!(Box::pin(store.automate_rebalance_study(
        seed.project_id,
        |_, _| async { panic!("existing Study reads nothing") },
        |_| async { panic!("existing Study publishes nothing") }
    ))
    .await
    .unwrap()
    .is_none());
    Box::pin(continue_release(
        pool, store, actor, f, study, seed, receipts,
    ))
    .await;
}

async fn continue_release(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    study: &contracts::runs::RunSnapshotV1,
    seed: &ReleaseViewV1,
    receipts: i64,
) {
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(seed.package_artifact_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    let size = DbCounter::new(size as u64).unwrap();
    let old = f.read(seed.package_artifact_id, size).await.unwrap();
    assert!(Box::pin(store.automate_rebalance_release(
        seed.project_id,
        |_, _| async { panic!("unpublished Study reads nothing") },
        |_| async { panic!("unpublished Study publishes nothing") }
    ))
    .await
    .unwrap()
    .is_none());
    let message = validation_publication::message(pool, study.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "automatic-study-native", 60)
        .await
        .unwrap()
    else {
        panic!("Study lease")
    };
    let job = store.native_job(study.id, &lease.fence).await.unwrap();
    Box::pin(study_result::complete(
        pool, store, f, &lease, &job, false, true,
    ))
    .await;
    let evaluation = validation_publication::publish(store, f, study.id)
        .await
        .unwrap()
        .resource;
    assert_eq!(
        store.evaluation(actor, evaluation).await.unwrap().decision,
        contracts::evidence::Decision::Pass
    );
    store.acknowledge_run(&message).await.unwrap();
    assert!(matches!(
        Box::pin(store.automate_rebalance_release(
            seed.project_id,
            |id, size| f.read(id, size),
            |_| async { Err(StoreError::Integrity) }
        ))
        .await,
        Err(StoreError::Integrity)
    ));
    let publish = |object: store::lifecycle::native::NativeObjectPublication| {
        std::future::ready(
            f.objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity),
        )
    };
    let (left, right) = tokio::join!(
        Box::pin(store.automate_rebalance_release(
            seed.project_id,
            |id, size| f.read(id, size),
            publish
        )),
        Box::pin(store.automate_rebalance_release(
            seed.project_id,
            |id, size| f.read(id, size),
            publish
        ))
    );
    let releases: Vec<_> = [left.unwrap(), right.unwrap()]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(releases.len(), 1);
    let release = &releases[0];
    assert_eq!(release.evaluation_id, evaluation);
    assert_ne!(release.candidate_id, seed.candidate_id);
    assert_ne!(release.id, seed.id);
    assert_ne!(release.package_artifact_id, seed.package_artifact_id);
    assert_eq!(f.read(seed.package_artifact_id, size).await.unwrap(), old);
    let facts: (i64,i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM app.command_receipts),(SELECT count(*) FROM app.approvals),(SELECT count(*) FROM app.portfolio_rebalance_releases),a.created_by FROM app.artifacts a WHERE a.id=$1")
        .bind(release.package_artifact_id.as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(facts, (receipts, 0, 1, "RUNTIME".into()));
    assert!(Box::pin(store.automate_rebalance_release(
        seed.project_id,
        |_, _| async { panic!("existing Release reads nothing") },
        |_| async { panic!("existing Release publishes nothing") }
    ))
    .await
    .unwrap()
    .is_none());
    assert!(
        sqlx::query("DELETE FROM app.portfolio_rebalance_releases WHERE release_id=$1")
            .bind(release.id.as_uuid())
            .execute(pool)
            .await
            .is_err()
    );
}
