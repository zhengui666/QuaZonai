//! Freeze a reusable target policy from an accepted external result. This path
//! creates no computation, Mission, qualification or changed experiment outcome.
use super::*;
use contracts::{
    execution::NativeModelCompilationV1,
    science::FEATURE_MODEL_ABI_V2,
    strategy_portfolio::{
        AcceptedExperimentSourceV1, FeatureReplayInitializationV1, FrozenTargetPolicyV1,
        StrategyAlphaAdoptIntentV1, StrategyAlphaVersionV1, StrategyOutputKindV1,
    },
};

struct AcceptedStrategySource {
    project_id: Id,
    root_lineage_id: Id,
    runtime_id: Id,
    code_artifact_id: Id,
    parameter_artifact_id: Id,
    runtime_image_ref: String,
    model_abi: String,
    source: AcceptedExperimentSourceV1,
    report: NativeExperimentEvaluationResultV1,
}

pub(crate) struct ResolvedStrategyAlpha {
    pub version: StrategyAlphaVersionV1,
    pub report: NativeExperimentEvaluationResultV1,
    pub runtime_id: Id,
}

async fn read_document<T, R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    id: Id,
    kind: &str,
    schema: &str,
    read: &mut R,
) -> Result<T, StoreError>
where
    T: serde::de::DeserializeOwned + Send + 'static,
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind=$3 AND schema_name=$4 AND schema_version='1' AND media_type='application/json' AND access_class='RESEARCH' AND storage_backend='LOCAL' AND storage_object_ref=id::text")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(kind).bind(schema)
        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Integrity)?;
    let size = counter(size)?;
    if size == DbCounter::ZERO || size.get() > contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES {
        return Err(StoreError::Integrity);
    }
    let bytes = read(id, size).await?;
    if bytes.len() as u64 != size.get() {
        return Err(StoreError::Integrity);
    }
    decode_document(bytes).await
}

async fn decode_document<T>(bytes: Vec<u8>) -> Result<T, StoreError>
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    // Large reports can be reached through several lifecycle futures. Decode
    // owned bytes off that poll stack; the transaction stays with the caller.
    tokio::task::spawn_blocking(move || {
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)
    })
    .await
    .map_err(|_| StoreError::Integrity)?
}

