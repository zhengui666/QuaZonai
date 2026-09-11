//! Native data registration: short authorization transactions around a trusted
//! Runtime metadata read, then atomic immutable metadata/Universe/Dataset publication.
use crate::{authority::Actor, commands, data, db, lifecycle::RuntimeSnapshot, Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    catalogs::{NativeUniverseV1, RuntimeCatalogMetadataV1},
    control::{CommandResult, OperatorOperation},
    data::{DataLicenseState, DataSourceView, DatasetRegister, DatasetView, UniverseView},
    research::DataOrigin,
    DbCounter, Id,
};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};

const MAX_NATIVE_METADATA: usize = 1024 * 1024;

type Tx<'a> = Transaction<'a, Postgres>;

pub enum RegistrationPreparation {
    Replay(Box<CommandResult<DatasetView>>),
    Execute(Box<RegistrationTicket>),
}

/// Only Store-created tickets can enter native completion; no Deserialize or client ID.
pub struct RegistrationTicket {
    pub source: DataSourceView,
    pub runtime_snapshot: RuntimeSnapshot,
    pub request: DatasetRegister,
    actor: Actor,
    key: String,
    started_at: DateTime<Utc>,
}

/// Exact Store-selected metadata bytes for the existing immutable ArtifactStore.
/// This is a trusted native callback argument, not a public request DTO.
pub struct NativeMetadataPublication {
    pub id: Id,
    pub schema_name: &'static str,
    pub artifact_kind: &'static str,
    pub bytes: Vec<u8>,
}

async fn authorities(
    tx: &mut Tx<'_>,
    request: &DatasetRegister,
) -> Result<(DataSourceView, RuntimeSnapshot, DateTime<Utc>), StoreError> {
    let source_row = sqlx::query("SELECT * FROM app.data_sources WHERE id=$1 FOR SHARE")
        .bind(request.source_id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let source = data::source(&source_row)?;
    if source.revision != request.expected_source_revision {
        return Err(StoreError::RevisionConflict {
            current: source.revision,
        });
    }
    let runtime = sqlx::query("SELECT * FROM app.runtime_integrations WHERE id=$1 FOR SHARE")
        .bind(source.runtime_id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let revision = db::revision(runtime.try_get("revision")?)?;
    if revision != request.expected_runtime_revision {
        return Err(StoreError::RevisionConflict { current: revision });
    }
    let grant = sqlx::query(
        "SELECT source_id,valid_from,valid_until FROM app.data_use_grants WHERE id=$1 FOR SHARE",
    )
    .bind(request.grant_id.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::NotFound)?;
    if db::id(grant.try_get("source_id")?)? != source.id {
        return Err(StoreError::Invalid("data_use_grant_id"));
    }
    let now = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let revoked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.data_use_revocations WHERE grant_id=$1 AND effective_at<=$2)")
        .bind(request.grant_id.as_uuid()).bind(now).fetch_one(&mut **tx).await?;
    if !source.enabled
        || !runtime.try_get::<bool, _>("enabled")?
        || domain::data::license_state(
            grant.try_get("valid_from")?,
            grant.try_get("valid_until")?,
            revoked,
            now,
        ) != DataLicenseState::Active
    {
        return Err(StoreError::Invalid("data_source_or_grant_unavailable"));
    }
    let snapshot = RuntimeSnapshot {
        schema_version: contracts::SchemaV1,
        endpoint: runtime.try_get("endpoint")?,
        credential_ref: runtime.try_get("credential_ref")?,
        tls_policy: runtime.try_get("tls_policy")?,
        ca_certificate_ref: runtime.try_get("ca_certificate_ref")?,
        development_http: runtime.try_get("development_http")?,
        protocol_version: runtime.try_get("protocol_version")?,
        allowed_capabilities: runtime.try_get("allowed_capabilities")?,
    };
    Ok((source, snapshot, now))
}

fn fresh(ticket: &RegistrationTicket, now: DateTime<Utc>) -> Result<(), StoreError> {
    if now < ticket.started_at || now > ticket.started_at + Duration::seconds(20) {
        return Err(StoreError::Invalid("native_registration_expired"));
    }
    Ok(())
}

fn publication(
    schema_name: &'static str,
    artifact_kind: &'static str,
    bytes: Vec<u8>,
) -> Result<NativeMetadataPublication, StoreError> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_METADATA + 1024 {
        return Err(StoreError::Invalid("native_metadata_size"));
    }
    Ok(NativeMetadataPublication {
        id: Id::new(),
        schema_name,
        artifact_kind,
        bytes,
    })
}

