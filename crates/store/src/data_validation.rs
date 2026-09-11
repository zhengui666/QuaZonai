//! Formal native DATA_VALIDATE: authorization, immutable parameters, Run, task and
//! PGMQ notification share the same existing Operator/Run admission transaction.
use crate::{
    authority::Actor,
    commands, db,
    lifecycle::{
        native::{bind_task, NativeObjectPublication, NativeTaskDefinition},
        StandaloneRunSubmission,
    },
    Store, StoreError,
};
use chrono::{DateTime, Utc};
use contracts::{
    artifacts::ArtifactAccess,
    catalogs::RuntimeCatalogMetadataV1,
    control::{CommandResult, OperatorOperation},
    data::DataValidateRequest,
    execution::{NativeDatasetSelectionV1, NativeTaskParametersV1},
    research::{ArtifactInputRole, DataOrigin, DataPartition},
    runs::{RunKind, RunSnapshotV1},
    runtime_jobs::RuntimeInputV1,
    DbCounter, Id, SchemaV1,
};
use sqlx::Row;

fn counter(value: i64) -> Result<DbCounter, StoreError> {
    DbCounter::new(u64::try_from(value).map_err(|_| StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)
}
fn input(field: &str) -> StoreError {
    domain::research::invalid(field, "NATIVE_DATA_VALIDATION_INPUT_REQUIRED").into()
}
fn combine_origin(current: DataOrigin, next: DataOrigin) -> DataOrigin {
    use DataOrigin::*;
    match (current, next) {
        (LegacyUnknown, _) | (_, LegacyUnknown) => LegacyUnknown,
        (Fixture, _) | (_, Fixture) => Fixture,
        (Synthetic, _) | (_, Synthetic) => Synthetic,
        (Real, Real) => Real,
    }
}

impl Store {
    pub async fn start_data_validation<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &DataValidateRequest,
        mut read: R,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::data::validate_request(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DataValidate,
            key,
            Some(request.input_set_id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        crate::research::project_for_write(&mut tx, request.project_id).await?;
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            request.input_set_id,
            request.project_id,
            request.runtime_id,
        )
        .await?;
        let header = sqlx::query("SELECT purpose,decision_cutoff FROM app.input_sets WHERE id=$1 AND project_id=$2 AND frozen_at IS NOT NULL")
            .bind(request.input_set_id.as_uuid()).bind(request.project_id.as_uuid())
            .fetch_optional(&mut *tx).await?.ok_or_else(|| input("input_set_id"))?;
        if !matches!(
            header.try_get::<String, _>("purpose")?.as_str(),
            "DISCOVERY" | "VALIDATION"
        ) {
            return Err(input("input_set_id"));
        }
        let cutoff: DateTime<Utc> = header.try_get("decision_cutoff")?;
        let cutoff_ns = u64::try_from(
            cutoff
                .timestamp_nanos_opt()
                .ok_or_else(|| input("decision_cutoff"))?,
        )
        .map_err(|_| input("decision_cutoff"))?;
        let rows = sqlx::query("SELECT i.ordinal,i.role,i.artifact_id,d.*,s.runtime_id,s.native_catalog_ref,e.native_metadata_artifact_id,a.byte_count AS metadata_bytes FROM app.input_set_items i LEFT JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id LEFT JOIN app.data_sources s ON s.id=d.source_id LEFT JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id LEFT JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE i.input_set_id=$1 ORDER BY i.ordinal LIMIT 256")
            .bind(request.input_set_id.as_uuid()).fetch_all(&mut *tx).await?;
        if rows.is_empty() || rows.len() > 255 {
            return Err(input("input_set_id"));
        }
        let mut selections = Vec::with_capacity(rows.len());
        let mut inputs = Vec::with_capacity(rows.len() + 1);
        let mut origin = DataOrigin::Real;
        for row in rows {
            if db::optional_id(&row, "artifact_id")?.is_some()
                || db::optional_id(&row, "runtime_id")? != Some(request.runtime_id)
            {
                return Err(input("input_set_id"));
            }
            let id = db::optional_id(&row, "id")?.ok_or_else(|| input("input_set_id"))?;
            let metadata_id = db::optional_id(&row, "native_metadata_artifact_id")?
                .ok_or_else(|| input("dataset_registration"))?;
            let bytes = counter(
                row.try_get::<Option<i64>, _>("metadata_bytes")?
                    .ok_or(StoreError::Integrity)?,
            )?;
            if bytes.get() == 0 || bytes.get() > 1024 * 1024 {
                return Err(StoreError::Integrity);
            }
            let raw = read(metadata_id, bytes).await?;
            if raw.len() as u64 != bytes.get() {
                return Err(StoreError::Integrity);
            }
            let native: RuntimeCatalogMetadataV1 =
                serde_json::from_slice(&raw).map_err(|_| StoreError::Integrity)?;
            let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
                .fetch_one(&mut *tx)
                .await?;
            domain::catalogs::metadata(&native, now)?;
            let role: DataPartition = db::enum_value(&row, "role")?;
            let registered_ref: String = row.try_get("native_catalog_ref")?;
            let version: String = row.try_get("native_storage_version")?;
            let row_origin: DataOrigin = db::enum_value(&row, "origin")?;
            if native.registered_ref != registered_ref
                || native.storage_version != version
                || native.native_snapshot_ref != row.try_get::<String, _>("native_snapshot_ref")?
                || native.origin != row_origin
                || native.partition != role
                || native.event_start != row.try_get::<DateTime<Utc>, _>("event_start")?
                || native.event_end != row.try_get::<DateTime<Utc>, _>("event_end")?
                || native.available_through
                    != row.try_get::<DateTime<Utc>, _>("available_through")?
                || native.row_count != counter(row.try_get("row_count")?)?
                || !matches!(role, DataPartition::Discovery | DataPartition::Validation)
            {
                return Err(StoreError::Integrity);
            }
            let [quality] = native.quality.datasets.as_slice() else {
                return Err(StoreError::Integrity);
            };
            let mut selection = quality.selection.clone();
            // A later project cutoff cannot widen the immutable catalog's attested
            // visibility ceiling. Validating that old snapshot uses the earlier bound.
            selection.decision_cutoff_ns =
                DbCounter::new(cutoff_ns.min(selection.decision_cutoff_ns.get()))
                    .map_err(|_| input("decision_cutoff"))?;
            if selection.event_end_ns > selection.decision_cutoff_ns {
                return Err(input("decision_cutoff"));
            }
            selections.push(NativeDatasetSelectionV1 {
                dataset_revision_id: id,
                selection,
            });
            inputs.push(RuntimeInputV1::Dataset {
                revision_id: id,
                registered_ref,
                storage_version: version,
                role,
            });
            origin = combine_origin(origin, row_origin);
        }
        let parameters = NativeTaskParametersV1::ValidateData {
            schema_version: SchemaV1,
            selections,
        };
        let bytes = serde_json::to_vec(&parameters).map_err(|_| StoreError::Integrity)?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(input("input_set_id"));
        }
        let parameter_id = Id::new();
        let parameter_bytes = bytes.len() as i64;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: parameter_id,
            storage_version: "1".into(),
            byte_count: counter(parameter_bytes)?,
            role: ArtifactInputRole::Parameters,
        });
        let capabilities = crate::runtime::require_capabilities(
            &mut tx,
            request.runtime_id,
            request.expected_runtime_revision,
            RunKind::DataValidate,
        )
        .await?;
        domain::runtime::job_limits(&capabilities, &request.limits)?;
        let cpu = u16::try_from(
            request
                .limits
                .cpu_seconds
                .get()
                .div_ceil(u64::from(request.limits.wall_seconds)),
        )
        .map_err(|_| domain::DomainError::CapabilityUnavailable("native_cpu_capacity"))?;
        if cpu == 0 || cpu > capabilities.max_cpu {
            return Err(domain::DomainError::CapabilityUnavailable("native_cpu_capacity").into());
        }
        let image_ref = capabilities
            .image_refs
            .iter()
            .find(|image| image.job_kind == RunKind::DataValidate)
            .ok_or(domain::DomainError::CapabilityUnavailable(
                "data_validate_image",
            ))?
            .image_ref
            .clone();
        let capability_artifact: uuid::Uuid = sqlx::query_scalar(
            "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
        )
        .bind(request.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let output_schemas = parameters.output_schemas();
        if !output_schemas.iter().all(|expected| {
            capabilities.artifact_schemas.iter().any(|available| {
                available.name == expected.name && available.version == expected.version
            })
        }) {
            return Err(
                domain::DomainError::CapabilityUnavailable("data_validate_output_schema").into(),
            );
        }
        publish(NativeObjectPublication {
            id: parameter_id,
            bytes,
        })
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'RESEARCH','SYNTHETIC','OPERATOR','REFERENCED')")
            .bind(parameter_id.as_uuid()).bind(request.project_id.as_uuid()).bind(parameter_id.to_string())
            .bind(parameter_bytes).execute(&mut *tx).await?;
        let submission = StandaloneRunSubmission {
            project_id: request.project_id,
            input_set_id: request.input_set_id,
            runtime_id: request.runtime_id,
            runtime_revision: request.expected_runtime_revision,
            kind: RunKind::DataValidate,
            limits: request.limits.clone(),
            max_parallel_runs: 2,
        };
        // The public command receipt owns replay. This internal native receipt is
        // created in the same transaction, never exposed as an alternate retry key.
        let internal_key = format!("data-validate/{}", Id::new());
        let (mut tx, admitted) =
            Self::enqueue_standalone_run_in_transaction(tx, &internal_key, &submission).await?;
        bind_task(
            &mut tx,
            &admitted.resource,
            NativeTaskDefinition {
                parameters_artifact_id: parameter_id,
                inputs,
                image_ref,
                cpu,
                capability_snapshot_artifact_id: db::id(capability_artifact)?,
                output_schemas,
                origin,
                access: ArtifactAccess::Research,
            },
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, admitted.resource, 202).await?;
        tx.commit().await?;
        Ok(result)
    }
}
