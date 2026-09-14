//! Original downstream measurement publication; validity never grants approval.
use super::*;
use contracts::{
    evidence::{EvidenceStatus, MetricStatus},
    execution::NativeTaskParametersV1,
    forward::NativeForwardResultV1,
    runtime_jobs::{JobSpecV1, ResultManifestV1},
};
use native::NativeObjectPublication;

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
    let run = &locked.run;
    let binding = sqlx::query("SELECT f.*,h.release_id,r.candidate_id,m.required_evaluation_policy_id FROM app.forward_evaluation_inputs f JOIN app.handoff_offers h ON h.id=f.handoff_id JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id AND c.project_id=f.project_id JOIN app.portfolio_mandates m ON m.id=c.mandate_id WHERE f.input_set_id=$1 AND f.project_id=$2")
        .bind(run.input_set_id.as_uuid()).bind(run.project_id.as_uuid()).fetch_one(&mut *tx).await?;
    let prior: Option<uuid::Uuid> = sqlx::query_scalar("SELECT e.id FROM app.evaluations e JOIN app.forward_evidence_windows w ON w.evaluation_id=e.id WHERE e.run_id=$1 AND e.evaluation_kind='FORWARD'")
        .bind(run.id.as_uuid()).fetch_optional(&mut *tx).await?;
    if let Some(id) = prior {
        tx.commit().await?;
        return Ok(Some(CommandResult {
            schema_version: SchemaV1,
            replayed: true,
            resource: db::id(id)?,
        }));
    }
    if !run.state.is_terminal() {
        return Err(StoreError::Conflict);
    }
    let terminal = sqlx::query("SELECT a.result_manifest_artifact_id FROM app.run_terminal_receipts t LEFT JOIN app.run_attempts a ON a.id=t.attempt_id WHERE t.run_id=$1 AND t.terminal_state=$2 AND t.attempt_id IS NOT DISTINCT FROM $3")
        .bind(run.id.as_uuid()).bind(db::code(&run.state)?).bind(run.active_attempt_id.map(Id::as_uuid)).fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
    let frozen: NativeTaskParametersV1 =
        serde_json::from_value(binding.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    let NativeTaskParametersV1::EvaluateForward { request, .. } = &frozen else {
        return Err(StoreError::Integrity);
    };
    domain::forward::evaluation::request(request)?;
    let candidate = db::id(binding.try_get("candidate_id")?)?;
    let policy = db::id(binding.try_get("required_evaluation_policy_id")?)?;
    let evaluation = Id::new();
    let report = Id::new();
    let manifest_id = db::optional_id(&terminal, "result_manifest_artifact_id")?;
    let concluded_at = now(&mut tx).await?;
    let deadline: DateTime<Utc> = binding.try_get("valid_until")?;
    let mut metrics = Vec::new();
    let mut native_report = None;
    let mut versions = None;
    let mut reasons = Vec::<String>::new();
    if run.state == RunState::Succeeded {
        let attempt = run.active_attempt_id.ok_or(StoreError::Integrity)?;
        let original = sqlx::query("SELECT s.spec_json,a.created_at FROM app.run_native_attempts s JOIN app.run_attempts a ON a.id=s.attempt_id WHERE s.run_id=$1 AND s.attempt_id=$2 AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL")
            .bind(run.id.as_uuid()).bind(attempt.as_uuid()).fetch_one(&mut *tx).await?;
        let spec: JobSpecV1 = serde_json::from_value(original.try_get("spec_json")?)
            .map_err(|_| StoreError::Integrity)?;
        if spec.run_id != run.id
            || spec.input_set_id != run.input_set_id
            || spec.parameters_artifact_id.as_uuid()
                != binding.try_get::<uuid::Uuid, _>("parameters_artifact_id")?
        {
            return Err(StoreError::Integrity);
        }
        let bytes = validation::read_document(
            &mut tx,
            spec.parameters_artifact_id,
            None,
            "qz.native_task",
            8 * 1024 * 1024,
            &mut read,
        )
        .await?;
        let task: NativeTaskParametersV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        if db::json(&task)? != db::json(&frozen)? {
            return Err(StoreError::Integrity);
        }
        domain::execution::task(&spec, &task).map_err(|_| StoreError::Integrity)?;
        let bytes = validation::read_document(
            &mut tx,
            manifest_id.ok_or(StoreError::Integrity)?,
            Some((run.id, attempt)),
            "qz.job_result",
            domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES,
            &mut read,
        )
        .await?;
        let manifest: ResultManifestV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        domain::runtime_jobs::manifest(
            &manifest,
            &spec,
            original.try_get("created_at")?,
            concluded_at,
        )
        .map_err(|_| StoreError::Integrity)?;
        let [output] = manifest.artifacts.as_slice() else {
            return Err(StoreError::Integrity);
        };
        if output.schema.name != "qz.forward_evaluation" || output.schema.version != "1" {
            return Err(StoreError::Integrity);
        }
        let ids:Vec<uuid::Uuid> = sqlx::query_scalar("SELECT a.id FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.producer_run_id=$2 AND a.producer_attempt_id=$1 AND a.schema_name='qz.forward_evaluation' AND a.schema_version='1' AND a.kind='REPORT' AND a.access_class='EVALUATOR_ONLY' AND a.origin='REAL' AND o.remote_storage_ref=$3 AND a.media_type=$4 AND a.byte_count=$5")
            .bind(attempt.as_uuid()).bind(run.id.as_uuid()).bind(output.storage_ref.as_uuid()).bind(&output.media_type).bind(output.byte_count.get() as i64).fetch_all(&mut *tx).await?;
        let [id] = ids.as_slice() else {
            return Err(StoreError::Integrity);
        };
        let id = db::id(*id)?;
        let bytes = validation::read_document(
            &mut tx,
            id,
            Some((run.id, attempt)),
            "qz.forward_evaluation",
            contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
            &mut read,
        )
        .await?;
        let result: NativeForwardResultV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        metrics = domain::forward::evaluation::metrics(evaluation, id, request, &result)
            .map_err(|_| StoreError::Integrity)?
            .0;
        if metrics.iter().any(|m| m.status != MetricStatus::Ok) {
            reasons.push("FORWARD_STATISTICS_INCOMPLETE".into());
        }
        native_report = Some(id);
        versions = Some(manifest.engine_versions);
        // Project lock in revalidate serializes corrections/revocation with this publication.
        match crate::forward::revalidate(
            &mut tx,
            run.input_set_id,
            run.project_id,
            db::id(binding.try_get("runtime_id")?)?,
        )
        .await
        {
            Ok(()) => (),
            Err(StoreError::Invalid(_) | StoreError::Domain(_) | StoreError::NotFound) => {
                reasons.push("FORWARD_SOURCE_OR_POLICY_NO_LONGER_CURRENT".into())
            }
            Err(error) => return Err(error),
        }
    } else {
        reasons.push(
            if run.state == RunState::Cancelled {
                "FORWARD_CANCELLED"
            } else {
                "FORWARD_EXECUTION_FAILED"
            }
            .into(),
        );
    }
    if deadline <= concluded_at {
        reasons.push("FORWARD_EVIDENCE_EXPIRED".into());
    }
    let revoked: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT min(effective_at) FROM app.policy_revocations WHERE automation_policy_id=$1",
    )
    .bind(binding.try_get::<uuid::Uuid, _>("policy_id")?)
    .fetch_one(&mut *tx)
    .await?;
    let effective_deadline = revoked.map_or(deadline, |at| at.min(deadline));
    let valid = reasons.is_empty();
    let status = if valid {
        EvidenceStatus::Valid
    } else {
        EvidenceStatus::Incomplete
    };
    let until = valid.then_some(effective_deadline);
    portfolio::document(&mut tx,run,report,"qz.forward_measurement","REAL",json!({"schema_version":1,"evaluation_id":evaluation,"candidate_id":candidate,"policy_id":policy,"automation_policy_id":db::id(binding.try_get("policy_id")?)?,"run_id":run.id,"input_set_id":run.input_set_id,"evaluation_kind":"FORWARD","execution_status":run.state,"evidence_status":status,"decision":"INCONCLUSIVE","reasons":reasons,"window":request.window,"native_manifest_artifact_id":manifest_id,"native_report_artifact_id":native_report,"native_versions":versions,"concluded_at":concluded_at,"valid_until":until}),&mut publish).await?;
    if valid && effective_deadline <= now(&mut tx).await? {
        return Err(StoreError::Conflict);
    }
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) VALUES($1,$2,$3,$4,$5,$6,'FORWARD',$7,$8,'INCONCLUSIVE',$9,$9,$10,$11)")
        .bind(evaluation.as_uuid()).bind(run.project_id.as_uuid()).bind(candidate.as_uuid()).bind(run.input_set_id.as_uuid()).bind(policy.as_uuid()).bind(run.id.as_uuid()).bind(db::code(&run.state)?).bind(db::code(&status)?).bind(report.as_uuid()).bind(concluded_at).bind(until).execute(&mut *tx).await?;
    for metric in metrics {
        sqlx::query("INSERT INTO app.metric_values(evaluation_id,metric_code,scope,value,status,reason_code,unit,period_start,period_end,observation_count,frequency,annualization_factor,method_id,method_version,source_artifact_id,higher_is_better) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(evaluation.as_uuid()).bind(metric.metric_code).bind(metric.scope).bind(metric.value).bind(db::code(&metric.status)?).bind(metric.reason_code).bind(metric.unit).bind(metric.period_start).bind(metric.period_end).bind(metric.observation_count.get() as i64).bind(metric.frequency).bind(metric.annualization_factor).bind(metric.method_id).bind(metric.method_version).bind(metric.source_artifact_id.as_uuid()).bind(metric.higher_is_better).execute(&mut *tx).await?;
    }
    // This reference seals the aggregate, so all metrics must already exist.
    sqlx::query("INSERT INTO app.forward_evidence_windows(release_id,input_set_id,evaluation_id,window_start,window_end,complete_observations,is_contiguous,freshness_deadline) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(binding.try_get::<uuid::Uuid,_>("release_id")?).bind(run.input_set_id.as_uuid()).bind(evaluation.as_uuid()).bind(request.window.window_start.ok_or(StoreError::Integrity)?).bind(request.window.window_end.ok_or(StoreError::Integrity)?).bind(if valid {request.window.complete_observations.get() as i64} else {0}).bind(valid).bind(deadline).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(CommandResult {
        schema_version: SchemaV1,
        replayed: false,
        resource: evaluation,
    }))
}

