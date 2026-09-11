//! Native PostgreSQL/PGMQ and immutable files with an explicitly controlled Runtime.
//! This fixture proves orchestration, not OCI execution, REAL data, PIT or qualification.
#![allow(dead_code)]
#[path = "data.rs"]
pub mod data;
#[path = "../../../../tests/support/runtime.rs"]
mod native_runtime;
use chrono::{DateTime, Utc};
use contracts::{
    control::CommandResult,
    data::{DataValidateRequest, DatasetView},
    execution::{NativeDataQualityReportV1, NativeTaskParametersV1},
    lifecycle::JobLimitsV1,
    research::{DataPartition, InputItemV1, InputPurpose, InputSetCreate},
    runs::RunSnapshotV1,
    runtime::{RuntimeCapabilitiesV1, RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    runtime_jobs::{
        ResultManifestV1, RuntimeInputV1, RuntimeOutputKind, RuntimeOutputV1,
        RuntimeResourceUsageV1, RuntimeResultState,
    },
    DbCounter, Id, Revision, SchemaV1,
};
use integrations::artifacts::ArtifactStore;
use sqlx::PgPool;
use std::sync::Arc;
use store::{
    lifecycle::{
        native::{NativeJob, NativeObjectPublication, NativePayloads},
        ClaimResult, RunLease, RunMessage,
    },
    runtime::ProbePreparation,
    StoreError,
};

pub struct Fixture {
    pub data: data::Fixture,
    pub dataset: DatasetView,
    pub request: DataValidateRequest,
    pub capabilities: RuntimeCapabilitiesV1,
}

pub async fn clock(pool: &PgPool) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn setup(pool: &PgPool) -> Fixture {
    let f = data::setup(pool, None).await;
    prepare(pool, f).await
}

pub async fn prepare(pool: &PgPool, f: data::Fixture) -> Fixture {
    let metadata = data::catalog_fixture::metadata();
    let request = data::request(&f);
    let dataset = data::complete(
        &f,
        data::ticket(&f, "native-task-dataset", &request).await,
        serde_json::to_vec(&metadata).unwrap(),
    )
    .await
    .unwrap()
    .resource;
    let input = f
        .store
        .create_input_set(
            &f.actor,
            "native-task-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.project,
                purpose: InputPurpose::Discovery,
                decision_cutoff: metadata.available_through,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset.id,
                    role: DataPartition::Discovery,
                }],
            },
        )
        .await
        .unwrap()
        .resource;
    let mut capabilities = native_runtime::capabilities(clock(pool).await);
    capabilities
        .artifact_schemas
        .push(contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.data_quality".into(),
            version: "1".into(),
        });
    probe(&f, capabilities.clone()).await;
    let request = DataValidateRequest {
        schema_version: SchemaV1,
        project_id: f.project,
        input_set_id: input.header.id,
        runtime_id: f.runtime.id,
        expected_runtime_revision: f.runtime.revision,
        limits: JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: DbCounter::new(10).unwrap(),
            wall_seconds: 60,
            memory_mib: 512,
            output_bytes: DbCounter::new(65_536).unwrap(),
        },
    };
    Fixture {
        data: f,
        dataset,
        request,
        capabilities,
    }
}

pub async fn probe(f: &data::Fixture, capabilities: RuntimeCapabilitiesV1) {
    let request = RuntimeProbeRequestV1 {
        schema_version: SchemaV1,
        expected_revision: f.runtime.revision,
    };
    let ticket = match f
        .store
        .prepare_runtime_probe(
            &f.actor,
            &format!("probe/{}", Id::new()),
            f.runtime.id,
            &request,
        )
        .await
        .unwrap()
    {
        ProbePreparation::Pending(ticket) => *ticket,
        ProbePreparation::Replay(_) => panic!("new native probe key unexpectedly replayed"),
    };
    let objects = f.objects.clone();
    f.store
        .complete_runtime_probe(
            ticket,
            RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            move |id, bytes| publish_one(objects, NativeObjectPublication { id, bytes }),
        )
        .await
        .unwrap();
}