/// The original result and both native producers must still agree. The query
/// deliberately has no open-cycle or compute-budget condition: adoption can
/// happen after the finite external research batch has closed.
async fn accepted_source<R, Read>(
    tx: &mut Tx<'_>,
    experiment: Id,
    read: &mut R,
) -> Result<AcceptedStrategySource, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let rows = sqlx::query(
        "SELECT e.project_id,family.root_lineage_id,e.code_artifact_id,e.parameter_artifact_id,\
         result.run_id,result.report_artifact_id,r.active_attempt_id,attempt.runtime_id,\
         q.dataset_revision_id,q.feature_artifact_ids,q.evaluation_request,task.model_artifact_id,\
         native.image_ref,native.parameters_artifact_id AS native_parameters,\
         compiled.parameters_artifact_id AS compile_parameters,compilation.id AS compilation_report,\
         model.byte_count AS model_bytes,model_output.remote_storage_ref AS model_remote_ref \
         FROM app.experiments e \
         JOIN app.experiment_families family ON family.id=e.family_id AND family.project_id=e.project_id \
         JOIN app.experiment_authorship author ON author.experiment_id=e.id AND author.actor_kind IN ('OPERATOR','CLI') \
         JOIN app.external_research_cycles cycle ON cycle.cycle_id=e.cycle_id \
         JOIN app.external_experiment_requests q ON q.experiment_id=e.id \
         JOIN app.external_experiment_results result ON result.experiment_id=e.id AND result.run_id=e.run_id AND result.report_artifact_id=e.conclusion_artifact_id AND result.reason='EXTERNAL_EVALUATION_COMPLETE' \
         JOIN app.external_experiment_tasks task ON task.experiment_id=e.id AND task.run_id=result.run_id \
         JOIN app.runs r ON r.id=task.run_id AND r.project_id=e.project_id AND r.cycle_id=e.cycle_id AND r.state='SUCCEEDED' \
         JOIN app.run_attempts attempt ON attempt.id=r.active_attempt_id AND attempt.run_id=r.id AND attempt.dispatch_state='TERMINAL' AND attempt.accepted_at IS NOT NULL \
         JOIN app.run_terminal_receipts terminal ON terminal.run_id=r.id AND terminal.attempt_id=attempt.id AND terminal.terminal_state='SUCCEEDED' \
         JOIN app.run_native_tasks native ON native.run_id=r.id \
         JOIN app.run_native_outputs output ON output.attempt_id=attempt.id AND output.artifact_id=result.report_artifact_id \
         JOIN app.artifacts report ON report.id=output.artifact_id AND report.project_id=e.project_id AND report.producer_run_id=r.id AND report.producer_attempt_id=attempt.id AND report.kind='REPORT' AND report.schema_name='qz.experiment_evaluation' AND report.schema_version='1' \
         JOIN app.runs compiler ON compiler.id=q.compile_run_id AND compiler.project_id=e.project_id AND compiler.cycle_id=e.cycle_id AND compiler.state='SUCCEEDED' \
         JOIN app.run_attempts ca ON ca.id=compiler.active_attempt_id AND ca.run_id=compiler.id AND ca.dispatch_state='TERMINAL' AND ca.accepted_at IS NOT NULL \
         JOIN app.run_terminal_receipts ct ON ct.run_id=compiler.id AND ct.attempt_id=ca.id AND ct.terminal_state='SUCCEEDED' \
         JOIN app.run_native_tasks compiled ON compiled.run_id=compiler.id \
         JOIN app.run_native_outputs model_output ON model_output.attempt_id=ca.id AND model_output.artifact_id=task.model_artifact_id \
         JOIN app.artifacts model ON model.id=model_output.artifact_id AND model.project_id=e.project_id AND model.producer_run_id=compiler.id AND model.producer_attempt_id=ca.id \
         JOIN app.run_native_outputs compilation_output ON compilation_output.attempt_id=ca.id \
         JOIN app.artifacts compilation ON compilation.id=compilation_output.artifact_id AND compilation.project_id=e.project_id AND compilation.producer_run_id=compiler.id AND compilation.producer_attempt_id=ca.id AND compilation.kind='REPORT' AND compilation.schema_name='qz.model_compilation' AND compilation.schema_version='1' \
         JOIN app.dataset_revisions dataset ON dataset.id=q.dataset_revision_id \
         WHERE e.id=$1",
    ).bind(experiment.as_uuid()).fetch_all(&mut **tx).await?;
    let [row] = rows.as_slice() else {
        return Err(StoreError::Invalid("accepted_external_evaluation_required"));
    };
    let project = db::id(row.try_get("project_id")?)?;
    let model = db::id(row.try_get("model_artifact_id")?)?;
    let code = db::id(row.try_get("code_artifact_id")?)?;
    let parameter = db::id(row.try_get("parameter_artifact_id")?)?;
    artifact_input(tx, project, code, ArtifactInputRole::Code, "qz.rust_source").await?;
    artifact_input(
        tx,
        project,
        model,
        ArtifactInputRole::Model,
        "qz.wasm_model",
    )
    .await?;
    let compilation: NativeModelCompilationV1 = read_document(
        tx,
        project,
        db::id(row.try_get("compilation_report")?)?,
        "REPORT",
        "qz.model_compilation",
        read,
    )
    .await?;
    if compilation.code_artifact_id != code
        || compilation.model_storage_ref != db::id(row.try_get("model_remote_ref")?)?
        || compilation.module_bytes != counter(row.try_get("model_bytes")?)?
        || compilation.abi != FEATURE_MODEL_ABI_V2
        || compilation.target != "wasm32-unknown-unknown"
    {
        return Err(StoreError::Integrity);
    }
    let compiled: NativeTaskParametersV1 = read_document(
        tx,
        project,
        db::id(row.try_get("compile_parameters")?)?,
        "PARAMETERS",
        "qz.native_task",
        read,
    )
    .await?;
    if !matches!(compiled, NativeTaskParametersV1::CompileFeatureModel { code_artifact_id, .. } if code_artifact_id == code)
    {
        return Err(StoreError::Integrity);
    }
    let request: NativeExperimentEvaluationRequestV1 =
        serde_json::from_value(row.try_get("evaluation_request")?)
            .map_err(|_| StoreError::Integrity)?;
    let dataset = db::id(row.try_get("dataset_revision_id")?)?;
    let features: Vec<Id> = serde_json::from_value(row.try_get("feature_artifact_ids")?)
        .map_err(|_| StoreError::Integrity)?;
    let native: NativeTaskParametersV1 = read_document(
        tx,
        project,
        db::id(row.try_get("native_parameters")?)?,
        "PARAMETERS",
        "qz.native_task",
        read,
    )
    .await?;
    let expected = NativeTaskParametersV1::EvaluateExperiment {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        model_artifact_id: model,
        feature_artifact_ids: features.clone(),
        request: Box::new(request.clone()),
    };
    if db::json(&native)? != db::json(&expected)? {
        return Err(StoreError::Integrity);
    }
    let parameters: ExperimentEvaluationParametersV1 = read_document(
        tx,
        project,
        parameter,
        "PARAMETERS",
        "qz.research_parameters",
        read,
    )
    .await?;
    domain::experiments::evaluation_parameters(&parameters)?;
    let expected_parameters = ExperimentEvaluationParametersV1 {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        feature_artifact_ids: features.clone(),
        instrument_id: request.instrument_id.clone(),
        feature_schema: request.feature_schema.clone(),
        label_horizon_observations: request.label_horizon_observations,
        total_fuel: request.total_fuel,
        target_ttl_ns: request.target_ttl_ns,
        decision_output: request.decision_output,
        settings: request.settings.clone(),
    };
    if db::json(&parameters)? != db::json(&expected_parameters)? {
        return Err(StoreError::Integrity);
    }
    for feature in &features {
        crate::recorded_features::resolve(tx, project, *feature, Some(dataset)).await?;
    }
    let source = AcceptedExperimentSourceV1 {
        experiment_id: experiment,
        evaluation_run_id: db::id(row.try_get("run_id")?)?,
        accepted_attempt_id: db::id(row.try_get("active_attempt_id")?)?,
        report_artifact_id: db::id(row.try_get("report_artifact_id")?)?,
    };
    let report = read_document(
        tx,
        project,
        source.report_artifact_id,
        "REPORT",
        "qz.experiment_evaluation",
        read,
    )
    .await?;
    domain::execution::check_experiment_evaluation(&request, dataset, model, &features, &report)?;
    Ok(AcceptedStrategySource {
        project_id: project,
        root_lineage_id: db::id(row.try_get("root_lineage_id")?)?,
        runtime_id: db::id(row.try_get("runtime_id")?)?,
        code_artifact_id: code,
        parameter_artifact_id: parameter,
        runtime_image_ref: row.try_get("image_ref")?,
        model_abi: compilation.abi,
        source,
        report,
    })
}

