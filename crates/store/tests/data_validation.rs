//! Formal native DATA_VALIDATE admission through real PostgreSQL/PGMQ and files.
#[path = "support/native_tasks.rs"]
mod support;
use contracts::{
    execution::NativeTaskParametersV1,
    research::{DataPartition, InputItemV1, InputPurpose, InputSetCreate},
    runs::{RunKind, RunState},
    runtime_jobs::RuntimeInputV1,
    DbCounter, SchemaV1,
};
use sqlx::PgPool;
use store::StoreError;
use support::*;

#[sqlx::test(migrations = "../../migrations")]
async fn native_validation_commits_parameters_definition_run_queue_and_one_original_receipt(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0));
    let (left, right) = tokio::join!(
        start(&f, "validate-once", &f.request),
        start(&f, "validate-once", &f.request)
    );
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_ne!(left.replayed, right.replayed);
    assert_eq!(left.resource, right.resource);
    assert_eq!(left.resource.kind, RunKind::DataValidate);
    assert_eq!(left.resource.state, RunState::Queued);
    assert!(left.resource.cycle_id.is_none());
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1));
    let msg = message(&f, left.resource.id).await;
    let lease = lease(&f, &msg, "native-inspector", 30).await;
    let job = f
        .data
        .store
        .native_job(left.resource.id, &lease.fence)
        .await
        .unwrap();
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
    let document: NativeTaskParametersV1 = serde_json::from_slice(
        &f.data
            .objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::ValidateData { selections, .. } = document else {
        panic!("arbitrary job operation was admitted")
    };
    assert_eq!(selections.len(), 1);
    assert_eq!(selections[0].dataset_revision_id, f.dataset.id);
    assert_eq!(
        selections[0].selection.decision_cutoff_ns,
        data::catalog_fixture::count(300_000_000_000)
    );
    assert_eq!(job.spec.limits.cpu, 1);
    let empty: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.experiments),(SELECT count(*) FROM app.evaluations),(SELECT count(*) FROM app.qualifications)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(empty, (0, 0, 0));
    let mut different = f.request.clone();
    different.limits.cpu_seconds = DbCounter::new(11).unwrap();
    assert!(matches!(
        start(&f, "validate-once", &different).await,
        Err(StoreError::IdempotencyConflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn later_project_cutoff_never_widens_the_registered_snapshot_visibility(pool: PgPool) {
    let f = setup(&pool).await;
    let input = f
        .data
        .store
        .create_input_set(
            &f.data.actor,
            "later-cutoff",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                purpose: InputPurpose::Discovery,
                decision_cutoff: data::catalog_fixture::instant(400),
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: f.dataset.id,
                    role: DataPartition::Discovery,
                }],
            },
        )
        .await
        .unwrap()
        .resource;
    let mut request = f.request.clone();
    request.input_set_id = input.header.id;
    let run = start(&f, "bounded-old-snapshot", &request)
        .await
        .unwrap()
        .resource;
    let msg = message(&f, run.id).await;
    let lease = lease(&f, &msg, "scope-owner", 30).await;
    let job = f.data.store.native_job(run.id, &lease.fence).await.unwrap();
    let (_, bytes) = job
        .spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } => Some((*artifact_id, *byte_count)),
            _ => None,
        })
        .unwrap();
    let document: NativeTaskParametersV1 = serde_json::from_slice(
        &f.data
            .objects
            .read(job.spec.parameters_artifact_id, bytes)
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::ValidateData { selections, .. } = document else {
        panic!("wrong operation")
    };
    assert_eq!(
        selections[0].selection.decision_cutoff_ns.get(),
        300_000_000_000
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cpu_shape_and_native_capacity_are_checked_before_parameter_publication_or_queue_admission(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    let mut too_large = f.request.clone();
    too_large.limits.cpu_seconds = DbCounter::new(
        u64::from(f.capabilities.max_cpu + 1) * u64::from(too_large.limits.wall_seconds),
    )
    .unwrap();
    assert!(matches!(
        start(&f, "cpu-capacity", &too_large).await,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("native_cpu_capacity")
        ))
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0));
    let mut two_cpus = f.request.clone();
    two_cpus.limits.cpu_seconds = DbCounter::new(61).unwrap();
    let run = start(&f, "cpu-capacity", &two_cpus).await.unwrap().resource;
    let msg = message(&f, run.id).await;
    let lease = lease(&f, &msg, "two-cpu-owner", 30).await;
    let job = f.data.store.native_job(run.id, &lease.fence).await.unwrap();
    assert_eq!(job.spec.limits.cpu, 2);
    domain::runtime_jobs::spec_shape(&job.spec).unwrap();
    let mut forbidden_trial = f.request.clone();
    forbidden_trial.limits.experiments = 1;
    assert!(start(&f, "not-a-research-trial", &forbidden_trial)
        .await
        .is_err());
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn sealed_input_is_not_read_by_the_general_data_validation_command(pool: PgPool) {
    let f = setup(&pool).await;
    let mut metadata = data::catalog_fixture::metadata();
    metadata.partition = DataPartition::Sealed;
    metadata.storage_version = "sealed-fixture-v1".into();
    metadata.native_snapshot_ref = "controlled-sealed-snapshot".into();
    let mut registration = data::request(&f.data);
    registration.native_storage_version = metadata.storage_version.clone();
    let dataset = data::complete(
        &f.data,
        data::ticket(&f.data, "sealed-register", &registration).await,
        serde_json::to_vec(&metadata).unwrap(),
    )
    .await
    .unwrap()
    .resource;
    let input = f
        .data
        .store
        .create_input_set(
            &f.data.actor,
            "sealed-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                purpose: InputPurpose::Sealed,
                decision_cutoff: metadata.available_through,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset.id,
                    role: DataPartition::Sealed,
                }],
            },
        )
        .await
        .unwrap()
        .resource;
    let mut request = f.request.clone();
    request.input_set_id = input.header.id;
    let error = f
        .data
        .store
        .start_data_validation(
            &f.data.actor,
            "no-sealed-backdoor",
            &request,
            |_, _| async { panic!("general validation must not read sealed metadata or rows") },
            |_| async { panic!("general validation must not allocate sealed task parameters") },
        )
        .await;
    assert!(matches!(
        error,
        Err(StoreError::Domain(domain::DomainError::Fields(_)))
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn receipt_failure_rolls_back_all_native_admission_and_only_unreferenced_parameter_is_reclaimed(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION app.test_reject_data_validate() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='DATA_VALIDATE' THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='controlled receipt failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_reject_data_validate BEFORE INSERT ON app.command_receipts FOR EACH ROW EXECUTE FUNCTION app.test_reject_data_validate();")
        .execute(&pool).await.unwrap();
    let mut allocated = None;
    let reading = f.data.objects.clone();
    let writing = f.data.objects.clone();
    let failed = f
        .data
        .store
        .start_data_validation(
            &f.data.actor,
            "transaction-retry",
            &f.request,
            move |id, size| data::read(reading.clone(), id, size),
            |object| {
                allocated = Some(object.id);
                publish_one(writing, object)
            },
        )
        .await;
    assert!(failed.is_err());
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0));
    let allocated =
        allocated.expect("file publication must precede the deliberately failed final receipt");
    let objects = f.data.objects.clone();
    assert!(f
        .data
        .store
        .discard_unpublished_operator_artifact(allocated, move |id| async move {
            objects
                .discard_unpublished(id)
                .map_err(|_| StoreError::Integrity)
        })
        .await
        .unwrap());
    assert!(!f
        .data
        .directory
        .path()
        .join("objects")
        .join(allocated.to_string())
        .exists());
    sqlx::raw_sql("DROP TRIGGER test_reject_data_validate ON app.command_receipts; DROP FUNCTION app.test_reject_data_validate();")
        .execute(&pool).await.unwrap();
    let run = start(&f, "transaction-retry", &f.request)
        .await
        .unwrap()
        .resource;
    assert_eq!(run.state, RunState::Queued);
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn three_concurrent_administrative_validations_cannot_exceed_two_native_slots(pool: PgPool) {
    let f = setup(&pool).await;
    let (a, b, c) = tokio::join!(
        start(&f, "slot-a", &f.request),
        start(&f, "slot-b", &f.request),
        start(&f, "slot-c", &f.request)
    );
    let results = [a, b, c];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 2);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(
                r,
                Err(StoreError::Domain(domain::DomainError::BudgetExhausted(
                    "standalone_parallel_runs"
                )))
            ))
            .count(),
        1
    );
    assert_eq!(counts(&pool).await, (2, 2, 2, 2, 2));
}
