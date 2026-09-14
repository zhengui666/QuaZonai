//! Controlled Forward scientific protocol, reused by Store and actual Worker tests.
//! This is not numerical/OCI execution or a production market observation.
use chrono::{DateTime, Utc};
use contracts::{forward::*, runtime::RuntimeCapabilitiesV1, DbCounter, Id, SchemaV1};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use store::{
    lifecycle::{native::NativeJob, RunMessage},
    Store, StoreError,
};
fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}

pub async fn complete(
    pool: &PgPool,
    store: &Store,
    run: Id,
    caps: &RuntimeCapabilitiesV1,
    objects: &Arc<Mutex<BTreeMap<Id, Vec<u8>>>>,
    mean: f64,
) -> (RunMessage, NativeJob, NativeForwardRequestV1, DateTime<Utc>) {
    let read = |id: Id, size: DbCounter| {
        let bytes = objects.lock().unwrap().get(&id).cloned();
        async move {
            let bytes = bytes.ok_or(StoreError::NotFound)?;
            assert_eq!(bytes.len() as u64, size.get());
            Ok(bytes)
        }
    };
    let messages = store.read_run_messages(60, 100).await.unwrap();
    let queued = messages.iter().find(|m| m.run_id == run).unwrap();
    let store::lifecycle::ClaimResult::Leased(lease) =
        store.claim_run(queued, "measurement", 60).await.unwrap()
    else {
        panic!("measurement lease")
    };
    let job = store.native_job(run, &lease.fence).await.unwrap();
    let definition: contracts::execution::NativeTaskParametersV1 =
        serde_json::from_slice(&objects.lock().unwrap()[&job.spec.parameters_artifact_id]).unwrap();
    // Controlled native result protocol, not a claim of numerical/OCI execution.
    use contracts::runtime_jobs::*;
    use contracts::science::{NativeStatisticGroup, NativeStatisticV1};
    let contracts::execution::NativeTaskParametersV1::EvaluateForward { request, .. } = &definition
    else {
        panic!("forward")
    };
    let result = NativeForwardResultV1 {
        schema_version: SchemaV1,
        native_version: "0.63.0".into(),
        window: request.window.clone(),
        statistics: [
            "Average (Return)",
            "Returns Volatility (365 days)",
            "Sharpe Ratio (365 days)",
        ]
        .into_iter()
        .map(|key| NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key: key.into(),
            currency: None,
            value: Some(if key == "Average (Return)" { mean } else { 0.1 }),
            reason_code: None,
        })
        .collect(),
    };
    let bytes = serde_json::to_vec(&result).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::Report,
        schema: job.spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: contracts::Revision::INITIAL,
        byte_count: count(bytes.len() as u64),
        media_type: "application/json".into(),
    };
    assert!(store
        .begin_run_dispatch(job.run.id, &lease.fence)
        .await
        .unwrap());
    let now: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    store
        .observe_native_accepted(
            job.run.id,
            &lease.fence,
            &RuntimeJobStatusV1 {
                schema_version: SchemaV1,
                run_id: job.run.id,
                attempt_no: job.spec.attempt_no,
                external_job_id: job.spec.external_job_id.clone(),
                state: RuntimeJobState::Accepted,
                has_result: false,
                submitted_at: now,
                started_at: None,
                finished_at: None,
            },
        )
        .await
        .unwrap();
    let finished: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: job.run.id,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id.clone(),
        input_set_id: job.run.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: caps.engine_versions.clone(),
        started_at: Some(now),
        finished_at: finished,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: count((finished - now).num_milliseconds().max(0) as u64),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: output.byte_count,
        },
        artifacts: vec![output.clone()],
        error: None,
    };
    store
        .publish_native_result(
            job.run.id,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            store::lifecycle::native::NativePayloads::Verified(vec![(output, bytes)]),
            read,
            |values| {
                for value in values {
                    objects.lock().unwrap().insert(value.id, value.bytes);
                }
                async { Ok(()) }
            },
        )
        .await
        .unwrap();
    let request = request.as_ref().clone();
    (queued.clone(), job, request, finished)
}
