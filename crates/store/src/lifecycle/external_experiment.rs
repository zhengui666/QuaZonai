//! Finite external scientific execution. Native terminal PGMQ replay advances
//! compilation exactly once; no Mission, Agent turn or second scheduler is created.
use super::*;
use contracts::{
    artifacts::ArtifactAccess,
    control::OperatorOperation,
    execution::NativeTaskParametersV1,
    experiments::{ExperimentEvaluateIntent, ExperimentEvaluateV1},
    research::{ArtifactInputRole, DataOrigin, DataPartition, InputPurpose},
    runtime::RuntimeCapabilitiesV1,
    runtime_jobs::RuntimeInputV1,
    science::{
        ExperimentEvaluationParametersV1, NativeExperimentEvaluationRequestV1,
        NativeExperimentEvaluationResultV1,
    },
};
use native::{bind_task, NativeObjectPublication, NativeTaskDefinition};

mod alpha;
pub(crate) use alpha::{read_strategy_source, resolve_strategy_alpha};

async fn artifact_input(
    tx: &mut Tx<'_>,
    project: Id,
    id: Id,
    role: ArtifactInputRole,
    schema: &str,
) -> Result<RuntimeInputV1, StoreError> {
    let media_type = match role {
        ArtifactInputRole::Code => "text/x-rust",
        ArtifactInputRole::Model => "application/wasm",
        _ => "application/json",
    };
    let row = sqlx::query("SELECT byte_count,storage_version FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind=$3 AND schema_name=$4 AND schema_version='1' AND access_class='RESEARCH' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND media_type=$5")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(role.code()).bind(schema).bind(media_type).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("external_experiment_artifact"))?;
    let byte_count = counter(row.try_get("byte_count")?)?;
    if !(1..=2 * 1024 * 1024).contains(&byte_count.get()) {
        return Err(StoreError::Invalid("external_experiment_artifact_size"));
    }
    Ok(RuntimeInputV1::Artifact {
        artifact_id: id,
        storage_version: row.try_get("storage_version")?,
        byte_count,
        role,
    })
}

async fn document<T, R, Read>(input: &RuntimeInputV1, read: &mut R) -> Result<T, StoreError>
where
    T: serde::de::DeserializeOwned,
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let RuntimeInputV1::Artifact {
        artifact_id,
        byte_count,
        ..
    } = input
    else {
        return Err(StoreError::Integrity);
    };
    let bytes = read(*artifact_id, *byte_count).await?;
    if bytes.len() as u64 != byte_count.get() {
        return Err(StoreError::Integrity);
    }
    serde_json::from_slice(&bytes).map_err(|_| StoreError::Invalid("external_experiment_document"))
}

async fn native_definition<P, Published>(
    tx: &mut Tx<'_>,
    context: (Id, Id),
    native: (&NativeTaskParametersV1, &RuntimeCapabilitiesV1),
    mut inputs: Vec<RuntimeInputV1>,
    origin: DataOrigin,
    limits: &JobLimitsV1,
    publish: P,
) -> Result<NativeTaskDefinition, StoreError>
where
    P: FnOnce(NativeObjectPublication) -> Published,
    Published: std::future::Future<Output = Result<(), StoreError>>,
{
    let (project, runtime) = context;
    let (task, capabilities) = native;
    let image = image(capabilities, task.job_kind())?;
    let schemas = task.output_schemas();
    if !schemas.iter().all(|s| {
        capabilities
            .artifact_schemas
            .iter()
            .any(|v| v.name == s.name && v.version == s.version)
    }) {
        return Err(DomainError::CapabilityUnavailable("external_experiment_outputs").into());
    }
    let cpu = experiment::native_cpu(limits, capabilities)?;
    let capability: uuid::Uuid = sqlx::query_scalar(
        "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
    )
    .bind(runtime.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    let id = Id::new();
    let bytes = serde_json::to_vec(task).map_err(|_| StoreError::Integrity)?;
    let size = counter(bytes.len() as i64)?;
    publish(NativeObjectPublication { id, bytes }).await?;
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'RESEARCH',$5,'OPERATOR','REFERENCED')")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(id.to_string()).bind(size.get() as i64).bind(db::code(&origin)?).execute(&mut **tx).await?;
    inputs.push(RuntimeInputV1::Artifact {
        artifact_id: id,
        storage_version: "1".into(),
        byte_count: size,
        role: ArtifactInputRole::Parameters,
    });
    Ok(NativeTaskDefinition {
        parameters_artifact_id: id,
        inputs,
        image_ref: image,
        cpu,
        capability_snapshot_artifact_id: db::id(capability)?,
        output_schemas: schemas,
        origin,
        access: ArtifactAccess::Research,
    })
}

