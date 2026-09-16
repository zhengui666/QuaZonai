//! Real Worker ticks and PG/PGMQ; numerical results use the shared controlled protocol.
use super::*;
use contracts::{delivery::*, forward::ForwardEnvironmentV1, settings::*};
use qualified_portfolio::automatic_rebalance::{complete_stage, prepare};

pub(super) async fn scenario(pool: PgPool) {
    let (store, actor, f, build, candidate, directory) =
        Box::pin(qualified_portfolio::qualified_chain_scheduled(
            pool.clone(),
            cycle_support::Liquidity::None,
            ForwardEnvironmentV1::Live,
            contracts::research::DataUse::ResearchAndPaper,
            qualified_portfolio::release_policy,
            Some(5),
        ))
        .await
        .unwrap();
    let (seed, policy, input) =
        Box::pin(prepare(&pool, &store, &actor, &f, &build, candidate)).await;
    let web = support::fixture(pool.clone()).await;
    let vault = integrations::secrets::SecretVault::open(
        &web._state.path().join("secrets"),
        &web._state.path().join("master.key"),
    )
    .unwrap();
    let worker = server::worker::Worker::new(
        store.clone(),
        vault,
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
        server::runtime_transport::RuntimeTargets::default(),
        1,
    )
    .unwrap();
    Box::pin(check(
        &pool,
        &store,
        &actor,
        &f,
        &worker,
        (&seed, &policy, input),
    ))
    .await;
}

async fn tick(
    worker: &server::worker::Worker,
    project: Id,
) -> Result<(), server::worker::WorkerFailure> {
    use tracing::instrument::WithSubscriber;
    let diagnostics = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_test_writer()
        .with_ansi(false)
        .finish();
    let (cursor, result) = Box::pin(worker.process_automation(None))
        .with_subscriber(diagnostics)
        .await;
    assert_eq!(cursor, Some(project));
    result
}

