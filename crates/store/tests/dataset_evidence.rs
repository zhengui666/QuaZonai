//! Disposable PostgreSQL and real immutable fixture files; no production/PIT claims.
#[path = "support/data.rs"]
mod support;
use chrono::{Duration, Utc};
use contracts::{
    Id, SchemaV1,
    catalogs::RuntimeCatalogMetadataV1,
    control::{AssignablePrincipalKind, CredentialIssue, MachineScope, PrincipalCreate},
    data::{DataGrantRevoke, DataLicenseState, DatasetEvidenceViewV1, DatasetView},
    research::{DataPartition, PitStatus},
};
use sqlx::PgPool;
use store::{StoreError, authority::Actor, control::CredentialPreparation};
use support::*;

async fn registered(f: &Fixture, metadata: &RuntimeCatalogMetadataV1) -> DatasetView {
    let mut request = request(f);
    request.native_storage_version = metadata.storage_version.clone();
    complete(
        f,
        ticket(f, &Id::new().to_string(), &request).await,
        serde_json::to_vec(metadata).unwrap(),
    )
    .await
    .unwrap()
    .resource
}

async fn evidence(f: &Fixture, actor: &Actor, id: Id) -> Result<DatasetEvidenceViewV1, StoreError> {
    f.store
        .get_dataset_evidence(actor, id, |id, size| read(f.objects.clone(), id, size))
        .await
}