fn image(capabilities: &RuntimeCapabilitiesV1, kind: RunKind) -> Result<String, StoreError> {
    capabilities
        .image_refs
        .iter()
        .find(|v| v.job_kind == kind)
        .map(|v| v.image_ref.clone())
        .ok_or(DomainError::CapabilityUnavailable("external_experiment_image").into())
}

async fn settle(
    tx: &mut Tx<'_>,
    experiment: Id,
    run: Id,
    report: Option<Id>,
    reason: &str,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO app.external_experiment_results(experiment_id,run_id,report_artifact_id,reason) VALUES($1,$2,$3,$4)")
        .bind(experiment.as_uuid()).bind(run.as_uuid()).bind(report.map(Id::as_uuid)).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE app.experiments SET outcome='INCONCLUSIVE',outcome_reason=$2,conclusion_artifact_id=$3,revision=revision+1 WHERE id=$1")
        .bind(experiment.as_uuid()).bind(reason).bind(report.map(Id::as_uuid)).execute(&mut **tx).await?;
    Ok(())
}

fn continuation_stop_reason(error: &StoreError) -> Option<&'static str> {
    match error {
        StoreError::Domain(DomainError::AdmissionClosed) => Some("EXTERNAL_CONTINUATION_CLOSED"),
        StoreError::Domain(DomainError::BudgetExhausted(
            "cpu_seconds" | "job_resource_limit" | "experiments",
        )) => Some("EXTERNAL_CONTINUATION_BUDGET_EXHAUSTED"),
        StoreError::Domain(DomainError::CapabilityUnavailable(
            "runtime_job_kind_or_revision"
            | "runtime_wall_seconds"
            | "runtime_memory_mib"
            | "runtime_output_bytes"
            | "native_cpu_capacity"
            | "external_experiment_outputs"
            | "external_experiment_image"
            | "external_frozen_engine_image",
        )) => Some("EXTERNAL_CONTINUATION_RUNTIME_CHANGED"),
        StoreError::Domain(DomainError::Fields(issues))
            if !issues.is_empty()
                && issues.iter().all(|issue| {
                    matches!(
                        issue.code.as_str(),
                        "SOURCE_DISABLED"
                            | "DATA_USE_NOT_AUTHORIZED"
                            | "DATA_USE_PURPOSE_NOT_AUTHORIZED"
                    )
                }) =>
        {
            Some("EXTERNAL_CONTINUATION_AUTHORITY_CLOSED")
        }
        _ => None,
    }
}

