//! Actual SQLite and filesystem recovery; no fixture is presented as native OCI execution.
#[path = "../../../tests/support/runtime.rs"]
mod protocol;
use contracts::{
    execution::NativeTaskParametersV1, research::ArtifactInputRole, runs::RunKind,
    runtime::RuntimeCapabilitiesV1, runtime_jobs::*, DbCounter, Id, Revision, SchemaV1,
};
use runtime::{files::RuntimeRoot, journal::Journal, materialize, now, Failure};
use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};

struct Fixture {
    _directory: tempfile::TempDir,
    root: Arc<RuntimeRoot>,
    journal: Journal,
    spec: JobSpecV1,
    capability: RuntimeCapabilitiesV1,
    parameters: Vec<u8>,
}
async fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let root = Arc::new(RuntimeRoot::open(&directory.path().join("runtime")).unwrap());
    let journal = Journal::open(&root.path.join("journal.sqlite"), 64 * 1024 * 1024, 16)
        .await
        .unwrap();
    let parameters = NativeTaskParametersV1::BuildPortfolio {
        schema_version: SchemaV1,
        request: Box::new(
            serde_json::from_str(include_str!(
                "../../../tests/contracts/allocation-input.json"
            ))
            .unwrap(),
        ),
    };
    let schemas = parameters.output_schemas();
    let bytes = serde_json::to_vec(&parameters).unwrap();
    let parameter_id = Id::new();
    journal.put_object(parameter_id, "1", &bytes).await.unwrap();
    let mut capability = protocol::capabilities(now());
    capability.job_kinds = vec![RunKind::PortfolioBuild];
    capability.image_refs.truncate(1);
    capability.image_refs[0].job_kind = RunKind::PortfolioBuild;
    capability.artifact_schemas = schemas.clone();
    let run_id = Id::new();
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run_id, 1).unwrap(),
        job_kind: RunKind::PortfolioBuild,
        image_ref: capability.image_refs[0].image_ref.clone(),
        input_set_id: Id::new(),
        inputs: vec![RuntimeInputV1::Artifact {
            artifact_id: parameter_id,
            storage_version: "1".into(),
            byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
            role: ArtifactInputRole::Parameters,
        }],
        parameters_artifact_id: parameter_id,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: DbCounter::new(1).unwrap(),
            memory_mib: 64,
            wall_seconds: 30,
            output_bytes: DbCounter::new(4096).unwrap(),
        },
        deadline_at: now() + chrono::Duration::seconds(60),
        requested_output_schemas: schemas,
    };
    Fixture {
        _directory: directory,
        root,
        journal,
        spec,
        capability,
        parameters: bytes,
    }
}
async fn finish_unstarted(f: &Fixture) -> Vec<u8> {
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: f.spec.run_id,
        attempt_no: 1,
        external_job_id: f.spec.external_job_id.clone(),
        input_set_id: f.spec.input_set_id,
        state: RuntimeResultState::Failed,
        engine_versions: [("controlled-fixture".into(), "1".into())].into(),
        started_at: None,
        finished_at: now(),
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::ZERO,
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: DbCounter::ZERO,
        },
        artifacts: Vec::new(),
        error: Some(domain::runtime_jobs::error(
            RuntimeFailureCode::InvalidInput,
        )),
    };
    f.journal
        .finish(&f.spec.external_job_id, manifest, vec![])
        .await
        .unwrap();
    f.journal.result(&f.spec.external_job_id).await.unwrap()
}

