//! A proposal's concrete native compilation; existing Run admission owns execution.
use super::*;
use contracts::{
    artifacts::{ArtifactAccess, ResearchArtifactKind},
    execution::NativeTaskParametersV1,
    research::{ArtifactInputRole, DataOrigin},
    runtime_jobs::RuntimeInputV1,
};
use native::{bind_task, NativeObjectPublication, NativeTaskDefinition};

impl Store {
    /// Trusted Mission worker only. Limits are a proposed allocation, not authority
    /// to exceed the frozen budget; an existing compilation always retains its Run.
    pub async fn start_experiment_compilation<P, Published>(
        &self,
        mission: Id,
        owner: &WorkerFence,
        experiment: Id,
        limits: &JobLimitsV1,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, mission).await?;
        fence(&mut tx, &locked.run, owner).await?;
        let cycle = locked.run.cycle_id.ok_or(StoreError::Forbidden)?;
        let role: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.run_missions WHERE run_id=$1 AND role='RESEARCHER')",
        )
        .bind(mission.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if locked.run.kind != RunKind::AgentResearch || !role {
            return Err(StoreError::Forbidden);
        }
        let e = sqlx::query("SELECT e.* FROM app.experiments e JOIN app.experiment_authorship a ON a.experiment_id=e.id WHERE e.id=$1 AND e.project_id=$2 AND e.cycle_id=$3 FOR UPDATE OF e")
            .bind(experiment.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(cycle.as_uuid())
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if let Some(existing) = sqlx::query("SELECT mission_run_id,compile_run_id FROM app.experiment_compilations WHERE experiment_id=$1")
            .bind(experiment.as_uuid()).fetch_optional(&mut *tx).await?
        {
            if db::id(existing.try_get("mission_run_id")?)? != mission {
                return Err(StoreError::Conflict);
            }
            let run = db::id(existing.try_get("compile_run_id")?)?;
            let resource = snapshot(&run_row(&mut tx, run, false).await?)?;
            tx.commit().await?;
            return Ok(CommandResult { schema_version: SchemaV1, replayed: true, resource });
        }
        if !matches!(
            locked.run.state,
            RunState::Dispatching | RunState::Running | RunState::Reconciling
        ) || now(&mut tx).await? >= locked.run.deadline_at
            || e.try_get::<String, _>("outcome")? != "PENDING"
            || db::optional_id(&e, "run_id")?.is_some()
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        if limits.experiments != 0
            || limits.wall_seconds == 0
            || limits.cpu_seconds == DbCounter::ZERO
        {
            return Err(StoreError::Invalid("compilation_limits"));
        }
        let code = db::optional_id(&e, "code_artifact_id")?
            .ok_or(StoreError::Invalid("experiment_code_required"))?;
        let parameters = db::optional_id(&e, "parameter_artifact_id")?
            .ok_or(StoreError::Invalid("experiment_parameters_required"))?;
        let mut inputs = Vec::with_capacity(2);
        for (id, kind) in [
            (code, ResearchArtifactKind::Code),
            (parameters, ResearchArtifactKind::Parameters),
        ] {
            let a = sqlx::query("SELECT byte_count,storage_version FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind=$3 AND media_type=$4 AND schema_name=$5 AND schema_version='1' AND access_class='RESEARCH' AND storage_backend='LOCAL' AND storage_object_ref=id::text")
                .bind(id.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(kind.code())
                .bind(kind.media_type()).bind(kind.schema_name()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
            let size = counter(a.try_get("byte_count")?)?;
            if size == DbCounter::ZERO || size.get() > 2 * 1024 * 1024 {
                return Err(StoreError::Integrity);
            }
            if id == code {
                inputs.push(RuntimeInputV1::Artifact {
                    artifact_id: code,
                    storage_version: a.try_get("storage_version")?,
                    byte_count: size,
                    role: ArtifactInputRole::Code,
                });
            }
        }
        let brief: uuid::Uuid =
            sqlx::query_scalar("SELECT brief_id FROM app.research_cycles WHERE id=$1")
                .bind(cycle.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let context = crate::cycles::execution_context(&mut tx, db::id(brief)?).await?;
        if context.runtime_id != db::id(locked.admission.try_get("runtime_id")?)?
            || context.runtime_revision
                != db::revision(locked.admission.try_get("runtime_revision")?)?
        {
            return Err(StoreError::Integrity);
        }
        let capabilities = crate::runtime::require_capabilities(
            &mut tx,
            context.runtime_id,
            context.runtime_revision,
            RunKind::DataValidate,
        )
        .await?;
        domain::runtime::job_limits(&capabilities, limits)?;
        let cpu = u16::try_from(
            limits
                .cpu_seconds
                .get()
                .div_ceil(u64::from(limits.wall_seconds)),
        )
        .map_err(|_| DomainError::CapabilityUnavailable("native_cpu_capacity"))?;
        if cpu == 0 || cpu > capabilities.max_cpu {
            return Err(DomainError::CapabilityUnavailable("native_cpu_capacity").into());
        }
        let task = NativeTaskParametersV1::CompileModel {
            schema_version: SchemaV1,
            code_artifact_id: code,
        };
        let schemas = task.output_schemas();
        if !schemas.iter().all(|schema| {
            capabilities.artifact_schemas.iter().any(|supported| {
                supported.name == schema.name && supported.version == schema.version
            })
        }) {
            return Err(DomainError::CapabilityUnavailable("native_compilation_outputs").into());
        }
        let image_ref = capabilities
            .image_refs
            .iter()
            .find(|image| image.job_kind == RunKind::DataValidate)
            .ok_or(DomainError::CapabilityUnavailable(
                "native_compilation_image",
            ))?
            .image_ref
            .clone();
        let capability: uuid::Uuid = sqlx::query_scalar(
            "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
        )
        .bind(context.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let parameter_id = Id::new();
        let bytes = serde_json::to_vec(&task).map_err(|_| StoreError::Integrity)?;
        let size = counter(bytes.len() as i64)?;
        publish(NativeObjectPublication {
            id: parameter_id,
            bytes,
        })
        .await?;
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'RESEARCH','SYNTHETIC','OPERATOR','REFERENCED')")
            .bind(parameter_id.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(parameter_id.to_string()).bind(size.get() as i64).execute(&mut *tx).await?;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: parameter_id,
            storage_version: "1".into(),
            byte_count: size,
            role: ArtifactInputRole::Parameters,
        });
        fence(&mut tx, &locked.run, owner).await?;
        let remaining = (locked.run.deadline_at - now(&mut tx).await?).num_seconds();
        let mut bounded = limits.clone();
        bounded.wall_seconds = bounded.wall_seconds.min(
            u32::try_from(remaining).map_err(|_| DomainError::BudgetExhausted("wall_seconds"))?,
        );
        if bounded.wall_seconds == 0 {
            return Err(DomainError::BudgetExhausted("wall_seconds").into());
        }
        let request = RunSubmission {
            cycle_id: cycle,
            input_set_id: context.discovery_input_set_id,
            runtime_id: context.runtime_id,
            runtime_revision: context.runtime_revision,
            kind: RunKind::DataValidate,
            limits: bounded,
        };
        let (mut tx, admitted) = Self::enqueue_run_in_transaction(
            tx,
            &format!("experiment/{experiment}/compile"),
            &request,
        )
        .await?;
        if admitted.replayed {
            return Err(StoreError::Integrity);
        }
        if admitted.resource.deadline_at > locked.run.deadline_at {
            return Err(DomainError::BudgetExhausted("wall_seconds").into());
        }
        bind_task(
            &mut tx,
            &admitted.resource,
            NativeTaskDefinition {
                parameters_artifact_id: parameter_id,
                inputs,
                image_ref,
                cpu,
                capability_snapshot_artifact_id: db::id(capability)?,
                output_schemas: schemas,
                origin: DataOrigin::Synthetic,
                access: ArtifactAccess::Research,
            },
        )
        .await?;
        sqlx::query("INSERT INTO app.experiment_compilations(experiment_id,project_id,cycle_id,mission_run_id,compile_run_id,code_artifact_id,parameter_artifact_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(experiment.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(cycle.as_uuid())
            .bind(mission.as_uuid()).bind(admitted.resource.id.as_uuid()).bind(code.as_uuid()).bind(parameters.as_uuid()).execute(&mut *tx).await?;
        fence(&mut tx, &locked.run, owner).await?;
        tx.commit().await?;
        Ok(admitted)
    }
}
