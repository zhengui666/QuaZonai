//! Original feature buffers and their immutable registered Dataset relationship.
//! Identity reads remain available after revocation; new use is authorized by
//! research's existing source/runtime/grant checks before native object reads.
use crate::{
    authority::{self, Actor},
    commands, data,
    data_registration::NativeMetadataPublication,
    db,
    research::{self, DatasetUse},
    Store, StoreError,
};
use contracts::{
    catalogs::RuntimeCatalogMetadataV1,
    control::{CommandResult, MachineScope, OperatorOperation},
    data::{
        DatasetView, RecordedFeatureListQuery, RecordedFeatureListV1, RecordedFeatureRegisterV1,
        RecordedFeatureSourceBindingV1, RecordedFeatureViewV1,
    },
    research::{ArtifactInputRole, DataOrigin, DataPartition, InputPurpose},
    runtime_jobs::RuntimeInputV1,
    science::FeatureObservationsV1,
    DbCounter, Id, Timestamp,
};
use sqlx::{Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

pub(crate) struct FeatureInput {
    pub input: RuntimeInputV1,
    pub origin: DataOrigin,
    pub binding: Option<RecordedFeatureSourceBindingV1>,
}

/// Resolve immutable identity only. Callers authorize all actual source Datasets
/// together before I/O; history/export must not introduce a current license gate.
pub(crate) async fn resolve(
    tx: &mut Tx<'_>,
    project: Id,
    artifact: Id,
    expected_dataset: Option<Id>,
) -> Result<FeatureInput, StoreError> {
    let row = sqlx::query("SELECT a.schema_name,a.byte_count,a.origin,a.created_by,a.producer_run_id,a.producer_attempt_id,f.dataset_revision_id,f.native_metadata_artifact_id,f.feature_part_key,d.source_id,d.data_use_grant_id,d.origin AS dataset_origin,d.pit_status,d.revision_policy,d.partition_role,e.native_metadata_artifact_id AS registered_metadata FROM app.artifacts a LEFT JOIN app.feature_artifact_sources f ON f.artifact_id=a.id AND f.project_id=a.project_id LEFT JOIN app.dataset_revisions d ON d.id=f.dataset_revision_id LEFT JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id WHERE a.id=$1 AND a.project_id=$2 AND a.kind='PARAMETERS' AND a.schema_version='1' AND a.media_type='application/json' AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1' AND a.access_class='RESEARCH'")
        .bind(artifact.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(StoreError::Invalid("feature_artifact_source"))?;
    let bytes = row.try_get::<i64, _>("byte_count")?;
    if bytes <= 0 {
        return Err(StoreError::Invalid("feature_artifact_size"));
    }
    let origin = db::enum_value(&row, "origin")?;
    let binding = match row.try_get::<String, _>("schema_name")?.as_str() {
        "qz.research_parameters" => {
            if db::optional_id(&row, "dataset_revision_id")?.is_some() {
                return Err(StoreError::Integrity);
            }
            None
        }
        "qz.feature_observations" => {
            let dataset =
                db::optional_id(&row, "dataset_revision_id")?.ok_or(StoreError::Integrity)?;
            let metadata = db::optional_id(&row, "native_metadata_artifact_id")?
                .ok_or(StoreError::Integrity)?;
            if expected_dataset.is_some_and(|expected| expected != dataset) {
                return Err(StoreError::Invalid("feature_dataset_binding"));
            }
            if db::optional_id(&row, "registered_metadata")? != Some(metadata)
                || db::enum_value::<DataOrigin>(&row, "dataset_origin")? != origin
                || row.try_get::<String, _>("created_by")? != "IMPORT"
                || db::optional_id(&row, "producer_run_id")?.is_some()
                || db::optional_id(&row, "producer_attempt_id")?.is_some()
                || db::enum_value::<DataPartition>(&row, "partition_role")? == DataPartition::Sealed
            {
                return Err(StoreError::Integrity);
            }
            let binding = RecordedFeatureSourceBindingV1 {
                dataset_revision_id: dataset,
                source_id: db::id(row.try_get("source_id")?)?,
                data_use_grant_id: db::id(row.try_get("data_use_grant_id")?)?,
                native_metadata_artifact_id: metadata,
                feature_part_key: row.try_get("feature_part_key")?,
                origin,
                pit_status: db::enum_value(&row, "pit_status")?,
                revision_policy: db::enum_value(&row, "revision_policy")?,
            };
            domain::data::recorded_feature_binding(&binding).map_err(|_| StoreError::Integrity)?;
            Some(binding)
        }
        _ => return Err(StoreError::Invalid("feature_artifact_schema")),
    };
    Ok(FeatureInput {
        input: RuntimeInputV1::Artifact {
            artifact_id: artifact,
            storage_version: "1".into(),
            byte_count: DbCounter::new(bytes as u64).map_err(|_| StoreError::Integrity)?,
            role: ArtifactInputRole::Parameters,
        },
        origin,
        binding,
    })
}

pub(crate) async fn read<R, Read>(
    feature: &FeatureInput,
    reader: &mut R,
) -> Result<FeatureObservationsV1, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let RuntimeInputV1::Artifact {
        artifact_id,
        byte_count,
        ..
    } = &feature.input
    else {
        return Err(StoreError::Integrity);
    };
    let bytes = reader(*artifact_id, *byte_count).await?;
    if bytes.len() as u64 != byte_count.get() {
        return Err(StoreError::Integrity);
    }
    serde_json::from_slice(&bytes).map_err(|_| StoreError::Invalid("feature_artifact_document"))
}

/// Partition records source identity; purpose records what the new operation
/// uses it for, including historical observations in a current decision.
pub(crate) async fn dataset_use(
    tx: &mut Tx<'_>,
    id: Id,
    purpose: InputPurpose,
    cutoff: Option<Timestamp>,
) -> Result<DatasetUse, StoreError> {
    let role = sqlx::query("SELECT partition_role FROM app.dataset_revisions WHERE id=$1")
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    Ok(DatasetUse {
        id,
        role: db::enum_value(&role, "partition_role")?,
        field: "feature.dataset_revision_id".into(),
        cutoff,
        purpose,
    })
}

async fn metadata<R, Read>(
    tx: &mut Tx<'_>,
    dataset: &DatasetView,
    reader: &mut R,
) -> Result<RuntimeCatalogMetadataV1, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let id = dataset
        .native_metadata_artifact_id
        .ok_or(StoreError::Invalid("native_dataset_legacy_registration"))?;
    let row = sqlx::query("SELECT a.byte_count,s.native_catalog_ref FROM app.artifacts a JOIN app.data_sources s ON s.id=$2 WHERE a.id=$1 AND a.kind='REPORT' AND a.schema_name='qz.native_catalog_metadata' AND a.schema_version='1' AND a.media_type='application/json' AND a.project_id IS NULL AND a.producer_run_id IS NULL AND a.producer_attempt_id IS NULL AND a.access_class='OPERATOR' AND a.created_by='RUNTIME' AND a.origin=$3 AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1' AND a.byte_count>0")
        .bind(id.as_uuid()).bind(dataset.source_id.as_uuid()).bind(db::code(&dataset.origin)?)
        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Integrity)?;
    let size = DbCounter::new(row.try_get::<i64, _>("byte_count")? as u64)
        .map_err(|_| StoreError::Integrity)?;
    let bytes = reader(id, size).await?;
    if bytes.len() as u64 != size.get() {
        return Err(StoreError::Integrity);
    }
    let native: RuntimeCatalogMetadataV1 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    domain::catalogs::metadata(
        &native,
        dataset
            .registration_observed_at
            .ok_or(StoreError::Integrity)?,
    )
    .map_err(|_| StoreError::Integrity)?;
    if native.registered_ref != row.try_get::<String, _>("native_catalog_ref")?
        || native.native_snapshot_ref != dataset.native_snapshot_ref
        || native.storage_version != dataset.storage_version
        || native.partition != dataset.partition
        || native.origin != dataset.origin
        || native.pit_status != dataset.pit_status
        || native.revision_policy != dataset.revision_policy
        || native.event_start != dataset.event_start
        || native.event_end != dataset.event_end
        || native.available_through != dataset.available_through
        || native.row_count != dataset.row_count
    {
        return Err(StoreError::Integrity);
    }
    Ok(native)
}

