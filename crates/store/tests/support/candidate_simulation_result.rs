//! Controlled intraday protocol receipt, not native execution or a PASS claim.
use super::*;
use contracts::{runtime_jobs::*, science::*};
use store::lifecycle::native::{NativeJob, NativePayloads};

pub(super) async fn complete(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    job: &NativeJob,
) {
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
    let NativeTaskParametersV1::SimulateCandidate {
        dataset_revision_id,
        source_selection,
        request,
        ..
    } = serde_json::from_slice(&f.read(job.spec.parameters_artifact_id, size).await.unwrap())
        .unwrap()
    else {
        panic!("original Candidate task");
    };
    let (metadata,size):(uuid::Uuid,i64)=sqlx::query_as("SELECT a.id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1").bind(dataset_revision_id.as_uuid()).fetch_one(pool).await.unwrap();
    let metadata: contracts::catalogs::RuntimeCatalogMetadataV1 = serde_json::from_slice(
        &f.read(
            metadata.to_string().try_into().unwrap(),
            DbCounter::new(size as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut quality = metadata.quality;
    quality.checked_at = now;
    quality.datasets[0].dataset_revision_id = dataset_revision_id;
    quality.datasets[0].selection = source_selection;
    let result = intraday(&request);
    let outputs: Vec<_> = job
        .spec
        .requested_output_schemas
        .iter()
        .map(|schema| {
            let bytes = match schema.name.as_str() {
                "qz.data_quality" => serde_json::to_vec(&quality).unwrap(),
                "qz.native_simulation" => serde_json::to_vec(&result).unwrap(),
                _ => panic!("fixed outputs"),
            };
            let output = RuntimeOutputV1 {
                kind: native_output_contract(&schema.name, &schema.version)
                    .unwrap()
                    .kind,
                schema: schema.clone(),
                storage_ref: Id::new(),
                storage_version: contracts::Revision::INITIAL,
                byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
                media_type: "application/json".into(),
            };
            (output, bytes)
        })
        .collect();
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: lease.run.id,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id.clone(),
        input_set_id: job.spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: [
            ("candidate-simulation".into(), "2".into()),
            ("nautilus".into(), "0.63.0".into()),
        ]
        .into(),
        started_at: Some(now),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::ZERO,
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: DbCounter::new(outputs.iter().map(|(o, _)| o.byte_count.get()).sum())
                .unwrap(),
        },
        artifacts: outputs.iter().map(|(o, _)| o.clone()).collect(),
        error: None,
    };
    store
        .publish_native_result(
            lease.run.id,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            NativePayloads::Verified(outputs),
            |id, size| f.read(id, size),
            |batch| {
                std::future::ready(batch.into_iter().try_for_each(|object| {
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }))
            },
        )
        .await
        .unwrap();
}

pub(super) fn intraday(request: &NativeSimulationRequestV1) -> NativeSimulationResultV1 {
    let start = request.selection.event_start_ns;
    let end = DbCounter::new(request.selection.event_end_ns.get() - 1).unwrap();
    let currency = &request.settings.base_currency;
    let amount = format!(
        "{} {currency}",
        request.settings.starting_capital.as_decimal()
    );
    let (variant, account_type) = match request.settings.account_kind {
        NativeAccountKind::Cash => ("Cash", "CASH"),
        NativeAccountKind::Margin => ("Margin", "MARGIN"),
    };
    let summary = serde_json::json!({"venues.total":"1","orders.open":"0","orders.inflight":"0"});
    serde_json::from_value(serde_json::json!({
        "schema_version":1,"native_version":"0.63.0","iterations":"1","events":"0","orders":"0","positions":"0","consumed_target_points":request.target_points.len().to_string(),"summary":summary,
        "statistics":[
            {"group":"RETURNS","native_key":"Average (Return)","currency":null,"value":null,"reason_code":"NATIVE_STATISTIC_UNAVAILABLE"},
            {"group":"RETURNS","native_key":"Returns Volatility (252 days)","currency":null,"value":null,"reason_code":"NATIVE_STATISTIC_UNAVAILABLE"},
            {"group":"RETURNS","native_key":"Sharpe Ratio (252 days)","currency":null,"value":null,"reason_code":"NATIVE_STATISTIC_UNAVAILABLE"}
        ],"returns_kind":"PORTFOLIO_DAILY","returns_status":"INSUFFICIENT_DATA","returns_reason":"PORTFOLIO_DAILY_RETURNS_UNAVAILABLE","returns":[],
        "canonical_result":{"schema":"nautilus-backtest-result/v1","summary":summary,
            "run":{"outcome":"completed","iterations":"1","total_events":"0","total_orders":"0","total_positions":"0","backtest_start_ns":start,"backtest_end_ns":end},
            "accounts":[{(variant):{"base":{"id":"SIM-001","account_type":account_type,"base_currency":currency,"balances_starting":{(currency):amount}}}}],
            "portfolio_snapshots":[{"account_id":"SIM-001","account_type":account_type,"base_currency":currency,"ts_event":start,"total_equity":[amount]}]
        }
    })).unwrap()
}
