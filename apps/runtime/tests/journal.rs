//! Real SQLite transactions/reopen/concurrency. Controlled native IDs are not OCI evidence.
#[path = "../../../tests/support/runtime.rs"]
mod protocol;
use contracts::{
    research::ArtifactInputRole, runs::RunKind, runtime::RuntimeCapabilitiesV1, runtime_jobs::*,
    DbCounter, Id, Revision, SchemaV1,
};
use runtime::{journal::Journal, now, Failure};
use std::collections::BTreeMap;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
async fn fixture() -> (tempfile::TempDir, Journal, JobSpecV1, RuntimeCapabilitiesV1) {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(
        &directory.path().join("journal.sqlite"),
        64 * 1024 * 1024,
        4,
    )
    .await
    .unwrap();
    let parameter = Id::new();
    let raw = br#"{"schema_version":1}"#;
    journal.put_object(parameter, "1", raw).await.unwrap();
    let mut capabilities = protocol::capabilities(now());
    capabilities
        .artifact_schemas
        .push(contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.data_quality".into(),
            version: "1".into(),
        });
    let run_id = Id::new();
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run_id, 1).unwrap(),
        job_kind: RunKind::DataValidate,
        image_ref: capabilities.image_refs[0].image_ref.clone(),
        input_set_id: Id::new(),
        inputs: vec![RuntimeInputV1::Artifact {
            artifact_id: parameter,
            storage_version: "1".into(),
            byte_count: count(raw.len() as u64),
            role: ArtifactInputRole::Parameters,
        }],
        parameters_artifact_id: parameter,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: count(1),
            memory_mib: 64,
            wall_seconds: 30,
            output_bytes: count(4096),
        },
        deadline_at: now() + chrono::Duration::seconds(60),
        requested_output_schemas: vec![contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.data_quality".into(),
            version: "1".into(),
        }],
    };
    (directory, journal, spec, capabilities)
}
fn cancellation(spec: &JobSpecV1) -> RuntimeCancelV1 {
    RuntimeCancelV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        owner_epoch: spec.owner_epoch,
    }
}
async fn start(journal: &Journal, spec: &JobSpecV1) -> chrono::DateTime<chrono::Utc> {
    let native = bollard::models::ContainerCreateBody {
        image: Some(spec.image_ref.clone()),
        ..Default::default()
    };
    assert!(journal
        .prepare_launch(&spec.external_job_id, &native)
        .await
        .unwrap());
    journal
        .bind_container(&spec.external_job_id, &"a".repeat(64))
        .await
        .unwrap();
    assert!(journal.start_intent(&spec.external_job_id).await.unwrap());
    assert!(!journal.start_intent(&spec.external_job_id).await.unwrap());
    let started = now();
    journal
        .observe_started(&spec.external_job_id, started)
        .await
        .unwrap();
    started
}
fn completed(
    spec: &JobSpecV1,
    started: chrono::DateTime<chrono::Utc>,
) -> (ResultManifestV1, Vec<(RuntimeOutputV1, Vec<u8>)>) {
    let bytes = br#"{"schema_version":1,"controlled_fixture":true}"#.to_vec();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::DataQuality,
        schema: spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: count(bytes.len() as u64),
        media_type: "application/json".into(),
    };
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        external_job_id: spec.external_job_id.clone(),
        input_set_id: spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: BTreeMap::from([("controlled-fixture".into(), "1".into())]),
        started_at: Some(started),
        finished_at: now(),
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: count(1),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: output.byte_count,
        },
        artifacts: vec![output.clone()],
        error: None,
    };
    (manifest, vec![(output, bytes)])
}

