//! Real PostgreSQL and original local object buffers; all source evidence is a
//! controlled FIXTURE, never a claim of REAL market data or executed science.
#[path = "support/data.rs"]
mod support;
use contracts::{
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    catalogs::{RecordedFeatureFragmentV1, RecordedFeatureInputsV1},
    control::CommandResult,
    data::*,
    research::{
        ArtifactInputRole, DataOrigin, DataPartition, InputItemV1, InputPurpose, InputSetCreate,
        PitStatus,
    },
    DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use std::sync::atomic::{AtomicUsize, Ordering};
use store::StoreError;
use support::*;

fn n(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}

fn content() -> String {
    concat!(" \n{\"schema_version\":1,\"partition\":\"DISCOVERY\",",
        "\"feature_schema\":[{\"feature_key\":\"price\",\"source_ref\":\"fixture:paired-report\",\"source_key\":\"paired\",",
        "\"availability\":{\"basis\":\"OBSERVED\"},\"max_age_ns\":null}],",
        "\"observations\":[{\"feature_index\":0,\"event_ns\":\"50000000000\",\"observed_available_ns\":\"55000000000\",",
        "\"sequence\":\"0\",\"value\":0.10000000000000002,\"missing_reason\":null},",
        "{\"feature_index\":0,\"event_ns\":\"100000000000\",\"observed_available_ns\":\"110000000000\",",
        "\"sequence\":\"1\",\"value\":null,\"missing_reason\":\"recorded_missing\"}]}\n").into()
}
fn descriptor(raw: &str) -> RecordedFeatureInputsV1 {
    RecordedFeatureInputsV1 {
        schema_version: SchemaV1,
        source_selection_start_ns: n(40_000_000_000),
        source_selection_end_ns: n(101_000_000_000),
        partition: DataPartition::Discovery,
        fragments: vec![RecordedFeatureFragmentV1 {
            part_key: "features-0001".into(),
            byte_count: n(raw.len() as u64),
            observations: n(2),
            min_event_ns: n(50_000_000_000),
            max_event_ns: n(100_000_000_000),
            min_observed_available_ns: n(55_000_000_000),
            max_observed_available_ns: n(110_000_000_000),
        }],
    }
}
async fn dataset(f: &Fixture, key: &str, version: &str) -> DatasetView {
    let mut metadata = catalog_fixture::metadata();
    metadata.storage_version = version.into();
    metadata.recorded_feature_inputs = Some(descriptor(&content()));
    let mut request = request(f);
    request.native_storage_version = version.into();
    complete(
        f,
        ticket(f, key, &request).await,
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .await
    .unwrap()
    .resource
}
fn registration(f: &Fixture, dataset: Id) -> RecordedFeatureRegisterV1 {
    RecordedFeatureRegisterV1 {
        schema_version: SchemaV1,
        project_id: f.project,
        dataset_revision_id: dataset,
        feature_part_key: "features-0001".into(),
        content: content(),
    }
}
async fn register(
    f: &Fixture,
    key: &str,
    request: &RecordedFeatureRegisterV1,
) -> Result<CommandResult<RecordedFeatureViewV1>, StoreError> {
    f.store
        .register_recorded_feature(
            &f.actor,
            key,
            request,
            |id, size| read(f.objects.clone(), id, size),
            |object| publish(f.objects.clone(), vec![object]),
        )
        .await
}
fn input(f: &Fixture, dataset: Id, feature: Id) -> InputSetCreate {
    InputSetCreate {
        schema_version: SchemaV1,
        project_id: f.project,
        purpose: InputPurpose::Discovery,
        decision_cutoff: catalog_fixture::instant(300),
        items: vec![
            InputItemV1::Dataset {
                dataset_revision_id: dataset,
                role: DataPartition::Discovery,
            },
            InputItemV1::Artifact {
                artifact_id: feature,
                role: ArtifactInputRole::Parameters,
            },
        ],
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_feature_registration_replays_one_identity_and_freezes_actual_source(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let dataset = dataset(&f, "dataset", "recorded-1").await;
    let request = registration(&f, dataset.id);
    let publications = AtomicUsize::new(0);
    let writer = |object| {
        publications.fetch_add(1, Ordering::SeqCst);
        publish(f.objects.clone(), vec![object])
    };
    let reader = |id, size| read(f.objects.clone(), id, size);
    let (first, second) = tokio::join!(
        f.store
            .register_recorded_feature(&f.actor, "feature", &request, reader, writer),
        f.store
            .register_recorded_feature(&f.actor, "feature", &request, reader, writer),
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(publications.load(Ordering::SeqCst), 1);
    assert_ne!(first.replayed, second.replayed);
    assert_eq!(
        serde_json::to_value(&first.resource).unwrap(),
        serde_json::to_value(&second.resource).unwrap()
    );
    let feature = first.resource;
    assert_eq!(feature.source_binding.dataset_revision_id, dataset.id);
    assert_eq!(feature.source_binding.source_id, f.source.id);
    assert_eq!(feature.source_binding.data_use_grant_id, f.grant.id);
    assert_eq!(
        feature.source_binding.native_metadata_artifact_id,
        dataset.native_metadata_artifact_id.unwrap()
    );
    assert_eq!(feature.source_binding.origin, DataOrigin::Fixture);
    assert_eq!(feature.source_binding.pit_status, PitStatus::Unverified);
    assert_eq!(
        feature.fragment.max_observed_available_ns,
        n(110_000_000_000)
    );
    assert_eq!(feature.fragment.min_event_ns, n(50_000_000_000));
    assert!(feature.fragment.max_observed_available_ns > feature.source_selection_end_ns);
    let original = f
        .store
        .artifact_content(&f.actor, feature.artifact_id)
        .await
        .unwrap();
    assert_eq!(original.metadata.schema_name, "qz.feature_observations");
    assert_eq!(original.metadata.origin, DataOrigin::Fixture);
    assert_eq!(
        f.objects
            .read(original.local_object_id, original.metadata.byte_count)
            .unwrap(),
        request.content.as_bytes()
    );
    let same = register(&f, "new-key", &request).await.unwrap();
    assert_eq!(same.resource.artifact_id, feature.artifact_id);
    let receipt: serde_json::Value = sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='RECORDED_FEATURE_REGISTER' AND idempotency_key='feature'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        receipt,
        serde_json::to_value(request.intent().unwrap()).unwrap()
    );
    assert!(receipt.get("content").is_none());
    let frozen = f
        .store
        .create_input_set(
            &f.actor,
            "freeze-recorded",
            &input(&f, dataset.id, feature.artifact_id),
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(frozen.items[1].origin, DataOrigin::Fixture);
    assert_eq!(frozen.items[1].pit_status, Some(PitStatus::Unverified));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.feature_artifact_sources")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert!(!f
        .store
        .discard_unpublished_operator_artifact(feature.artifact_id, |_| async {
            panic!("committed original must survive uncertain-result cleanup")
        })
        .await
        .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn equivalent_json_wrong_dataset_and_unbound_manual_content_cannot_acquire_a_recorded_identity(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let first = dataset(&f, "dataset-one", "recorded-1").await;
    let request = registration(&f, first.id);
    let feature = register(&f, "original", &request).await.unwrap().resource;
    let mut changed = registration(&f, first.id);
    changed.content.push('\n');
    assert!(matches!(
        register(&f, "original", &changed).await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        register(&f, "new-content-key", &changed).await,
        Err(StoreError::NativeIdentityConflict)
    ));
    for raw in [
        request
            .content
            .replace("0.10000000000000002", "0.20000000000000002"),
        request.content.replacen(' ', "\t", 1),
    ] {
        let mut same_size = request.clone();
        same_size.content = raw;
        assert_eq!(same_size.content.len(), request.content.len());
        assert_ne!(same_size.content, request.content);
        assert!(matches!(
            register(&f, "original", &same_size).await,
            Err(StoreError::IdempotencyConflict)
        ));
        assert!(matches!(
            register(&f, "same-size-new-key", &same_size).await,
            Err(StoreError::NativeIdentityConflict)
        ));
    }
    changed.feature_part_key = "unlisted-part".into();
    assert!(register(&f, "unlisted", &changed).await.is_err());
    let second = dataset(&f, "dataset-two", "recorded-2").await;
    assert!(f
        .store
        .create_input_set(
            &f.actor,
            "wrong-dataset",
            &input(&f, second.id, feature.artifact_id)
        )
        .await
        .is_err());
    let mut missing = input(&f, first.id, feature.artifact_id);
    missing.items.remove(0);
    assert!(f
        .store
        .create_input_set(&f.actor, "missing-dataset", &missing)
        .await
        .is_err());
    let upload = f
        .store
        .prepare_artifact_upload(
            &f.actor,
            "manual",
            &ArtifactCreate {
                schema_version: SchemaV1,
                project_id: f.project,
                kind: ResearchArtifactKind::Parameters,
                content: request.content.clone(),
            },
        )
        .await
        .unwrap();
    f.objects
        .put(upload.id(), request.content.as_bytes())
        .unwrap();
    let manual = upload.publish().await.unwrap().resource;
    assert_eq!(manual.origin, DataOrigin::Synthetic);
    assert_eq!(manual.schema_name, "qz.research_parameters");
    let list = f
        .store
        .list_recorded_features(
            &f.actor,
            first.id,
            &RecordedFeatureListQuery {
                project_id: f.project,
            },
            |id, size| read(f.objects.clone(), id, size),
        )
        .await
        .unwrap();
    assert_eq!(list.items.len(), 1);
    assert_ne!(list.items[0].artifact_id, manual.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn withdrawal_blocks_new_use_and_cross_key_replay_but_preserves_original_receipt_list_and_export(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let dataset = dataset(&f, "dataset", "recorded-1").await;
    let request = registration(&f, dataset.id);
    let feature = register(&f, "feature", &request).await.unwrap().resource;
    f.store
        .revoke_data_grant(
            &f.actor,
            "withdraw",
            f.grant.id,
            &DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: None,
                reason_code: "CONTROLLED_WITHDRAWAL".into(),
                reason: "Controlled paired BAR and feature grant withdrawn".into(),
            },
        )
        .await
        .unwrap();
    assert!(register(&f, "new-key", &request).await.is_err());
    let original = register(&f, "feature", &request).await.unwrap();
    assert!(original.replayed);
    assert_eq!(original.resource.artifact_id, feature.artifact_id);
    assert!(f
        .store
        .create_input_set(
            &f.actor,
            "new-science",
            &input(&f, dataset.id, feature.artifact_id)
        )
        .await
        .is_err());
    let listed = f
        .store
        .list_recorded_features(
            &f.actor,
            dataset.id,
            &RecordedFeatureListQuery {
                project_id: f.project,
            },
            |id, size| read(f.objects.clone(), id, size),
        )
        .await
        .unwrap();
    assert_eq!(listed.items[0].artifact_id, feature.artifact_id);
    let original = f
        .store
        .artifact_content(&f.actor, feature.artifact_id)
        .await
        .unwrap();
    assert_eq!(
        f.objects
            .read(original.local_object_id, original.metadata.byte_count)
            .unwrap(),
        request.content.as_bytes()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn publication_failure_rolls_back_binding_and_receipt_and_preserves_retry(pool: PgPool) {
    let f = setup(&pool, None).await;
    let dataset = dataset(&f, "dataset", "recorded-1").await;
    let request = registration(&f, dataset.id);
    let error = f
        .store
        .register_recorded_feature(
            &f.actor,
            "feature",
            &request,
            |id, size| read(f.objects.clone(), id, size),
            |_| async { Err(StoreError::StorageFull) },
        )
        .await;
    assert!(matches!(error, Err(StoreError::StorageFull)));
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.feature_artifact_sources),(SELECT count(*) FROM app.command_receipts WHERE operation='RECORDED_FEATURE_REGISTER')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
    assert!(!register(&f, "feature", &request).await.unwrap().replayed);
}
