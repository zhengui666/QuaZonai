//! A proposal's concrete native compilation; existing Run admission owns execution.
use super::*;
use contracts::{
    artifacts::{ArtifactAccess, ResearchArtifactKind},
    execution::NativeTaskParametersV1,
    research::{ArtifactInputRole, DataOrigin, InputPurpose},
    runtime_jobs::RuntimeInputV1,
    science::{NativeForecastParametersV1, NativeForecastRequestV1},
};
use native::{bind_task, NativeObjectPublication, NativeTaskDefinition};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ForecastProposal {
    #[serde(rename = "schema_version")]
    _schema_version: SchemaV1,
    dataset_revision_id: Id,
    parameters: NativeForecastParametersV1,
}

#[derive(Clone, Copy, PartialEq)]
enum AlphaStage {
    Forecast,
    Validation,
}

pub(super) fn native_cpu(
    limits: &JobLimitsV1,
    capabilities: &contracts::runtime::RuntimeCapabilitiesV1,
) -> Result<u16, StoreError> {
    domain::runtime::job_limits(capabilities, limits)?;
    if limits.wall_seconds == 0 {
        return Err(DomainError::BudgetExhausted("wall_seconds").into());
    }
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
    Ok(cpu)
}

pub enum ExperimentWork {
    Compile(Id),
    Forecast(Id),
    RecordAlpha(Id),
    Validate(Id),
}

#[test]
fn cpu_capacity_uses_the_actual_bounded_wall_allocation() {
    let capabilities = serde_json::from_str(include_str!(
        "../../../../tests/contracts/runtime-capabilities.fixture.json"
    ))
    .unwrap();
    let mut limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: DbCounter::new(10).unwrap(),
        wall_seconds: 5,
        memory_mib: 1024,
        output_bytes: DbCounter::new(1024).unwrap(),
    };
    assert_eq!(native_cpu(&limits, &capabilities).unwrap(), 2);
    limits.wall_seconds = 4;
    assert!(native_cpu(&limits, &capabilities).is_err());
    limits.wall_seconds = 0;
    assert!(native_cpu(&limits, &capabilities).is_err());
}

