//! Shared trusted preparation uses the original paid model and Cycle budget.
use super::*;
use crate::data_validation::{combine_origin, dataset_bindings};
use crate::lifecycle::experiment::native_cpu;
use contracts::{
    artifacts::ArtifactAccess, control::OperatorOperation, evidence::AlphaEvaluateRequestV1,
    research::ArtifactInputRole, runtime_jobs::RuntimeInputV1, science::NativeAlphaSealedRequestV1,
};
use native::{bind_task, NativeTaskDefinition};

impl Store {
    pub async fn start_alpha_evaluation<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        alpha: Id,
        request: &AlphaEvaluateRequestV1,
        read: R,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::data::bounded_native_limits(&request.limits)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::AlphaEvaluate,
            key,
            Some(alpha),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let (mut tx, admitted) = admit(tx, alpha, request, "OPERATOR", None, read, publish).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, admitted.resource, 202).await?;
        tx.commit().await?;
        Ok(result)
    }
}

/// No caller identity is granted here. The authorized outer transaction owns
/// both admission and publication; dropping it rolls back all database effects.
pub(super) async fn admit<'a, R, Read, P, Published>(
    mut tx: Tx<'a>,
    alpha: Id,
    request: &AlphaEvaluateRequestV1,
    created_by: &str,
    parent_deadline: Option<DateTime<Utc>>,
    mut read: R,
    publish: P,
) -> Result<(Tx<'a>, CommandResult<RunSnapshotV1>), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    P: FnOnce(NativeObjectPublication) -> Published,
    Published: std::future::Future<Output = Result<(), StoreError>>,
{
    domain::data::bounded_native_limits(&request.limits)?;
    let project: uuid::Uuid =
        sqlx::query_scalar("SELECT project_id FROM app.alpha_versions WHERE id=$1")
            .bind(alpha.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    let project = db::id(project)?;
    crate::research::project_for_write(&mut tx, project).await?;
    let cycle = sqlx::query("SELECT c.brief_id,b.evaluation_policy_id FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN' WHERE c.id=$1 AND c.project_id=$2 AND c.state='RUNNING' FOR UPDATE OF c")
            .bind(request.cycle_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("alpha_evaluation_cycle"))?;
    let context =
        crate::cycles::execution_context(&mut tx, db::id(cycle.try_get("brief_id")?)?).await?;
    if context.sealed_input_set_id != request.input_set_id
        || context.runtime_id != request.runtime_id
        || context.runtime_revision != request.expected_runtime_revision
        || cycle.try_get::<uuid::Uuid, _>("evaluation_policy_id")? != request.policy_id.as_uuid()
    {
        return Err(StoreError::Invalid("alpha_evaluation_frozen_context"));
    }
    let policy = crate::research::frozen_policy(&mut tx, request.policy_id).await?;
    let requirements = policy
        .sealed_metric_requirements
        .as_ref()
        .ok_or(StoreError::Invalid("sealed_policy_not_defined"))?;
    let source_sql = format!("SELECT ev.id AS evaluation_id,ev.input_set_id AS training_input_set,original.dataset_revision_id AS training_dataset,t.parameters_artifact_id,discovery.origin AS discovery_origin,source_cycle.brief_id AS source_brief_id,v.model_artifact_id,v.runtime_image_ref,model.byte_count AS model_bytes,model.storage_version AS model_version,cal.model_artifact_id AS calibration_artifact_id FROM app.alpha_versions v JOIN app.alphas alpha ON alpha.id=v.alpha_id AND alpha.lifecycle IN ('RESEARCH','QUALIFIED') LEFT JOIN app.calibrations cal ON cal.id=v.calibration_id JOIN ({}) ev ON ((v.signal_kind='SCORE' AND cal.validation_evaluation_id=ev.id) OR (v.signal_kind='EXPECTED_RETURN' AND v.calibration_id IS NULL AND ev.subject_alpha_version_id=v.id)) JOIN app.experiment_validations original ON original.run_id=ev.run_id AND original.experiment_id=v.experiment_id JOIN app.alpha_versions source ON source.id=original.alpha_version_id AND source.alpha_id=v.alpha_id AND source.project_id=v.project_id AND source.root_lineage_id=v.root_lineage_id AND source.code_artifact_id=v.code_artifact_id AND source.model_artifact_id=v.model_artifact_id JOIN app.runs source_run ON source_run.id=original.run_id JOIN app.research_cycles source_cycle ON source_cycle.id=source_run.cycle_id JOIN app.run_native_tasks t ON t.run_id=original.run_id JOIN app.experiment_forecasts f ON f.experiment_id=original.experiment_id JOIN app.run_native_tasks discovery ON discovery.run_id=f.run_id JOIN app.experiment_compilations compiled ON compiled.experiment_id=original.experiment_id JOIN app.run_admissions paid ON paid.run_id=compiled.compile_run_id AND paid.limits->>'experiments'='1' JOIN app.artifacts model ON model.id=v.model_artifact_id AND model.kind='MODEL' AND model.schema_name='qz.wasm_model' AND model.schema_version='1' AND model.project_id=v.project_id AND model.producer_run_id=compiled.compile_run_id WHERE v.id=$1 AND v.project_id=$2 AND ev.project_id=v.project_id AND ev.execution_status='SUCCEEDED' AND ev.evidence_status='VALID' AND ev.decision='PASS' AND ev.valid_until>clock_timestamp()", crate::evidence::EVALUATION);
    let sources = sqlx::query(&source_sql)
        .bind(alpha.as_uuid())
        .bind(project.as_uuid())
        .fetch_all(&mut *tx)
        .await?;
    let [source] = sources.as_slice() else {
        return Err(StoreError::Invalid("original_validation_required"));
    };
    let evaluation = db::id(source.try_get("evaluation_id")?)?;
    let model = db::id(source.try_get("model_artifact_id")?)?;
    let model_bytes = counter(source.try_get("model_bytes")?)?;
    if model_bytes == DbCounter::ZERO || model_bytes.get() > 2 * 1024 * 1024 {
        return Err(StoreError::Integrity);
    }
    let parameters = read_document(
        &mut tx,
        db::id(source.try_get("parameters_artifact_id")?)?,
        None,
        "qz.native_task",
        8 * 1024 * 1024,
        &mut read,
    )
    .await?;
    let NativeTaskParametersV1::ValidateAlpha {
        request: original,
        model_artifact_id,
        ..
    } = serde_json::from_slice(&parameters).map_err(|_| StoreError::Integrity)?
    else {
        return Err(StoreError::Integrity);
    };
    if model_artifact_id != model {
        return Err(StoreError::Integrity);
    }
    let training = dataset_bindings(
        &mut tx,
        db::id(source.try_get("training_input_set")?)?,
        project,
        request.runtime_id,
        &[contracts::research::InputPurpose::Validation],
        &mut read,
    )
    .await?;
    let [training] = training.as_slice() else {
        return Err(StoreError::Integrity);
    };
    if training.selection.dataset_revision_id.as_uuid()
        != source.try_get::<uuid::Uuid, _>("training_dataset")?
    {
        return Err(StoreError::Integrity);
    }
    let old_context =
        crate::cycles::execution_context(&mut tx, db::id(source.try_get("source_brief_id")?)?)
            .await?;
    let discovery = dataset_bindings(
        &mut tx,
        old_context.discovery_input_set_id,
        project,
        request.runtime_id,
        &[contracts::research::InputPurpose::Discovery],
        &mut read,
    )
    .await?;
    let available = discovery
        .iter()
        .map(|d| d.available_through_ns)
        .chain(std::iter::once(training.available_through_ns))
        .max()
        .ok_or(StoreError::Integrity)?;
    let mut datasets = dataset_bindings(
        &mut tx,
        request.input_set_id,
        project,
        request.runtime_id,
        &[contracts::research::InputPurpose::Sealed],
        &mut read,
    )
    .await?;
    if datasets.len() != 1 {
        return Err(StoreError::Invalid("native_alpha_single_sealed_revision"));
    }
    let dataset = datasets.remove(0);
    if dataset.selection.dataset_revision_id != policy.split_policy.sealed_revision_id {
        return Err(StoreError::Invalid("sealed_policy_dataset"));
    }
    let mut forecast = original.forecast;
    forecast.selection = dataset.selection.selection;
    let held = NativeAlphaSealedRequestV1 {
        schema_version: SchemaV1,
        forecast,
        target_kind: original.target_kind,
        research_available_through_ns: available,
    };
    let calibration_artifact_id = db::optional_id(source, "calibration_artifact_id")?;
    let mut inputs = vec![
        dataset.input,
        RuntimeInputV1::Artifact {
            artifact_id: model,
            storage_version: source.try_get("model_version")?,
            byte_count: model_bytes,
            role: ArtifactInputRole::Model,
        },
    ];
    let calibration: Option<NativeFrozenCalibrationV1> = if let Some(id) = calibration_artifact_id {
        let bytes = read_document(
            &mut tx,
            id,
            None,
            "qz.alpha_calibration",
            8 * 1024 * 1024,
            &mut read,
        )
        .await?;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: counter(bytes.len() as i64)?,
            role: ArtifactInputRole::Model,
        });
        Some(serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?)
    } else {
        None
    };
    domain::execution::alpha_sealed_request(&held, calibration.as_ref())?;
    domain::execution::alpha_sealed_policy(
        requirements,
        &held.forecast.selection,
        held.forecast.parameters.label_horizon_observations,
    )?;
    let task = NativeTaskParametersV1::EvaluateSealedAlpha {
        schema_version: SchemaV1,
        dataset_revision_id: dataset.selection.dataset_revision_id,
        model_artifact_id: model,
        calibration_artifact_id,
        request: Box::new(held),
    };
    let capabilities = crate::runtime::require_capabilities(
        &mut tx,
        request.runtime_id,
        request.expected_runtime_revision,
        RunKind::AlphaEvaluate,
    )
    .await?;
    let schemas = task.output_schemas();
    if !schemas.iter().all(|s| {
        capabilities
            .artifact_schemas
            .iter()
            .any(|c| c.name == s.name && c.version == s.version)
    }) {
        return Err(DomainError::CapabilityUnavailable("native_sealed_outputs").into());
    }
    let image = capabilities
        .image_refs
        .iter()
        .find(|i| i.job_kind == RunKind::AlphaEvaluate)
        .ok_or(DomainError::CapabilityUnavailable("native_sealed_image"))?
        .image_ref
        .clone();
    if image != source.try_get::<String, _>("runtime_image_ref")? {
        return Err(DomainError::CapabilityUnavailable("frozen_alpha_image_changed").into());
    }
    let capability: uuid::Uuid = sqlx::query_scalar(
        "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
    )
    .bind(request.runtime_id.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    let origin = combine_origin(
        dataset.origin,
        combine_origin(training.origin, db::enum_value(source, "discovery_origin")?),
    );
    let parameter = Id::new();
    let bytes = serde_json::to_vec(&task).map_err(|_| StoreError::Integrity)?;
    let size = counter(bytes.len() as i64)?;
    publish(NativeObjectPublication {
        id: parameter,
        bytes,
    })
    .await?;
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY',$5,$6,'REFERENCED')")
            .bind(parameter.as_uuid()).bind(project.as_uuid()).bind(parameter.to_string()).bind(size.get() as i64).bind(db::code(&origin)?).bind(created_by).execute(&mut *tx).await?;
    inputs.push(RuntimeInputV1::Artifact {
        artifact_id: parameter,
        storage_version: "1".into(),
        byte_count: size,
        role: ArtifactInputRole::Parameters,
    });
    let (mut tx, admitted, effective) = Store::enqueue_with_trial_charge(
        tx,
        &format!("alpha-evaluate/{}", Id::new()),
        &RunSubmission {
            cycle_id: request.cycle_id,
            input_set_id: request.input_set_id,
            runtime_id: request.runtime_id,
            runtime_revision: request.expected_runtime_revision,
            kind: RunKind::AlphaEvaluate,
            limits: request.limits.clone(),
        },
        false,
        parent_deadline,
    )
    .await?;
    let cpu = native_cpu(&effective, &capabilities)?;
    bind_task(
        &mut tx,
        &admitted.resource,
        NativeTaskDefinition {
            parameters_artifact_id: parameter,
            inputs,
            image_ref: image,
            cpu,
            capability_snapshot_artifact_id: db::id(capability)?,
            output_schemas: schemas,
            origin,
            access: ArtifactAccess::EvaluatorOnly,
        },
    )
    .await?;
    sqlx::query("INSERT INTO app.sealed_evaluation_tasks(run_id,alpha_version_id,policy_id,validation_evaluation_id,dataset_revision_id) VALUES($1,$2,$3,$4,$5)")
            .bind(admitted.resource.id.as_uuid()).bind(alpha.as_uuid()).bind(request.policy_id.as_uuid()).bind(evaluation.as_uuid()).bind(dataset.selection.dataset_revision_id.as_uuid()).execute(&mut *tx).await?;
    native::sealed::validate_binding(&mut tx, admitted.resource.id).await?;
    Ok((tx, admitted))
}
