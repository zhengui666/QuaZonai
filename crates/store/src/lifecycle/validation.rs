//! Publish the original native Alpha evaluation before acknowledging its queue message.
//! The immutable terminal receipt owns execution; this transaction owns the evaluation.
use super::*;
use contracts::{
    evidence::{Decision, EvidenceStatus, MetricValueV1},
    execution::NativeTaskParametersV1,
    runtime_jobs::{JobSpecV1, ResultManifestV1},
    science::NativeAlphaValidationResultV1,
};
use domain::evidence::MetricGate;
use native::NativeObjectPublication;

impl Store {
    /// Trusted Worker continuation, also used after a crash between terminal
    /// adoption and ACK. No caller supplies metrics, a policy or a verdict.
    pub async fn publish_scientific_result<R, Read, P, Published>(
        &self,
        run: Id,
        mut read: R,
        mut publish: P,
    ) -> Result<Option<CommandResult<Id>>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        if locked.run.kind == RunKind::PortfolioBuild {
            return super::portfolio::publish(tx, locked, read, publish).await;
        }
        let held_out: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.sealed_evaluation_tasks WHERE run_id=$1)",
        )
        .bind(run.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if held_out {
            return super::sealed::publish(tx, locked, read, publish).await;
        }
        let Some(binding) = sqlx::query("SELECT v.*,e.outcome,d.row_count,d.origin,d.pit_status,d.revision_policy FROM app.experiment_validations v JOIN app.experiments e ON e.id=v.experiment_id JOIN app.dataset_revisions d ON d.id=v.dataset_revision_id WHERE v.run_id=$1 FOR UPDATE OF e")
            .bind(run.as_uuid()).fetch_optional(&mut *tx).await? else {
                tx.commit().await?;
                return Ok(None);
            };
        let alpha = db::id(binding.try_get("alpha_version_id")?)?;
        let policy_id = db::id(binding.try_get("policy_id")?)?;
        let existing: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.evaluations WHERE run_id=$1 AND subject_alpha_version_id=$2 AND policy_id=$3 AND evaluation_kind='WALK_FORWARD'")
            .bind(run.as_uuid()).bind(alpha.as_uuid()).bind(policy_id.as_uuid()).fetch_optional(&mut *tx).await?;
        if let Some(existing) = existing {
            tx.commit().await?;
            return Ok(Some(CommandResult {
                schema_version: SchemaV1,
                replayed: true,
                resource: db::id(existing)?,
            }));
        }
        let receipt = sqlx::query("SELECT a.result_manifest_artifact_id FROM app.run_terminal_receipts t LEFT JOIN app.run_attempts a ON a.id=t.attempt_id WHERE t.run_id=$1 AND t.terminal_state=$2 AND t.attempt_id IS NOT DISTINCT FROM $3")
            .bind(run.as_uuid()).bind(db::code(&locked.run.state)?).bind(locked.run.active_attempt_id.map(Id::as_uuid))
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::Conflict)?;
        if !locked.run.state.is_terminal() || binding.try_get::<String, _>("outcome")? != "PENDING"
        {
            return Err(StoreError::Conflict);
        }
        let policy = crate::research::frozen_policy(&mut tx, policy_id).await?;
        if policy.project_id != locked.run.project_id
            || policy.selection_rule.comparison_input_set_id != locked.run.input_set_id
        {
            return Err(StoreError::Integrity);
        }
        let evaluation = Id::new();
        let report_id = Id::new();
        let mut gate = MetricGate {
            evidence_status: EvidenceStatus::Incomplete,
            decision: Decision::Inconclusive,
            reasons: vec![if locked.run.state == RunState::Cancelled {
                "VALIDATION_CANCELLED"
            } else {
                "VALIDATION_EXECUTION_FAILED"
            }
            .into()],
        };
        let mut metrics: Vec<MetricValueV1> = Vec::new();
        let mut native_report = None;
        let mut versions = None;
        let mut source_rows = None;
        let mut observations = None;
        let mut calibration = None;
        let manifest_id = db::optional_id(&receipt, "result_manifest_artifact_id")?;
        let concluded_at = now(&mut tx).await?;
        let mut completed_at = locked.run.finished_at.ok_or(StoreError::Integrity)?;
        if locked.run.state == RunState::Succeeded {
            let attempt = locked.run.active_attempt_id.ok_or(StoreError::Integrity)?;
            let row = sqlx::query("SELECT s.spec_json,a.created_at FROM app.run_native_attempts s JOIN app.run_attempts a ON a.id=s.attempt_id WHERE s.run_id=$1 AND s.attempt_id=$2 AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL")
                .bind(run.as_uuid()).bind(attempt.as_uuid()).fetch_one(&mut *tx).await?;
            let spec: JobSpecV1 = serde_json::from_value(row.try_get("spec_json")?)
                .map_err(|_| StoreError::Integrity)?;
            let parameters = read_document(
                &mut tx,
                spec.parameters_artifact_id,
                None,
                "qz.native_task",
                8 * 1024 * 1024,
                &mut read,
            )
            .await?;
            let task: NativeTaskParametersV1 =
                serde_json::from_slice(&parameters).map_err(|_| StoreError::Integrity)?;
            domain::execution::task(&spec, &task).map_err(|_| StoreError::Integrity)?;
            let NativeTaskParametersV1::ValidateAlpha {
                dataset_revision_id,
                request,
                ..
            } = task
            else {
                return Err(StoreError::Integrity);
            };
            if dataset_revision_id.as_uuid()
                != binding.try_get::<uuid::Uuid, _>("dataset_revision_id")?
                || db::json(&request.split_policy)? != db::json(&policy.split_policy)?
            {
                return Err(StoreError::Integrity);
            }
            domain::execution::alpha_validation_policy(
                &policy.selection_rule,
                &policy.metric_requirements,
                &request.forecast.selection,
                &request.split_policy,
            )?;
            let manifest = read_document(
                &mut tx,
                manifest_id.ok_or(StoreError::Integrity)?,
                Some((run, attempt)),
                "qz.job_result",
                domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES,
                &mut read,
            )
            .await?;
            let manifest: ResultManifestV1 =
                serde_json::from_slice(&manifest).map_err(|_| StoreError::Integrity)?;
            domain::runtime_jobs::manifest(
                &manifest,
                &spec,
                row.try_get("created_at")?,
                concluded_at,
            )
            .map_err(|_| StoreError::Integrity)?;
            completed_at = manifest.finished_at;
            let outputs: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT a.id FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.producer_run_id=$2 AND a.producer_attempt_id=$1 AND a.schema_name='qz.alpha_validation' AND a.schema_version='1' AND a.kind='REPORT' AND a.access_class='EVALUATOR_ONLY'")
                .bind(attempt.as_uuid()).bind(run.as_uuid()).fetch_all(&mut *tx).await?;
            let [output] = outputs.as_slice() else {
                return Err(StoreError::Integrity);
            };
            let output = db::id(*output)?;
            let bytes = read_document(
                &mut tx,
                output,
                Some((run, attempt)),
                "qz.alpha_validation",
                contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
                &mut read,
            )
            .await?;
            let report: NativeAlphaValidationResultV1 =
                serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
            let (values, capabilities) =
                domain::execution::alpha_validation_metrics(evaluation, output, &request, &report)
                    .map_err(|_| StoreError::Integrity)?;
            gate = domain::evidence::evaluate_metrics(
                evaluation,
                &policy.metric_requirements,
                &values,
                &capabilities,
            )?;
            metrics = values;
            let registered = counter(binding.try_get("row_count")?)?;
            let actual: u64 = report
                .folds
                .iter()
                .filter(|fold| fold.fold_index == 0)
                .map(|fold| fold.source_row_count.get())
                .sum();
            let missing = registered.get().saturating_sub(actual);
            let missing_exceeded = bigdecimal::BigDecimal::from(missing)
                > policy.maximum_missing_fraction.as_decimal()
                    * bigdecimal::BigDecimal::from(registered.get());
            let mut incomplete = Vec::new();
            if missing_exceeded {
                incomplete.push("REGISTERED_DATA_MISSING".into());
            }
            if report.unique_test_observations.get() < u64::from(policy.minimum_observations) {
                incomplete.push("INSUFFICIENT_TEST_OBSERVATIONS".into());
            }
            if policy.require_real_data
                && (binding.try_get::<String, _>("origin")? != "REAL"
                    || binding.try_get::<String, _>("pit_status")? != "VERIFIED"
                    || binding.try_get::<String, _>("revision_policy")? != "AS_KNOWN_THEN")
            {
                incomplete.push("REAL_POINT_IN_TIME_DATA_REQUIRED".into());
            }
            if !incomplete.is_empty() {
                if gate.evidence_status == EvidenceStatus::Valid {
                    gate.evidence_status = EvidenceStatus::Incomplete;
                }
                gate.decision = Decision::Inconclusive;
                gate.reasons.extend(incomplete);
            }
            if actual > registered.get() {
                gate.evidence_status = EvidenceStatus::Invalid;
                gate.decision = Decision::Inconclusive;
                gate.reasons
                    .push("REGISTERED_DATA_ROW_COUNT_EXCEEDED".into());
            }
            calibration = domain::execution::freeze_alpha_calibration(&request, &report, output)
                .map_err(|_| StoreError::Integrity)?;
            native_report = Some(output);
            versions = Some(report.native_versions);
            source_rows = Some(counter(actual as i64)?);
            observations = Some(report.unique_test_observations);
        }
        let valid_until = i64::try_from(policy.validity_seconds.get())
            .ok()
            .and_then(Duration::try_seconds)
            .and_then(|ttl| completed_at.checked_add_signed(ttl))
            .filter(|until| *until > concluded_at);
        if valid_until.is_none() && locked.run.state == RunState::Succeeded {
            if gate.evidence_status == EvidenceStatus::Valid {
                gate.evidence_status = EvidenceStatus::Incomplete;
            }
            gate.decision = Decision::Inconclusive;
            gate.reasons.push("VALIDATION_EXPIRED".into());
        }
        let valid_until = (locked.run.state == RunState::Succeeded)
            .then_some(valid_until)
            .flatten();
        let experiment = db::id(binding.try_get("experiment_id")?)?;
        let report = json!({"schema_version":1,"evaluation_id":evaluation,"experiment_id":experiment,"alpha_version_id":alpha,"run_id":run,"input_set_id":locked.run.input_set_id,"policy_id":policy_id,"evaluation_kind":"WALK_FORWARD","execution_status":locked.run.state,"evidence_status":gate.evidence_status,"decision":gate.decision,"reasons":gate.reasons,"origin":binding.try_get::<String,_>("origin")?,"native_report_artifact_id":native_report,"native_manifest_artifact_id":manifest_id,"native_versions":versions,"source_observations":source_rows,"unique_test_observations":observations,"concluded_at":concluded_at,"valid_until":valid_until});
        let bytes = serde_json::to_vec(&report).map_err(|_| StoreError::Integrity)?;
        if bytes.len() > 64 * 1024 {
            return Err(StoreError::Integrity);
        }
        let size = bytes.len() as i64;
        publish(NativeObjectPublication {
            id: report_id,
            bytes,
        })
        .await?;
        let published_at = now(&mut tx).await?;
        if valid_until.is_some_and(|until| until <= published_at) {
            return Err(StoreError::Conflict);
        }
        sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'REPORT','application/json','qz.alpha_evaluation','1','LOCAL',$5,'1',$6,'EVALUATOR_ONLY',$7,'RUNTIME','REFERENCED')")
            .bind(report_id.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(run.as_uuid()).bind(locked.run.active_attempt_id.map(Id::as_uuid)).bind(report_id.to_string()).bind(size).bind(binding.try_get::<String,_>("origin")?).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_alpha_version_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) VALUES($1,$2,$3,$4,$5,$6,'WALK_FORWARD',$7,$8,$9,$10,$10,$11,$12)")
            .bind(evaluation.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(alpha.as_uuid()).bind(locked.run.input_set_id.as_uuid()).bind(policy_id.as_uuid()).bind(run.as_uuid()).bind(db::code(&locked.run.state)?).bind(db::code(&gate.evidence_status)?).bind(db::code(&gate.decision)?).bind(report_id.as_uuid()).bind(concluded_at).bind(valid_until).execute(&mut *tx).await?;
        for metric in metrics {
            sqlx::query("INSERT INTO app.metric_values(evaluation_id,metric_code,scope,value,status,reason_code,unit,period_start,period_end,observation_count,frequency,annualization_factor,method_id,method_version,source_artifact_id,higher_is_better) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
                .bind(evaluation.as_uuid()).bind(metric.metric_code).bind(metric.scope).bind(metric.value).bind(db::code(&metric.status)?).bind(metric.reason_code).bind(metric.unit).bind(metric.period_start).bind(metric.period_end).bind(metric.observation_count.get() as i64).bind(metric.frequency).bind(metric.annualization_factor).bind(metric.method_id).bind(metric.method_version).bind(metric.source_artifact_id.as_uuid()).bind(metric.higher_is_better).execute(&mut *tx).await?;
        }
        // The first experiment verdict belongs to this still-open aggregate.
        // A calibration reference seals it through the existing native trigger.
        let (outcome, reason) = match (gate.evidence_status, gate.decision) {
            (EvidenceStatus::Valid, Decision::Pass) => ("SUPPORTED", "VALIDATION_METRICS_PASSED"),
            (EvidenceStatus::Valid, Decision::Reject) => {
                ("REJECTED", "VALIDATION_METRICS_REJECTED")
            }
            (EvidenceStatus::Invalid, _) => ("INVALID", "VALIDATION_EVIDENCE_INVALID"),
            _ => ("INCONCLUSIVE", "VALIDATION_EVIDENCE_INCOMPLETE"),
        };
        sqlx::query("UPDATE app.experiments SET outcome=$2,outcome_reason=$3,conclusion_artifact_id=$4,revision=revision+1 WHERE id=$1")
            .bind(experiment.as_uuid()).bind(outcome).bind(reason).bind(report_id.as_uuid()).execute(&mut *tx).await?;
        if let Some(model) = calibration.filter(|_| gate.evidence_status == EvidenceStatus::Valid) {
            let id = Id::new();
            let artifact = Id::new();
            let bytes = serde_json::to_vec(&model).map_err(|_| StoreError::Integrity)?;
            if bytes.len() > contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize {
                return Err(StoreError::Integrity);
            }
            let size = bytes.len() as i64;
            // PostgreSQL stores microseconds. Never round a future training
            // label backwards; the native model retains its original nanoseconds.
            let ns = model.fit_end_available_ns.get() as i64;
            let end = DateTime::<Utc>::from_timestamp_micros(ns / 1000 + i64::from(ns % 1000 != 0))
                .ok_or(StoreError::Integrity)?;
            publish(NativeObjectPublication {
                id: artifact,
                bytes,
            })
            .await?;
            let published_at = now(&mut tx).await?;
            if valid_until.is_none_or(|until| until <= published_at) {
                return Err(StoreError::Conflict);
            }
            sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'MODEL','application/json','qz.alpha_calibration','1','LOCAL',$5,'1',$6,'EVALUATOR_ONLY',$7,'RUNTIME','REFERENCED')")
                .bind(artifact.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(run.as_uuid()).bind(locked.run.active_attempt_id.map(Id::as_uuid)).bind(artifact.to_string()).bind(size).bind(binding.try_get::<String,_>("origin")?).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO app.calibrations(id,estimator_kind,estimator_version,model_artifact_id,train_input_set_id,fit_end_available_at,output_unit,horizon_kind,horizon_value,validation_evaluation_id) VALUES($1,$2,$3,$4,$5,$6,'RETURN_PER_HORIZON','FIXED_BARS',$7,$8)")
                .bind(id.as_uuid()).bind(&model.estimator_kind).bind(&model.estimator_version).bind(artifact.as_uuid()).bind(locked.run.input_set_id.as_uuid()).bind(end).bind(model.horizon_observations.get() as i64).bind(evaluation.as_uuid()).execute(&mut *tx).await?;
            let version: uuid::Uuid = sqlx::query_scalar("INSERT INTO app.alpha_versions(project_id,alpha_id,version,experiment_id,root_lineage_id,code_artifact_id,model_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,calibration_id,runtime_image_ref) SELECT project_id,alpha_id,version+1,experiment_id,root_lineage_id,code_artifact_id,model_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,$2,runtime_image_ref FROM app.alpha_versions WHERE id=$1 RETURNING id")
                .bind(alpha.as_uuid()).bind(id.as_uuid()).fetch_one(&mut *tx).await?;
            sqlx::query("UPDATE app.alphas a SET active_version_id=$2,revision=revision+1 WHERE a.active_version_id=$1 AND a.lifecycle='RESEARCH'")
                .bind(alpha.as_uuid()).bind(version).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(Some(CommandResult {
            schema_version: SchemaV1,
            replayed: false,
            resource: evaluation,
        }))
    }
}

pub(super) async fn read_document<R, Read>(
    tx: &mut Tx<'_>,
    id: Id,
    producer: Option<(Id, Id)>,
    schema: &str,
    maximum: usize,
    read: &mut R,
) -> Result<Vec<u8>, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row = sqlx::query("SELECT byte_count,producer_run_id,producer_attempt_id FROM app.artifacts WHERE id=$1 AND schema_name=$2 AND schema_version='1' AND media_type='application/json' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1' AND access_class='EVALUATOR_ONLY'")
        .bind(id.as_uuid()).bind(schema).fetch_one(&mut **tx).await?;
    if let Some((run, attempt)) = producer {
        if db::optional_id(&row, "producer_run_id")? != Some(run)
            || db::optional_id(&row, "producer_attempt_id")? != Some(attempt)
        {
            return Err(StoreError::Integrity);
        }
    }
    let size = counter(row.try_get("byte_count")?)?;
    if size == DbCounter::ZERO || size.get() > maximum as u64 {
        return Err(StoreError::Integrity);
    }
    let bytes = read(id, size).await?;
    if bytes.len() as u64 != size.get() {
        return Err(StoreError::Integrity);
    }
    Ok(bytes)
}
