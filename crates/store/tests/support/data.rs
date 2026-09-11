//! Public Store data administration with real PostgreSQL and immutable local objects.
//! The native metadata and Runtime credential check are explicitly controlled fixtures.
#![allow(dead_code)]
#[path = "../../../../tests/support/research.rs"]
mod auth_fixture;
#[path = "../../../../tests/support/catalog_metadata.rs"]
pub mod catalog_fixture;
use chrono::{DateTime, Duration, Utc};
use contracts::{
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    control::{CommandResult, ProjectCreate},
    data::*,
    research::DataUse,
    runs::RunKind,
    settings::*,
    DbCounter, Id, SchemaV1,
};
use integrations::artifacts::ArtifactStore;
use sqlx::PgPool;
use std::sync::Arc;
use store::{
    authority::Actor,
    data_registration::{NativeMetadataPublication, RegistrationPreparation, RegistrationTicket},
    Store, StoreError,
};

pub struct Fixture {
    pub store: Store,
    pub actor: Actor,
    pub project: Id,
    pub runtime: RuntimeView,
    pub source: DataSourceView,
    pub grant: DataGrantView,
    pub proof: Id,
    pub objects: Arc<ArtifactStore>,
    pub directory: tempfile::TempDir,
}

pub async fn setup(pool: &PgPool, valid_until: Option<DateTime<Utc>>) -> Fixture {
    let (store, actor) = auth_fixture::operator(pool).await;
    let directory = tempfile::tempdir().unwrap();
    let objects = Arc::new(ArtifactStore::open(&directory.path().join("objects")).unwrap());
    let project = store
        .create_project(
            &actor,
            "license-project",
            &ProjectCreate {
                schema_version: SchemaV1,
                name: "Native source administration".into(),
                description: "Explicit controlled fixture, not a research qualification".into(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let proof_request = ArtifactCreate {
        schema_version: SchemaV1,
        project_id: project,
        kind: ResearchArtifactKind::Report,
        content: "{\"schema_version\":1,\"fixture_license\":\"No live or real-data evidence\"}"
            .into(),
    };
    let prepared = store
        .prepare_artifact_upload(&actor, "license-report", &proof_request)
        .await
        .unwrap();
    objects
        .put(prepared.id(), proof_request.content.as_bytes())
        .unwrap();
    let proof = prepared.publish().await.unwrap().resource.id;
    let runtime = store
        .create_runtime(
            &actor,
            "native-runtime",
            &RuntimeCreate {
                schema_version: SchemaV1,
                configuration: RuntimeConfigurationV1 {
                    name: "Controlled Runtime".into(),
                    endpoint: "https://runtime.example".into(),
                    tls_policy: TlsPolicy::SystemCa,
                    allowed_capabilities: vec![RunKind::DataValidate],
                    enabled: true,
                    development_http: false,
                },
                credential_ref: Id::new(),
                ca_certificate_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    let source = store
        .create_data_source(
            &actor,
            "native-source",
            &DataSourceCreate {
                schema_version: SchemaV1,
                name: "Controlled source".into(),
                runtime_id: runtime.id,
                native_catalog_ref: catalog_fixture::metadata().registered_ref,
                provider_kind: DataProviderKind::NautilusCatalog,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let issued: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let grant = store
        .create_data_grant(
            &actor,
            "native-grant",
            &DataGrantCreate {
                schema_version: SchemaV1,
                source_id: source.id,
                license_reference: "controlled-regression-license".into(),
                evidence_artifact_id: proof,
                allowed_uses: DataUse::Research,
                valid_from: issued - Duration::hours(1),
                valid_until,
            },
        )
        .await
        .unwrap()
        .resource;
    Fixture {
        store,
        actor,
        project,
        runtime,
        source,
        grant,
        proof,
        objects,
        directory,
    }
}

pub fn request(f: &Fixture) -> DatasetRegister {
    DatasetRegister {
        schema_version: SchemaV1,
        source_id: f.source.id,
        grant_id: f.grant.id,
        expected_source_revision: f.source.revision,
        expected_runtime_revision: f.runtime.revision,
        native_storage_version: catalog_fixture::metadata().storage_version,
        existing_universe_version_id: None,
    }
}

pub async fn ticket(f: &Fixture, key: &str, request: &DatasetRegister) -> RegistrationTicket {
    match f
        .store
        .prepare_dataset_registration(&f.actor, key, request)
        .await
        .unwrap()
    {
        RegistrationPreparation::Execute(ticket) => *ticket,
        RegistrationPreparation::Replay(_) => {
            panic!("expected a new controlled native observation")
        }
    }
}

pub async fn publish(
    objects: Arc<ArtifactStore>,
    publications: Vec<NativeMetadataPublication>,
) -> Result<(), StoreError> {
    tokio::task::spawn_blocking(move || {
        for item in publications {
            objects
                .put(item.id, &item.bytes)
                .map_err(|_| StoreError::Integrity)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| StoreError::Integrity)?
}

pub async fn read(
    objects: Arc<ArtifactStore>,
    id: Id,
    size: DbCounter,
) -> Result<Vec<u8>, StoreError> {
    tokio::task::spawn_blocking(move || objects.read(id, size).map_err(|_| StoreError::Integrity))
        .await
        .map_err(|_| StoreError::Integrity)?
}

pub async fn complete(
    f: &Fixture,
    ticket: RegistrationTicket,
    bytes: Vec<u8>,
) -> Result<CommandResult<DatasetView>, StoreError> {
    let reader = f.objects.clone();
    let publisher = f.objects.clone();
    f.store
        .complete_dataset_registration(
            ticket,
            bytes,
            move |id, size| read(reader.clone(), id, size),
            move |items| publish(publisher, items),
        )
        .await
}

pub async fn counts(pool: &PgPool) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.dataset_revisions),(SELECT count(*) FROM app.universe_versions),(SELECT count(*) FROM app.dataset_registration_evidence),(SELECT count(*) FROM app.command_receipts WHERE operation='DATASET_REGISTER')")
        .fetch_one(pool).await.unwrap()
}