impl Store {
    pub async fn evaluate_experiment<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &ExperimentEvaluateIntent,
        mut read: R,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::experiments::evaluate(&request.request)?;
        let mut tx = self.pool.begin().await?;
        let experiment = request.experiment_id;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ExperimentEvaluate,
            key,
            Some(experiment),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let parent = sqlx::query("SELECT project_id,cycle_id FROM app.experiments WHERE id=$1")
            .bind(experiment.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let project = db::id(parent.try_get("project_id")?)?;
        let cycle = db::id(parent.try_get("cycle_id")?)?;
        crate::research::project_for_write(&mut tx, project).await?;
        let cycle_row=sqlx::query("SELECT c.brief_id,c.state,c.budget_snapshot,c.reserved_cpu_seconds FROM app.research_cycles c JOIN app.external_research_cycles x ON x.cycle_id=c.id WHERE c.id=$1 FOR UPDATE OF c")
            .bind(cycle.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("external_cycle_required"))?;
        let e=sqlx::query("SELECT e.* FROM app.experiments e JOIN app.experiment_authorship a ON a.experiment_id=e.id AND a.actor_kind IN ('OPERATOR','CLI') WHERE e.id=$1 FOR UPDATE OF e")
            .bind(experiment.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Forbidden)?;
        let current = db::revision(e.try_get("revision")?)?;
        if current != request.request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        if cycle_row.try_get::<String, _>("state")? != "RUNNING"
            || e.try_get::<String, _>("outcome")? != "PENDING"
            || db::optional_id(&e, "run_id")?.is_some()
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        let limits = &request.request;
        let budget: BudgetV1 = serde_json::from_value(cycle_row.try_get("budget_snapshot")?)
            .map_err(|_| StoreError::Integrity)?;
        for stage in [&limits.compile_limits, &limits.evaluation_limits] {
            domain::admission::job_resource_limits(
                &budget,
                stage.wall_seconds,
                stage.memory_mib,
                stage.output_bytes,
            )?;
        }
        if let Some(maximum) = budget.max_cpu_seconds {
            let remaining = maximum
                .get()
                .saturating_sub(counter(cycle_row.try_get("reserved_cpu_seconds")?)?.get());
            let compile = limits
                .compile_limits
                .cpu_seconds
                .ok_or(DomainError::BudgetExhausted("cpu_seconds"))?;
            let evaluation = limits
                .evaluation_limits
                .cpu_seconds
                .ok_or(DomainError::BudgetExhausted("cpu_seconds"))?;
            let requested = compile
                .get()
                .checked_add(evaluation.get())
                .ok_or(DomainError::BudgetExhausted("cpu_seconds"))?;
            if requested > remaining {
                return Err(DomainError::BudgetExhausted("cpu_seconds").into());
            }
        }
        if limits.compile_limits.experiments != 1 || limits.evaluation_limits.experiments != 0 {
            return Err(StoreError::Invalid("external_experiment_trial_limits"));
        }
        let brief_id = db::id(cycle_row.try_get("brief_id")?)?;
        let context = crate::cycles::execution_context(&mut tx, brief_id).await?;
        let code = db::optional_id(&e, "code_artifact_id")?
            .ok_or(StoreError::Invalid("experiment_code_required"))?;
        let parameters = db::optional_id(&e, "parameter_artifact_id")?
            .ok_or(StoreError::Invalid("experiment_parameters_required"))?;
        let code_input = artifact_input(
            &mut tx,
            project,
            code,
            ArtifactInputRole::Code,
            "qz.rust_source",
        )
        .await?;
        let parameter_input = artifact_input(
            &mut tx,
            project,
            parameters,
            ArtifactInputRole::Parameters,
            "qz.research_parameters",
        )
        .await?;
        let parameters: ExperimentEvaluationParametersV1 =
            document(&parameter_input, &mut read).await?;
        domain::experiments::evaluation_parameters(&parameters)?;
        let bindings = crate::data_validation::dataset_bindings(
            &mut tx,
            context.validation_input_set_id,
            project,
            context.runtime_id,
            &[InputPurpose::Validation],
            &mut read,
        )
        .await?;
        let [dataset] = bindings.as_slice() else {
            return Err(StoreError::Invalid("external_single_validation_dataset"));
        };
        if dataset.selection.dataset_revision_id != parameters.dataset_revision_id {
            return Err(StoreError::Invalid("external_validation_dataset"));
        }
        let mut features = Vec::new();
        for id in &parameters.feature_artifact_ids {
            let feature = crate::recorded_features::resolve(
                &mut tx,
                project,
                *id,
                Some(parameters.dataset_revision_id),
            )
            .await?;
            features.push(crate::recorded_features::read(&feature, &mut read).await?);
        }
        domain::execution::features::bind_observations(
            &features,
            &parameters.feature_schema,
            DataPartition::Validation,
        )?;
        let brief=sqlx::query("SELECT b.horizon_kind,b.horizon_value,p.split_policy,a.engine_image_ref,s.settings,s.input_set_id FROM app.research_briefs b JOIN app.evaluation_policies p ON p.id=b.evaluation_policy_id JOIN app.execution_assumptions a ON a.id=b.execution_assumptions_id JOIN app.execution_assumption_sources s ON s.assumptions_id=a.id AND s.project_id=b.project_id AND s.runtime_id=$2 WHERE b.id=$1 AND b.state='FROZEN'")
            .bind(brief_id.as_uuid()).bind(context.runtime_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("external_execution_assumptions"))?;
        if brief.try_get::<String, _>("horizon_kind")? != "FIXED_BARS"
            || brief.try_get::<Option<i64>, _>("horizon_value")?
                != Some(i64::from(parameters.label_horizon_observations))
        {
            return Err(StoreError::Invalid("external_observed_bar_horizon"));
        }
        if db::json(&parameters.settings)? != brief.try_get::<Value, _>("settings")? {
            return Err(StoreError::Invalid("external_frozen_execution_settings"));
        }
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            db::id(brief.try_get("input_set_id")?)?,
            project,
            context.runtime_id,
        )
        .await?;
        domain::catalogs::execution_fees(&dataset.metadata, &parameters.settings)?;
        let evaluation_request = NativeExperimentEvaluationRequestV1 {
            schema_version: SchemaV1,
            selection: dataset.selection.selection.clone(),
            split_policy: serde_json::from_value(brief.try_get("split_policy")?)
                .map_err(|_| StoreError::Integrity)?,
            instrument_id: parameters.instrument_id,
            feature_schema: parameters.feature_schema,
            label_horizon_observations: parameters.label_horizon_observations,
            total_fuel: parameters.total_fuel,
            decision_output: parameters.decision_output,
            settings: parameters.settings,
            target_ttl_ns: parameters.target_ttl_ns,
        };
        domain::execution::experiment_request(&evaluation_request)?;
        let cap = crate::runtime::require_capabilities(
            &mut tx,
            context.runtime_id,
            context.runtime_revision,
            RunKind::PortfolioSimulate,
        )
        .await?;
        experiment::native_cpu(&limits.evaluation_limits, &cap)?;
        if image(&cap, RunKind::PortfolioSimulate)?
            != brief.try_get::<String, _>("engine_image_ref")?
        {
            return Err(StoreError::Invalid("external_frozen_engine_image"));
        }
        if !cap
            .artifact_schemas
            .iter()
            .any(|s| s.name == "qz.experiment_evaluation" && s.version == "1")
        {
            return Err(DomainError::CapabilityUnavailable("external_experiment_outputs").into());
        }
        let cap = crate::runtime::require_capabilities(
            &mut tx,
            context.runtime_id,
            context.runtime_revision,
            RunKind::DataValidate,
        )
        .await?;
        experiment::native_cpu(&limits.compile_limits, &cap)?;
        let definition = native_definition(
            &mut tx,
            (project, context.runtime_id),
            (
                &NativeTaskParametersV1::CompileFeatureModel {
                    schema_version: SchemaV1,
                    code_artifact_id: code,
                },
                &cap,
            ),
            vec![code_input],
            DataOrigin::Synthetic,
            &limits.compile_limits,
            publish,
        )
        .await?;
        let submission = RunSubmission {
            cycle_id: cycle,
            input_set_id: context.discovery_input_set_id,
            runtime_id: context.runtime_id,
            runtime_revision: context.runtime_revision,
            kind: RunKind::DataValidate,
            limits: limits.compile_limits.clone(),
        };
        let (mut tx, admitted, _) = Self::enqueue_with_trial_charge(
            tx,
            &format!("external-experiment/{experiment}/compile"),
            &submission,
            true,
            None,
        )
        .await?;
        if admitted.replayed {
            return Err(StoreError::Integrity);
        }
        bind_task(&mut tx, &admitted.resource, definition).await?;
        sqlx::query("INSERT INTO app.external_experiment_requests(experiment_id,compile_run_id,dataset_revision_id,feature_artifact_ids,request,evaluation_request) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(experiment.as_uuid()).bind(admitted.resource.id.as_uuid()).bind(parameters.dataset_revision_id.as_uuid()).bind(db::json(&parameters.feature_artifact_ids)?).bind(db::json(&request.request)?).bind(db::json(&evaluation_request)?).execute(&mut *tx).await?;
        sqlx::query("UPDATE app.experiments SET run_id=$2,revision=revision+1 WHERE id=$1")
            .bind(experiment.as_uuid())
            .bind(admitted.resource.id.as_uuid())
            .execute(&mut *tx)
            .await?;
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            context.validation_input_set_id,
            project,
            context.runtime_id,
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, admitted.resource, 202).await?;
        tx.commit().await?;
        Ok(result)
    }
}

