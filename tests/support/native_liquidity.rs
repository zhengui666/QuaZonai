use contracts::{execution_assumptions::ExecutionAssumptionsCreateV1, SchemaV1};
use sqlx::PgPool;

/// Controlled numerical receipt through the real original native admission.
/// This is not actual numerical computation, market provenance or qualification.
pub async fn measured_report(
    pool: &PgPool,
    store: &store::Store,
    actor: &store::authority::Actor,
    objects: &std::sync::Arc<integrations::artifacts::ArtifactStore>,
    request: &ExecutionAssumptionsCreateV1,
) -> contracts::Id {
    use contracts::{
        execution::*, lifecycle::JobLimitsV1, runtime_jobs::*, DbCounter, Id, Revision,
    };
    use store::{
        lifecycle::{native::NativePayloads, ClaimResult},
        StoreError,
    };
    let run = store
        .start_data_validation(
            actor,
            "measure-liquidity",
            &contracts::data::DataValidateRequest {
                schema_version: SchemaV1,
                project_id: request.project_id,
                input_set_id: request.input_set_id,
                runtime_id: request.runtime_id,
                expected_runtime_revision: request.expected_runtime_revision,
                limits: JobLimitsV1 {
                    schema_version: SchemaV1,
                    experiments: 0,
                    cpu_seconds: DbCounter::new(10).unwrap(),
                    wall_seconds: 60,
                    memory_mib: 512,
                    output_bytes: DbCounter::new(65536).unwrap(),
                },
            },
            |id, size| {
                std::future::ready(objects.read(id, size).map_err(|_| StoreError::Integrity))
            },
            |object| {
                std::future::ready(
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    let message = store
        .read_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == run.id)
        .unwrap();
    let ClaimResult::Leased(lease) = store
        .claim_run(&message, "liquidity-measurement", 60)
        .await
        .unwrap()
    else {
        panic!("native measurement lease")
    };
    let job = store.native_job(run.id, &lease.fence).await.unwrap();
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|v| match v {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let NativeTaskParametersV1::ValidateData { selections, .. } =
        serde_json::from_slice(&objects.read(job.spec.parameters_artifact_id, size).unwrap())
            .unwrap()
    else {
        panic!("native validation task")
    };
    assert!(store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let (metadata_id, size): (uuid::Uuid, i64) = sqlx::query_as("SELECT e.native_metadata_artifact_id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1")
        .bind(request.dataset_revision_id.as_uuid()).fetch_one(pool).await.unwrap();
    let metadata: contracts::catalogs::RuntimeCatalogMetadataV1 = serde_json::from_slice(
        &objects
            .read(
                metadata_id.to_string().try_into().unwrap(),
                DbCounter::new(size as u64).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let mut quality = metadata.quality;
    quality.checked_at = now;
    quality.datasets[0].dataset_revision_id = request.dataset_revision_id;
    quality.datasets[0].selection = selections[0].selection.clone();
    let q = &mut quality.datasets[0];
    q.last_bar_notionals = Some(vec![NativeBarNotionalV1 {
        instrument_id: q.instrument_ids[0].clone(),
        currency: "USD".into(),
        event_ns: q.last_event_ns,
        available_ns: q.available_through_ns,
        close_price: "1".parse().unwrap(),
        traded_volume: "1000".parse().unwrap(),
        notional_value: "1000".parse().unwrap(),
    }]);
    let bytes = serde_json::to_vec(&quality).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::DataQuality,
        schema: job.spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        media_type: "application/json".into(),
    };
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: run.id,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id,
        input_set_id: request.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: [("bar-notional".into(), "1".into())].into(),
        started_at: Some(job.submitted_not_before),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::ZERO,
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: output.byte_count,
        },
        artifacts: vec![output.clone()],
        error: None,
    };
    store
        .publish_native_result(
            run.id,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            NativePayloads::Verified(vec![(output, bytes)]),
            |id, size| {
                std::future::ready(objects.read(id, size).map_err(|_| StoreError::Integrity))
            },
            |published| {
                std::future::ready(published.into_iter().try_for_each(|o| {
                    objects
                        .put(o.id, &o.bytes)
                        .map_err(|_| StoreError::Integrity)
                }))
            },
        )
        .await
        .unwrap();
    let id: uuid::Uuid =
        sqlx::query_scalar("SELECT artifact_id FROM app.run_native_outputs WHERE attempt_id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    id.to_string().try_into().unwrap()
}