#[tokio::test]
async fn immutable_object_replay_and_native_reopen_keep_exact_original_bytes() {
    let (directory, journal, _, _) = fixture().await;
    let id = Id::new();
    let (first, replay) = journal
        .put_object(id, "native-v7", b"original bytes")
        .await
        .unwrap();
    assert!(!replay);
    let (second, replay) = journal
        .put_object(id, "native-v7", b"original bytes")
        .await
        .unwrap();
    assert!(replay);
    assert_eq!(first, second);
    assert!(matches!(
        journal.put_object(id, "native-v7", b"modified bytes").await,
        Err(Failure::Conflict)
    ));
    assert!(matches!(
        journal.put_object(id, "native-v8", b"original bytes").await,
        Err(Failure::Conflict)
    ));
    let instance = journal.instance_id;
    journal.close().await;
    let reopened = Journal::open(
        &directory.path().join("journal.sqlite"),
        64 * 1024 * 1024,
        4,
    )
    .await
    .unwrap();
    assert_eq!(reopened.instance_id, instance);
    assert_eq!(
        reopened.input_object(id).await.unwrap(),
        ("native-v7".into(), b"original bytes".to_vec())
    );
    reopened.close().await;
}

#[tokio::test]
async fn concurrent_submissions_allocate_one_identity_and_conflicting_specs_never_replace_it() {
    let (_directory, journal, spec, capability) = fixture().await;
    let (a, b) = tokio::join!(
        journal.submit(&spec, &capability),
        journal.submit(&spec, &capability)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.0, b.0);
    assert_ne!(a.1, b.1);
    assert_eq!(journal.pending(10).await.unwrap().len(), 1);
    let mut conflict = spec.clone();
    conflict.limits.memory_mib += 1;
    assert!(matches!(
        journal.submit(&conflict, &capability).await,
        Err(Failure::Conflict)
    ));
    assert_eq!(
        journal
            .get(&spec.external_job_id)
            .await
            .unwrap()
            .spec_json
            .unwrap(),
        serde_json::to_string(&spec).unwrap()
    );
    journal.close().await;
}

#[tokio::test]
async fn pre_submission_cancellation_tombstone_survives_reopen_and_never_admits_late_post() {
    let (directory, journal, spec, capability) = fixture().await;
    let tombstone = journal
        .cancel(&spec.external_job_id, &cancellation(&spec))
        .await
        .unwrap();
    assert_eq!(tombstone.state, RuntimeJobState::Cancelled);
    assert!(!tombstone.has_result);
    journal.close().await;
    let reopened = Journal::open(
        &directory.path().join("journal.sqlite"),
        64 * 1024 * 1024,
        4,
    )
    .await
    .unwrap();
    let (late, replayed) = reopened.submit(&spec, &capability).await.unwrap();
    assert!(replayed);
    assert_eq!(late, tombstone);
    assert!(reopened.pending(10).await.unwrap().is_empty());
    assert!(matches!(
        reopened.result(&spec.external_job_id).await,
        Err(Failure::Missing)
    ));
    reopened.close().await;
}

#[tokio::test]
async fn cancel_owner_is_monotone_and_a_requested_cancellation_is_not_a_terminal_result() {
    let (_directory, journal, spec, capability) = fixture().await;
    journal.submit(&spec, &capability).await.unwrap();
    let mut request = cancellation(&spec);
    request.owner_epoch = request.owner_epoch.next().unwrap();
    let pending = journal
        .cancel(&spec.external_job_id, &request)
        .await
        .unwrap();
    assert_eq!(pending.state, RuntimeJobState::CancelRequested);
    assert!(pending.finished_at.is_none());
    assert!(matches!(
        journal
            .cancel(&spec.external_job_id, &cancellation(&spec))
            .await,
        Err(Failure::StaleOwner)
    ));
    assert!(!journal.start_intent(&spec.external_job_id).await.unwrap());
    assert!(matches!(
        journal.result(&spec.external_job_id).await,
        Err(Failure::NotReady)
    ));
    journal.close().await;
}