fn frozen_policy(
    source: &AcceptedStrategySource,
    fold_index: u16,
) -> Result<FrozenTargetPolicyV1, StoreError> {
    let report = &source.report;
    let first = report
        .folds
        .iter()
        .find(|fold| fold.fold_index == fold_index)
        .and_then(|fold| fold.decisions.first())
        .ok_or(StoreError::Invalid("strategy_source_fold"))?;
    Ok(FrozenTargetPolicyV1 {
        schema_version: SchemaV1,
        output_kind: StrategyOutputKindV1::TargetWeight,
        source: source.source.clone(),
        code_artifact_id: source.code_artifact_id,
        model_artifact_id: report.model_artifact_id,
        parameter_artifact_id: source.parameter_artifact_id,
        dataset_revision_id: report.dataset_revision_id,
        feature_artifact_ids: report.feature_artifact_ids.clone(),
        feature_schema: report.request.feature_schema.clone(),
        instrument_id: report.instrument_id.clone(),
        base_currency: report.request.settings.base_currency.clone(),
        model_abi: source.model_abi.clone(),
        initialization: FeatureReplayInitializationV1 {
            source_fold_index: fold_index,
            first_ordinal: first.ordinal,
            first_event_ns: first.event_ns,
            first_decision_ns: first.decision_ns,
        },
        target_ttl_ns: report.request.target_ttl_ns,
        runtime_image_ref: source.runtime_image_ref.clone(),
    })
}

