//! Native task definitions and atomic producer-bound result adoption.
//! This child uses the existing Run locks/fence/terminal state machine, not another scheduler.
use super::*;
use contracts::{
    artifacts::ArtifactAccess,
    research::{DataOrigin, DataPartition},
    runtime_jobs::{
        JobSpecV1, ResultManifestV1, RuntimeFailureClass, RuntimeInputV1, RuntimeJobLimitsV1,
        RuntimeJobState, RuntimeJobStatusV1, RuntimeOutputV1, RuntimeResultState,
    },
};
use std::collections::{BTreeMap, BTreeSet};

mod probe;
pub use probe::RunProbeTicket;

/// Constructed only by an authorized domain service. Not a public request DTO.
pub(crate) struct NativeTaskDefinition {
    pub parameters_artifact_id: Id,
    pub inputs: Vec<RuntimeInputV1>,
    pub image_ref: String,
    pub cpu: u16,
    pub capability_snapshot_artifact_id: Id,
    pub output_schemas: Vec<contracts::runtime::RuntimeArtifactSchemaV1>,
    pub origin: DataOrigin,
    pub access: ArtifactAccess,
}

pub struct NativeJob {
    pub run: RunSnapshotV1,
    pub spec: JobSpecV1,
    pub runtime: RuntimeSnapshot,
    pub submitted_not_before: DateTime<Utc>,
    pub action: NextRuntimeAction,
}

/// Store-selected immutable local objects. No path or executable is accepted from a caller.
pub struct NativeObjectPublication {
    pub id: Id,
    pub bytes: Vec<u8>,
}

/// A verified native terminal report can expose malformed output without changing
/// the fact that the remote process stopped. Rejected output is never published.
pub enum NativePayloads {
    Verified(Vec<(RuntimeOutputV1, Vec<u8>)>),
    InvalidOutput,
}

pub(crate) async fn bind_task(
    tx: &mut Tx<'_>,
    run: &RunSnapshotV1,
    definition: NativeTaskDefinition,
) -> Result<(), StoreError> {
    if !matches!(
        definition.access,
        ArtifactAccess::Research | ArtifactAccess::EvaluatorOnly
    ) || !(1..=1024).contains(&definition.cpu)
    {
        return Err(StoreError::Invalid("native_task_definition"));
    }
    sqlx::query("INSERT INTO app.run_native_tasks(run_id,parameters_artifact_id,input_bindings,image_ref,cpu,capability_snapshot_artifact_id,output_schemas,origin,access_class) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(run.id.as_uuid()).bind(definition.parameters_artifact_id.as_uuid())
        .bind(db::json(&definition.inputs)?).bind(definition.image_ref).bind(definition.cpu as i16)
        .bind(definition.capability_snapshot_artifact_id.as_uuid())
        .bind(db::json(&definition.output_schemas)?).bind(db::code(&definition.origin)?)
        .bind(db::code(&definition.access)?).execute(&mut **tx).await?;
    Ok(())
}

async fn verify_bindings(
    tx: &mut Tx<'_>,
    run: &RunSnapshotV1,
    runtime: Id,
    inputs: &[RuntimeInputV1],
    access: ArtifactAccess,
) -> Result<(), StoreError> {
    for input in inputs {
        let valid: bool = match input {
            RuntimeInputV1::Dataset { revision_id, registered_ref, storage_version, role } => {
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id JOIN app.data_sources s ON s.id=d.source_id JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id WHERE i.input_set_id=$1 AND d.id=$2 AND i.role=$3 AND d.partition_role=$3 AND s.runtime_id=$4 AND s.native_catalog_ref=$5 AND d.native_storage_version=$6)")
                    .bind(run.input_set_id.as_uuid()).bind(revision_id.as_uuid()).bind(role.code())
                    .bind(runtime.as_uuid()).bind(registered_ref).bind(storage_version)
                    .fetch_one(&mut **tx).await?
            }
            RuntimeInputV1::Artifact { artifact_id, storage_version, byte_count, role } => {
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind=$3 AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version=$4 AND byte_count=$5 AND access_class IN ('RESEARCH',$6))")
                    .bind(artifact_id.as_uuid()).bind(run.project_id.as_uuid()).bind(role.code())
                    .bind(storage_version).bind(byte_count.get() as i64).bind(db::code(&access)?)
                    .fetch_one(&mut **tx).await?
            }
        };
        if !valid {
            return Err(StoreError::Invalid("native_input_binding"));
        }
        if matches!(
            input,
            RuntimeInputV1::Dataset {
                role: DataPartition::Sealed,
                ..
            }
        ) && access != ArtifactAccess::EvaluatorOnly
        {
            return Err(StoreError::Invalid("sealed_native_task_authority"));
        }
    }
    Ok(())
}

