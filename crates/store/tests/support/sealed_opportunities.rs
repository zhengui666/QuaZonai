//! Real PostgreSQL/PGMQ access reservations. Controlled reports are not scientific qualification.
use super::*;
use contracts::evidence::AlphaEvaluateRequestV1;
use store::{authority::Actor, lifecycle::native::NativeJob};

async fn task(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    actor: &Actor,
    evaluation: Id,
    key: &str,
    lease_seconds: u16,
) -> RunLease {
    let version = sqlx::query("SELECT v.id,r.cycle_id FROM app.alpha_versions v JOIN app.calibrations c ON c.id=v.calibration_id JOIN app.evaluations e ON e.id=c.validation_evaluation_id JOIN app.runs r ON r.id=e.run_id WHERE e.id=$1")
        .bind(evaluation.as_uuid()).fetch_one(pool).await.unwrap();
    let alpha = Id::try_from(version.get::<uuid::Uuid, _>("id").to_string()).unwrap();
    let context = &f.freeze.execution_context;
    let mut limits = super::limits();
    limits.experiments = 0;
    let request = AlphaEvaluateRequestV1 {
        schema_version: SchemaV1,
        cycle_id: Id::try_from(version.get::<uuid::Uuid, _>("cycle_id").to_string()).unwrap(),
        policy_id: f.brief.content.evaluation_policy_id,
        input_set_id: context.sealed_input_set_id,
        runtime_id: context.runtime_id,
        expected_runtime_revision: context.runtime_revision,
        limits,
    };
    let run = store
        .start_alpha_evaluation(
            actor,
            key,
            alpha,
            &request,
            |id, size| f.read(id, size),
            |object| {
                let objects = f.objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await
        .unwrap()
        .resource;
    let trials: i64 = sqlx::query_scalar(
        "SELECT (limits->>'experiments')::bigint FROM app.run_admissions WHERE run_id=$1",
    )
    .bind(run.id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        trials, 0,
        "the original compiled trial is not charged again"
    );
    let replay = store
        .start_alpha_evaluation(
            actor,
            key,
            alpha,
            &request,
            |_, _| async { panic!("replay must not read inputs") },
            |_| async { panic!("replay must not publish") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, run.id);
    let message = validation_publication::message(pool, run.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, key, lease_seconds)
        .await
        .unwrap()
    else {
        panic!("native lease required")
    };
    *lease
}

#[sqlx::test(migrations = "../../migrations")]
async fn sealed_metrics_use_own_policy_and_original_opportunity_not_source_pass(pool: PgPool) {
    let (store, actor, f, parent, experiment, validation) =
        validation_publication::prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    let source = validation_publication::publish(&store, &f, validation)
        .await
        .unwrap()
        .resource;
    cycle_selection::cancelled_parent(&pool, &store, &actor, &parent).await;
    let lease = task(&pool, &store, &f, &actor, source, "sealed-scientific", 60).await;
    let run = lease.run.id;
    let native = experiment_support::complete_sealed(&pool, &store, &f, lease).await;
    // A later disclosure cannot rewrite the already reserved opportunity.
    sqlx::query("INSERT INTO app.evidence_exposures(root_lineage_id,dataset_revision_id,actor_kind,exposure_kind,exposed_at,purpose) SELECT root_lineage_id,$2,'OPERATOR','SUMMARY',clock_timestamp(),'Controlled disclosure after original reservation' FROM app.projects WHERE id=$1")
        .bind(f.data.project.as_uuid()).bind(f.data.sealed.as_uuid()).execute(&pool).await.unwrap();
    let message = validation_publication::message(&pool, run).await;
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let evaluation = validation_publication::publish(&store, &f, run)
        .await
        .unwrap()
        .resource;
    let row = sqlx::query("SELECT e.execution_status,e.evidence_status,e.decision,v.calibration_id FROM app.evaluations e JOIN app.alpha_versions v ON v.id=e.subject_alpha_version_id WHERE e.id=$1")
        .bind(evaluation.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("execution_status"), "SUCCEEDED");
    assert_eq!(row.get::<String, _>("evidence_status"), "VALID");
    assert_eq!(
        row.get::<String, _>("decision"),
        "REJECT",
        "0.15 passes Validation's 0.1 but fails the separately frozen Sealed 0.2"
    );
    assert!(row.get::<Option<uuid::Uuid>, _>("calibration_id").is_some());
    let metrics = sqlx::query("SELECT scope,value,source_artifact_id FROM app.metric_values WHERE evaluation_id=$1 ORDER BY metric_code")
        .bind(evaluation.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].get::<Option<f64>, _>("value"), Some(0.15));
    for m in metrics {
        assert_eq!(m.get::<String, _>("scope"), "asset:0");
        assert_eq!(
            m.get::<uuid::Uuid, _>("source_artifact_id"),
            native.as_uuid()
        );
    }
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM app.experiments WHERE id=$1")
        .bind(experiment.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        outcome, "SUPPORTED",
        "the source experiment verdict is immutable"
    );
    assert_eq!(reservations(&pool).await, 1);
    let qualified: i64 = sqlx::query_scalar("SELECT count(*) FROM app.qualifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(qualified, 0);
    store.acknowledge_run(&message).await.unwrap();
    let replay = validation_publication::publish(&store, &f, run)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, evaluation);
}

async fn reservations(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM app.sealed_opportunities")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_sealed_publication_is_atomic_replayable_and_required_before_ack(pool: PgPool) {
    let (store, actor, f, parent, _, validation) = validation_publication::prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    let source = validation_publication::publish(&store, &f, validation)
        .await
        .unwrap()
        .resource;
    cycle_selection::cancelled_parent(&pool, &store, &actor, &parent).await;
    let lease = task(&pool, &store, &f, &actor, source, "sealed-publication", 60).await;
    store.native_job(lease.run.id, &lease.fence).await.unwrap();
    let current = store.get_run(&actor, lease.run.id).await.unwrap();
    store
        .cancel_run(
            &actor,
            "cancel-sealed-publication",
            current.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    store
        .settle_unsubmitted_native_run(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .unwrap();
    let message = validation_publication::message(&pool, lease.run.id).await;
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let failed = store
        .publish_alpha_evaluation(
            lease.run.id,
            |_, _| async { panic!("cancelled task has no native report to read") },
            |_| async { Err(StoreError::Integrity) },
        )
        .await;
    assert!(matches!(failed, Err(StoreError::Integrity)));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
        .bind(lease.run.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let published = validation_publication::publish(&store, &f, lease.run.id)
        .await
        .unwrap();
    assert!(!published.replayed);
    let row = sqlx::query("SELECT e.*,a.byte_count FROM app.evaluations e JOIN app.artifacts a ON a.id=e.report_artifact_id WHERE e.id=$1").bind(published.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("evaluation_kind"), "SEALED");
    assert_eq!(row.get::<String, _>("execution_status"), "CANCELLED");
    assert_eq!(row.get::<String, _>("evidence_status"), "INCOMPLETE");
    assert_eq!(row.get::<String, _>("decision"), "INCONCLUSIVE");
    let report: serde_json::Value = serde_json::from_slice(
        &f.read(
            Id::try_from(row.get::<uuid::Uuid, _>("report_artifact_id").to_string()).unwrap(),
            DbCounter::new(row.get::<i64, _>("byte_count") as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["validation_evaluation_id"], source.to_string());
    assert!(report["exposure_id"].is_string());
    assert!(report["native_report_artifact_id"].is_null());
    let replay = store
        .publish_alpha_evaluation(
            lease.run.id,
            |_, _| async { panic!("replay must not read") },
            |_| async { panic!("replay must not publish") },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, published.resource);
    assert_eq!(reservations(&pool).await, 1);
    store.acknowledge_run(&message).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn root_opportunity_is_atomic_once_per_attempt_and_cancellation_never_refunds(pool: PgPool) {
    let (store, actor, f, parent, _, validation) = validation_publication::prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    let evaluation = validation_publication::publish(&store, &f, validation)
        .await
        .unwrap()
        .resource;
    cycle_selection::cancelled_parent(&pool, &store, &actor, &parent).await;
    let first = task(&pool, &store, &f, &actor, evaluation, "sealed-a", 60).await;
    let second = task(&pool, &store, &f, &actor, evaluation, "sealed-b", 60).await;
    let (a, b) = tokio::join!(
        store.native_job(first.run.id, &first.fence),
        store.native_job(second.run.id, &second.fence)
    );
    let (winner, job, loser) = match (a, b) {
        (Ok(job), Err(StoreError::Domain(domain::DomainError::BudgetExhausted("sealed_uses")))) => {
            (&first, job, &second)
        }
        (Err(StoreError::Domain(domain::DomainError::BudgetExhausted("sealed_uses"))), Ok(job)) => {
            (&second, job, &first)
        }
        (a, b) => panic!(
            "exactly one native capability may consume the frozen root budget: {:?}, {:?}",
            a.err(),
            b.err()
        ),
    };
    assert_eq!(reservations(&pool).await, 1);
    let NativeJob { spec, .. } = store
        .native_job(winner.run.id, &winner.fence)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(spec).unwrap(),
        serde_json::to_value(job.spec).unwrap()
    );
    assert_eq!(reservations(&pool).await, 1);
    let ungranted: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_native_attempts WHERE attempt_id=$1")
            .bind(loser.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ungranted, 0);
    assert!(
        store
            .settle_unsubmitted_native_run(winner.run.id, &winner.fence)
            .await
            .unwrap()
            .is_none(),
        "the already granted opportunity is retained even when its quota is now full"
    );
    let current = store.get_run(&actor, winner.run.id).await.unwrap();
    store
        .cancel_run(
            &actor,
            "sealed-stop",
            current.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    assert!(store
        .settle_unsubmitted_native_run(winner.run.id, &winner.fence)
        .await
        .unwrap()
        .is_some());
    assert_eq!(reservations(&pool).await, 1);
    assert!(matches!(
        store.native_job(loser.run.id, &loser.fence).await,
        Err(StoreError::Domain(domain::DomainError::BudgetExhausted(
            "sealed_uses"
        )))
    ));
    let rejected = store
        .settle_unsubmitted_native_run(loser.run.id, &loser.fence)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rejected.state, contracts::runs::RunState::Failed);
    assert_eq!(
        rejected.terminal_reason_code.as_deref(),
        Some("SEALED_OPPORTUNITY_UNAVAILABLE")
    );
    let message = validation_publication::message(&pool, loser.run.id).await;
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let published = validation_publication::publish(&store, &f, loser.run.id)
        .await
        .unwrap()
        .resource;
    let status: (String, String, String) = sqlx::query_as(
        "SELECT execution_status,evidence_status,decision FROM app.evaluations WHERE id=$1",
    )
    .bind(published.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        status,
        ("FAILED".into(), "INCOMPLETE".into(), "INCONCLUSIVE".into())
    );
    store.acknowledge_run(&message).await.unwrap();
    assert_eq!(reservations(&pool).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn prior_summary_exposure_blocks_new_capability_without_a_spec_or_refund(pool: PgPool) {
    let (store, actor, f, parent, _, validation) = validation_publication::prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    let evaluation = validation_publication::publish(&store, &f, validation)
        .await
        .unwrap()
        .resource;
    cycle_selection::cancelled_parent(&pool, &store, &actor, &parent).await;
    let lease = task(
        &pool,
        &store,
        &f,
        &actor,
        evaluation,
        "sealed-disclosed",
        60,
    )
    .await;
    sqlx::query("INSERT INTO app.evidence_exposures(root_lineage_id,dataset_revision_id,actor_kind,exposure_kind,exposed_at,purpose) SELECT root_lineage_id,$2,'OPERATOR','SUMMARY',clock_timestamp(),'Controlled prior disclosure' FROM app.projects WHERE id=$1")
        .bind(f.data.project.as_uuid()).bind(f.data.sealed.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store.native_job(lease.run.id, &lease.fence).await,
        Err(StoreError::Invalid("sealed_independence_unavailable"))
    ));
    assert_eq!(reservations(&pool).await, 0);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_native_attempts WHERE attempt_id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    let rejected = store
        .settle_unsubmitted_native_run(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        rejected.terminal_reason_code.as_deref(),
        Some("SEALED_OPPORTUNITY_UNAVAILABLE")
    );
    assert_eq!(reservations(&pool).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_lease_after_opportunity_check_cannot_commit_unsent_rejection(pool: PgPool) {
    let (store, actor, f, parent, _, validation) = validation_publication::prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    let evaluation = validation_publication::publish(&store, &f, validation)
        .await
        .unwrap()
        .resource;
    cycle_selection::cancelled_parent(&pool, &store, &actor, &parent).await;
    let lease = task(&pool, &store, &f, &actor, evaluation, "unsent-expiring", 1).await;
    sqlx::query("INSERT INTO app.evidence_exposures(root_lineage_id,dataset_revision_id,actor_kind,exposure_kind,exposed_at,purpose) SELECT root_lineage_id,$2,'OPERATOR','SUMMARY',clock_timestamp(),'Controlled prior disclosure' FROM app.projects WHERE id=$1")
        .bind(f.data.project.as_uuid()).bind(f.data.sealed.as_uuid()).execute(&pool).await.unwrap();
    let mut holding = pool.begin().await.unwrap();
    sqlx::query("SELECT lineage.id FROM app.research_lineages lineage JOIN app.projects project ON project.root_lineage_id=lineage.id WHERE project.id=$1 FOR UPDATE OF lineage")
        .bind(f.data.project.as_uuid()).fetch_one(&mut *holding).await.unwrap();
    let pending = store.settle_unsubmitted_native_run(lease.run.id, &lease.fence);
    tokio::pin!(pending);
    tokio::select! {
        _ = &mut pending => panic!("another transaction owns the lineage lock"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(1200)) => {},
    }
    holding.rollback().await.unwrap();
    assert!(matches!(
        pending.await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    let receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1")
            .bind(lease.run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(receipts, 0);
    assert_eq!(reservations(&pool).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_lease_after_lineage_lock_wait_cannot_commit_an_opportunity(pool: PgPool) {
    let (store, actor, f, parent, _, validation) = validation_publication::prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    let evaluation = validation_publication::publish(&store, &f, validation)
        .await
        .unwrap()
        .resource;
    cycle_selection::cancelled_parent(&pool, &store, &actor, &parent).await;
    let lease = task(&pool, &store, &f, &actor, evaluation, "sealed-expiring", 1).await;
    let mut holding = pool.begin().await.unwrap();
    sqlx::query("SELECT lineage.id FROM app.research_lineages lineage JOIN app.projects project ON project.root_lineage_id=lineage.id WHERE project.id=$1 FOR UPDATE OF lineage")
        .bind(f.data.project.as_uuid()).fetch_one(&mut *holding).await.unwrap();
    let pending = store.native_job(lease.run.id, &lease.fence);
    tokio::pin!(pending);
    tokio::select! {
        _ = &mut pending => panic!("another transaction owns the lineage lock"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(1200)) => {},
    }
    holding.rollback().await.unwrap();
    assert!(matches!(
        pending.await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    assert_eq!(reservations(&pool).await, 0);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_native_attempts WHERE attempt_id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}