/// Read the accepted immutable research source after execution, independently
/// of whether the Alpha is still eligible for a new admission or delivery.
pub(crate) async fn read_strategy_source<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    policy: &FrozenTargetPolicyV1,
    read: &mut R,
) -> Result<NativeExperimentEvaluationResultV1, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let source = accepted_source(tx, policy.source.experiment_id, read).await?;
    let original = frozen_policy(&source, policy.initialization.source_fold_index)?;
    if source.project_id != project || db::json(&original)? != db::json(policy)? {
        return Err(StoreError::Integrity);
    }
    Ok(source.report)
}

/// Composition callers supply their already-authorized project and enforce the
/// new execution's dataset permissions/runtime compatibility themselves.
pub(crate) async fn resolve_strategy_alpha<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    alpha_version: Id,
    read: &mut R,
) -> Result<ResolvedStrategyAlpha, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row = sqlx::query("SELECT v.* FROM app.alpha_versions v JOIN app.alphas a ON a.id=v.alpha_id AND a.project_id=v.project_id WHERE v.id=$1 AND v.project_id=$2 AND v.output_kind='TARGET_WEIGHT' AND a.lifecycle='RESEARCH'")
        .bind(alpha_version.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(StoreError::Invalid("strategy_alpha_required"))?;
    let version = crate::evidence::strategy_version(&row)?;
    let source = accepted_source(tx, version.experiment_id, read).await?;
    let policy = frozen_policy(&source, version.policy.initialization.source_fold_index)?;
    if source.project_id != project
        || source.root_lineage_id != version.root_lineage_id
        || db::json(&policy)? != db::json(&version.policy)?
    {
        return Err(StoreError::Integrity);
    }
    Ok(ResolvedStrategyAlpha {
        version,
        report: source.report,
        runtime_id: source.runtime_id,
    })
}

