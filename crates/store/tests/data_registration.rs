//! Native PostgreSQL + actual immutable files. Controlled Runtime metadata stays FIXTURE.
#[path = "support/data.rs"]
mod support;
use contracts::{
    control::ListQuery, data::*, research::DataOrigin, settings::RuntimeUpdate, Id, SchemaV1,
};
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use store::{data_registration::RegistrationPreparation, StoreError};
use support::*;

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_native_registration_publishes_one_dataset_universe_batch_and_receipt(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let request = request(&f);
    let metadata = catalog_fixture::metadata();
    let bytes = serde_json::to_vec_pretty(&metadata).unwrap();
    let a = ticket(&f, "register", &request).await;
    let b = ticket(&f, "register", &request).await;
    let publications = AtomicUsize::new(0);
    let publish_once = |items| {
        publications.fetch_add(1, Ordering::SeqCst);
        publish(f.objects.clone(), items)
    };
    let reader = |id, size| read(f.objects.clone(), id, size);
    let (a, b) = tokio::join!(
        f.store
            .complete_dataset_registration(a, bytes.clone(), reader, publish_once),
        f.store
            .complete_dataset_registration(b, bytes.clone(), reader, publish_once),
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(publications.load(Ordering::SeqCst), 1);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        serde_json::to_value(&a.resource).unwrap(),
        serde_json::to_value(&b.resource).unwrap()
    );
    assert_eq!(a.resource.origin, DataOrigin::Fixture);
    assert_eq!(a.resource.row_count, metadata.row_count);
    assert_eq!(counts(&pool).await, (1, 1, 1, 1));
    assert_eq!(
        std::fs::read_dir(f.directory.path().join("objects"))
            .unwrap()
            .count(),
        5
    );
    let raw: (Id, i64) = {
        let stored: (uuid::Uuid, i64) = sqlx::query_as("SELECT a.id,a.byte_count FROM app.artifacts a JOIN app.dataset_registration_evidence e ON e.native_metadata_artifact_id=a.id WHERE e.dataset_revision_id=$1")
            .bind(a.resource.id.as_uuid()).fetch_one(&pool).await.unwrap();
        (stored.0.to_string().try_into().unwrap(), stored.1)
    };
    assert_eq!(
        f.objects
            .read(raw.0, raw.1.to_string().try_into().unwrap())
            .unwrap(),
        bytes
    );
    let replay = f
        .store
        .prepare_dataset_registration(&f.actor, "register", &request)
        .await
        .unwrap();
    let RegistrationPreparation::Replay(replay) = replay else {
        panic!("original receipt must precede new network work")
    };
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, a.resource.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_identity_cross_key_replay_cannot_change_grant_origin_or_original_metadata(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let request = request(&f);
    let metadata = catalog_fixture::metadata();
    let first = complete(
        &f,
        ticket(&f, "first", &request).await,
        serde_json::to_vec(&metadata).unwrap(),
    )
    .await
    .unwrap();
    let read_original = |id, size| read(f.objects.clone(), id, size);
    let second = f
        .store
        .complete_dataset_registration(
            ticket(&f, "different-key", &request).await,
            serde_json::to_vec_pretty(&metadata).unwrap(),
            read_original,
            |_| async { panic!("same native identity must not allocate another artifact batch") },
        )
        .await
        .unwrap();
    assert!(!second.replayed);
    assert_eq!(second.resource.id, first.resource.id);
    assert_eq!(counts(&pool).await, (1, 1, 1, 2));
    let mut changed = metadata.clone();
    changed.origin = DataOrigin::Real;
    assert!(matches!(
        complete(
            &f,
            ticket(&f, "origin-conflict", &request).await,
            serde_json::to_vec(&changed).unwrap()
        )
        .await,
        Err(StoreError::NativeIdentityConflict)
    ));
    let mut changed = metadata.clone();
    changed.provenance_reference = "Changed immutable provenance".into();
    assert!(matches!(
        complete(
            &f,
            ticket(&f, "metadata-conflict", &request).await,
            serde_json::to_vec(&changed).unwrap()
        )
        .await,
        Err(StoreError::NativeIdentityConflict)
    ));
    let grant = f
        .store
        .create_data_grant(
            &f.actor,
            "new-grant",
            &DataGrantCreate {
                schema_version: SchemaV1,
                source_id: f.source.id,
                license_reference: "different explicit license".into(),
                evidence_artifact_id: f.proof,
                allowed_uses: contracts::research::DataUse::ResearchPaperLive,
                valid_from: f.grant.valid_from,
                valid_until: None,
            },
        )
        .await
        .unwrap()
        .resource;
    let mut changed_request = request.clone();
    changed_request.grant_id = grant.id;
    assert!(matches!(
        complete(
            &f,
            ticket(&f, "grant-conflict", &changed_request).await,
            serde_json::to_vec(&metadata).unwrap()
        )
        .await,
        Err(StoreError::NativeIdentityConflict)
    ));
    assert_eq!(counts(&pool).await, (1, 1, 1, 2));
    assert_eq!(
        f.store
            .get_dataset_revision(&f.actor, first.resource.id)
            .await
            .unwrap()
            .data_use_grant_id,
        f.grant.id
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn received_rfc3339_spelling_is_preserved_on_different_key_native_replay(pool: PgPool) {
    let f = setup(&pool, None).await;
    let request = request(&f);
    let mut document = serde_json::to_value(catalog_fixture::metadata()).unwrap();
    document["event_start"] = serde_json::json!("1970-01-01T00:01:00+00:00");
    document["universe"]["selection_asof"] = serde_json::json!("1970-01-01T00:00:00.000000Z");
    let bytes = serde_json::to_vec(&document).unwrap();
    let first = complete(
        &f,
        ticket(&f, "rfc3339-first", &request).await,
        bytes.clone(),
    )
    .await
    .unwrap()
    .resource;
    let second = f
        .store
        .complete_dataset_registration(
            ticket(&f, "rfc3339-second", &request).await,
            serde_json::to_vec_pretty(&document).unwrap(),
            |id, size| read(f.objects.clone(), id, size),
            |_| async { panic!("unchanged received metadata must not publish another object") },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(first.id, second.id);
    assert_eq!(
        first.native_metadata_artifact_id,
        second.native_metadata_artifact_id
    );
    assert_eq!(counts(&pool).await, (1, 1, 1, 2));
    let size = contracts::DbCounter::new(bytes.len() as u64).unwrap();
    assert_eq!(
        f.objects
            .read(first.native_metadata_artifact_id.unwrap(), size)
            .unwrap(),
        bytes
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_and_legacy_universes_have_explicit_evidence_state_without_history_rewrite(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let request = request(&f);
    let first = complete(
        &f,
        ticket(&f, "universe-proof", &request).await,
        serde_json::to_vec(&catalog_fixture::metadata()).unwrap(),
    )
    .await
    .unwrap()
    .resource;
    let legacy = Id::new();
    // A historical relational record with no registration evidence. Its member
    // references alone must not be promoted to a formally registered Universe.
    sqlx::query("INSERT INTO app.universe_versions(id,name,membership_artifact_id,instrument_definition_artifact_id,calendar_ref,calendar_version,selection_asof,has_historical_membership,coverage_start,coverage_end) SELECT $1,'Legacy record',membership_artifact_id,instrument_definition_artifact_id,calendar_ref,calendar_version,selection_asof,has_historical_membership,coverage_start,coverage_end FROM app.universe_versions WHERE id=$2")
        .bind(legacy.as_uuid()).bind(first.universe_version_id.as_uuid()).execute(&pool).await.unwrap();
    let historical: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(u) FROM app.universe_versions u WHERE id=$1")
            .bind(legacy.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let native = f
        .store
        .get_universe_version(&f.actor, first.universe_version_id)
        .await
        .unwrap();
    let old = f
        .store
        .get_universe_version(&f.actor, legacy)
        .await
        .unwrap();
    assert_eq!(
        native.registration_state,
        UniverseRegistrationState::NativeMetadata
    );
    assert_eq!(
        old.registration_state,
        UniverseRegistrationState::LegacyUnverified
    );
    assert_eq!(first.origin, DataOrigin::Fixture);
    let page = f
        .store
        .list_universe_versions(
            &f.actor,
            &ListQuery {
                cursor: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(
        page.items
            .iter()
            .find(|u| u.id == legacy)
            .unwrap()
            .registration_state,
        UniverseRegistrationState::LegacyUnverified
    );
    let unchanged: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(u) FROM app.universe_versions u WHERE id=$1")
            .bind(legacy.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(historical, unchanged);
    assert_eq!(counts(&pool).await, (1, 2, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn optional_existing_universe_requires_exact_native_membership_and_original_origin(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let mut request = request(&f);
    let mut metadata = catalog_fixture::metadata();
    let first = complete(
        &f,
        ticket(&f, "original", &request).await,
        serde_json::to_vec(&metadata).unwrap(),
    )
    .await
    .unwrap()
    .resource;
    metadata.storage_version = "fixture-v2".into();
    metadata.native_snapshot_ref = "another-controlled-snapshot".into();
    request.native_storage_version = metadata.storage_version.clone();
    request.existing_universe_version_id = Some(first.universe_version_id);
    let second = complete(
        &f,
        ticket(&f, "reuse", &request).await,
        serde_json::to_vec(&metadata).unwrap(),
    )
    .await
    .unwrap()
    .resource;
    assert_eq!(second.universe_version_id, first.universe_version_id);
    assert_ne!(first.id, second.id);
    assert_eq!(counts(&pool).await, (2, 1, 2, 2));
    let universe = f
        .store
        .get_universe_version(&f.actor, first.universe_version_id)
        .await
        .unwrap();
    assert_eq!(universe.calendar_ref, metadata.universe.calendar_ref);
    metadata.storage_version = "fixture-v3".into();
    request.native_storage_version = metadata.storage_version.clone();
    metadata.universe.membership[0].valid_until = Some(catalog_fixture::instant(550));
    assert!(matches!(
        complete(
            &f,
            ticket(&f, "changed-membership", &request).await,
            serde_json::to_vec(&metadata).unwrap()
        )
        .await,
        Err(StoreError::NativeIdentityConflict)
    ));
    assert_eq!(counts(&pool).await, (2, 1, 2, 2));
    let listed = f
        .store
        .list_universe_versions(
            &f.actor,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert!(listed.next_cursor.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_publication_failure_rolls_back_metadata_and_receipt_and_same_key_can_retry(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let request = request(&f);
    let metadata = catalog_fixture::metadata();
    let reader = |id, size| read(f.objects.clone(), id, size);
    let result = f
        .store
        .complete_dataset_registration(
            ticket(&f, "retryable-publication", &request).await,
            serde_json::to_vec(&metadata).unwrap(),
            reader,
            |_| async { Err(StoreError::Integrity) },
        )
        .await;
    assert!(matches!(result, Err(StoreError::Integrity)));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0));
    let retry = complete(
        &f,
        ticket(&f, "retryable-publication", &request).await,
        serde_json::to_vec(&metadata).unwrap(),
    )
    .await
    .unwrap();
    assert!(!retry.replayed);
    assert_eq!(counts(&pool).await, (1, 1, 1, 1));
    let error = sqlx::query("UPDATE app.dataset_registration_evidence SET observed_at=observed_at+interval '1 second' WHERE dataset_revision_id=$1")
        .bind(retry.resource.id.as_uuid()).execute(&pool).await.unwrap_err();
    let native = error.as_database_error().unwrap();
    assert_eq!(native.code().as_deref(), Some("23000"));
    assert_eq!(native.message(), "immutable domain record");
    let observed: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT observed_at FROM app.dataset_registration_evidence WHERE dataset_revision_id=$1",
    )
    .bind(retry.resource.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(Some(observed), retry.resource.registration_observed_at);
}

#[sqlx::test(migrations = "../../migrations")]
async fn source_revision_runtime_revision_and_revocation_are_rechecked_after_native_io(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let request = request(&f);
    let metadata = catalog_fixture::metadata();
    let source_ticket = ticket(&f, "source-changed", &request).await;
    let source = f
        .store
        .update_data_source(
            &f.actor,
            "rename",
            f.source.id,
            &DataSourceUpdate {
                schema_version: SchemaV1,
                expected_revision: f.source.revision,
                name: "Renamed source".into(),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    assert!(matches!(
        complete(&f, source_ticket, serde_json::to_vec(&metadata).unwrap()).await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let mut current = request.clone();
    current.expected_source_revision = source.revision;
    let runtime_ticket = ticket(&f, "runtime-changed", &current).await;
    let mut configuration = f.runtime.configuration.clone();
    configuration.name = "Changed Runtime".into();
    let runtime = f
        .store
        .update_runtime(
            &f.actor,
            "runtime-update",
            f.runtime.id,
            &RuntimeUpdate {
                schema_version: SchemaV1,
                expected_revision: f.runtime.revision,
                configuration,
                credential_ref: None,
                ca_certificate_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert!(matches!(
        complete(&f, runtime_ticket, serde_json::to_vec(&metadata).unwrap()).await,
        Err(StoreError::RevisionConflict { .. })
    ));
    current.expected_runtime_revision = runtime.revision;
    let revoked = ticket(&f, "grant-revoked", &current).await;
    f.store
        .revoke_data_grant(
            &f.actor,
            "revoke",
            f.grant.id,
            &DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: None,
                reason_code: "FIXTURE_REVOKED".into(),
                reason: "Explicit controlled revocation".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        complete(&f, revoked, serde_json::to_vec(&metadata).unwrap()).await,
        Err(StoreError::Invalid("data_source_or_grant_unavailable"))
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn current_grant_expiry_during_cross_key_native_object_read_cannot_publish_new_receipt(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let expires = now + chrono::Duration::seconds(3);
    let grant = f
        .store
        .create_data_grant(
            &f.actor,
            "short-grant",
            &DataGrantCreate {
                schema_version: SchemaV1,
                source_id: f.source.id,
                license_reference: "time-bounded fixture".into(),
                evidence_artifact_id: f.proof,
                allowed_uses: contracts::research::DataUse::Research,
                valid_from: now - chrono::Duration::hours(1),
                valid_until: Some(expires),
            },
        )
        .await
        .unwrap()
        .resource;
    let mut request = request(&f);
    request.grant_id = grant.id;
    let metadata = catalog_fixture::metadata();
    complete(
        &f,
        ticket(&f, "initial-short", &request).await,
        serde_json::to_vec(&metadata).unwrap(),
    )
    .await
    .unwrap();
    let second = ticket(&f, "expired-read", &request).await;
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    let reader = f.objects.clone();
    let clock = pool.clone();
    let result = f
        .store
        .complete_dataset_registration(
            second,
            serde_json::to_vec(&metadata).unwrap(),
            move |id, size| {
                let reader = reader.clone();
                let clock = clock.clone();
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        loop {
                            let time: chrono::DateTime<chrono::Utc> =
                                sqlx::query_scalar("SELECT clock_timestamp()")
                                    .fetch_one(&clock)
                                    .await
                                    .unwrap();
                            if time >= expires {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .unwrap();
                    read(reader, id, size).await
                }
            },
            |_| async { panic!("existing native identity cannot republish") },
        )
        .await;
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert!(matches!(
        result,
        Err(StoreError::Invalid("data_source_or_grant_unavailable"))
    ));
    assert_eq!(counts(&pool).await, (1, 1, 1, 1));
    let RegistrationPreparation::Replay(original) = f
        .store
        .prepare_dataset_registration(&f.actor, "initial-short", &request)
        .await
        .unwrap()
    else {
        panic!("original receipt must survive grant expiry")
    };
    assert!(original.replayed);
    let fresh = f
        .store
        .get_dataset_revision(&f.actor, original.resource.id)
        .await
        .unwrap();
    assert_eq!(fresh.license_state, DataLicenseState::Expired);
}
