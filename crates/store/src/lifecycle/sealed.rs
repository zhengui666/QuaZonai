//! Publish one held-out execution, never refit or borrow the source Validation verdict.
use super::*;
use contracts::{
    evidence::{Decision, EvidenceStatus},
    execution::NativeTaskParametersV1,
    runtime_jobs::{JobSpecV1, ResultManifestV1},
    science::{NativeAlphaSealedResultV1, NativeFrozenCalibrationV1},
};
use domain::evidence::MetricGate;
use native::NativeObjectPublication;
use validation::read_document;

mod admission;
mod qualification;
mod review;
pub(super) use qualification::grant;
pub(super) use review::pending;

pub(super) async fn publish<R, Read, P, Published>(
    mut tx: Tx<'_>,
    locked: LockedRun,
    mut read: R,
    mut publish: P,
) -> Result<Option<CommandResult<Id>>, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    P: FnMut(NativeObjectPublication) -> Published,
    Published: std::future::Future<Output = Result<(), StoreError>>,
{
    let run = locked.run.id;
    let binding = sqlx::query("SELECT s.*,d.row_count,d.pit_status,d.revision_policy,t.origin,v.calibration_id FROM app.sealed_evaluation_tasks s JOIN app.dataset_revisions d ON d.id=s.dataset_revision_id JOIN app.run_native_tasks t ON t.run_id=s.run_id JOIN app.alpha_versions v ON v.id=s.alpha_version_id WHERE s.run_id=$1")
        .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
    let alpha = db::id(binding.try_get("alpha_version_id")?)?;
    let policy_id = db::id(binding.try_get("policy_id")?)?;
    let existing: Option<uuid::Uuid> = sqlx::query_scalar("SELECT e.id FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE e.run_id=$1 AND e.subject_alpha_version_id=$2 AND e.policy_id=$3 AND e.evaluation_kind='SEALED'")
        .bind(run.as_uuid()).bind(alpha.as_uuid()).bind(policy_id.as_uuid()).fetch_optional(&mut *tx).await?;
    if let Some(existing) = existing {
        tx.commit().await?;
        return Ok(Some(CommandResult {
            schema_version: SchemaV1,
            replayed: true,
            resource: db::id(existing)?,
        }));
    }
    let receipt = sqlx::query("SELECT a.result_manifest_artifact_id,o.exposure_id FROM app.run_terminal_receipts t LEFT JOIN app.run_attempts a ON a.id=t.attempt_id LEFT JOIN app.sealed_opportunities o ON o.attempt_id=a.id WHERE t.run_id=$1 AND t.terminal_state=$2 AND t.attempt_id IS NOT DISTINCT FROM $3")
        .bind(run.as_uuid()).bind(db::code(&locked.run.state)?).bind(locked.run.active_attempt_id.map(Id::as_uuid))
        .fetch_optional(&mut *tx).await?.ok_or(StoreError::Conflict)?;
    if !locked.run.state.is_terminal() {
        return Err(StoreError::Conflict);
    }
    let policy = crate::research::frozen_policy(&mut tx, policy_id).await?;
    let requirements = policy
        .sealed_metric_requirements
        .as_ref()
        .ok_or(StoreError::Integrity)?;
    if policy.project_id != locked.run.project_id {
        return Err(StoreError::Integrity);
    }
    let evaluation = Id::new();
    let report_id = Id::new();
    let manifest_id = db::optional_id(&receipt, "result_manifest_artifact_id")?;
    let exposure_id = db::optional_id(&receipt, "exposure_id")?;
    let concluded_at = now(&mut tx).await?;
    let mut completed_at = locked.run.finished_at.ok_or(StoreError::Integrity)?;
    let mut gate = MetricGate {
        evidence_status: EvidenceStatus::Incomplete,
        decision: Decision::Inconclusive,
        reasons: vec![if locked.run.state == RunState::Cancelled {
            "SEALED_CANCELLED"
        } else {
            "SEALED_EXECUTION_FAILED"
        }
        .into()],
    };
    let mut metrics = Vec::new();
    let mut native_report = None;
    let mut versions = None;
    let mut source_rows = None;
    let mut observations = None;
    if locked.run.state == RunState::Succeeded {
        let attempt = locked.run.active_attempt_id.ok_or(StoreError::Integrity)?;
        exposure_id.ok_or(StoreError::Integrity)?;
        let row = sqlx::query("SELECT s.spec_json,a.created_at FROM app.run_native_attempts s JOIN app.run_attempts a ON a.id=s.attempt_id WHERE s.run_id=$1 AND s.attempt_id=$2 AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL")
            .bind(run.as_uuid()).bind(attempt.as_uuid()).fetch_one(&mut *tx).await?;
        let spec: JobSpecV1 =
            serde_json::from_value(row.try_get("spec_json")?).map_err(|_| StoreError::Integrity)?;
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
        let NativeTaskParametersV1::EvaluateSealedAlpha {
            dataset_revision_id,
            calibration_artifact_id,
            request,
            ..
        } = task
        else {
            return Err(StoreError::Integrity);
        };
        if dataset_revision_id.as_uuid()
            != binding.try_get::<uuid::Uuid, _>("dataset_revision_id")?
        {
            return Err(StoreError::Integrity);
        }
        let calibration: Option<NativeFrozenCalibrationV1> =
            if let Some(id) = calibration_artifact_id {
                let bytes = read_document(
                    &mut tx,
                    id,
                    None,
                    "qz.alpha_calibration",
                    8 * 1024 * 1024,
                    &mut read,
                )
                .await?;
                Some(serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?)
            } else {
                None
            };
        domain::execution::alpha_sealed_policy(
            requirements,
            &request.forecast.selection,
            request.forecast.parameters.label_horizon_observations,
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
        domain::runtime_jobs::manifest(&manifest, &spec, row.try_get("created_at")?, concluded_at)
            .map_err(|_| StoreError::Integrity)?;
        completed_at = manifest.finished_at;
        let outputs: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT a.id FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.producer_run_id=$2 AND a.producer_attempt_id=$1 AND a.schema_name='qz.alpha_sealed' AND a.schema_version='1' AND a.kind='REPORT' AND a.access_class='EVALUATOR_ONLY'")
            .bind(attempt.as_uuid()).bind(run.as_uuid()).fetch_all(&mut *tx).await?;
        let [output] = outputs.as_slice() else {
            return Err(StoreError::Integrity);
        };
        let output = db::id(*output)?;
        let bytes = read_document(
            &mut tx,
            output,
            Some((run, attempt)),
            "qz.alpha_sealed",
            contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
            &mut read,
        )
        .await?;
        let report: NativeAlphaSealedResultV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        let (values, capabilities) = domain::execution::alpha_sealed_metrics(
            evaluation,
            output,
            &request,
            calibration.as_ref(),
            &report,
        )
        .map_err(|_| StoreError::Integrity)?;
        gate =
            domain::evidence::evaluate_metrics(evaluation, requirements, &values, &capabilities)?;
        metrics = values;
        let registered = counter(binding.try_get("row_count")?)?.get();
        let actual = report.forecast.points.len() as u64;
        let labelled: u64 = report
            .assets
            .iter()
            .map(|a| a.observation_count.get())
            .sum();
        let mut incomplete = Vec::new();
        if bigdecimal::BigDecimal::from(registered.saturating_sub(actual))
            > policy.maximum_missing_fraction.as_decimal()
                * bigdecimal::BigDecimal::from(registered)
        {
            incomplete.push("REGISTERED_DATA_MISSING".into());
        }
        if labelled < u64::from(policy.minimum_observations) {
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
        if actual > registered {
            gate.evidence_status = EvidenceStatus::Invalid;
            gate.decision = Decision::Inconclusive;
            gate.reasons
                .push("REGISTERED_DATA_ROW_COUNT_EXCEEDED".into());
        }
        native_report = Some(output);
        versions = Some(report.native_versions);
        source_rows = Some(counter(actual as i64)?);
        observations = Some(counter(labelled as i64)?);
    }
    let valid_until = (locked.run.state == RunState::Succeeded)
        .then(|| {
            i64::try_from(policy.validity_seconds.get())
                .ok()
                .and_then(Duration::try_seconds)
                .and_then(|ttl| completed_at.checked_add_signed(ttl))
                .filter(|until| *until > concluded_at)
        })
        .flatten();
    if valid_until.is_none() && locked.run.state == RunState::Succeeded {
        if gate.evidence_status == EvidenceStatus::Valid {
            gate.evidence_status = EvidenceStatus::Incomplete;
        }
        gate.decision = Decision::Inconclusive;
        gate.reasons.push("SEALED_EXPIRED".into());
    }
    let report = json!({"schema_version":1,"evaluation_id":evaluation,"alpha_version_id":alpha,"run_id":run,"input_set_id":locked.run.input_set_id,"policy_id":policy_id,"evaluation_kind":"SEALED","execution_status":locked.run.state,"evidence_status":gate.evidence_status,"decision":gate.decision,"reasons":gate.reasons,"origin":binding.try_get::<String,_>("origin")?,"validation_evaluation_id":db::id(binding.try_get("validation_evaluation_id")?)?,"calibration_id":db::optional_id(&binding,"calibration_id")?,"exposure_id":exposure_id,"native_report_artifact_id":native_report,"native_manifest_artifact_id":manifest_id,"native_versions":versions,"source_observations":source_rows,"unique_test_observations":observations,"concluded_at":concluded_at,"valid_until":valid_until});
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
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_alpha_version_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) VALUES($1,$2,$3,$4,$5,$6,'SEALED',$7,$8,$9,$10,$10,$11,$12)")
        .bind(evaluation.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(alpha.as_uuid()).bind(locked.run.input_set_id.as_uuid()).bind(policy_id.as_uuid()).bind(run.as_uuid()).bind(db::code(&locked.run.state)?).bind(db::code(&gate.evidence_status)?).bind(db::code(&gate.decision)?).bind(report_id.as_uuid()).bind(concluded_at).bind(valid_until).execute(&mut *tx).await?;
    for metric in metrics {
        sqlx::query("INSERT INTO app.metric_values(evaluation_id,metric_code,scope,value,status,reason_code,unit,period_start,period_end,observation_count,frequency,annualization_factor,method_id,method_version,source_artifact_id,higher_is_better) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(evaluation.as_uuid()).bind(metric.metric_code).bind(metric.scope).bind(metric.value).bind(db::code(&metric.status)?).bind(metric.reason_code).bind(metric.unit).bind(metric.period_start).bind(metric.period_end).bind(metric.observation_count.get() as i64).bind(metric.frequency).bind(metric.annualization_factor).bind(metric.method_id).bind(metric.method_version).bind(metric.source_artifact_id.as_uuid()).bind(metric.higher_is_better).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Some(CommandResult {
        schema_version: SchemaV1,
        replayed: false,
        resource: evaluation,
    }))
}