#[tokio::test]
async fn repeated_materialization_reuses_one_quota_reservation_and_one_immutable_copy() {
    let f = fixture().await;
    f.journal.submit(&f.spec, &f.capability).await.unwrap();
    let first = materialize::inputs(f.root.clone(), &f.journal, &f.spec, &[])
        .await
        .unwrap();
    let expected = f.parameters.len() as u64
        + serde_json::to_vec(&f.spec).unwrap().len() as u64
        + f.spec.limits.output_bytes.get()
        + domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES as u64;
    assert_eq!(
        f.journal
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        Some(expected)
    );
    let again = materialize::inputs(f.root.clone(), &f.journal, &f.spec, &[])
        .await
        .unwrap();
    assert_eq!(again.output, first.output);
    assert_eq!(
        f.journal
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        Some(expected)
    );
    assert_eq!(fs::read_dir(f.root.path.join("jobs")).unwrap().count(), 1);
    assert_eq!(
        fs::read_dir(f.root.path.join("staging")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read(
            f.root
                .job(f.spec.run_id, 1)
                .join("input/objects")
                .join(f.spec.parameters_artifact_id.to_string())
        )
        .unwrap(),
        f.parameters
    );
    let result = finish_unstarted(&f).await;
    assert_eq!(
        materialize::cleanup_terminal(f.root.clone(), &f.journal)
            .await
            .unwrap(),
        1
    );
    assert!(!f.root.job(f.spec.run_id, 1).exists());
    assert_eq!(
        f.journal
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        f.journal.result(&f.spec.external_job_id).await.unwrap(),
        result
    );
    assert_eq!(
        f.journal
            .input_object(f.spec.parameters_artifact_id)
            .await
            .unwrap()
            .1,
        f.parameters
    );
    assert!(f.journal.submit(&f.spec, &f.capability).await.unwrap().1);
    f.journal.close().await;
}

#[tokio::test]
async fn disk_quota_includes_native_copy_and_eventual_sqlite_output_without_creating_partial_files()
{
    let mut f = fixture().await;
    f.spec.limits.output_bytes = DbCounter::new(40 * 1024 * 1024).unwrap();
    assert!(matches!(
        f.journal.submit(&f.spec, &f.capability).await,
        Err(Failure::Capacity)
    ));
    assert!(matches!(
        f.journal.get(&f.spec.external_job_id).await,
        Err(Failure::Missing)
    ));
    assert!(f.journal.scheduling().await.unwrap().is_empty());
    assert_eq!(
        f.journal
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        None
    );
    assert_eq!(fs::read_dir(f.root.path.join("jobs")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(f.root.path.join("staging")).unwrap().count(),
        0
    );
    assert_eq!(
        f.journal
            .input_object(f.spec.parameters_artifact_id)
            .await
            .unwrap()
            .1,
        f.parameters
    );
    f.journal.close().await;
}

#[tokio::test]
async fn concurrent_admission_reserves_disk_once_and_replays_do_not_charge_again() {
    let mut f = fixture().await;
    f.spec.limits.output_bytes = DbCounter::new(22 * 1024 * 1024).unwrap();
    let mut other = f.spec.clone();
    other.run_id = Id::new();
    other.external_job_id = domain::runtime_jobs::external_id(other.run_id, 1).unwrap();
    let (a, b) = tokio::join!(
        f.journal.submit(&f.spec, &f.capability),
        f.journal.submit(&other, &f.capability)
    );
    let (accepted, rejected) = match (a, b) {
        (Ok((_, false)), Err(Failure::Capacity)) => (&f.spec, &other),
        (Err(Failure::Capacity), Ok((_, false))) => (&other, &f.spec),
        _ => panic!("native disk reservations must have one admission winner"),
    };
    let reserved = f
        .journal
        .materialization_bytes(&accepted.external_job_id)
        .await
        .unwrap()
        .expect("accepted jobs already reserve their filesystem copies");
    assert_eq!(f.journal.scheduling().await.unwrap().len(), 1);
    assert!(matches!(
        f.journal.get(&rejected.external_job_id).await,
        Err(Failure::Missing)
    ));
    assert_eq!(
        f.journal
            .materialization_bytes(&rejected.external_job_id)
            .await
            .unwrap(),
        None
    );
    assert!(f.journal.submit(accepted, &f.capability).await.unwrap().1);
    assert_eq!(
        f.journal.reserve_materialization(accepted).await.unwrap(),
        reserved
    );
    assert_eq!(
        f.journal
            .materialization_bytes(&accepted.external_job_id)
            .await
            .unwrap(),
        Some(reserved)
    );
    assert_eq!(fs::read_dir(f.root.path.join("jobs")).unwrap().count(), 0);
    f.journal.close().await;
    let reopened = Journal::open(&f.root.path.join("journal.sqlite"), 64 * 1024 * 1024, 16)
        .await
        .unwrap();
    assert!(reopened.submit(accepted, &f.capability).await.unwrap().1);
    assert_eq!(
        reopened
            .materialization_bytes(&accepted.external_job_id)
            .await
            .unwrap(),
        Some(reserved)
    );
    materialize::inputs(f.root.clone(), &reopened, accepted, &[])
        .await
        .unwrap();
    assert_eq!(
        reopened
            .materialization_bytes(&accepted.external_job_id)
            .await
            .unwrap(),
        Some(reserved)
    );
    reopened.close().await;
}

#[tokio::test]
async fn admission_at_exact_disk_quota_succeeds_and_one_extra_byte_rolls_back() {
    let mut f = fixture().await;
    // The actual serialized request participates in the quota; do not estimate
    // its size or discard a requested input to make the job appear admissible.
    f.spec.limits.output_bytes = DbCounter::new(32 * 1024 * 1024).unwrap();
    let materialized = f.parameters.len() as u64
        + serde_json::to_vec(&f.spec).unwrap().len() as u64
        + f.spec.limits.output_bytes.get()
        + domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES as u64;
    let exact = f.parameters.len() as u64 + f.spec.limits.output_bytes.get() + materialized;
    f.journal.close().await;
    let too_small = Journal::open(&f.root.path.join("journal.sqlite"), exact - 1, 16)
        .await
        .unwrap();
    assert!(matches!(
        too_small.submit(&f.spec, &f.capability).await,
        Err(Failure::Capacity)
    ));
    assert!(too_small.scheduling().await.unwrap().is_empty());
    assert_eq!(
        too_small
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        None
    );
    too_small.close().await;
    let sufficient = Journal::open(&f.root.path.join("journal.sqlite"), exact, 16)
        .await
        .unwrap();
    assert!(!sufficient.submit(&f.spec, &f.capability).await.unwrap().1);
    assert_eq!(
        sufficient
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        Some(materialized)
    );
    materialize::inputs(f.root.clone(), &sufficient, &f.spec, &[])
        .await
        .unwrap();
    assert!(matches!(
        sufficient.put_object(Id::new(), "1", b"x").await,
        Err(Failure::Capacity)
    ));
    assert!(sufficient.submit(&f.spec, &f.capability).await.unwrap().1);
    sufficient.close().await;
}

#[tokio::test]
async fn interrupted_staging_before_spec_write_recovers_from_the_exact_durable_reservation() {
    let f = fixture().await;
    f.journal.submit(&f.spec, &f.capability).await.unwrap();
    let reserved = f.journal.reserve_materialization(&f.spec).await.unwrap();
    let staging = f
        .root
        .path
        .join("staging")
        .join(format!("{}-1", f.spec.run_id));
    fs::create_dir(&staging).unwrap();
    fs::create_dir(staging.join("input")).unwrap();
    fs::write(staging.join("input/partial"), b"interrupted private copy").unwrap();
    f.journal.close().await;
    let reopened = Journal::open(&f.root.path.join("journal.sqlite"), 64 * 1024 * 1024, 16)
        .await
        .unwrap();
    materialize::recover(f.root.clone(), &reopened)
        .await
        .unwrap();
    assert!(!staging.exists());
    assert_eq!(
        reopened
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        Some(reserved)
    );
    materialize::inputs(f.root.clone(), &reopened, &f.spec, &[])
        .await
        .unwrap();
    assert_eq!(fs::read_dir(f.root.path.join("jobs")).unwrap().count(), 1);
    reopened.close().await;
}

#[tokio::test]
async fn interrupted_terminal_cache_deletion_cannot_release_quota_early_or_destroy_durable_result()
{
    let f = fixture().await;
    f.journal.submit(&f.spec, &f.capability).await.unwrap();
    materialize::inputs(f.root.clone(), &f.journal, &f.spec, &[])
        .await
        .unwrap();
    let saved = finish_unstarted(&f).await;
    let staging = f
        .root
        .path
        .join("staging")
        .join(format!("{}-1", f.spec.run_id));
    fs::rename(f.root.job(f.spec.run_id, 1), &staging).unwrap();
    fs::set_permissions(staging.join("input"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_file(staging.join("input/spec.json")).unwrap();
    assert!(f
        .journal
        .materialization_bytes(&f.spec.external_job_id)
        .await
        .unwrap()
        .is_some());
    f.journal.close().await;
    let reopened = Journal::open(&f.root.path.join("journal.sqlite"), 64 * 1024 * 1024, 16)
        .await
        .unwrap();
    materialize::recover(f.root.clone(), &reopened)
        .await
        .unwrap();
    assert!(!staging.exists());
    assert_eq!(
        reopened
            .materialization_bytes(&f.spec.external_job_id)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        reopened.result(&f.spec.external_job_id).await.unwrap(),
        saved
    );
    assert_eq!(
        reopened
            .input_object(f.spec.parameters_artifact_id)
            .await
            .unwrap()
            .1,
        f.parameters
    );
    reopened.close().await;
}