impl Store {
    /// Complete this finite continuation before ACK. A crash after commit reuses
    /// the same next Run; a crash before commit leaves the original PGMQ message.
    pub async fn advance_external_experiment<R, Read, P, Published>(
        &self,
        run: Id,
        mut read: R,
        publish: P,
    ) -> Result<(), StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        let row=sqlx::query("SELECT q.*,e.outcome,e.run_id AS current_run_id,t.run_id AS evaluation_run_id FROM app.external_experiment_requests q JOIN app.experiments e ON e.id=q.experiment_id LEFT JOIN app.external_experiment_tasks t ON t.experiment_id=e.id WHERE q.compile_run_id=$1 OR t.run_id=$1 FOR UPDATE OF e")
            .bind(run.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(());
        };
        let experiment = db::id(row.try_get("experiment_id")?)?;
        let completed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.external_experiment_results WHERE experiment_id=$1)",
        )
        .bind(experiment.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if completed
            || (db::id(row.try_get("compile_run_id")?)? == run
                && db::optional_id(&row, "evaluation_run_id")?.is_some())
        {
            tx.commit().await?;
            return Ok(());
        }
        if !locked.run.state.is_terminal() {
            return Err(StoreError::Conflict);
        }
        let accepted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.run_terminal_receipts WHERE run_id=$1 AND terminal_state=$2 AND attempt_id IS NOT DISTINCT FROM $3)")
            .bind(run.as_uuid()).bind(db::code(&locked.run.state)?).bind(locked.run.active_attempt_id.map(Id::as_uuid)).fetch_one(&mut *tx).await?;
        if !accepted {
            return Err(StoreError::Integrity);
        }
        if locked.run.state != RunState::Succeeded {
            settle(
                &mut tx,
                experiment,
                run,
                None,
                if locked.run.state == RunState::Cancelled {
                    "EXTERNAL_EVALUATION_CANCELLED"
                } else {
                    "EXTERNAL_EXECUTION_FAILED"
                },
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
        if db::optional_id(&row, "evaluation_run_id")? == Some(run) {
            let report = accepted_report(&mut tx, &locked.run).await?;
            settle(
                &mut tx,
                experiment,
                run,
                Some(report),
                "EXTERNAL_EVALUATION_COMPLETE",
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
        if !locked.admission_open() {
            settle(
                &mut tx,
                experiment,
                run,
                None,
                "EXTERNAL_CONTINUATION_CLOSED",
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
        let request: ExperimentEvaluateV1 =
            serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
        let evaluation_request: NativeExperimentEvaluationRequestV1 =
            serde_json::from_value(row.try_get("evaluation_request")?)
                .map_err(|_| StoreError::Integrity)?;
        let cycle = locked.run.cycle_id.ok_or(StoreError::Integrity)?;
        let budget=sqlx::query("SELECT budget_snapshot,(budget_snapshot->>'max_cpu_seconds')::bigint AS maximum,reserved_cpu_seconds,brief_id FROM app.research_cycles WHERE id=$1").bind(cycle.as_uuid()).fetch_one(&mut *tx).await?;
        let remaining = budget
            .try_get::<Option<i64>, _>("maximum")?
            .map(|maximum| maximum.saturating_sub(budget.get::<i64, _>("reserved_cpu_seconds")));
        if remaining.is_some_and(|remaining| {
            request
                .evaluation_limits
                .cpu_seconds
                .is_none_or(|cpu| cpu.get() > remaining.max(0) as u64)
        }) {
            settle(
                &mut tx,
                experiment,
                run,
                None,
                "EXTERNAL_CONTINUATION_BUDGET_EXHAUSTED",
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
        // A closed original authority or incompatible immutable allocation is a
        // terminal research interruption. Storage/SQL failures and stale probes
        // retain the same queue message for reconciliation.
        let prepared: Result<_, StoreError> = async {
            let frozen_budget: BudgetV1 = serde_json::from_value(budget.try_get("budget_snapshot")?).map_err(|_| StoreError::Integrity)?;
            domain::admission::job_resource_limits(&frozen_budget, request.evaluation_limits.wall_seconds, request.evaluation_limits.memory_mib, request.evaluation_limits.output_bytes)?;
            let models=sqlx::query("SELECT model.id FROM app.run_attempts a JOIN app.run_native_outputs o ON o.attempt_id=a.id JOIN app.artifacts model ON model.id=o.artifact_id WHERE a.run_id=$1 AND a.id=$2 AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL AND model.producer_run_id=a.run_id AND model.producer_attempt_id=a.id AND model.kind='MODEL' AND model.schema_name='qz.wasm_model' AND model.schema_version='1' AND model.access_class='RESEARCH'")
                .bind(run.as_uuid()).bind(locked.run.active_attempt_id.map(Id::as_uuid)).fetch_all(&mut *tx).await?;
            let [model] = models.as_slice() else {
                return Err(StoreError::Integrity);
            };
            let model = db::id(model.try_get("id")?)?;
            let model_input = artifact_input(
                &mut tx,
                locked.run.project_id,
                model,
                ArtifactInputRole::Model,
                "qz.wasm_model",
            )
            .await?;
            let context =
                crate::cycles::execution_context(&mut tx, db::id(budget.try_get("brief_id")?)?).await?;
            if context.runtime_id != db::id(locked.admission.try_get("runtime_id")?)?
                || context.runtime_revision
                    != db::revision(locked.admission.try_get("runtime_revision")?)?
            {
                return Err(StoreError::Integrity);
            }
            let mut datasets = crate::data_validation::dataset_bindings(
                &mut tx,
                context.validation_input_set_id,
                locked.run.project_id,
                context.runtime_id,
                &[InputPurpose::Validation],
                &mut read,
            )
            .await?;
            if datasets.len() != 1 {
                return Err(StoreError::Integrity);
            }
            let dataset = datasets.pop().ok_or(StoreError::Integrity)?;
            let dataset_id = db::id(row.try_get("dataset_revision_id")?)?;
            if dataset.selection.dataset_revision_id != dataset_id
                || db::json(&dataset.selection.selection)? != db::json(&evaluation_request.selection)?
            {
                return Err(StoreError::Integrity);
            }
            let feature_ids: Vec<Id> = serde_json::from_value(row.try_get("feature_artifact_ids")?)
                .map_err(|_| StoreError::Integrity)?;
            let mut inputs = vec![dataset.input, model_input];
            let mut parts = Vec::new();
            let mut origin = dataset.origin;
            for id in &feature_ids {
                let feature = crate::recorded_features::resolve(
                    &mut tx, locked.run.project_id, *id, Some(dataset_id),
                ).await?;
                parts.push(crate::recorded_features::read(&feature, &mut read).await?);
                origin = crate::data_validation::combine_origin(origin, feature.origin);
                inputs.push(feature.input);
            }
            domain::execution::features::bind_observations(
                &parts,
                &evaluation_request.feature_schema,
                DataPartition::Validation,
            )?;
            let cap = crate::runtime::require_capabilities(
                &mut tx,
                context.runtime_id,
                context.runtime_revision,
                RunKind::PortfolioSimulate,
            )
            .await?;
            let image = image(&cap, RunKind::PortfolioSimulate)?;
            let assumptions=sqlx::query("SELECT a.engine_image_ref,s.input_set_id,s.settings FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id JOIN app.execution_assumptions a ON a.id=b.execution_assumptions_id JOIN app.execution_assumption_sources s ON s.assumptions_id=a.id AND s.project_id=c.project_id AND s.runtime_id=$2 WHERE c.id=$1")
                .bind(cycle.as_uuid()).bind(context.runtime_id.as_uuid()).fetch_one(&mut *tx).await?;
            crate::research::revalidate_frozen_inputs(
                &mut tx,
                db::id(assumptions.try_get("input_set_id")?)?,
                locked.run.project_id,
                context.runtime_id,
            )
            .await?;
            domain::catalogs::execution_fees(&dataset.metadata, &evaluation_request.settings)?;
            if image != assumptions.try_get::<String, _>("engine_image_ref")?
                || db::json(&evaluation_request.settings)?
                    != assumptions.try_get::<Value, _>("settings")?
            {
                return Err(DomainError::CapabilityUnavailable("external_frozen_engine_image").into());
            }
            let task = NativeTaskParametersV1::EvaluateExperiment {
                schema_version: SchemaV1,
                dataset_revision_id: dataset_id,
                model_artifact_id: model,
                feature_artifact_ids: feature_ids,
                request: Box::new(evaluation_request),
            };
            let definition = native_definition(
                &mut tx,
                (locked.run.project_id, context.runtime_id),
                (&task, &cap),
                inputs,
                origin,
                &request.evaluation_limits,
                publish,
            )
            .await?;
            let submission = RunSubmission {
                cycle_id: cycle,
                input_set_id: context.validation_input_set_id,
                runtime_id: context.runtime_id,
                runtime_revision: context.runtime_revision,
                kind: RunKind::PortfolioSimulate,
                limits: request.evaluation_limits,
            };
            Ok((definition, submission, model))
        }.await;
        let (definition, submission, model) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let Some(reason) = continuation_stop_reason(&error) else {
                    return Err(error);
                };
                settle(&mut tx, experiment, run, None, reason).await?;
                tx.commit().await?;
                return Ok(());
            }
        };
        let (mut tx, admitted, _) = Self::enqueue_with_trial_charge(
            tx,
            &format!("external-experiment/{experiment}/evaluate"),
            &submission,
            false,
            None,
        )
        .await?;
        if admitted.replayed {
            return Err(StoreError::Integrity);
        }
        bind_task(&mut tx, &admitted.resource, definition).await?;
        sqlx::query("INSERT INTO app.external_experiment_tasks(experiment_id,run_id,model_artifact_id) VALUES($1,$2,$3)").bind(experiment.as_uuid()).bind(admitted.resource.id.as_uuid()).bind(model.as_uuid()).execute(&mut *tx).await?;
        sqlx::query("UPDATE app.experiments SET run_id=$2,revision=revision+1 WHERE id=$1")
            .bind(experiment.as_uuid())
            .bind(admitted.resource.id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Read the same immutable native report exposed by artifact export. Nothing
    /// in this projection upgrades a result into support, qualification or PIT.
    pub async fn experiment_evaluation<R, Read>(
        &self,
        actor: &Actor,
        experiment: Id,
        read: R,
    ) -> Result<NativeExperimentEvaluationResultV1, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        Ok(self
            .adopted_experiment_report(actor, experiment, read)
            .await?
            .2)
    }

    pub async fn experiment_summary<R, Read>(
        &self,
        actor: &Actor,
        experiment: Id,
        read: R,
    ) -> Result<contracts::experiment_summary::ExperimentSummaryV1, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let (run, artifact, report) = self
            .adopted_experiment_report(actor, experiment, read)
            .await?;
        Ok(domain::execution::experiment_summary(
            experiment, run, artifact, &report,
        )?)
    }

    async fn adopted_experiment_report<R, Read>(
        &self,
        actor: &Actor,
        experiment: Id,
        mut read: R,
    ) -> Result<(Id, Id, NativeExperimentEvaluationResultV1), StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.experiments WHERE id=$1")
                .bind(experiment.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        authority::read_project(&mut tx, actor, db::id(project)?, MachineScope::ResearchRead)
            .await?;
        let row=sqlx::query("SELECT r.run_id,r.report_artifact_id,a.byte_count FROM app.external_experiment_results r JOIN app.experiments e ON e.id=r.experiment_id AND e.run_id=r.run_id AND e.conclusion_artifact_id=r.report_artifact_id JOIN app.artifacts a ON a.id=r.report_artifact_id AND a.project_id=e.project_id AND a.access_class='RESEARCH' AND a.schema_name='qz.experiment_evaluation' AND a.schema_version='1' AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text WHERE r.experiment_id=$1").bind(experiment.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let id = db::id(row.try_get("report_artifact_id")?)?;
        let size = counter(row.try_get("byte_count")?)?;
        if size == DbCounter::ZERO || size.get() > contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES {
            return Err(StoreError::Integrity);
        }
        let bytes = read(id, size).await?;
        if bytes.len() as u64 != size.get() {
            return Err(StoreError::Integrity);
        }
        let result = serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        let run = db::id(row.try_get("run_id")?)?;
        tx.commit().await?;
        Ok((run, id, result))
    }
}

async fn accepted_report(tx: &mut Tx<'_>, run: &RunSnapshotV1) -> Result<Id, StoreError> {
    let reports:Vec<uuid::Uuid>=sqlx::query_scalar("SELECT report.id FROM app.run_attempts a JOIN app.run_native_outputs o ON o.attempt_id=a.id JOIN app.artifacts report ON report.id=o.artifact_id WHERE a.run_id=$1 AND a.id=$2 AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL AND report.producer_run_id=a.run_id AND report.producer_attempt_id=a.id AND report.kind='REPORT' AND report.schema_name='qz.experiment_evaluation' AND report.schema_version='1' AND report.access_class='RESEARCH'")
        .bind(run.id.as_uuid()).bind(run.active_attempt_id.map(Id::as_uuid)).fetch_all(&mut **tx).await?;
    let [report] = reports.as_slice() else {
        return Err(StoreError::Integrity);
    };
    db::id(*report)
}