async fn machine(f: &Fixture, project: Option<Id>, scope: MachineScope) -> Actor {
    let principal = f
        .store
        .create_principal(
            &f.actor,
            &Id::new().to_string(),
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "controlled evidence reader".into(),
                kind: AssignablePrincipalKind::Cli,
                project_id: project,
                downstream_id: None,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let prepared = f
        .store
        .prepare_credential_issuance(
            &f.actor,
            &Id::new().to_string(),
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![scope],
                expires_at: Utc::now() + Duration::hours(1),
            },
        )
        .await
        .unwrap();
    let CredentialPreparation::New(prepared) = prepared else {
        panic!("fresh fixture credential")
    };
    let credential = prepared
        .publish(Id::new(), Id::new())
        .await
        .unwrap()
        .resource;
    f.store
        .machine_challenge(credential.public_token_id)
        .await
        .unwrap()
        .verified_actor(None)
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_browser_and_device_read_original_bound_summary_without_raw_values(pool: PgPool) {
    let f = setup(&pool, None).await;
    let metadata = catalog_fixture::metadata();
    let dataset = registered(&f, &metadata).await;
    let original = evidence(&f, &f.actor, dataset.id).await.unwrap();
    assert_eq!(original.dataset_revision_id, dataset.id);
    assert_eq!(
        original.native_metadata_artifact_id,
        dataset.native_metadata_artifact_id.unwrap()
    );
    assert_eq!(original.quality_artifact_id, dataset.quality_artifact_id);
    assert_eq!(original.quality.row_count, metadata.row_count);
    assert_eq!(original.quality.instrument_count, 1);
    assert_eq!(original.quality.checked_at, metadata.quality.checked_at);
    assert_eq!(original.pit_status, PitStatus::Unverified);
    assert_eq!(original.origin, metadata.origin);
    assert_eq!(original.license_state, DataLicenseState::Active);
    let encoded = serde_json::to_string(&original).unwrap();
    for forbidden in [
        "provenance_reference",
        "availability_provenance",
        "native_snapshot_ref",
        "storage_object_ref",
        "instrument_ids",
        "instrument_definitions",
        "settlements",
        "last_bar_notionals",
        "selection",
        "controlled-snapshot",
        "EUR/USD.SIM",
    ] {
        assert!(!encoded.contains(forbidden), "leaked {forbidden}");
    }
    // Existing generic access remains closed even for the owner.
    assert!(matches!(
        f.store
            .artifact(&f.actor, original.native_metadata_artifact_id)
            .await,
        Err(StoreError::NotFound)
    ));
    let login = f
        .store
        .setup_password("fixture-password-verifier", false)
        .await
        .unwrap();
    let snapshot = f.store.authentication_snapshot().await.unwrap();
    let device = f
        .store
        .register_cli_device(&snapshot, "fixture owner", "fixture-verifier")
        .await
        .unwrap();
    let owner = Actor::OwnerDevice {
        device_id: device.id,
        verifier: "fixture-verifier".into(),
    };
    let observed = evidence(&f, &owner, dataset.id).await.unwrap();
    assert_eq!(
        serde_json::to_value(original.quality).unwrap(),
        serde_json::to_value(observed.quality).unwrap()
    );
    f.store
        .revoke_cli_device(login.id, device.id)
        .await
        .unwrap();
    assert!(matches!(
        evidence(&f, &owner, dataset.id).await,
        Err(StoreError::InvalidCredentials)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn genuine_diagnostic_and_project_machine_credentials_never_read_evidence(pool: PgPool) {
    let f = setup(&pool, None).await;
    let dataset = registered(&f, &catalog_fixture::metadata()).await;
    let diagnostic = machine(&f, None, MachineScope::DoctorRead).await;
    // This is a valid existing data-management label reader, not an invented actor.
    f.store
        .get_dataset_revision(&diagnostic, dataset.id)
        .await
        .unwrap();
    let science = machine(&f, Some(f.project), MachineScope::ResearchRead).await;
    f.store.project(&science, f.project).await.unwrap();
    for actor in [diagnostic, science] {
        let denied = f
            .store
            .get_dataset_evidence(&actor, dataset.id, |_, _| async {
                panic!("denied identity must not read an object")
            })
            .await;
        assert!(matches!(denied, Err(StoreError::Forbidden)));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn sealed_and_non_dataset_artifact_references_stop_before_native_io(pool: PgPool) {
    let f = setup(&pool, None).await;
    let mut native = catalog_fixture::metadata();
    native.partition = DataPartition::Sealed;
    let dataset = registered(&f, &native).await;
    for (id, sealed) in [
        (dataset.id, true),
        (dataset.quality_artifact_id, false),
        (f.proof, false),
        (Id::new(), false),
    ] {
        let result = f
            .store
            .get_dataset_evidence(&f.actor, id, |_, _| async {
                panic!("Sealed/arbitrary artifact/unknown IDs must not read bytes")
            })
            .await;
        if sealed {
            assert!(matches!(
                result,
                Err(StoreError::Invalid("dataset_evidence_sealed"))
            ));
        } else {
            assert!(matches!(result, Err(StoreError::NotFound)));
        }
    }
    assert_eq!(
        f.store
            .get_dataset_revision(&f.actor, dataset.id)
            .await
            .unwrap()
            .partition,
        DataPartition::Sealed
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_real_metadata_or_quality_objects_never_fall_back_to_registration_labels(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let dataset = registered(&f, &catalog_fixture::metadata()).await;
    let empty_dir = tempfile::tempdir().unwrap();
    let empty = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&empty_dir.path().join("objects")).unwrap(),
    );
    for absent in [
        dataset.native_metadata_artifact_id.unwrap(),
        dataset.quality_artifact_id,
    ] {
        let result = f
            .store
            .get_dataset_evidence(&f.actor, dataset.id, |id, size| {
                read(
                    if id == absent {
                        empty.clone()
                    } else {
                        f.objects.clone()
                    },
                    id,
                    size,
                )
            })
            .await;
        assert!(matches!(result, Err(StoreError::Integrity)));
    }
    assert!(evidence(&f, &f.actor, dataset.id).await.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn foreign_document_bytes_and_quality_schema_mismatch_are_rejected(pool: PgPool) {
    let f = setup(&pool, None).await;
    let first = catalog_fixture::metadata();
    let dataset = registered(&f, &first).await;
    let mut foreign = first.clone();
    foreign.storage_version = "fixture-v2".into();
    foreign.quality.datasets[0].dataset_revision_id = Id::new();
    let second = registered(&f, &foreign).await;
    for (expected, replacement) in [
        (
            dataset.native_metadata_artifact_id.unwrap(),
            second.native_metadata_artifact_id.unwrap(),
        ),
        (dataset.quality_artifact_id, second.quality_artifact_id),
    ] {
        let result = f
            .store
            .get_dataset_evidence(&f.actor, dataset.id, |id, size| {
                read(
                    f.objects.clone(),
                    if id == expected { replacement } else { id },
                    size,
                )
            })
            .await;
        assert!(matches!(result, Err(StoreError::Integrity)));
    }
    // Strict typed decoding rejects both wrong shape and unknown schema versions.
    for replacement in [b"{}".to_vec(), b"{\"schema_version\":2}".to_vec()] {
        let result = f
            .store
            .get_dataset_evidence(&f.actor, dataset.id, |id, size| {
                let replacement = replacement.clone();
                let objects = f.objects.clone();
                async move {
                    if id == dataset.quality_artifact_id {
                        let mut padded = replacement;
                        padded.resize(size.get() as usize, b' ');
                        Ok(padded)
                    } else {
                        read(objects, id, size).await
                    }
                }
            })
            .await;
        assert!(matches!(result, Err(StoreError::Integrity)));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_license_remains_visible_for_audit_without_relabeling_or_new_use(pool: PgPool) {
    let f = setup(&pool, None).await;
    let dataset = registered(&f, &catalog_fixture::metadata()).await;
    f.store
        .revoke_data_grant(
            &f.actor,
            "revoke-for-audit",
            f.grant.id,
            &DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: None,
                reason_code: "FIXTURE".into(),
                reason: "controlled audit regression".into(),
            },
        )
        .await
        .unwrap();
    let result = evidence(&f, &f.actor, dataset.id).await.unwrap();
    assert_eq!(result.license_state, DataLicenseState::Revoked);
    assert_eq!(result.origin, dataset.origin);
    assert_eq!(result.pit_status, dataset.pit_status);
    assert_eq!(result.dataset_revision_id, dataset.id);
    assert_eq!(counts(&pool).await, (1, 1, 1, 1));
}
