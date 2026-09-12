//! Real PostgreSQL/PGMQ access reservations. Controlled reports are not scientific qualification.
use super::*;
use contracts::{
    catalogs::RuntimeCatalogMetadataV1,
    research::{ArtifactInputRole, DataPartition},
    science::NativeAlphaSealedRequestV1,
};
use store::lifecycle::{native::NativeJob, RunSubmission};

async fn metadata(
    pool: &PgPool,
    f: &cycle_support::Fixture,
    dataset: Id,
) -> RuntimeCatalogMetadataV1 {
    let row = sqlx::query("SELECT a.id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1")
        .bind(dataset.as_uuid()).fetch_one(pool).await.unwrap();
    serde_json::from_slice(
        &f.read(
            Id::try_from(row.get::<uuid::Uuid, _>("id").to_string()).unwrap(),
            DbCounter::new(row.get::<i64, _>("byte_count") as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap()
}

async fn task(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    source_run: Id,
    evaluation: Id,
    key: &str,
    lease_seconds: u16,
) -> RunLease {
    let source = sqlx::query("SELECT a.id,a.byte_count,t.capability_snapshot_artifact_id,t.image_ref,r.cycle_id FROM app.run_native_tasks t JOIN app.runs r ON r.id=t.run_id JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE t.run_id=$1")
        .bind(source_run.as_uuid()).fetch_one(pool).await.unwrap();
    let id = Id::try_from(source.get::<uuid::Uuid, _>("id").to_string()).unwrap();
    let bytes = f
        .read(
            id,
            DbCounter::new(source.get::<i64, _>("byte_count") as u64).unwrap(),
        )
        .await
        .unwrap();
    let NativeTaskParametersV1::ValidateAlpha { request, .. } =
        serde_json::from_slice(&bytes).unwrap()
    else {
        panic!("original validation required")
    };
    let version = sqlx::query("SELECT v.id,v.model_artifact_id,c.model_artifact_id AS fitted,w.byte_count AS wasm_bytes,m.byte_count AS fitted_bytes FROM app.alpha_versions v JOIN app.calibrations c ON c.id=v.calibration_id JOIN app.artifacts w ON w.id=v.model_artifact_id JOIN app.artifacts m ON m.id=c.model_artifact_id WHERE c.validation_evaluation_id=$1")
        .bind(evaluation.as_uuid()).fetch_one(pool).await.unwrap();
    let alpha = Id::try_from(version.get::<uuid::Uuid, _>("id").to_string()).unwrap();
    let wasm = Id::try_from(
        version
            .get::<uuid::Uuid, _>("model_artifact_id")
            .to_string(),
    )
    .unwrap();
    let fitted = Id::try_from(version.get::<uuid::Uuid, _>("fitted").to_string()).unwrap();
    let held = metadata(pool, f, f.data.sealed).await;
    let training = metadata(pool, f, f.data.validation).await;
    let mut forecast = request.forecast;
    forecast.selection = held.quality.datasets[0].selection.clone();
    let operation = NativeTaskParametersV1::EvaluateSealedAlpha {
        schema_version: SchemaV1,
        dataset_revision_id: f.data.sealed,
        model_artifact_id: wasm,
        calibration_artifact_id: Some(fitted),
        request: Box::new(NativeAlphaSealedRequestV1 {
            schema_version: SchemaV1,
            forecast,
            target_kind: request.target_kind,
            research_available_through_ns: training.quality.datasets[0].available_through_ns,
        }),
    };
    let parameters = Id::new();
    let bytes = serde_json::to_vec(&operation).unwrap();
    f.objects.put(parameters, &bytes).unwrap();
    let context = &f.freeze.execution_context;
    // Generic test administration pays the ordinary trial charge. This fixture
    // tests access accounting, not the forthcoming non-trial Sealed admission.
    let limits = super::limits();
    let run = store
        .enqueue_run(
            key,
            &RunSubmission {
                cycle_id: Id::try_from(source.get::<uuid::Uuid, _>("cycle_id").to_string())
                    .unwrap(),
                input_set_id: context.sealed_input_set_id,
                runtime_id: context.runtime_id,
                runtime_revision: context.runtime_revision,
                kind: contracts::runs::RunKind::AlphaEvaluate,
                limits,
            },
        )
        .await
        .unwrap()
        .resource;
    let inputs = vec![
        RuntimeInputV1::Dataset {
            revision_id: f.data.sealed,
            registered_ref: held.registered_ref,
            storage_version: held.storage_version,
            role: DataPartition::Sealed,
        },
        RuntimeInputV1::Artifact {
            artifact_id: wasm,
            storage_version: "1".into(),
            byte_count: DbCounter::new(version.get::<i64, _>("wasm_bytes") as u64).unwrap(),
            role: ArtifactInputRole::Model,
        },
        RuntimeInputV1::Artifact {
            artifact_id: fitted,
            storage_version: "1".into(),
            byte_count: DbCounter::new(version.get::<i64, _>("fitted_bytes") as u64).unwrap(),
            role: ArtifactInputRole::Model,
        },
        RuntimeInputV1::Artifact {
            artifact_id: parameters,
            storage_version: "1".into(),
            byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
            role: ArtifactInputRole::Parameters,
        },
    ];
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','FIXTURE','RUNTIME','REFERENCED')")
        .bind(parameters.as_uuid()).bind(f.data.project.as_uuid()).bind(parameters.to_string()).bind(bytes.len() as i64).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_native_tasks(run_id,parameters_artifact_id,input_bindings,image_ref,cpu,capability_snapshot_artifact_id,output_schemas,origin,access_class) VALUES($1,$2,$3,$4,1,$5,$6,'FIXTURE','EVALUATOR_ONLY')")
        .bind(run.id.as_uuid()).bind(parameters.as_uuid()).bind(serde_json::to_value(&inputs).unwrap()).bind(source.get::<String,_>("image_ref")).bind(source.get::<uuid::Uuid,_>("capability_snapshot_artifact_id")).bind(serde_json::to_value(operation.output_schemas()).unwrap()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.sealed_evaluation_tasks(run_id,alpha_version_id,policy_id,validation_evaluation_id,dataset_revision_id) VALUES($1,$2,$3,$4,$5)")
        .bind(run.id.as_uuid()).bind(alpha.as_uuid()).bind(f.brief.content.evaluation_policy_id.as_uuid()).bind(evaluation.as_uuid()).bind(f.data.sealed.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
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

async fn reservations(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM app.sealed_opportunities")
        .fetch_one(pool)
        .await
        .unwrap()
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
    let first = task(&pool, &store, &f, validation, evaluation, "sealed-a", 60).await;
    let second = task(&pool, &store, &f, validation, evaluation, "sealed-b", 60).await;
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
        validation,
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
    let lease = task(
        &pool,
        &store,
        &f,
        validation,
        evaluation,
        "sealed-expiring",
        1,
    )
    .await;
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
