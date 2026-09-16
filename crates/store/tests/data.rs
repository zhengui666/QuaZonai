//! Native source identities, license versions and current read projections on PostgreSQL.
#[path = "support/data.rs"]
mod support;
use chrono::{Duration, Utc};
use contracts::{control::ListQuery, data::*, research::DataUse, Id, SchemaV1};
use sqlx::PgPool;
use store::{authority::Actor, StoreError};
use support::*;

#[sqlx::test(migrations = "../../migrations")]
async fn source_identity_is_unique_and_cas_updates_do_not_rewrite_native_bindings_or_receipts(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let create = DataSourceCreate {
        schema_version: SchemaV1,
        name: f.source.name.clone(),
        runtime_id: f.runtime.id,
        native_catalog_ref: f.source.native_catalog_ref.clone(),
        provider_kind: DataProviderKind::NautilusCatalog,
        enabled: true,
    };
    let replay = f
        .store
        .create_data_source(&f.actor, "native-source", &create)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, f.source.id);
    assert!(matches!(
        f.store
            .create_data_source(&f.actor, "same-native-another-key", &create)
            .await,
        Err(StoreError::NativeIdentityConflict)
    ));
    let update = DataSourceUpdate {
        schema_version: SchemaV1,
        expected_revision: f.source.revision,
        name: "Renamed and disabled".into(),
        enabled: false,
    };
    let changed = f
        .store
        .update_data_source(&f.actor, "disable", f.source.id, &update)
        .await
        .unwrap()
        .resource;
    assert!(!changed.enabled);
    assert_eq!(changed.revision, f.source.revision.next().unwrap());
    assert_eq!(changed.native_catalog_ref, f.source.native_catalog_ref);
    assert_eq!(changed.runtime_id, f.source.runtime_id);
    assert!(matches!(
        f.store
            .update_data_source(&f.actor, "stale", f.source.id, &update)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let original = f
        .store
        .create_data_source(&f.actor, "native-source", &create)
        .await
        .unwrap();
    assert!(original.resource.enabled && original.replayed);
    let rows = f
        .store
        .list_data_sources(
            &f.actor,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(rows.items.len(), 1);
    assert_eq!(rows.items[0].revision, changed.revision);
    assert!(rows.next_cursor.is_none());
    let raw = serde_json::to_value(
        f.store
            .get_data_source(&f.actor, f.source.id)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(raw.get("credential_ref").is_none());
    assert!(raw.get("secret").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_license_versions_and_revocations_are_append_only_and_same_key_replays(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let create = DataGrantCreate {
        schema_version: SchemaV1,
        source_id: f.source.id,
        license_reference: "Updated controlled license".into(),
        evidence_artifact_id: f.proof,
        allowed_uses: DataUse::ResearchAndPaper,
        valid_from: f.grant.valid_from,
        valid_until: None,
    };
    let (a, b) = tokio::join!(
        f.store.create_data_grant(&f.actor, "grant-a", &create),
        f.store.create_data_grant(&f.actor, "grant-b", &create)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    let mut versions = [a.resource.version.get(), b.resource.version.get()];
    versions.sort();
    assert_eq!(versions, [2, 3]);
    let original = f
        .store
        .create_data_grant(&f.actor, "grant-a", &create)
        .await
        .unwrap();
    assert!(original.replayed);
    assert_eq!(original.resource.id, a.resource.id);
    let revoke = DataGrantRevoke {
        schema_version: SchemaV1,
        effective_at: None,
        reason_code: "TEST_WITHDRAWAL".into(),
        reason: "Current controlled license withdrawn".into(),
    };
    let receipt = f
        .store
        .revoke_data_grant(&f.actor, "revoke-a", a.resource.id, &revoke)
        .await
        .unwrap();
    let replay = f
        .store
        .revoke_data_grant(&f.actor, "revoke-a", a.resource.id, &revoke)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(receipt.resource.id, replay.resource.id);
    assert_eq!(receipt.resource.effective_at, replay.resource.effective_at);
    let rows = f
        .store
        .list_data_grants(
            &f.actor,
            f.source.id,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(rows.items.len(), 1);
    assert!(rows.next_cursor.is_some());
    let all = f
        .store
        .list_data_grants(
            &f.actor,
            f.source.id,
            &ListQuery {
                cursor: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(all.items.len(), 3);
    assert_eq!(
        all.items
            .iter()
            .find(|grant| grant.id == a.resource.id)
            .unwrap()
            .license_state,
        DataLicenseState::Revoked
    );
    assert_eq!(
        all.items
            .iter()
            .find(|grant| grant.id == b.resource.id)
            .unwrap()
            .license_state,
        DataLicenseState::Active
    );
    let history = f
        .store
        .list_data_revocations(
            &f.actor,
            a.resource.id,
            &ListQuery {
                cursor: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(history.items.len(), 1);
    let update = sqlx::query(
        "UPDATE app.data_use_grants SET allowed_uses='RESEARCH_PAPER_LIVE' WHERE id=$1",
    )
    .bind(a.resource.id.as_uuid())
    .execute(&pool)
    .await;
    assert!(update.is_err());
    assert_eq!(
        f.store
            .create_data_grant(&f.actor, "grant-a", &create)
            .await
            .unwrap()
            .resource
            .allowed_uses,
        DataUse::ResearchAndPaper
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn license_evidence_source_and_time_boundaries_cannot_be_bypassed_by_registration(
    pool: PgPool,
) {
    let f = setup(&pool, None).await;
    let now: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut create = DataGrantCreate {
        schema_version: SchemaV1,
        source_id: f.source.id,
        license_reference: "Historical license fixture".into(),
        evidence_artifact_id: Id::new(),
        allowed_uses: DataUse::Research,
        valid_from: now - Duration::days(1),
        valid_until: Some(now - Duration::hours(1)),
    };
    assert!(matches!(
        f.store
            .create_data_grant(&f.actor, "missing-evidence", &create)
            .await,
        Err(StoreError::Invalid("license_evidence_artifact"))
    ));
    create.evidence_artifact_id = f.proof;
    let expired = f
        .store
        .create_data_grant(&f.actor, "expired-evidence", &create)
        .await
        .unwrap()
        .resource;
    assert_eq!(expired.license_state, DataLicenseState::Expired);
    create.valid_from = now + Duration::hours(1);
    create.valid_until = None;
    let future = f
        .store
        .create_data_grant(&f.actor, "future-evidence", &create)
        .await
        .unwrap()
        .resource;
    assert_eq!(future.license_state, DataLicenseState::NotYetValid);
    for grant in [expired.id, future.id] {
        let mut request = request(&f);
        request.grant_id = grant;
        assert!(matches!(
            f.store
                .prepare_dataset_registration(&f.actor, "unavailable-grant", &request)
                .await,
            Err(StoreError::Invalid("data_source_or_grant_unavailable"))
        ));
    }
    let revoke = DataGrantRevoke {
        schema_version: SchemaV1,
        effective_at: Some(now - Duration::seconds(1)),
        reason_code: "BACKDATED".into(),
        reason: "Cannot retroactively retract past consumption".into(),
    };
    assert!(matches!(
        f.store
            .revoke_data_grant(&f.actor, "backdated", f.grant.id, &revoke)
            .await,
        Err(StoreError::Invalid("revocation_effective_at"))
    ));
    assert_eq!(counts(&pool).await, (0, 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn global_metadata_is_operator_or_doctor_only_not_research_machine_authority(pool: PgPool) {
    let f = setup(&pool, None).await;
    let principal = Id::new();
    let credential = Id::new();
    let verifier_ref = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,enabled,credential_epoch) VALUES($1,'Controlled doctor','CLI',true,1)")
        .bind(principal.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,$3,$4,1,ARRAY['DOCTOR_READ'],clock_timestamp(),clock_timestamp()+interval '1 hour','OPERATOR')")
        .bind(credential.as_uuid()).bind(principal.as_uuid()).bind(Id::new().to_string()).bind(verifier_ref.to_string()).execute(&pool).await.unwrap();
    let doctor = Actor::Machine {
        credential_id: credential,
        verifier_ref,
        operator_grant: None,
    };
    let read = f
        .store
        .list_data_sources(
            &doctor,
            &ListQuery {
                cursor: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(read.items[0].id, f.source.id);
    let update = DataSourceUpdate {
        schema_version: SchemaV1,
        expected_revision: f.source.revision,
        name: "Not authorized".into(),
        enabled: false,
    };
    assert!(matches!(
        f.store
            .update_data_source(&doctor, "denied", f.source.id, &update)
            .await,
        Err(StoreError::Forbidden)
    ));
    sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) VALUES($1,clock_timestamp(),'Controlled revocation')")
        .bind(credential.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        f.store.get_data_source(&doctor, f.source.id).await,
        Err(StoreError::InvalidCredentials)
    ));
}