impl Store {
    pub async fn adopt_experiment_alpha<R, Read>(
        &self,
        actor: &Actor,
        key: &str,
        request: &StrategyAlphaAdoptIntentV1,
        mut read: R,
    ) -> Result<CommandResult<StrategyAlphaVersionV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        domain::experiments::adopt_alpha(&request.request)?;
        let mut tx = self.pool.begin().await?;
        let experiment = request.experiment_id;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ExperimentAdoptAlpha,
            key,
            Some(experiment),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.experiments WHERE id=$1")
                .bind(experiment.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let project = db::id(project)?;
        crate::research::project_for_write(&mut tx, project).await?;
        if let Actor::Machine { .. } = actor {
            authority::machine(&mut tx, actor, true)
                .await?
                .project(project)?;
        }
        let revision: i64 =
            sqlx::query_scalar("SELECT revision FROM app.experiments WHERE id=$1 FOR UPDATE")
                .bind(experiment.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let current = db::revision(revision)?;
        if current != request.request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        let source = accepted_source(&mut tx, experiment, &mut read).await?;
        // Adoption is a new scientific use. The accepted-source reader above
        // also serves historical audits and therefore remains identity-only.
        let input: uuid::Uuid =
            sqlx::query_scalar("SELECT input_set_id FROM app.runs WHERE id=$1 AND project_id=$2")
                .bind(source.source.evaluation_run_id.as_uuid())
                .bind(project.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            db::id(input)?,
            project,
            source.runtime_id,
        )
        .await?;
        let policy = frozen_policy(&source, request.request.source_fold_index)?;
        if let Some(row) = sqlx::query("SELECT v.*,a.name FROM app.alpha_versions v JOIN app.alphas a ON a.id=v.alpha_id WHERE v.experiment_id=$1 AND v.source_report_artifact_id=$2 AND v.output_kind='TARGET_WEIGHT'")
            .bind(experiment.as_uuid()).bind(source.source.report_artifact_id.as_uuid()).fetch_optional(&mut *tx).await? {
            let version = crate::evidence::strategy_version(&row)?;
            if db::json(&version.policy)? != db::json(&policy)? || row.try_get::<String,_>("name")? != request.request.name {
                return Err(StoreError::Conflict);
            }
            commands::recheck_authority(&mut tx, actor, &prepared).await?;
            let mut result = commands::finish(&mut tx, prepared, version, 201).await?;
            result.replayed = true;
            tx.commit().await?;
            return Ok(result);
        }
        let alpha = Id::new();
        let version = Id::new();
        sqlx::query("INSERT INTO app.alphas(id,project_id,name,lifecycle,active_version_id) VALUES($1,$2,$3,'RESEARCH',$4)")
            .bind(alpha.as_uuid()).bind(project.as_uuid()).bind(&request.request.name).bind(version.as_uuid()).execute(&mut *tx).await?;
        let row = sqlx::query("INSERT INTO app.alpha_versions(id,project_id,alpha_id,version,experiment_id,root_lineage_id,code_artifact_id,model_artifact_id,signal_contract_version,runtime_image_ref,output_kind,strategy_policy,source_evaluation_run_id,source_accepted_attempt_id,source_report_artifact_id) VALUES($1,$2,$3,1,$4,$5,$6,$7,'1',$8,'TARGET_WEIGHT',$9,$10,$11,$12) RETURNING *")
            .bind(version.as_uuid()).bind(project.as_uuid()).bind(alpha.as_uuid()).bind(experiment.as_uuid())
            .bind(source.root_lineage_id.as_uuid()).bind(policy.code_artifact_id.as_uuid()).bind(policy.model_artifact_id.as_uuid())
            .bind(&policy.runtime_image_ref).bind(db::json(&policy)?).bind(policy.source.evaluation_run_id.as_uuid())
            .bind(policy.source.accepted_attempt_id.as_uuid()).bind(policy.source.report_artifact_id.as_uuid()).fetch_one(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(
            &mut tx,
            prepared,
            crate::evidence::strategy_version(&row)?,
            201,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::portfolio::NativeModelRefV1;
    use serde::{Deserialize, Deserializer};
    use std::thread::ThreadId;

    struct DecodedOn<T> {
        value: T,
        thread: ThreadId,
    }

    impl<'de, T: Deserialize<'de>> Deserialize<'de> for DecodedOn<T> {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            Ok(Self {
                value: T::deserialize(deserializer)?,
                thread: std::thread::current().id(),
            })
        }
    }

    fn native_fill() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "adapter_kind": "NAUTILUS_DEFAULT_FILL",
            "upstream_class": contracts::portfolio::NAUTILUS_FILL_CLASS,
            "upstream_version": contracts::portfolio::NAUTILUS_EXECUTION_VERSION,
            "parameters": {
                "prob_fill_on_limit": "1",
                "prob_slippage": "0",
                "random_seed": "7"
            }
        })
    }

    #[tokio::test(flavor = "current_thread")]
    async fn artifact_decode_leaves_poll_thread_and_preserves_typed_contract() {
        let original = native_fill();
        let decoded: DecodedOn<NativeModelRefV1> =
            decode_document(serde_json::to_vec(&original).unwrap())
                .await
                .unwrap();
        assert_ne!(decoded.thread, std::thread::current().id());
        assert_eq!(serde_json::to_value(decoded.value).unwrap(), original);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn artifact_decode_keeps_schema_and_nested_field_rejections() {
        let mut wrong_version = native_fill();
        wrong_version["schema_version"] = json!(2);
        let mut unknown_parameter = native_fill();
        unknown_parameter["parameters"]["invented"] = json!(true);
        let mut invalid_decimal = native_fill();
        invalid_decimal["parameters"]["prob_slippage"] = json!("NaN");
        for bytes in [
            b"{".to_vec(),
            serde_json::to_vec(&wrong_version).unwrap(),
            serde_json::to_vec(&unknown_parameter).unwrap(),
            serde_json::to_vec(&invalid_decimal).unwrap(),
        ] {
            assert!(matches!(
                decode_document::<NativeModelRefV1>(bytes).await,
                Err(StoreError::Integrity)
            ));
        }
    }
}