async fn view<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    artifact: Id,
    dataset: &DatasetView,
    native: &RuntimeCatalogMetadataV1,
    reader: &mut R,
) -> Result<RecordedFeatureViewV1, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let feature = resolve(tx, project, artifact, Some(dataset.id)).await?;
    let content = read(&feature, reader).await?;
    let binding = feature.binding.ok_or(StoreError::Integrity)?;
    let RuntimeInputV1::Artifact { byte_count, .. } = feature.input else {
        return Err(StoreError::Integrity);
    };
    domain::data::recorded_feature_matches(native, &binding.feature_part_key, byte_count, &content)
        .map_err(|_| StoreError::Integrity)?;
    let descriptor = native
        .recorded_feature_inputs
        .as_ref()
        .ok_or(StoreError::Integrity)?;
    let fragment = descriptor
        .fragments
        .iter()
        .find(|part| part.part_key == binding.feature_part_key)
        .ok_or(StoreError::Integrity)?
        .clone();
    let created_at = sqlx::query_scalar("SELECT created_at FROM app.artifacts WHERE id=$1")
        .bind(artifact.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    Ok(RecordedFeatureViewV1 {
        artifact_id: artifact,
        project_id: project,
        source_binding: binding,
        partition: descriptor.partition,
        source_selection_start_ns: descriptor.source_selection_start_ns,
        source_selection_end_ns: descriptor.source_selection_end_ns,
        fragment,
        created_at,
    })
}

