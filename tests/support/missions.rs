//! Formal startup with a controlled native DATA_QUALITY receipt, never real market/OCI evidence.
#![allow(dead_code)]
use super::{cycle_support, research_support, runtime_support};
use contracts::{
    catalogs::RuntimeCatalogMetadataV1, execution::NativeTaskParametersV1, runs::RunState,
    runtime_jobs::*, DbCounter, Id, Revision, SchemaV1,
};
use sqlx::PgPool;
use store::{
    authority::Actor,
    lifecycle::{native::NativePayloads, ClaimResult},
    Store, StoreError,
};

pub async fn setup(pool: &PgPool) -> (Store, Actor, cycle_support::Fixture, Id, Id) {
    setup_with_cost(pool, false).await
}

pub async fn setup_with_cost(
    pool: &PgPool,
    priced: bool,
) -> (Store, Actor, cycle_support::Fixture, Id, Id) {
    let (store, actor) = research_support::operator(pool).await;
    let f = cycle_support::setup(pool, &store, &actor).await;
    start(store, actor, f, priced).await
}

pub async fn start(
    store: Store,
    actor: Actor,
    mut f: cycle_support::Fixture,
    priced: bool,
) -> (Store, Actor, cycle_support::Fixture, Id, Id) {
    if priced {
        let mut content = f.brief.content.clone();
        content.budget.max_cost_decimal = Some("10".parse().unwrap());
        content.budget.cost_currency = Some("USD".into());
        content.budget.cost_enforcement = contracts::budget::CostEnforcement::Estimated;
        f.brief = store
            .update_brief(
                &actor,
                "cost-capped-native-fixture",
                f.brief.id,
                &contracts::brief::BriefUpdate {
                    schema_version: SchemaV1,
                    expected_revision: f.brief.revision,
                    content,
                    bindings: f.brief.bindings.clone(),
                },
            )
            .await
            .unwrap()
            .resource;
        f.freeze.expected_revision = f.brief.revision;
    }
    store
        .freeze_brief(&actor, "freeze", f.brief.id, &f.freeze)
        .await
        .unwrap();
    let request = cycle_support::start_request(&store, &actor, &f).await;
    let started = f
        .start(&store, &actor, "start", &request)
        .await
        .unwrap()
        .resource;
    (store, actor, f, started.cycle.id, started.run.id)
}

pub async fn complete(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    run: Id,
    invalid: bool,
) {
    assert!(
        !store.advance_initial_cycle(run).await.unwrap(),
        "queued preparation cannot admit a Mission"
    );
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == run)
        .unwrap();
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "cycle-preparation-fixture", 60)
        .await
        .unwrap()
    else {
        panic!("native lease required");
    };
    let job = store.native_job(run, &lease.fence).await.unwrap();
    assert!(store.begin_run_dispatch(run, &lease.fence).await.unwrap());
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
    let NativeTaskParametersV1::ValidateData { selections, .. } = serde_json::from_slice(
        &f.objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap() else {
        panic!("fixed validation required");
    };
    let selection = &selections[0];
    let (metadata, bytes): (uuid::Uuid, i64) = sqlx::query_as("SELECT e.native_metadata_artifact_id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1")
        .bind(selection.dataset_revision_id.as_uuid()).fetch_one(pool).await.unwrap();
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(
        &f.objects
            .read(
                metadata.to_string().try_into().unwrap(),
                DbCounter::new(bytes as u64).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut quality = metadata.quality;
    quality.checked_at = now;
    quality.datasets[0].dataset_revision_id = selection.dataset_revision_id;
    quality.datasets[0].selection = selection.selection.clone();
    if invalid {
        quality.datasets.clear();
    }
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
        run_id: run,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id,
        input_set_id: job.run.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: runtime_support::capabilities(now).engine_versions,
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
    let reading = f.objects.clone();
    let publishing = f.objects.clone();
    let result = store.publish_native_result(run,&lease.fence,serde_json::to_vec(&manifest).unwrap(),NativePayloads::Verified(vec![(output,bytes)]),
        move |id,size| async move { reading.read(id,size).map_err(|_|StoreError::Integrity) },
        move |batch| async move {
            for object in batch { publishing.put(object.id,&object.bytes).map_err(|_|StoreError::Integrity)?; }
            Ok(())
        }).await.unwrap();
    assert_eq!(
        result.resource.state,
        if invalid {
            RunState::Failed
        } else {
            RunState::Succeeded
        }
    );
}