impl Store {
    /// The evaluation subject is an unqualified immutable version, not a claim
    /// that the forecast supported the hypothesis or had calibrated return units.
    pub async fn prepare_research_alpha(
        &self,
        mission: Id,
        owner: &WorkerFence,
        experiment: Id,
    ) -> Result<CommandResult<Id>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, mission).await?;
        fence(&mut tx, &locked.run, owner).await?;
        let row = sqlx::query("SELECT e.ordinal,e.code_artifact_id,e.outcome,f.model_artifact_id,f.run_id,family.root_lineage_id,b.target_kind,b.horizon_kind,b.horizon_value,n.image_ref,r.state,r.active_attempt_id FROM app.experiments e JOIN app.experiment_compilations c ON c.experiment_id=e.id AND c.mission_run_id=$1 JOIN app.run_missions m ON m.run_id=c.mission_run_id AND m.role='RESEARCHER' JOIN app.experiment_forecasts f ON f.experiment_id=e.id JOIN app.runs r ON r.id=f.run_id JOIN app.run_native_tasks n ON n.run_id=r.id JOIN app.experiment_families family ON family.id=e.family_id JOIN app.research_cycles cycle ON cycle.id=e.cycle_id JOIN app.research_briefs b ON b.id=cycle.brief_id AND b.state='FROZEN' WHERE e.id=$2 AND e.project_id=$3 AND e.cycle_id=$4 FOR UPDATE OF e")
            .bind(mission.as_uuid()).bind(experiment.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(locked.run.cycle_id.map(Id::as_uuid))
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let prepared = commands::research_alpha(&mut tx, mission, experiment).await?;
        if let Some(replay) = prepared.replay::<Id>()? {
            tx.commit().await?;
            return Ok(replay);
        }
        if !locked.admission_open()
            || !matches!(
                locked.run.state,
                RunState::Dispatching | RunState::Running | RunState::Reconciling
            )
            || now(&mut tx).await? >= locked.run.deadline_at
            || row.try_get::<String, _>("outcome")? != "PENDING"
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        let forecast = db::id(row.try_get("run_id")?)?;
        let accepted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.run_terminal_receipts t JOIN app.run_attempts a ON a.id=t.attempt_id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL JOIN app.run_native_outputs o ON o.attempt_id=a.id JOIN app.artifacts report ON report.id=o.artifact_id WHERE t.run_id=$1 AND t.attempt_id=$2 AND t.terminal_state='SUCCEEDED' AND report.project_id=$3 AND report.producer_run_id=t.run_id AND report.producer_attempt_id=a.id AND report.kind='REPORT' AND report.schema_name='qz.native_forecast' AND report.schema_version='1' AND report.access_class='RESEARCH')")
            .bind(forecast.as_uuid()).bind(row.try_get::<Option<uuid::Uuid>,_>("active_attempt_id")?).bind(locked.run.project_id.as_uuid()).fetch_one(&mut *tx).await?;
        if row.try_get::<String, _>("state")? != "SUCCEEDED" || !accepted {
            return Err(StoreError::Invalid("accepted_forecast_required"));
        }
        let signal: String = row.try_get("target_kind")?;
        let unit = match signal.as_str() {
            "SCORE" => "UNITLESS_SCORE",
            "EXPECTED_RETURN" => "RETURN_PER_HORIZON",
            _ => return Err(StoreError::Integrity),
        };
        let alpha = Id::new();
        let version = prepared.target;
        sqlx::query("INSERT INTO app.alphas(id,project_id,name,lifecycle,active_version_id) VALUES($1,$2,$3,'RESEARCH',$4)")
            .bind(alpha.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(format!("Experiment {}",row.try_get::<i32,_>("ordinal")?)).bind(version.as_uuid()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.alpha_versions(id,project_id,alpha_id,version,experiment_id,root_lineage_id,code_artifact_id,model_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,calibration_id,runtime_image_ref) VALUES($1,$2,$3,1,$4,$5,$6,$7,'1',$8,$9,$10,$11,NULL,$12)")
            .bind(version.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(alpha.as_uuid()).bind(experiment.as_uuid())
            .bind(row.try_get::<uuid::Uuid,_>("root_lineage_id")?).bind(row.try_get::<uuid::Uuid,_>("code_artifact_id")?).bind(row.try_get::<uuid::Uuid,_>("model_artifact_id")?)
            .bind(signal).bind(row.try_get::<String,_>("horizon_kind")?).bind(row.try_get::<Option<i64>,_>("horizon_value")?).bind(unit).bind(row.try_get::<String,_>("image_ref")?).execute(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, version, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// A ready step, not a second queue or a verdict on settled scientific work.
    pub async fn next_mission_experiment(
        &self,
        mission: Id,
        owner: &WorkerFence,
    ) -> Result<Option<ExperimentWork>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, mission).await?;
        fence(&mut tx, &locked.run, owner).await?;
        let role: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.run_missions WHERE run_id=$1 AND role='RESEARCHER')",
        )
        .bind(mission.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if locked.run.kind != RunKind::AgentResearch || !role {
            return Err(StoreError::Forbidden);
        }
        if !locked.admission_open()
            || !matches!(
                locked.run.state,
                RunState::Dispatching | RunState::Running | RunState::Reconciling
            )
            || now(&mut tx).await? >= locked.run.deadline_at
        {
            return Ok(None);
        }
        let row = sqlx::query("SELECT e.id,c.compile_run_id,f.run_id,receipt.resource_id AS alpha_version_id FROM app.experiments e JOIN app.experiment_authorship a ON a.experiment_id=e.id LEFT JOIN app.experiment_compilations c ON c.experiment_id=e.id LEFT JOIN app.runs compiled ON compiled.id=c.compile_run_id LEFT JOIN app.experiment_forecasts f ON f.experiment_id=e.id LEFT JOIN app.runs predicted ON predicted.id=f.run_id LEFT JOIN app.command_receipts receipt ON receipt.principal_scope='MISSION:'||$3::uuid::text AND receipt.operation='RESEARCH_ALPHA_CREATE' AND receipt.idempotency_key=e.id::text WHERE e.project_id=$1 AND e.cycle_id=$2 AND e.outcome='PENDING' AND e.code_artifact_id IS NOT NULL AND e.parameter_artifact_id IS NOT NULL AND ((e.run_id IS NULL AND (c.experiment_id IS NULL OR (c.mission_run_id=$3 AND compiled.state='SUCCEEDED' AND f.run_id IS NULL))) OR (c.mission_run_id=$3 AND predicted.state='SUCCEEDED' AND NOT EXISTS(SELECT 1 FROM app.experiment_validations WHERE experiment_id=e.id))) ORDER BY e.ordinal LIMIT 1")
            .bind(locked.run.project_id.as_uuid()).bind(locked.run.cycle_id.map(Id::as_uuid)).bind(mission.as_uuid()).fetch_optional(&mut *tx).await?;
        let ready = row
            .map(|row| {
                let id = db::id(row.try_get("id")?)?;
                Ok::<_, StoreError>(if db::optional_id(&row, "alpha_version_id")?.is_some() {
                    ExperimentWork::Validate(id)
                } else if db::optional_id(&row, "run_id")?.is_some() {
                    ExperimentWork::RecordAlpha(id)
                } else if db::optional_id(&row, "compile_run_id")?.is_none() {
                    ExperimentWork::Compile(id)
                } else {
                    ExperimentWork::Forecast(id)
                })
            })
            .transpose()?;
        tx.commit().await?;
        Ok(ready)
    }

    /// Trusted Mission worker: select no model or path from Agent-authored JSON.
    pub async fn start_experiment_forecast<R, Read, P, Published>(
        &self,
        mission: Id,
        owner: &WorkerFence,
        experiment: Id,
        limits: &JobLimitsV1,
        read: R,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        self.start_experiment_science(
            (mission, owner),
            experiment,
            AlphaStage::Forecast,
            limits,
            read,
            publish,
        )
        .await
    }

    pub async fn start_experiment_validation<R, Read, P, Published>(
        &self,
        mission: Id,
        owner: &WorkerFence,
        experiment: Id,
        limits: &JobLimitsV1,
        read: R,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        self.start_experiment_science(
            (mission, owner),
            experiment,
            AlphaStage::Validation,
            limits,
            read,
            publish,
        )
        .await
    }

    // Two concrete Alpha stages share admission, model provenance and native
    // publication. Neither call exposes a generic free-trial or task API.
    async fn start_experiment_science<R, Read, P, Published>(
        &self,
        mission_owner: (Id, &WorkerFence),
        experiment: Id,
        stage: AlphaStage,
        limits: &JobLimitsV1,
        mut read: R,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let (mission, owner) = mission_owner;
        let validation = stage == AlphaStage::Validation;
        let stage_name = if validation { "validation" } else { "forecast" };
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, mission).await?;
        fence(&mut tx, &locked.run, owner).await?;
        let cycle = locked.run.cycle_id.ok_or(StoreError::Forbidden)?;
        let e = sqlx::query("SELECT e.*,c.compile_run_id FROM app.experiments e JOIN app.experiment_compilations c ON c.experiment_id=e.id JOIN app.run_missions m ON m.run_id=c.mission_run_id AND m.role='RESEARCHER' WHERE e.id=$1 AND e.project_id=$2 AND e.cycle_id=$3 AND c.mission_run_id=$4 FOR UPDATE OF e")
            .bind(experiment.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(cycle.as_uuid()).bind(mission.as_uuid())
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if let Some(existing) = sqlx::query_scalar::<_, uuid::Uuid>(if validation {
            "SELECT run_id FROM app.experiment_validations WHERE experiment_id=$1"
        } else {
            "SELECT run_id FROM app.experiment_forecasts WHERE experiment_id=$1"
        })
        .bind(experiment.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        {
            let resource = snapshot(&run_row(&mut tx, db::id(existing)?, false).await?)?;
            tx.commit().await?;
            return Ok(CommandResult {
                schema_version: SchemaV1,
                replayed: true,
                resource,
            });
        }
        if !matches!(
            locked.run.state,
            RunState::Dispatching | RunState::Running | RunState::Reconciling
        ) || now(&mut tx).await? >= locked.run.deadline_at
            || e.try_get::<String, _>("outcome")? != "PENDING"
            || (!validation && db::optional_id(&e, "run_id")?.is_some())
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        if limits.experiments != 0
            || limits.wall_seconds == 0
            || limits.cpu_seconds == DbCounter::ZERO
        {
            return Err(StoreError::Invalid("forecast_limits"));
        }
        let compilation = db::id(e.try_get("compile_run_id")?)?;
        let charged: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.run_admissions WHERE run_id=$1 AND limits->>'experiments'='1')")
            .bind(compilation.as_uuid()).fetch_one(&mut *tx).await?;
        if !charged {
            return Err(StoreError::Invalid("original_trial_charge_required"));
        }
        let models = sqlx::query("SELECT model.id,model.byte_count,model.storage_version FROM app.runs r JOIN app.run_attempts a ON a.id=r.active_attempt_id AND a.run_id=r.id JOIN app.run_terminal_receipts receipt ON receipt.run_id=r.id AND receipt.attempt_id=a.id JOIN app.run_native_outputs o ON o.attempt_id=a.id JOIN app.artifacts model ON model.id=o.artifact_id WHERE r.id=$1 AND r.state='SUCCEEDED' AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL AND receipt.terminal_state='SUCCEEDED' AND model.producer_run_id=r.id AND model.producer_attempt_id=a.id AND model.project_id=r.project_id AND model.kind='MODEL' AND model.schema_name='qz.wasm_model' AND model.schema_version='1' AND model.access_class='RESEARCH' AND model.media_type='application/wasm' AND model.storage_backend='LOCAL' AND model.storage_object_ref=model.id::text")
            .bind(compilation.as_uuid()).fetch_all(&mut *tx).await?;
        let [model] = models.as_slice() else {
            return Err(StoreError::Invalid("accepted_compilation_required"));
        };
        let model_id = db::id(model.try_get("id")?)?;
        let alpha = if validation {
            let row = sqlx::query("SELECT v.id,v.runtime_image_ref,f.dataset_revision_id FROM app.command_receipts receipt JOIN app.alpha_versions v ON v.id=receipt.resource_id AND v.experiment_id=$2 JOIN app.experiment_forecasts f ON f.experiment_id=v.experiment_id AND f.model_artifact_id=v.model_artifact_id WHERE receipt.principal_scope='MISSION:'||$1::uuid::text AND receipt.operation='RESEARCH_ALPHA_CREATE' AND receipt.idempotency_key=$2::uuid::text AND v.project_id=$3 AND v.model_artifact_id=$4")
                .bind(mission.as_uuid()).bind(experiment.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(model_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("original_research_alpha_required"))?;
            Some(row)
        } else {
            None
        };
        let model_bytes = counter(model.try_get("byte_count")?)?;
        if model_bytes == DbCounter::ZERO || model_bytes.get() > 2 * 1024 * 1024 {
            return Err(StoreError::Integrity);
        }
        let parameters_id =
            db::optional_id(&e, "parameter_artifact_id")?.ok_or(StoreError::Integrity)?;
        let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='PARAMETERS' AND schema_name='qz.research_parameters' AND schema_version='1' AND media_type='application/json' AND access_class='RESEARCH' AND storage_backend='LOCAL' AND storage_object_ref=id::text")
            .bind(parameters_id.as_uuid()).bind(locked.run.project_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
        let size = counter(size)?;
        if size == DbCounter::ZERO || size.get() > 2 * 1024 * 1024 {
            return Err(StoreError::Integrity);
        }
        let raw = read(parameters_id, size).await?;
        if raw.len() as u64 != size.get() {
            return Err(StoreError::Integrity);
        }
        let proposal: ForecastProposal =
            serde_json::from_slice(&raw).map_err(|_| StoreError::Invalid("forecast_parameters"))?;
        let brief = sqlx::query("SELECT b.id,b.horizon_kind,b.horizon_value,b.target_kind,b.evaluation_policy_id,a.engine_image_ref FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id JOIN app.execution_assumptions a ON a.id=b.execution_assumptions_id WHERE c.id=$1")
            .bind(cycle.as_uuid()).fetch_one(&mut *tx).await?;
        if brief.try_get::<String, _>("horizon_kind")? != "FIXED_BARS" {
            return Err(DomainError::CapabilityUnavailable("native_fixed_bar_horizon").into());
        }
        if brief.try_get::<Option<i64>, _>("horizon_value")?
            != Some(i64::from(proposal.parameters.label_horizon_observations))
        {
            return Err(StoreError::Invalid("forecast_label_horizon"));
        }
        let context =
            crate::cycles::execution_context(&mut tx, db::id(brief.try_get("id")?)?).await?;
        if context.runtime_id != db::id(locked.admission.try_get("runtime_id")?)?
            || context.runtime_revision
                != db::revision(locked.admission.try_get("runtime_revision")?)?
        {
            return Err(StoreError::Integrity);
        }
        let input_set = if validation {
            context.validation_input_set_id
        } else {
            context.discovery_input_set_id
        };
        let bindings = crate::data_validation::dataset_bindings(
            &mut tx,
            input_set,
            locked.run.project_id,
            context.runtime_id,
            &[if validation {
                InputPurpose::Validation
            } else {
                InputPurpose::Discovery
            }],
            &mut read,
        )
        .await?;
        let binding = if validation {
            if bindings.len() != 1 {
                return Err(DomainError::CapabilityUnavailable(
                    "native_alpha_single_validation_revision",
                )
                .into());
            }
            if alpha
                .as_ref()
                .ok_or(StoreError::Integrity)?
                .try_get::<uuid::Uuid, _>("dataset_revision_id")?
                != proposal.dataset_revision_id.as_uuid()
            {
                return Err(StoreError::Integrity);
            }
            bindings.into_iter().next().ok_or(StoreError::Integrity)?
        } else {
            bindings
                .into_iter()
                .find(|binding| {
                    binding.selection.dataset_revision_id == proposal.dataset_revision_id
                })
                .ok_or(StoreError::Invalid("forecast_discovery_dataset"))?
        };
        let dataset = binding.selection.dataset_revision_id;
        let request = NativeForecastRequestV1 {
            schema_version: SchemaV1,
            selection: binding.selection.selection,
            parameters: proposal.parameters,
        };
        domain::execution::forecast_request(&request)?;
        let policy_id = db::id(brief.try_get("evaluation_policy_id")?)?;
        let task = if validation {
            let p = sqlx::query("SELECT split_policy,selection_rule,metric_requirements FROM app.evaluation_policies WHERE id=$1 AND project_id=$2")
                .bind(policy_id.as_uuid()).bind(locked.run.project_id.as_uuid()).fetch_one(&mut *tx).await?;
            let split = serde_json::from_value(p.try_get("split_policy")?)
                .map_err(|_| StoreError::Integrity)?;
            let selection = serde_json::from_value(p.try_get("selection_rule")?)
                .map_err(|_| StoreError::Integrity)?;
            let requirements: Vec<contracts::evidence::MetricRequirementV1> =
                serde_json::from_value(p.try_get("metric_requirements")?)
                    .map_err(|_| StoreError::Integrity)?;
            domain::execution::alpha_validation_policy(
                &selection,
                &requirements,
                &request.selection,
                &split,
            )?;
            let request = contracts::science::NativeAlphaValidationRequestV1 {
                schema_version: SchemaV1,
                forecast: request,
                split_policy: split,
                target_kind: db::enum_value(&brief, "target_kind")?,
            };
            domain::execution::alpha_validation_request(&request)?;
            NativeTaskParametersV1::ValidateAlpha {
                schema_version: SchemaV1,
                dataset_revision_id: dataset,
                model_artifact_id: model_id,
                request: Box::new(request),
            }
        } else {
            NativeTaskParametersV1::EvaluateAlpha {
                schema_version: SchemaV1,
                dataset_revision_id: dataset,
                model_artifact_id: model_id,
                request,
            }
        };
        let capabilities = crate::runtime::require_capabilities(
            &mut tx,
            context.runtime_id,
            context.runtime_revision,
            RunKind::AlphaEvaluate,
        )
        .await?;
        domain::runtime::job_limits(&capabilities, limits)?;
        if validation {
            domain::execution::validation::capabilities(&capabilities)?;
        }
        let schemas = task.output_schemas();
        if !schemas.iter().all(|schema| {
            capabilities.artifact_schemas.iter().any(|supported| {
                supported.name == schema.name && supported.version == schema.version
            })
        }) {
            return Err(DomainError::CapabilityUnavailable("native_forecast_outputs").into());
        }
        let image_ref = capabilities
            .image_refs
            .iter()
            .find(|image| image.job_kind == RunKind::AlphaEvaluate)
            .ok_or(DomainError::CapabilityUnavailable("native_forecast_image"))?
            .image_ref
            .clone();
        if image_ref != brief.try_get::<String, _>("engine_image_ref")?
            || alpha.as_ref().is_some_and(|a| {
                a.try_get::<String, _>("runtime_image_ref").ok().as_ref() != Some(&image_ref)
            })
        {
            return Err(DomainError::CapabilityUnavailable("frozen_alpha_image_changed").into());
        }
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
        let access = if validation {
            ArtifactAccess::EvaluatorOnly
        } else {
            ArtifactAccess::Research
        };
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,$5,'SYNTHETIC','OPERATOR','REFERENCED')")
            .bind(parameter_id.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(parameter_id.to_string()).bind(size.get() as i64).bind(db::code(&access)?).execute(&mut *tx).await?;
        fence(&mut tx, &locked.run, owner).await?;
        let remaining = (locked.run.deadline_at - now(&mut tx).await?).num_seconds();
        let mut bounded = limits.clone();
        bounded.wall_seconds = bounded.wall_seconds.min(
            u32::try_from(remaining).map_err(|_| DomainError::BudgetExhausted("wall_seconds"))?,
        );
        if bounded.wall_seconds == 0 {
            return Err(DomainError::BudgetExhausted("wall_seconds").into());
        }
        let cpu = native_cpu(&bounded, &capabilities)?;
        let submission = RunSubmission {
            cycle_id: cycle,
            input_set_id: input_set,
            runtime_id: context.runtime_id,
            runtime_revision: context.runtime_revision,
            kind: RunKind::AlphaEvaluate,
            limits: bounded,
        };
        let (mut tx, admitted) = Self::enqueue_with_trial_charge(
            tx,
            &format!("experiment/{experiment}/{stage_name}"),
            &submission,
            false,
            Some(locked.run.deadline_at),
        )
        .await?;
        if admitted.replayed {
            return Err(StoreError::Integrity);
        }
        bind_task(
            &mut tx,
            &admitted.resource,
            NativeTaskDefinition {
                parameters_artifact_id: parameter_id,
                inputs: vec![
                    binding.input,
                    RuntimeInputV1::Artifact {
                        artifact_id: model_id,
                        storage_version: model.try_get("storage_version")?,
                        byte_count: model_bytes,
                        role: ArtifactInputRole::Model,
                    },
                    RuntimeInputV1::Artifact {
                        artifact_id: parameter_id,
                        storage_version: "1".into(),
                        byte_count: size,
                        role: ArtifactInputRole::Parameters,
                    },
                ],
                image_ref,
                cpu,
                capability_snapshot_artifact_id: db::id(capability)?,
                output_schemas: schemas,
                origin: binding.origin,
                access,
            },
        )
        .await?;
        if let Some(alpha) = alpha {
            sqlx::query("INSERT INTO app.experiment_validations(experiment_id,run_id,alpha_version_id,policy_id,dataset_revision_id) VALUES($1,$2,$3,$4,$5)")
                .bind(experiment.as_uuid()).bind(admitted.resource.id.as_uuid()).bind(alpha.try_get::<uuid::Uuid,_>("id")?).bind(policy_id.as_uuid()).bind(dataset.as_uuid()).execute(&mut *tx).await?;
        } else {
            sqlx::query("INSERT INTO app.experiment_forecasts(experiment_id,run_id,model_artifact_id,dataset_revision_id) VALUES($1,$2,$3,$4)")
                .bind(experiment.as_uuid()).bind(admitted.resource.id.as_uuid()).bind(model_id.as_uuid()).bind(dataset.as_uuid()).execute(&mut *tx).await?;
            sqlx::query("UPDATE app.experiments SET run_id=$2 WHERE id=$1")
                .bind(experiment.as_uuid())
                .bind(admitted.resource.id.as_uuid())
                .execute(&mut *tx)
                .await?;
        }
        fence(&mut tx, &locked.run, owner).await?;
        tx.commit().await?;
        Ok(admitted)
    }

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
        if limits.experiments != 1
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
        let cpu = native_cpu(&bounded, &capabilities)?;
        let request = RunSubmission {
            cycle_id: cycle,
            input_set_id: context.discovery_input_set_id,
            runtime_id: context.runtime_id,
            runtime_revision: context.runtime_revision,
            kind: RunKind::DataValidate,
            limits: bounded,
        };
        let (mut tx, admitted) = Self::enqueue_with_trial_charge(
            tx,
            &format!("experiment/{experiment}/compile"),
            &request,
            true,
            Some(locked.run.deadline_at),
        )
        .await?;
        if admitted.replayed {
            return Err(StoreError::Integrity);
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