#[tokio::test]
async fn output_and_manifest_publish_atomically_and_terminal_replay_cannot_replace_them() {
    let (_directory, journal, spec, capability) = fixture().await;
    journal.submit(&spec, &capability).await.unwrap();
    let started = start(&journal, &spec).await;
    let (manifest, mut outputs) = completed(&spec, started);
    let original = outputs.clone();
    outputs[0].1.push(0);
    assert!(matches!(
        journal
            .finish(&spec.external_job_id, manifest.clone(), outputs)
            .await,
        Err(Failure::Integrity)
    ));
    assert!(matches!(
        journal.result(&spec.external_job_id).await,
        Err(Failure::NotReady)
    ));
    let status = journal
        .finish(&spec.external_job_id, manifest.clone(), original.clone())
        .await
        .unwrap();
    assert_eq!(status.state, RuntimeJobState::Succeeded);
    assert!(journal.pending(10).await.unwrap().is_empty());
    let bytes = journal.result(&spec.external_job_id).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        serde_json::to_value(&manifest).unwrap()
    );
    let stored = journal
        .output(&spec.external_job_id, original[0].0.storage_ref)
        .await
        .unwrap();
    assert_eq!(stored.1, original[0].1);
    let mut other = manifest.clone();
    other.state = RuntimeResultState::Cancelled;
    other.artifacts.clear();
    assert_eq!(
        journal
            .finish(&spec.external_job_id, other, vec![])
            .await
            .unwrap(),
        status
    );
    assert_eq!(
        journal
            .cancel(&spec.external_job_id, &cancellation(&spec))
            .await
            .unwrap(),
        status
    );
    assert_eq!(journal.result(&spec.external_job_id).await.unwrap(), bytes);
    journal.close().await;
}

#[tokio::test]
async fn cancel_and_completion_have_one_terminal_winner_and_no_cancelled_outputs() {
    let (_directory, journal, spec, capability) = fixture().await;
    journal.submit(&spec, &capability).await.unwrap();
    let started = start(&journal, &spec).await;
    let (manifest, outputs) = completed(&spec, started);
    let request = cancellation(&spec);
    let (cancel, done) = tokio::join!(
        journal.cancel(&spec.external_job_id, &request),
        journal.finish(&spec.external_job_id, manifest.clone(), outputs.clone())
    );
    cancel.unwrap();
    let done = match done {
        Ok(done) => done,
        Err(Failure::NotReady) => {
            assert_eq!(
                journal
                    .get(&spec.external_job_id)
                    .await
                    .unwrap()
                    .status()
                    .unwrap()
                    .state,
                RuntimeJobState::CancelRequested
            );
            assert!(matches!(
                journal.result(&spec.external_job_id).await,
                Err(Failure::NotReady)
            ));
            // Relational proof fixture only. Separate Docker tests establish that
            // the native non-running barrier actually blocks delayed CREATE/START.
            journal
                .bind_barrier(&spec.external_job_id, &"b".repeat(64))
                .await
                .unwrap();
            journal
                .finish(&spec.external_job_id, manifest, outputs)
                .await
                .unwrap()
        }
        Err(_) => panic!("unexpected native journal completion failure"),
    };
    let latest = journal
        .get(&spec.external_job_id)
        .await
        .unwrap()
        .status()
        .unwrap();
    assert_eq!(latest, done);
    assert!(latest.state.is_terminal());
    let result: ResultManifestV1 =
        serde_json::from_slice(&journal.result(&spec.external_job_id).await.unwrap()).unwrap();
    if latest.state == RuntimeJobState::Cancelled {
        assert!(result.artifacts.is_empty());
        assert_eq!(result.resource_usage.output_bytes.get(), 0);
    } else {
        assert_eq!(latest.state, RuntimeJobState::Succeeded);
        assert_eq!(result.artifacts.len(), 1);
    }
    journal.close().await;
}

#[tokio::test]
async fn configured_pending_limit_rejects_new_jobs_without_leaving_half_admissions() {
    let (directory, journal, spec, capability) = fixture().await;
    journal.close().await;
    let bounded = Journal::open(
        &directory.path().join("journal.sqlite"),
        64 * 1024 * 1024,
        1,
    )
    .await
    .unwrap();
    bounded.submit(&spec, &capability).await.unwrap();
    let mut other = spec.clone();
    other.run_id = Id::new();
    other.external_job_id = domain::runtime_jobs::external_id(other.run_id, 1).unwrap();
    assert!(matches!(
        bounded.submit(&other, &capability).await,
        Err(Failure::Busy)
    ));
    assert!(matches!(
        bounded.get(&other.external_job_id).await,
        Err(Failure::Missing)
    ));
    assert!(bounded.submit(&spec, &capability).await.unwrap().1);
    bounded.close().await;
}