pub async fn publish_one(
    objects: Arc<ArtifactStore>,
    object: NativeObjectPublication,
) -> Result<(), StoreError> {
    tokio::task::spawn_blocking(move || {
        objects
            .put(object.id, &object.bytes)
            .map_err(|_| StoreError::Integrity)
    })
    .await
    .map_err(|_| StoreError::Integrity)?
}

pub async fn publish(
    objects: Arc<ArtifactStore>,
    batch: Vec<NativeObjectPublication>,
) -> Result<(), StoreError> {
    tokio::task::spawn_blocking(move || {
        for object in batch {
            objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| StoreError::Integrity)?
}

pub async fn start(
    f: &Fixture,
    key: &str,
    request: &DataValidateRequest,
) -> Result<CommandResult<RunSnapshotV1>, StoreError> {
    let read = f.data.objects.clone();
    let write = f.data.objects.clone();
    f.data
        .store
        .start_data_validation(
            &f.data.actor,
            key,
            request,
            move |id, size| data::read(read.clone(), id, size),
            move |object| publish_one(write, object),
        )
        .await
}

pub async fn message(f: &Fixture, run: Id) -> RunMessage {
    f.data
        .store
        .read_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == run)
        .expect("actual native PGMQ message absent")
}

pub async fn lease(f: &Fixture, message: &RunMessage, owner: &str, seconds: u16) -> RunLease {
    match f
        .data
        .store
        .claim_run(message, owner, seconds)
        .await
        .unwrap()
    {
        ClaimResult::Leased(lease) => *lease,
        _ => panic!("expected a new native Run lease"),
    }
}

pub async fn wait_expired(pool: &PgPool, attempt: Id) {
    tokio::time::timeout(std::time::Duration::from_secs(4), async {
        loop {
            let expired: bool = sqlx::query_scalar(
                "SELECT lease_expires_at<=clock_timestamp() FROM app.run_attempts WHERE id=$1",
            )
            .bind(attempt.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
            if expired {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native lease did not expire");
}

pub async fn result(
    pool: &PgPool,
    f: &Fixture,
    job: &NativeJob,
) -> (ResultManifestV1, Vec<(RuntimeOutputV1, Vec<u8>)>) {
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let parameters: NativeTaskParametersV1 = serde_json::from_slice(
        &f.data
            .objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::ValidateData { selections, .. } = parameters else {
        panic!("wrong fixed native operation")
    };
    let mut quality: NativeDataQualityReportV1 = data::catalog_fixture::metadata().quality;
    quality.checked_at = clock(pool).await;
    assert_eq!(selections.len(), 1);
    quality.datasets[0].dataset_revision_id = selections[0].dataset_revision_id;
    quality.datasets[0].selection = selections[0].selection.clone();
    let bytes = serde_json::to_vec(&quality).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::DataQuality,
        schema: job.spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        media_type: "application/json".into(),
    };
    let now = clock(pool).await;
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: job.run.id,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id.clone(),
        input_set_id: job.run.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: f.capabilities.engine_versions.clone(),
        started_at: Some(job.submitted_not_before),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::new(
                (now - job.submitted_not_before).num_milliseconds().max(0) as u64,
            )
            .unwrap(),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: output.byte_count,
        },
        artifacts: vec![output.clone()],
        error: None,
    };
    (manifest, vec![(output, bytes)])
}

pub async fn adopt(
    f: &Fixture,
    lease: &RunLease,
    manifest: &ResultManifestV1,
    outputs: Vec<(RuntimeOutputV1, Vec<u8>)>,
) -> Result<CommandResult<RunSnapshotV1>, StoreError> {
    let reading = f.data.objects.clone();
    let writing = f.data.objects.clone();
    f.data
        .store
        .publish_native_result(
            lease.run.id,
            &lease.fence,
            serde_json::to_vec(manifest).unwrap(),
            NativePayloads::Verified(outputs),
            move |id, size| data::read(reading, id, size),
            move |objects| publish(writing, objects),
        )
        .await
}

pub async fn counts(pool: &PgPool) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.run_native_tasks),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.command_receipts WHERE operation='DATA_VALIDATE'),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.native_task')")
        .fetch_one(pool).await.unwrap()
}