impl Store {
    pub async fn register_recorded_feature<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &RecordedFeatureRegisterV1,
        mut reader: R,
        publish: P,
    ) -> Result<CommandResult<RecordedFeatureViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeMetadataPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let intent = request.intent().map_err(|_| StoreError::Integrity)?;
        domain::data::recorded_feature_intent(&intent)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::RecordedFeatureRegister,
            key,
            Some(request.dataset_revision_id),
            db::json(&intent)?,
        )
        .await?;
        if let Actor::Machine { .. } = actor {
            authority::machine(&mut tx, actor, true)
                .await?
                .project(request.project_id)?;
        }
        let dataset = data::dataset_in_tx(&mut tx, request.dataset_revision_id).await?;
        let replay = prepared.replay::<RecordedFeatureViewV1>()?;
        // Same-key replay preserves the past receipt. A fresh key must consume
        // current source permission even when it resolves the same immutable part.
        let uses = [dataset_use(
            &mut tx,
            dataset.id,
            match dataset.partition {
                DataPartition::Discovery => InputPurpose::Discovery,
                DataPartition::Validation => InputPurpose::Validation,
                DataPartition::Forward => InputPurpose::Forward,
                DataPartition::Sealed => {
                    return Err(StoreError::Invalid("recorded_feature_sealed"))
                }
            },
            Some(dataset.available_through),
        )
        .await?];
        if replay.is_none() {
            research::project_for_write(&mut tx, request.project_id).await?;
            research::validate_dataset_uses(&mut tx, &uses, None).await?;
        }
        let existing = sqlx::query("SELECT artifact_id FROM app.feature_artifact_sources WHERE project_id=$1 AND dataset_revision_id=$2 AND feature_part_key=$3")
            .bind(request.project_id.as_uuid()).bind(dataset.id.as_uuid()).bind(&request.feature_part_key)
            .fetch_optional(&mut *tx).await?;
        let existing = existing
            .as_ref()
            .map(|row| db::id(row.try_get("artifact_id")?))
            .transpose()?;
        let content = domain::data::recorded_feature_register(request)?;
        let native = metadata(&mut tx, &dataset, &mut reader).await?;
        // The immutable metadata constrains the selected part's native shape and
        // clocks. It cannot establish equality to attachment bytes not supplied
        // here; subsequent registrations compare the accepted original buffer.
        if let Err(error) = domain::data::recorded_feature_matches(
            &native,
            &request.feature_part_key,
            intent.byte_count,
            &content,
        ) {
            return Err(if existing.is_some() {
                StoreError::NativeIdentityConflict
            } else {
                error.into()
            });
        }
        if let Some(existing) = existing {
            let current = view(
                &mut tx,
                request.project_id,
                existing,
                &dataset,
                &native,
                &mut reader,
            )
            .await?;
            // Reuse an identity only for the exact original stored buffer, including
            // same-length changes and differences in otherwise equivalent JSON.
            if reader(existing, intent.byte_count).await? != request.content.as_bytes() {
                return Err(if replay.is_some() {
                    StoreError::IdempotencyConflict
                } else {
                    StoreError::NativeIdentityConflict
                });
            }
            if let Some(result) = replay {
                if db::json(&result.resource)? != db::json(&current)? {
                    return Err(StoreError::Integrity);
                }
                commands::recheck_authority(&mut tx, actor, &prepared).await?;
                tx.commit().await?;
                return Ok(result);
            }
            commands::recheck_authority(&mut tx, actor, &prepared).await?;
            research::validate_dataset_uses(&mut tx, &uses, None).await?;
            let result = commands::finish(&mut tx, prepared, current, 201).await?;
            tx.commit().await?;
            return Ok(result);
        }
        if replay.is_some() {
            return Err(StoreError::Integrity);
        }
        let id = Id::new();
        publish(NativeMetadataPublication {
            id,
            schema_name: "qz.feature_observations",
            artifact_kind: "PARAMETERS",
            bytes: request.content.as_bytes().to_vec(),
        })
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        research::validate_dataset_uses(&mut tx, &uses, None).await?;
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.feature_observations','1','LOCAL',$3,'1',$4,'RESEARCH',$5,'IMPORT','REFERENCED')")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(id.to_string()).bind(intent.byte_count.get() as i64)
            .bind(db::code(&dataset.origin)?).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.feature_artifact_sources(artifact_id,project_id,dataset_revision_id,native_metadata_artifact_id,feature_part_key) VALUES($1,$2,$3,$4,$5)")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(dataset.id.as_uuid())
            .bind(dataset.native_metadata_artifact_id.ok_or(StoreError::Integrity)?.as_uuid())
            .bind(&request.feature_part_key).execute(&mut *tx).await?;
        let result = view(
            &mut tx,
            request.project_id,
            id,
            &dataset,
            &native,
            &mut reader,
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        research::validate_dataset_uses(&mut tx, &uses, None).await?;
        let result = commands::finish(&mut tx, prepared, result, 201).await?;
        // Entrypoint cleanup must wait on the original operator authority lock
        // via discard_unpublished_operator_artifact, including unknown commits.
        tx.commit().await?;
        Ok(result)
    }

    pub async fn list_recorded_features<R, Read>(
        &self,
        actor: &Actor,
        dataset_id: Id,
        query: &RecordedFeatureListQuery,
        mut reader: R,
    ) -> Result<RecordedFeatureListV1, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, query.project_id, MachineScope::ResearchRead)
            .await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.projects WHERE id=$1)")
                .bind(query.project_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let dataset = data::dataset_in_tx(&mut tx, dataset_id).await?;
        let ids: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT artifact_id FROM app.feature_artifact_sources WHERE project_id=$1 AND dataset_revision_id=$2 ORDER BY feature_part_key")
            .bind(query.project_id.as_uuid()).bind(dataset_id.as_uuid()).fetch_all(&mut *tx).await?;
        let mut items = Vec::with_capacity(ids.len());
        if !ids.is_empty() {
            let native = metadata(&mut tx, &dataset, &mut reader).await?;
            for id in ids {
                items.push(
                    view(
                        &mut tx,
                        query.project_id,
                        db::id(id)?,
                        &dataset,
                        &native,
                        &mut reader,
                    )
                    .await?,
                );
            }
        }
        authority::read_project(&mut tx, actor, query.project_id, MachineScope::ResearchRead)
            .await?;
        tx.commit().await?;
        Ok(RecordedFeatureListV1 { items })
    }
}
