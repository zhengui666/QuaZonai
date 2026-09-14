//! Native cancellation and policy replacement cannot erase daily Build attempts.
use super::*;

#[sqlx::test(migrations = "../../migrations")]
async fn rebalance_daily_quota_survives_cancellation_and_policy_replacement(pool: PgPool) {
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
    let (seed, policy, _) = Box::pin(prepare(&pool, &store, &actor, &f, &build, candidate)).await;
    Box::pin(check(&pool, &store, &actor, &f, &seed, policy)).await;
}

async fn check(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    seed: &ReleaseViewV1,
    mut policy: AutomationPolicyViewV1,
) {
    policy.content.max_rebalances_per_day = 1;
    for key in ["quota-one", "quota-replacement"] {
        let replacement = store
            .authorize_automation(
                actor,
                key,
                seed.project_id,
                &AutomationAuthorizeV1 {
                    schema_version: SchemaV1,
                    expected_project_revision: store
                        .project(actor, seed.project_id)
                        .await
                        .unwrap()
                        .revision,
                    content: policy.content.clone(),
                },
            )
            .await
            .unwrap()
            .resource;
        assert_ne!(replacement.id, policy.id);
        policy = replacement;
        if key == "quota-one" {
            let run = Box::pin(store.automate_rebalance_build(
                seed.project_id,
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
            .unwrap();
            let cancelled = store
                .cancel_run(
                    actor,
                    "cancel-quota-build",
                    run.id,
                    &contracts::lifecycle::RunCancelV1 {
                        schema_version: SchemaV1,
                        expected_revision: run.revision,
                    },
                )
                .await
                .unwrap()
                .resource;
            assert_eq!(cancelled.state, contracts::runs::RunState::Cancelled);
            assert!(Box::pin(store.automate_rebalance_build(
                seed.project_id,
                |id, size| f.read(id, size),
                |_| async { panic!("cancelled cutoff must not be repeated") },
            ))
            .await
            .unwrap()
            .is_none());
        }
    }
    let dataset = inputs::forward(pool, store, actor, f).await;
    let cutoff = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    store
        .create_input_set(
            actor,
            "quota-new-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: seed.project_id,
                purpose: InputPurpose::Forward,
                decision_cutoff: cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset,
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE project_id=$1")
        .bind(seed.project_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    for _ in 0..2 {
        let result = Box::pin(store.automate_rebalance_build(
            seed.project_id,
            |id, size| f.read(id, size),
            |_| async { panic!("daily quota cannot publish another task") },
        ))
        .await;
        assert!(matches!(
            result,
            Err(StoreError::Invalid("automation_daily_quota"))
        ));
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE project_id=$1")
        .bind(seed.project_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.portfolio_rebalances WHERE project_id=$1")
            .bind(seed.project_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(attempts, 1);
}