// Match the production scheduler's eventual advancement under SKIP LOCKED.
async fn advance(
    pool: &PgPool,
    worker: &server::worker::Worker,
    project: Id,
    query: &str,
    parent: Id,
) -> Option<uuid::Uuid> {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let rows: Vec<uuid::Uuid> = sqlx::query_scalar(query)
                .bind(parent.as_uuid())
                .fetch_all(pool)
                .await
                .unwrap();
            assert!(rows.len() <= 1, "a rebalance stage must publish only once");
            if let Some(id) = rows.first() {
                return *id;
            }
            let result = Box::pin(tick(worker, project)).await;
            assert!(
                matches!(result, Ok(()) | Err(server::worker::WorkerFailure::Store)),
                "{result:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .ok()
}

async fn check(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    worker: &server::worker::Worker,
    original: (&ReleaseViewV1, &AutomationPolicyViewV1, Id),
) {
    let (seed, policy, input) = original;
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM app.command_receipts")
        .fetch_one(pool)
        .await
        .unwrap();
    // A held project lock must defer, not block or bypass original admission.
    let mut held = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
        .bind(seed.project_id.as_uuid())
        .fetch_one(&mut *held)
        .await
        .unwrap();
    let deferred = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        Box::pin(store.automate_rebalance_build(
            seed.project_id,
            |_, _| async { panic!("locked Build must not read artifacts") },
            |_| async { panic!("locked Build must not publish artifacts") },
        )),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(deferred.is_none());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.portfolio_rebalances WHERE project_id=$1")
            .bind(seed.project_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    held.rollback().await.unwrap();
    // The Paper failure must not starve independent bounded research stages.
    Box::pin(tick(worker, seed.project_id)).await.unwrap_err();
    let build = Box::pin(advance(
        pool,
        worker,
        seed.project_id,
        "SELECT run_id FROM app.portfolio_rebalances WHERE project_id=$1",
        seed.project_id,
    ))
    .await
    .expect("Worker Build did not advance within the bounded polling window");
    let lineage: (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT policy_id,input_set_id FROM app.portfolio_rebalances WHERE run_id=$1",
    )
    .bind(build)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(lineage, (policy.id.as_uuid(), input.as_uuid()));
    let build: Id = build.to_string().try_into().unwrap();
    let candidate = Box::pin(complete_stage(pool, store, f, build)).await;
    let _ = tokio::join!(
        Box::pin(tick(worker, seed.project_id)),
        Box::pin(tick(worker, seed.project_id))
    );
    Box::pin(advance(
        pool,
        worker,
        seed.project_id,
        "SELECT study_run_id FROM app.portfolio_rebalance_studies WHERE build_run_id=$1",
        build,
    ))
    .await
    .expect("Worker Study did not advance within the bounded polling window");
    let studies: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT study_run_id FROM app.portfolio_rebalance_studies WHERE build_run_id=$1",
    )
    .bind(build.as_uuid())
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(studies.len(), 1);
    let evaluation = Box::pin(complete_stage(
        pool,
        store,
        f,
        studies[0].to_string().try_into().unwrap(),
    ))
    .await;
    let released = Box::pin(advance(
        pool,
        worker,
        seed.project_id,
        "SELECT release_id FROM app.portfolio_rebalance_releases WHERE build_run_id=$1",
        build,
    ))
    .await;
    let released = match released {
        Some(id) => id,
        None => {
            let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
                .fetch_one(pool)
                .await
                .unwrap();
            let probes: Vec<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as("SELECT observed_at,valid_until FROM app.runtime_probe_observations ORDER BY observed_at DESC LIMIT 3").fetch_all(pool).await.unwrap();
            let deadline: Option<chrono::DateTime<chrono::Utc>> =
                sqlx::query_scalar("SELECT valid_until FROM app.evaluations WHERE id=$1")
                    .bind(evaluation.as_uuid())
                    .fetch_one(pool)
                    .await
                    .unwrap();
            panic!("Worker Release absent: now={now:?}, latest Runtime observations={probes:?}, evaluation deadline={deadline:?}, policy deadline={:?}", policy.content.valid_until);
        }
    };
    let released = store
        .release(actor, released.to_string().try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(
        (released.candidate_id, released.evaluation_id),
        (candidate, evaluation)
    );
    assert_ne!(released.package_artifact_id, seed.package_artifact_id);
    let facts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.command_receipts),(SELECT count(*) FROM app.approvals),(SELECT count(*) FROM app.handoff_offers)")
        .fetch_one(pool).await.unwrap();
    assert_eq!(facts, (receipts, 0, 0));
    Box::pin(deliver(pool, store, actor, f, worker, &released, policy)).await;
}

async fn deliver(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    worker: &server::worker::Worker,
    release: &ReleaseViewV1,
    policy: &AutomationPolicyViewV1,
) {
    let down = store
        .downstream(actor, policy.content.downstream_id)
        .await
        .unwrap();
    let store::downstream::ProbePreparation::Pending(ticket) = store
        .prepare_downstream_probe(
            actor,
            "rebalance-worker-probe",
            down.id,
            &DownstreamProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: down.revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("original probe")
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
    let (a, b) = tokio::join!(
        Box::pin(worker.process_automation(None)),
        Box::pin(worker.process_automation(None))
    );
    a.1.unwrap();
    b.1.unwrap();
    Box::pin(worker.process_automation(None)).await.1.unwrap();
    let offers: Vec<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM app.handoff_offers WHERE downstream_id=$1")
            .bind(down.id.as_uuid())
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(offers.len(), 1);
    let offer = store
        .handoff(actor, offers[0].to_string().try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(
        (offer.release_id, offer.candidate_id, offer.environment),
        (
            release.id,
            release.candidate_id,
            ForwardEnvironmentV1::Paper
        )
    );
    assert_eq!(offer.state, HandoffStateV1::Offered);
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_rebalances),(SELECT count(*) FROM app.portfolio_rebalance_studies),(SELECT count(*) FROM app.portfolio_rebalance_releases)")
        .fetch_one(pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1));
}