impl Store {
    /// Trusted post-publication Worker continuation. No caller-provided verdict or Wake authority.
    pub async fn observe_forward(&self, run: Id) -> Result<Option<CommandResult<Id>>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        if locked.run.kind != RunKind::ForwardEvaluate {
            tx.commit().await?;
            return Ok(None);
        }
        let prior: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT observation_id FROM app.forward_observation_publications WHERE run_id=$1",
        )
        .bind(run.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(id) = prior {
            tx.commit().await?;
            return Ok(Some(CommandResult {
                schema_version: SchemaV1,
                replayed: true,
                resource: db::id(id)?,
            }));
        }
        if !locked.run.state.is_terminal() {
            return Err(StoreError::Conflict);
        }
        let row=sqlx::query("SELECT e.id AS evaluation_id,e.execution_status,e.evidence_status,e.valid_until,w.is_contiguous,w.complete_observations,w.freshness_deadline,f.policy_id,f.runtime_id,h.release_id FROM app.runs r JOIN app.forward_evaluation_inputs f ON f.input_set_id=r.input_set_id AND f.project_id=r.project_id JOIN app.handoff_offers h ON h.id=f.handoff_id JOIN app.releases release ON release.id=h.release_id JOIN app.evaluations e ON e.run_id=r.id AND e.input_set_id=r.input_set_id AND e.subject_candidate_id=release.candidate_id AND e.evaluation_kind='FORWARD' AND e.execution_status=r.state JOIN app.evaluation_publications published ON published.evaluation_id=e.id JOIN app.forward_evidence_windows w ON w.evaluation_id=e.id AND w.release_id=h.release_id AND w.input_set_id=e.input_set_id JOIN app.artifacts a ON a.id=e.report_artifact_id AND a.id=e.method_versions_artifact_id AND a.producer_run_id=r.id AND a.producer_attempt_id IS NOT DISTINCT FROM r.active_attempt_id AND a.schema_name='qz.forward_measurement' AND a.schema_version='1' AND a.origin='REAL' AND a.access_class='EVALUATOR_ONLY' JOIN app.run_terminal_receipts terminal ON terminal.run_id=r.id AND terminal.terminal_state=r.state AND terminal.attempt_id IS NOT DISTINCT FROM r.active_attempt_id WHERE r.id=$1")
            .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
        let evaluation = db::id(row.try_get("evaluation_id")?)?;
        let policy_row = sqlx::query("SELECT * FROM app.automation_policies WHERE id=$1")
            .bind(row.try_get::<uuid::Uuid, _>("policy_id")?)
            .fetch_one(&mut *tx)
            .await?;
        let policy = crate::automation::view(&policy_row)?;
        domain::delivery::automation_policy(&policy.content)?;
        let observed_at = now(&mut tx).await?;
        let until: Option<DateTime<Utc>> = row.try_get("valid_until")?;
        let mut current = row.try_get::<String, _>("execution_status")? == "SUCCEEDED"
            && row.try_get::<String, _>("evidence_status")? == "VALID"
            && row.try_get::<bool, _>("is_contiguous")?
            && row.try_get::<i64, _>("complete_observations")? > 0
            && until.is_some_and(|until| until > observed_at)
            && row.try_get::<DateTime<Utc>, _>("freshness_deadline")? > observed_at;
        let runtime = db::id(row.try_get("runtime_id")?)?;
        if current {
            match crate::forward::revalidate(
                &mut tx,
                locked.run.input_set_id,
                locked.run.project_id,
                runtime,
            )
            .await
            {
                Ok(()) => (),
                Err(StoreError::Invalid(_) | StoreError::Domain(_) | StoreError::NotFound) => {
                    current = false
                }
                Err(error) => return Err(error),
            }
        }
        let metrics = sqlx::query(
            "SELECT * FROM app.metric_values WHERE evaluation_id=$1 ORDER BY metric_code,scope",
        )
        .bind(evaluation.as_uuid())
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(crate::evidence::metric)
        .collect::<Result<Vec<_>, _>>()?;
        let classified = domain::forward::observation::classify(
            evaluation,
            current,
            &policy.content.degradation_metric_requirements,
            &policy.content.promotion_metric_requirements,
            &metrics,
        )?;
        let observation = Id::new();
        sqlx::query("INSERT INTO app.degradation_observations(id,project_id,release_id,evaluation_id,policy_id,classification,reason_codes,observed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(observation.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(row.try_get::<uuid::Uuid,_>("release_id")?).bind(evaluation.as_uuid()).bind(policy.id.as_uuid()).bind(classified.classification.code()).bind(&classified.reason_codes).bind(observed_at).execute(&mut *tx).await?;
        sqlx::query(
            "INSERT INTO app.forward_observation_publications(run_id,observation_id) VALUES($1,$2)",
        )
        .bind(run.as_uuid())
        .bind(observation.as_uuid())
        .execute(&mut *tx)
        .await?;
        if classified.classification == domain::forward::observation::Classification::Degraded {
            sqlx::query("INSERT INTO app.wake_events(project_id,observation_id,trigger,state,not_before,reason) VALUES($1,$2,'DEGRADATION','PENDING',$3,'NATIVE_FORWARD_DEGRADED')")
                .bind(locked.run.project_id.as_uuid()).bind(observation.as_uuid()).bind(observed_at).execute(&mut *tx).await?;
        }
        if current {
            crate::forward::revalidate(
                &mut tx,
                locked.run.input_set_id,
                locked.run.project_id,
                runtime,
            )
            .await
            .map_err(|error| match error {
                StoreError::Invalid(_) | StoreError::Domain(_) | StoreError::NotFound => {
                    StoreError::Conflict
                }
                other => other,
            })?;
            let final_at = now(&mut tx).await?;
            if until.is_none_or(|until| until <= final_at) {
                return Err(StoreError::Conflict);
            }
        }
        tx.commit().await?;
        Ok(Some(CommandResult {
            schema_version: SchemaV1,
            replayed: false,
            resource: observation,
        }))
    }
}