impl Store {
    /// Read or freeze the exact first dispatch body under the existing Attempt fence.
    /// Reconciliation never rebinds inputs, changes an image, or refreshes an old owner epoch.
    pub async fn native_job(&self, run: Id, owner: &WorkerFence) -> Result<NativeJob, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut locked = lock_run(&mut tx, run).await?;
        let attempt = fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        let definition = sqlx::query("SELECT * FROM app.run_native_tasks WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::Invalid("native_task_not_defined"))?;
        let existing: Option<Value> = sqlx::query_scalar(
            "SELECT spec_json FROM app.run_native_attempts WHERE attempt_id=$1 AND run_id=$2",
        )
        .bind(owner.attempt_id.as_uuid())
        .bind(run.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;
        let unsent = attempt.try_get::<String, _>("dispatch_state")? == "NOT_SENT";
        if !unsent
            && locked.run.state != RunState::CancelRequested
            && locked.run.deadline_at <= now(&mut tx).await?
        {
            // Expiry requests remote termination; it does not prove termination.
            // Persist the intent before a cancel RPC, so a native tombstone can
            // resolve it without inventing success or retrying the original job.
            let state = runs::request_cancel(locked.run.state)?;
            sqlx::query("UPDATE app.runs SET state=$2,cancellation_requested_at=clock_timestamp() WHERE id=$1")
                .bind(run.as_uuid()).bind(db::code(&state)?)
                .execute(&mut *tx).await?;
            locked.run = append(
                &mut tx,
                run,
                RunEventKind::StateChanged,
                RunReason::DeadlineExceeded,
            )
            .await?;
        }
        let new_spec = existing.is_none();
        let spec: JobSpecV1 = if let Some(document) = existing {
            serde_json::from_value(document).map_err(|_| StoreError::Integrity)?
        } else {
            if !unsent || locked.run.state == RunState::CancelRequested {
                return Err(StoreError::Invalid("native_spec_not_frozen"));
            }
            let limits: JobLimitsV1 = serde_json::from_value(locked.admission.try_get("limits")?)
                .map_err(|_| StoreError::Integrity)?;
            JobSpecV1 {
                schema_version: SchemaV1,
                run_id: run,
                attempt_no: locked.run.current_attempt_no,
                owner_epoch: owner.owner_epoch,
                external_job_id: attempt
                    .try_get::<Option<String>, _>("external_job_id")?
                    .ok_or(StoreError::Integrity)?,
                job_kind: locked.run.kind,
                image_ref: definition.try_get("image_ref")?,
                input_set_id: locked.run.input_set_id,
                inputs: serde_json::from_value(definition.try_get("input_bindings")?)
                    .map_err(|_| StoreError::Integrity)?,
                parameters_artifact_id: db::id(definition.try_get("parameters_artifact_id")?)?,
                limits: RuntimeJobLimitsV1 {
                    cpu: u16::try_from(definition.try_get::<i16, _>("cpu")?)
                        .map_err(|_| StoreError::Integrity)?,
                    cpu_seconds: limits.cpu_seconds,
                    memory_mib: limits.memory_mib,
                    wall_seconds: limits.wall_seconds,
                    output_bytes: limits.output_bytes,
                },
                deadline_at: locked.run.deadline_at,
                requested_output_schemas: serde_json::from_value(
                    definition.try_get("output_schemas")?,
                )
                .map_err(|_| StoreError::Integrity)?,
            }
        };
        domain::runtime_jobs::spec_shape(&spec)?;
        if unsent && locked.run.state != RunState::CancelRequested {
            let runtime = db::id(locked.admission.try_get("runtime_id")?)?;
            crate::research::revalidate_frozen_inputs(
                &mut tx,
                locked.run.input_set_id,
                locked.run.project_id,
                runtime,
            )
            .await?;
            verify_bindings(
                &mut tx,
                &locked.run,
                runtime,
                &spec.inputs,
                db::enum_value(&definition, "access_class")?,
            )
            .await?;
            let capabilities = crate::runtime::require_capabilities(
                &mut tx,
                runtime,
                db::revision(locked.admission.try_get("runtime_revision")?)?,
                locked.run.kind,
            )
            .await?;
            domain::runtime_jobs::admit_spec(&spec, &capabilities, now(&mut tx).await?)?;
            fence(&mut tx, &locked.run, owner).await?;
            if new_spec {
                sqlx::query("INSERT INTO app.run_native_attempts(attempt_id,run_id,spec_json) VALUES($1,$2,$3)")
                    .bind(owner.attempt_id.as_uuid()).bind(run.as_uuid()).bind(db::json(&spec)?)
                    .execute(&mut *tx).await?;
            }
        }
        let view = NativeJob {
            run: locked.run.clone(),
            spec,
            runtime: serde_json::from_value(locked.admission.try_get("runtime_snapshot")?)
                .map_err(|_| StoreError::Integrity)?,
            submitted_not_before: attempt.try_get("created_at")?,
            action: if locked.run.state == RunState::CancelRequested {
                NextRuntimeAction::Cancel
            } else if unsent {
                NextRuntimeAction::PrepareDispatch
            } else {
                NextRuntimeAction::Reconcile
            },
        };
        tx.commit().await?;
        Ok(view)
    }

    /// Accepted is an acknowledgement, not proof that native computation is running.
    pub async fn observe_native_accepted(
        &self,
        run: Id,
        owner: &WorkerFence,
        status: &RuntimeJobStatusV1,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        let attempt = fence(&mut tx, &locked.run, owner).await?;
        domain::runtime_jobs::status(
            status,
            run,
            locked.run.current_attempt_no,
            now(&mut tx).await?,
        )?;
        if status.state != RuntimeJobState::Accepted
            || locked.run.state.is_terminal()
            || attempt.try_get::<String, _>("dispatch_state")? == "NOT_SENT"
            || attempt
                .try_get::<Option<String>, _>("external_job_id")?
                .as_deref()
                != Some(status.external_job_id.as_str())
        {
            return Err(StoreError::Conflict);
        }
        sqlx::query("UPDATE app.run_attempts SET dispatch_state='ACKNOWLEDGED' WHERE id=$1")
            .bind(owner.attempt_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// A local unsent identity has never obtained a network submission permit.
    /// Cancellation/deadline can be resolved without inventing a native observation.
    pub async fn settle_unsubmitted_native_run(
        &self,
        run: Id,
        owner: &WorkerFence,
    ) -> Result<Option<RunSnapshotV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut locked = lock_run(&mut tx, run).await?;
        let attempt = fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        if attempt.try_get::<String, _>("dispatch_state")? != "NOT_SENT" {
            tx.commit().await?;
            return Ok(None);
        }
        let (state, reason) = if locked.run.state == RunState::CancelRequested {
            (RunState::Cancelled, RunReason::CancelledBeforeDispatch)
        } else if locked.run.deadline_at <= now(&mut tx).await? {
            (RunState::Failed, RunReason::DeadlineExceeded)
        } else {
            tx.commit().await?;
            return Ok(None);
        };
        sqlx::query("UPDATE app.run_attempts SET dispatch_state='TERMINAL' WHERE id=$1")
            .bind(owner.attempt_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        let result = finish(
            &mut tx,
            &mut locked,
            state,
            reason,
            json!({"schema_version":1,"source":"NOT_DISPATCHED","reason":reason}),
        )
        .await?;
        tx.commit().await?;
        Ok(Some(result))
    }

    /// This guards each native object read/transfer without keeping a transaction
    /// open across I/O. Unknown already-sent jobs never get a new input transfer.
    pub async fn native_input(
        &self,
        run: Id,
        owner: &WorkerFence,
        artifact: Id,
    ) -> Result<(DbCounter, String), StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        let attempt = fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal()
            || locked.run.state == RunState::CancelRequested
            || attempt.try_get::<String, _>("dispatch_state")? != "NOT_SENT"
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        let document: Value = sqlx::query_scalar(
            "SELECT spec_json FROM app.run_native_attempts WHERE attempt_id=$1 AND run_id=$2",
        )
        .bind(owner.attempt_id.as_uuid())
        .bind(run.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let spec: JobSpecV1 =
            serde_json::from_value(document).map_err(|_| StoreError::Integrity)?;
        let selected = spec
            .inputs
            .iter()
            .find_map(|input| match input {
                RuntimeInputV1::Artifact {
                    artifact_id,
                    byte_count,
                    storage_version,
                    ..
                } if *artifact_id == artifact => Some((*byte_count, storage_version.clone())),
                _ => None,
            })
            .ok_or(StoreError::Forbidden)?;
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            locked.run.input_set_id,
            locked.run.project_id,
            db::id(locked.admission.try_get("runtime_id")?)?,
        )
        .await?;
        fence(&mut tx, &locked.run, owner).await?;
        if locked.run.deadline_at <= now(&mut tx).await? {
            return Err(DomainError::AdmissionClosed.into());
        }
        tx.commit().await?;
        Ok(selected)
    }

    /// Raw artifacts, native identity mapping and the unique terminal receipt share
    /// one PostgreSQL transaction. Replays publish no additional filesystem object.
    pub async fn publish_native_result<R, Read, F, Fut>(
        &self,
        run: Id,
        owner: &WorkerFence,
        raw_manifest: Vec<u8>,
        payloads: NativePayloads,
        read: R,
        publish: F,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnOnce(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        F: FnOnce(Vec<NativeObjectPublication>) -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        if raw_manifest.is_empty()
            || raw_manifest.len() > domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES
        {
            return Err(StoreError::Invalid("native_manifest_size"));
        }
        let manifest: ResultManifestV1 = serde_json::from_slice(&raw_manifest)
            .map_err(|_| StoreError::Invalid("native_manifest_contract"))?;
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        if locked.run.state.is_terminal() {
            let receipt = sqlx::query("SELECT t.attempt_id,t.observation,t.result_snapshot,a.worker_owner_id,a.owner_epoch FROM app.run_terminal_receipts t JOIN app.run_attempts a ON a.id=t.attempt_id WHERE t.run_id=$1")
                .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
            if db::optional_id(&receipt, "attempt_id")? != Some(owner.attempt_id)
                || receipt.try_get::<String, _>("worker_owner_id")? != owner.worker_owner_id
                || db::revision(receipt.try_get("owner_epoch")?)? != owner.owner_epoch
            {
                return Err(DomainError::StaleAttempt.into());
            }
            let previous: Value = receipt.try_get("observation")?;
            let original: Id = previous
                .get("manifest_artifact_id")
                .and_then(Value::as_str)
                .ok_or(StoreError::Conflict)?
                .to_owned()
                .try_into()
                .map_err(|_| StoreError::Integrity)?;
            let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND producer_run_id=$3 AND producer_attempt_id=$4 AND schema_name='qz.job_result' AND schema_version='1' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1'")
                .bind(original.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(run.as_uuid())
                .bind(owner.attempt_id.as_uuid()).fetch_one(&mut *tx).await?;
            if !(1..=domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES as i64).contains(&size) {
                return Err(StoreError::Integrity);
            }
            let bytes = read(original, counter(size)?).await?;
            if bytes != raw_manifest {
                return Err(StoreError::Conflict);
            }
            let resource = serde_json::from_value(receipt.try_get("result_snapshot")?)
                .map_err(|_| StoreError::Integrity)?;
            tx.commit().await?;
            return Ok(CommandResult {
                schema_version: SchemaV1,
                replayed: true,
                resource,
            });
        }
        let attempt = fence(&mut tx, &locked.run, owner).await?;
        let definition = sqlx::query("SELECT * FROM app.run_native_tasks WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let document: Value = sqlx::query_scalar(
            "SELECT spec_json FROM app.run_native_attempts WHERE attempt_id=$1 AND run_id=$2",
        )
        .bind(owner.attempt_id.as_uuid())
        .bind(run.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let spec: JobSpecV1 =
            serde_json::from_value(document).map_err(|_| StoreError::Integrity)?;
        domain::runtime_jobs::manifest(
            &manifest,
            &spec,
            attempt.try_get("created_at")?,
            now(&mut tx).await?,
        )?;
        let invalid = matches!(payloads, NativePayloads::InvalidOutput);
        let outputs = match payloads {
            NativePayloads::Verified(outputs) => outputs,
            NativePayloads::InvalidOutput => Vec::new(),
        };
        if invalid && manifest.state != RuntimeResultState::Succeeded {
            return Err(StoreError::Invalid("invalid_output_outcome"));
        }
        if !invalid && outputs.len() != manifest.artifacts.len() {
            return Err(StoreError::Invalid("native_output_count"));
        }
        let discard_outputs = locked.run.state == RunState::CancelRequested
            && manifest.state == RuntimeResultState::Succeeded;
        let by_remote: BTreeMap<_, _> = manifest
            .artifacts
            .iter()
            .map(|item| (item.storage_ref, item))
            .collect();
        let mut seen = BTreeSet::new();
        let mut objects = Vec::with_capacity(outputs.len() + 1);
        let mut metadata = Vec::with_capacity(outputs.len());
        let mut total = 0u64;
        for (output, bytes) in outputs {
            let expected = by_remote
                .get(&output.storage_ref)
                .ok_or(StoreError::Invalid("native_output_identity"))?;
            if !seen.insert(output.storage_ref)
                || db::json(&output)? != db::json(*expected)?
                || bytes.len() as u64 != output.byte_count.get()
            {
                return Err(StoreError::Invalid("native_output_binding"));
            }
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or(StoreError::Integrity)?;
            if total > spec.limits.output_bytes.get() {
                return Err(StoreError::Invalid("native_output_bytes"));
            }
            domain::execution::output_shape(&output, &bytes)?;
            if !discard_outputs {
                let id = Id::new();
                metadata.push((id, output));
                objects.push(NativeObjectPublication { id, bytes });
            }
        }
        if !invalid && total != manifest.resource_usage.output_bytes.get() {
            return Err(StoreError::Invalid("native_output_sum"));
        }
        let manifest_id = Id::new();
        let manifest_size = raw_manifest.len() as i64;
        objects.push(NativeObjectPublication {
            id: manifest_id,
            bytes: raw_manifest,
        });
        publish(objects).await?;
        fence(&mut tx, &locked.run, owner).await?;
        let access: String = definition.try_get("access_class")?;
        let origin: String = definition.try_get("origin")?;
        for (id, output) in &metadata {
            native_artifact(
                &mut tx,
                &locked.run,
                owner.attempt_id,
                &access,
                &origin,
                NativeArtifact {
                    id: *id,
                    kind: output.kind.code(),
                    media: &output.media_type,
                    schema: &output.schema.name,
                    bytes: output.byte_count.get() as i64,
                },
            )
            .await?;
            sqlx::query("INSERT INTO app.run_native_outputs(attempt_id,remote_storage_ref,artifact_id) VALUES($1,$2,$3)")
                .bind(owner.attempt_id.as_uuid()).bind(output.storage_ref.as_uuid()).bind(id.as_uuid()).execute(&mut *tx).await?;
        }
        native_artifact(
            &mut tx,
            &locked.run,
            owner.attempt_id,
            &access,
            &origin,
            NativeArtifact {
                id: manifest_id,
                kind: "REPORT",
                media: "application/json",
                schema: "qz.job_result",
                bytes: manifest_size,
            },
        )
        .await?;
        let (outcome, class, code) = if invalid {
            (
                NativeOutcome::Failed,
                Some(FailureClass::InvalidInput),
                Some("NATIVE_OUTPUT_INVALID".into()),
            )
        } else {
            match manifest.state {
                RuntimeResultState::Succeeded => (NativeOutcome::Succeeded, None, None),
                RuntimeResultState::Cancelled => (NativeOutcome::Cancelled, None, None),
                RuntimeResultState::Failed => {
                    let failure = manifest.error.as_ref().ok_or(StoreError::Integrity)?;
                    let class = match failure.class {
                        RuntimeFailureClass::RetryableInfra => FailureClass::RetryableInfra,
                        RuntimeFailureClass::PermanentConfig => FailureClass::PermanentConfig,
                        RuntimeFailureClass::InvalidInput => FailureClass::InvalidInput,
                        RuntimeFailureClass::ResourceLimit => FailureClass::ResourceLimit,
                    };
                    (
                        NativeOutcome::Failed,
                        Some(class),
                        Some(db::code(&failure.code)?),
                    )
                }
            }
        };
        let observation = TerminalObservation {
            schema_version: SchemaV1,
            external_job_id: spec.external_job_id,
            outcome,
            manifest_artifact_id: Some(manifest_id),
            failure_class: class,
            failure_code: code,
            observed_at: now(&mut tx).await?,
        };
        let (tx, result) =
            Self::accept_run_terminal_in_transaction(tx, run, owner, &observation).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Reacquiring the original Run lock waits out an unknown result-publication
    /// commit before checking exact local references. Uncertainty always retains bytes.
    pub async fn discard_unpublished_native_object<F, Fut>(
        &self,
        run: Id,
        artifact: Id,
        discard: F,
    ) -> Result<bool, StoreError>
    where
        F: FnOnce(Id) -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        lock_run(&mut tx, run).await?;
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 OR (storage_backend='LOCAL' AND storage_object_ref=$2))")
            .bind(artifact.as_uuid()).bind(artifact.to_string()).fetch_one(&mut *tx).await?;
        if used {
            tx.commit().await?;
            return Ok(false);
        }
        discard(artifact).await?;
        tx.commit().await?;
        Ok(true)
    }
}

struct NativeArtifact<'a> {
    id: Id,
    kind: &'a str,
    media: &'a str,
    schema: &'a str,
    bytes: i64,
}

async fn native_artifact(
    tx: &mut Tx<'_>,
    run: &RunSnapshotV1,
    attempt: Id,
    access: &str,
    origin: &str,
    object: NativeArtifact<'_>,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,$5,$6,$7,'1','LOCAL',$8,'1',$9,$10,$11,'RUNTIME','REFERENCED')")
        .bind(object.id.as_uuid()).bind(run.project_id.as_uuid()).bind(run.id.as_uuid()).bind(attempt.as_uuid())
        .bind(object.kind).bind(object.media).bind(object.schema).bind(object.id.to_string()).bind(object.bytes).bind(access).bind(origin)
        .execute(&mut **tx).await?;
    Ok(())
}
