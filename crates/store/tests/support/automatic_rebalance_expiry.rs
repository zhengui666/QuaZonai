//! Original scheduled revocation crosses real PostgreSQL time during publication.
use super::*;

#[derive(Clone, Copy)]
enum Stage {
    Build,
    Study,
    Release,
}

#[sqlx::test(migrations = "../../migrations")]
async fn rebalance_build_publication_rechecks_original_policy_deadline(pool: PgPool) {
    Box::pin(scenario(pool, Stage::Build)).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn rebalance_study_publication_rechecks_original_policy_deadline(pool: PgPool) {
    Box::pin(scenario(pool, Stage::Study)).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn rebalance_release_publication_rechecks_original_policy_deadline(pool: PgPool) {
    Box::pin(scenario(pool, Stage::Release)).await;
}

async fn scenario(pool: PgPool, stage: Stage) {
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
    Box::pin(prepare_stage(&pool, &store, &f, seed.project_id, stage)).await;
    Box::pin(expire_during_publication(
        &pool, &store, &actor, &f, &policy, stage,
    ))
    .await;
}

async fn prepare_stage(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    project: Id,
    stage: Stage,
) {
    let publish = |object: store::lifecycle::native::NativeObjectPublication| {
        std::future::ready(
            f.objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity),
        )
    };
    if !matches!(stage, Stage::Build) {
        let run =
            Box::pin(store.automate_rebalance_build(project, |id, size| f.read(id, size), publish))
                .await
                .unwrap()
                .unwrap();
        Box::pin(complete_stage(pool, store, f, run.id)).await;
    }
    if matches!(stage, Stage::Release) {
        let run =
            Box::pin(store.automate_rebalance_study(project, |id, size| f.read(id, size), publish))
                .await
                .unwrap()
                .unwrap();
        Box::pin(complete_stage(pool, store, f, run.id)).await;
    }
}

async fn counts(pool: &PgPool) -> Vec<i64> {
    sqlx::query_scalar("SELECT ARRAY[(SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.run_native_tasks),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.portfolio_rebalances),(SELECT count(*) FROM app.portfolio_rebalance_studies),(SELECT count(*) FROM app.portfolio_rebalance_releases),(SELECT count(*) FROM app.releases),(SELECT count(*) FROM app.command_receipts)]")
        .fetch_one(pool).await.unwrap()
}

async fn expire_during_publication(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    policy: &AutomationPolicyViewV1,
    stage: Stage,
) {
    let deadline: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT clock_timestamp()+interval '3 seconds'")
            .fetch_one(pool)
            .await
            .unwrap();
    store
        .revoke_automation(
            actor,
            "expiry-during-publish",
            policy.id,
            &PolicyRevokeV1 {
                schema_version: SchemaV1,
                expected_latest_revocation_id: None,
                effective_at: Some(deadline),
                reason: "Original scheduled revocation during bounded publication".into(),
            },
        )
        .await
        .unwrap();
    let before = counts(pool).await;
    let allocated = std::sync::Mutex::new(Vec::new());
    let recorded = &allocated;
    let publish = move |object: store::lifecycle::native::NativeObjectPublication| async move {
        f.objects
            .put(object.id, &object.bytes)
            .map_err(|_| StoreError::Integrity)?;
        recorded.lock().unwrap().push((
            object.id,
            DbCounter::new(object.bytes.len() as u64).unwrap(),
        ));
        // Same clock as the policy gate, without mutating stored timestamps.
        sqlx::query("SELECT pg_sleep(GREATEST(EXTRACT(EPOCH FROM ($1::timestamptz-clock_timestamp())),0)::double precision+0.02)")
            .bind(deadline).execute(pool).await?;
        Ok(())
    };
    let read = |id, size| f.read(id, size);
    let result = match stage {
        Stage::Build => Box::pin(store.automate_rebalance_build(policy.project_id, read, publish))
            .await
            .map(|v| v.map(|run| run.id)),
        Stage::Study => Box::pin(store.automate_rebalance_study(policy.project_id, read, publish))
            .await
            .map(|v| v.map(|run| run.id)),
        Stage::Release => {
            Box::pin(store.automate_rebalance_release(policy.project_id, read, publish))
                .await
                .map(|v| v.map(|release| release.id))
        }
    };
    assert!(
        matches!(result, Err(StoreError::Invalid("automation_expiry"))),
        "{result:?}"
    );
    assert_eq!(
        counts(pool).await,
        before,
        "expired publication must roll back registration and queue writes"
    );
    let allocated = allocated.into_inner().unwrap();
    assert_eq!(
        allocated.len(),
        1,
        "the expiry must occur after actual file publication"
    );
    for (id, size) in allocated {
        assert!(f.read(id, size).await.is_ok());
        assert!(store
            .discard_unpublished_forward_artifact(policy.project_id, id, |id| async move {
                f.objects
                    .discard_unpublished(id)
                    .map_err(|_| StoreError::Integrity)
            })
            .await
            .unwrap());
        assert!(f.read(id, size).await.is_err());
    }
    assert!(matches!(
        Box::pin(store.automate_rebalance_build(
            policy.project_id,
            |_, _| async { panic!("expired original policy reads no source") },
            |_| async { panic!("expired original policy publishes nothing") }
        ))
        .await,
        Err(StoreError::Invalid("automation_expiry"))
    ));
}
