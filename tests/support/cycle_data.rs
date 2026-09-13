//! Register controlled runtime declarations through the immutable data service.
//! Even the REAL/PIT branch is protocol test input, NOT a market/PIT attestation.
use super::research_support;
#[path = "catalog_metadata.rs"]
mod catalog;
use chrono::{DateTime, Duration, Utc};
use contracts::{
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    data::{DataGrantCreate, DataProviderKind, DataSourceCreate, DatasetRegister},
    research::{DataOrigin, DataPartition, DataUse, PitStatus},
    DbCounter, Id, Revision, SchemaV1,
};
use integrations::artifacts::ArtifactStore;
use sqlx::PgPool;
use std::sync::Arc;
use store::{authority::Actor, data_registration::RegistrationPreparation, Store, StoreError};

fn time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
fn nanos(value: DateTime<Utc>) -> DbCounter {
    DbCounter::new(value.timestamp_nanos_opt().unwrap().try_into().unwrap()).unwrap()
}

pub async fn register(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    data: &mut research_support::ResearchFixture,
    revision: Revision,
    objects: Arc<ArtifactStore>,
    origin: DataOrigin,
) {
    let license = ArtifactCreate {
        schema_version: SchemaV1,
        project_id: data.project,
        kind: ResearchArtifactKind::Report,
        content: "{\"schema_version\":1,\"license\":\"synthetic regression fixture only\"}".into(),
    };
    let prepared = store
        .prepare_artifact_upload(actor, &Id::new().to_string(), &license)
        .await
        .unwrap();
    objects
        .put(prepared.id(), license.content.as_bytes())
        .unwrap();
    let proof = prepared.publish().await.unwrap().resource.id;
    let source = store
        .create_data_source(
            actor,
            &Id::new().to_string(),
            &DataSourceCreate {
                schema_version: SchemaV1,
                name: "Controlled Cycle catalog".into(),
                runtime_id: data.runtime,
                native_catalog_ref: format!("cycle-fixture/{}", data.project),
                provider_kind: DataProviderKind::NautilusCatalog,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let observed: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let grant = store
        .create_data_grant(
            actor,
            &Id::new().to_string(),
            &DataGrantCreate {
                schema_version: SchemaV1,
                source_id: source.id,
                license_reference: "Only synthetic regression data, not an investment license"
                    .into(),
                evidence_artifact_id: proof,
                allowed_uses: DataUse::Research,
                valid_from: observed - Duration::hours(1),
                valid_until: Some(observed + Duration::days(1)),
            },
        )
        .await
        .unwrap()
        .resource;
    data.source = source.id;
    data.grant = grant.id;
    let mut universe = None;
    for (role, name, start, end) in [
        (
            DataPartition::Discovery,
            "discovery",
            "2010-01-01T00:00:00Z",
            "2015-01-01T00:00:00Z",
        ),
        (
            DataPartition::Validation,
            "validation",
            "2015-01-01T00:00:00Z",
            "2018-01-01T00:00:00Z",
        ),
        (
            DataPartition::Sealed,
            "sealed",
            "2018-01-01T00:00:00Z",
            "2020-01-01T00:00:00Z",
        ),
    ] {
        let mut metadata = catalog::metadata();
        // Exercise trusted-runtime declarations at registration, never rewrite
        // immutable fixture provenance or claim these bytes came from a market.
        metadata.origin = origin;
        if origin == DataOrigin::Real {
            metadata.pit_status = PitStatus::Verified;
        }
        metadata.registered_ref = source.native_catalog_ref.clone();
        metadata.native_snapshot_ref = format!("cycle-fixture/{}/{name}", data.project);
        metadata.storage_version = format!("{name}-fixture-v1");
        metadata.partition = role;
        metadata.event_start = time(start);
        metadata.event_end = time(end);
        metadata.available_through = time("2020-01-02T00:00:00Z");
        metadata.row_count = DbCounter::new(1000).unwrap();
        metadata.universe.selection_asof = time("2010-01-01T00:00:00Z");
        metadata.universe.coverage_start = time("2010-01-01T00:00:00Z");
        metadata.universe.coverage_end = time("2021-01-01T00:00:00Z");
        metadata.universe.membership[0].valid_from = metadata.universe.coverage_start;
        metadata.universe.membership[0].available_at = metadata.universe.coverage_start;
        metadata.quality.checked_at = observed;
        let quality = &mut metadata.quality.datasets[0];
        quality.row_count = metadata.row_count;
        quality.first_event_ns = nanos(metadata.event_start);
        quality.last_event_ns = nanos(metadata.event_end - Duration::seconds(60));
        quality.available_through_ns = nanos(metadata.event_end - Duration::seconds(59));
        quality.selection.event_start_ns = nanos(metadata.event_start);
        quality.selection.event_end_ns = nanos(metadata.event_end);
        quality.selection.decision_cutoff_ns = nanos(metadata.available_through);
        quality.selection.maximum_rows = 1000;
        let request = DatasetRegister {
            schema_version: SchemaV1,
            source_id: source.id,
            grant_id: grant.id,
            expected_source_revision: source.revision,
            expected_runtime_revision: revision,
            native_storage_version: metadata.storage_version.clone(),
            existing_universe_version_id: universe,
        };
        let RegistrationPreparation::Execute(ticket) = store
            .prepare_dataset_registration(actor, &Id::new().to_string(), &request)
            .await
            .unwrap()
        else {
            panic!("fresh registration must not replay")
        };
        let reading = objects.clone();
        let writing = objects.clone();
        let dataset = store
            .complete_dataset_registration(
                *ticket,
                serde_json::to_vec(&metadata).unwrap(),
                move |id, size| {
                    let objects = reading.clone();
                    async move {
                        tokio::task::spawn_blocking(move || {
                            objects.read(id, size).map_err(|_| StoreError::Integrity)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                    }
                },
                move |publications| async move {
                    tokio::task::spawn_blocking(move || {
                        for object in publications {
                            writing
                                .put(object.id, &object.bytes)
                                .map_err(|_| StoreError::Integrity)?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(|_| StoreError::Integrity)?
                },
            )
            .await
            .unwrap()
            .resource;
        universe = Some(dataset.universe_version_id);
        data.universe = dataset.universe_version_id;
        match role {
            DataPartition::Discovery => data.discovery = dataset.id,
            DataPartition::Validation => data.validation = dataset.id,
            DataPartition::Sealed => data.sealed = dataset.id,
            DataPartition::Forward => unreachable!(),
        }
    }
}