fn member_document(value: &NativeUniverseV1) -> Value {
    json!({"schema_version":1,"members":value.membership})
}
fn instrument_document(value: &NativeUniverseV1) -> Value {
    json!({"schema_version":1,"instruments":value.instrument_definitions})
}

async fn read_metadata<F, Fut>(
    tx: &mut Tx<'_>,
    artifact: Id,
    schema: &str,
    origin: DataOrigin,
    reader: &mut F,
) -> Result<Value, StoreError>
where
    F: FnMut(Id, DbCounter) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row = sqlx::query("SELECT byte_count,origin FROM app.artifacts WHERE id=$1 AND schema_name=$2 AND schema_version='1' AND media_type='application/json' AND access_class='OPERATOR' AND created_by='RUNTIME' AND storage_backend='LOCAL' AND storage_object_ref=$3 AND storage_version='1'")
        .bind(artifact.as_uuid()).bind(schema).bind(artifact.to_string()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("native_metadata_artifact"))?;
    if db::enum_value::<DataOrigin>(&row, "origin")? != origin {
        return Err(StoreError::Invalid("native_metadata_origin"));
    }
    let count: DbCounter = row
        .try_get::<i64, _>("byte_count")?
        .to_string()
        .try_into()
        .map_err(|_| StoreError::Integrity)?;
    if count.get() == 0 || count.get() > (MAX_NATIVE_METADATA + 1024) as u64 {
        return Err(StoreError::Invalid("native_metadata_size"));
    }
    let bytes = reader(artifact, count).await?;
    if bytes.len() as u64 != count.get() {
        return Err(StoreError::Integrity);
    }
    serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)
}

async fn matching_universe<F, Fut>(
    tx: &mut Tx<'_>,
    id: Id,
    native: &NativeUniverseV1,
    origin: DataOrigin,
    reader: &mut F,
) -> Result<UniverseView, StoreError>
where
    F: FnMut(Id, DbCounter) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let stored = data::universe_in_tx(tx, id).await?;
    if stored.name != native.name
        || stored.calendar_ref != native.calendar_ref
        || stored.calendar_version != native.calendar_version
        || stored.selection_asof != native.selection_asof
        || stored.has_historical_membership != native.has_historical_membership
        || stored.coverage_start != native.coverage_start
        || stored.coverage_end != native.coverage_end
    {
        return Err(StoreError::NativeIdentityConflict);
    }
    let membership = read_metadata(
        tx,
        stored.membership_artifact_id,
        "qz.universe_membership",
        origin,
        reader,
    )
    .await?;
    let definitions = read_metadata(
        tx,
        stored.instrument_definitions_artifact_id,
        "qz.instrument_definitions",
        origin,
        reader,
    )
    .await?;
    if membership != member_document(native) || definitions != instrument_document(native) {
        return Err(StoreError::NativeIdentityConflict);
    }
    Ok(stored)
}

impl Store {
    pub async fn prepare_dataset_registration(
        &self,
        actor: &Actor,
        key: &str,
        request: &DatasetRegister,
    ) -> Result<RegistrationPreparation, StoreError> {
        domain::data::dataset_register(request)?;
        let mut tx = self.pool.begin().await?;
        // Registration targets an existing Source, like RuntimeProbe targets an
        // existing Runtime while publishing a new observation. The Dataset is not a
        // client-assigned one-time-grant target, so native-identity replay is exact.
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DatasetRegister,
            key,
            Some(request.source_id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(RegistrationPreparation::Replay(Box::new(result)));
        }
        let (source, runtime_snapshot, started_at) = authorities(&mut tx, request).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        tx.commit().await?;
        Ok(RegistrationPreparation::Execute(Box::new(
            RegistrationTicket {
                source,
                runtime_snapshot,
                request: request.clone(),
                actor: actor.clone(),
                key: key.to_owned(),
                started_at,
            },
        )))
    }

    pub async fn complete_dataset_registration<F, Fut, P, Published>(
        &self,
        ticket: RegistrationTicket,
        raw_metadata: Vec<u8>,
        mut reader: F,
        publish: P,
    ) -> Result<CommandResult<DatasetView>, StoreError>
    where
        F: FnMut(Id, DbCounter) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(Vec<NativeMetadataPublication>) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        if raw_metadata.is_empty() || raw_metadata.len() > MAX_NATIVE_METADATA {
            return Err(StoreError::Invalid("native_metadata_size"));
        }
        let native: RuntimeCatalogMetadataV1 = serde_json::from_slice(&raw_metadata)
            .map_err(|_| StoreError::Invalid("native_metadata_contract"))?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            &ticket.actor,
            OperatorOperation::DatasetRegister,
            &ticket.key,
            Some(ticket.request.source_id),
            db::json(&ticket.request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let (source, _, now) = authorities(&mut tx, &ticket.request).await?;
        fresh(&ticket, now)?;
        domain::data::registration_metadata(&native, &ticket.request, &source, now)?;
        let existing: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.dataset_revisions WHERE source_id=$1 AND native_snapshot_ref=$2 AND native_storage_version=$3")
            .bind(source.id.as_uuid()).bind(&native.native_snapshot_ref).bind(&native.storage_version).fetch_optional(&mut *tx).await?;
        if let Some(id) = existing {
            let id = db::id(id)?;
            let previous = data::dataset_in_tx(&mut tx, id).await?;
            if previous.data_use_grant_id != ticket.request.grant_id
                || ticket
                    .request
                    .existing_universe_version_id
                    .is_some_and(|expected| previous.universe_version_id != expected)
            {
                return Err(StoreError::NativeIdentityConflict);
            }
            let metadata = previous
                .native_metadata_artifact_id
                .ok_or(StoreError::Invalid("native_dataset_legacy_registration"))?;
            let original = read_metadata(
                &mut tx,
                metadata,
                "qz.native_catalog_metadata",
                previous.origin,
                &mut reader,
            )
            .await?;
            // Both sides are received JSON, including their RFC3339 spelling.
            // Serializing only the new typed DTO would canonicalize its dates.
            let received: Value = serde_json::from_slice(&raw_metadata)
                .map_err(|_| StoreError::Invalid("native_metadata_contract"))?;
            if original != received {
                return Err(StoreError::NativeIdentityConflict);
            }
            commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
            // A different-key registration still consumes current permission.
            // An immutable-object read can cross a grant expiry even while its
            // row is locked, so recheck the real clock after that native I/O.
            let (_, _, now) = authorities(&mut tx, &ticket.request).await?;
            fresh(&ticket, now)?;
            let current = data::dataset_in_tx(&mut tx, id).await?;
            let result = commands::finish(&mut tx, prepared, current, 200).await?;
            tx.commit().await?;
            return Ok(result);
        }
        let mut objects = Vec::new();
        let universe_id = if let Some(id) = ticket.request.existing_universe_version_id {
            matching_universe(&mut tx, id, &native.universe, native.origin, &mut reader).await?;
            id
        } else {
            let membership = publication(
                "qz.universe_membership",
                "REPORT",
                serde_json::to_vec(&member_document(&native.universe))
                    .map_err(|_| StoreError::Integrity)?,
            )?;
            let definitions = publication(
                "qz.instrument_definitions",
                "REPORT",
                serde_json::to_vec(&instrument_document(&native.universe))
                    .map_err(|_| StoreError::Integrity)?,
            )?;
            objects.push(membership);
            objects.push(definitions);
            Id::new()
        };
        let metadata = publication("qz.native_catalog_metadata", "REPORT", raw_metadata)?;
        let quality = publication(
            "qz.data_quality",
            "DATA_QUALITY",
            serde_json::to_vec(&native.quality).map_err(|_| StoreError::Integrity)?,
        )?;
        let metadata_id = metadata.id;
        let quality_id = quality.id;
        objects.push(metadata);
        objects.push(quality);
        // Retain only nonsecret metadata needed for the same transaction's inserts.
        let descriptors = objects
            .iter()
            .map(|object| {
                (
                    object.id,
                    object.schema_name,
                    object.artifact_kind,
                    object.bytes.len() as i64,
                )
            })
            .collect::<Vec<_>>();
        publish(objects).await?;
        commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
        let (_, _, observed_at) = authorities(&mut tx, &ticket.request).await?;
        fresh(&ticket, observed_at)?;
        for (id, schema, kind, bytes) in &descriptors {
            sqlx::query("INSERT INTO app.artifacts(id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'application/json',$3,'1','LOCAL',$4,'1',$5,'OPERATOR',$6,'RUNTIME','REFERENCED')")
                .bind(id.as_uuid()).bind(kind).bind(schema).bind(id.to_string()).bind(bytes).bind(db::code(&native.origin)?)
                .execute(&mut *tx).await?;
        }
        if ticket.request.existing_universe_version_id.is_none() {
            let membership = descriptors
                .iter()
                .find(|descriptor| descriptor.1 == "qz.universe_membership")
                .ok_or(StoreError::Integrity)?
                .0;
            let definitions = descriptors
                .iter()
                .find(|descriptor| descriptor.1 == "qz.instrument_definitions")
                .ok_or(StoreError::Integrity)?
                .0;
            let universe = &native.universe;
            sqlx::query("INSERT INTO app.universe_versions(id,name,membership_artifact_id,instrument_definition_artifact_id,calendar_ref,calendar_version,selection_asof,has_historical_membership,coverage_start,coverage_end) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                .bind(universe_id.as_uuid()).bind(&universe.name).bind(membership.as_uuid()).bind(definitions.as_uuid())
                .bind(&universe.calendar_ref).bind(&universe.calendar_version).bind(universe.selection_asof)
                .bind(universe.has_historical_membership).bind(universe.coverage_start).bind(universe.coverage_end)
                .execute(&mut *tx).await?;
        }
        let id = Id::new();
        sqlx::query("INSERT INTO app.dataset_revisions(id,source_id,data_use_grant_id,native_snapshot_ref,native_storage_version,universe_version_id,schema_version,data_kind,partition_role,event_start,event_end,available_through,row_count,timezone,quality_artifact_id,pit_status,revision_policy,origin) VALUES($1,$2,$3,$4,$5,$6,'1',$7,$8,$9,$10,$11,$12,'UTC',$13,$14,$15,$16)")
            .bind(id.as_uuid()).bind(source.id.as_uuid()).bind(ticket.request.grant_id.as_uuid())
            .bind(&native.native_snapshot_ref).bind(&native.storage_version).bind(universe_id.as_uuid())
            .bind(db::code(&native.data_kind)?).bind(native.partition.code()).bind(native.event_start).bind(native.event_end)
            .bind(native.available_through).bind(native.row_count.get() as i64).bind(quality_id.as_uuid())
            .bind(db::code(&native.pit_status)?).bind(db::code(&native.revision_policy)?).bind(db::code(&native.origin)?)
            .execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.dataset_registration_evidence(dataset_revision_id,native_metadata_artifact_id,source_revision,runtime_revision,observed_at) VALUES($1,$2,$3,$4,$5)")
            .bind(id.as_uuid()).bind(metadata_id.as_uuid()).bind(ticket.request.expected_source_revision.get() as i64)
            .bind(ticket.request.expected_runtime_revision.get() as i64).bind(observed_at).execute(&mut *tx).await?;
        let view = data::dataset_in_tx(&mut tx, id).await?;
        let result = commands::finish(&mut tx, prepared, view, 200).await?;
        tx.commit().await?;
        Ok(result)
    }
}
