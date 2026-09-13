//! Original Candidate HOLD evidence, not a Release or downstream observation.
use super::*;
use contracts::{
    evidence::{Decision, EvidenceStatus},
    execution::NativeDataQualityReportV1,
    runtime_jobs::{JobSpecV1, ResultManifestV1},
};
use domain::evidence::MetricGate;

pub(in crate::lifecycle) async fn publish<R, Read, P, Published>(
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
    let Some(binding) = sqlx::query("SELECT s.*,t.origin,t.parameters_artifact_id,t.image_ref,d.row_count,d.pit_status,d.revision_policy,d.origin AS dataset_origin,c.project_id,m.required_evaluation_policy_id FROM app.candidate_simulation_tasks s JOIN app.run_native_tasks t ON t.run_id=s.run_id JOIN app.dataset_revisions d ON d.id=s.dataset_revision_id JOIN app.portfolio_candidates c ON c.id=s.candidate_id JOIN app.portfolio_mandates m ON m.id=c.mandate_id WHERE s.run_id=$1")
        .bind(run.id.as_uuid()).fetch_optional(&mut *tx).await? else {
        tx.commit().await?;
        return Ok(None);
    };
    let candidate = db::id(binding.try_get("candidate_id")?)?;
    let policy_id = db::id(binding.try_get("policy_id")?)?;
    let request: CandidateSimulationRequestV1 =
        serde_json::from_value(binding.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    if request.candidate_id != candidate
        || request.input_set_id != run.input_set_id
        || Some(request.cycle_id) != run.cycle_id
        || binding.try_get::<uuid::Uuid, _>("project_id")? != run.project_id.as_uuid()
        || binding.try_get::<uuid::Uuid, _>("required_evaluation_policy_id")? != policy_id.as_uuid()
    {
        return Err(StoreError::Integrity);
    }
    let prior: Option<uuid::Uuid> = sqlx::query_scalar("SELECT e.id FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE e.run_id=$1 AND e.subject_candidate_id=$2 AND e.policy_id=$3 AND e.evaluation_kind='FORWARD'")
        .bind(run.id.as_uuid()).bind(candidate.as_uuid()).bind(policy_id.as_uuid()).fetch_optional(&mut *tx).await?;
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
    let manifest_id = db::optional_id(&terminal, "result_manifest_artifact_id")?;
    let policy = crate::research::frozen_policy(&mut tx, policy_id).await?;
    if policy.project_id != run.project_id {
        return Err(StoreError::Integrity);
    }
    let evaluation = Id::new();
    let report_id = Id::new();
    let concluded_at = now(&mut tx).await?;
    let mut completed_at = run.finished_at.ok_or(StoreError::Integrity)?;
    let mut gate = MetricGate {
        evidence_status: EvidenceStatus::Incomplete,
        decision: Decision::Inconclusive,
        reasons: vec![if run.state == RunState::Cancelled {
            "CANDIDATE_SIMULATION_CANCELLED"
        } else {
            "CANDIDATE_SIMULATION_FAILED"
        }
        .into()],
    };
    let mut metrics = Vec::new();
    let mut reports = Vec::new();
    let mut task = None;
    let mut target_until = None;
    let mut source_rows = None;
    let mut versions = None;
    if run.state == RunState::Succeeded {
        let attempt = run.active_attempt_id.ok_or(StoreError::Integrity)?;
        let original = sqlx::query("SELECT s.spec_json,a.created_at FROM app.run_native_attempts s JOIN app.run_attempts a ON a.id=s.attempt_id WHERE s.run_id=$1 AND s.attempt_id=$2 AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL")
            .bind(run.id.as_uuid()).bind(attempt.as_uuid()).fetch_one(&mut *tx).await?;
        let spec: JobSpecV1 = serde_json::from_value(original.try_get("spec_json")?)
            .map_err(|_| StoreError::Integrity)?;
        if spec.parameters_artifact_id.as_uuid()
            != binding.try_get::<uuid::Uuid, _>("parameters_artifact_id")?
            || spec.image_ref != binding.try_get::<String, _>("image_ref")?
        {
            return Err(StoreError::Integrity);
        }
        let parameters = validation::read_document(
            &mut tx,
            spec.parameters_artifact_id,
            None,
            "qz.native_task",
            8 * 1024 * 1024,
            &mut read,
        )
        .await?;
        let parameters: NativeTaskParametersV1 =
            serde_json::from_slice(&parameters).map_err(|_| StoreError::Integrity)?;
        domain::execution::task(&spec, &parameters).map_err(|_| StoreError::Integrity)?;
        let NativeTaskParametersV1::SimulateCandidate {
            candidate_id,
            dataset_revision_id,
            request: native,
            ..
        } = &parameters
        else {
            return Err(StoreError::Integrity);
        };
        if *candidate_id != candidate
            || dataset_revision_id.as_uuid()
                != binding.try_get::<uuid::Uuid, _>("dataset_revision_id")?
        {
            return Err(StoreError::Integrity);
        }
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
        if manifest
            .engine_versions
            .get("candidate-simulation")
            .map(String::as_str)
            != Some("2")
        {
            return Err(StoreError::Integrity);
        }
        completed_at = manifest.finished_at;
        versions = Some(manifest.engine_versions.clone());
        let mut outputs = Vec::new();
        for output in &manifest.artifacts {
            let ids:Vec<uuid::Uuid>=sqlx::query_scalar("SELECT a.id FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.producer_run_id=$2 AND a.producer_attempt_id=$1 AND a.schema_name=$3 AND a.schema_version='1' AND a.access_class='EVALUATOR_ONLY'")
                .bind(attempt.as_uuid()).bind(run.id.as_uuid()).bind(&output.schema.name).fetch_all(&mut *tx).await?;
            let [id] = ids.as_slice() else {
                return Err(StoreError::Integrity);
            };
            let id = db::id(*id)?;
            let bytes = validation::read_document(
                &mut tx,
                id,
                Some((run.id, attempt)),
                &output.schema.name,
                contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
                &mut read,
            )
            .await?;
            reports.push((output.schema.name.clone(), id));
            outputs.push((output.clone(), bytes));
        }
        domain::execution::output_bindings(
            &parameters,
            None,
            manifest.started_at.ok_or(StoreError::Integrity)?,
            manifest.finished_at,
            &outputs,
        )
        .map_err(|_| StoreError::Integrity)?;
        let simulation = outputs
            .iter()
            .find(|(o, _)| o.schema.name == "qz.native_simulation")
            .ok_or(StoreError::Integrity)?;
        let quality = outputs
            .iter()
            .find(|(o, _)| o.schema.name == "qz.data_quality")
            .ok_or(StoreError::Integrity)?;
        let result: NativeSimulationResultV1 =
            serde_json::from_slice(&simulation.1).map_err(|_| StoreError::Integrity)?;
        let quality: NativeDataQualityReportV1 =
            serde_json::from_slice(&quality.1).map_err(|_| StoreError::Integrity)?;
        let source = reports
            .iter()
            .find(|(name, _)| name == "qz.native_simulation")
            .ok_or(StoreError::Integrity)?
            .1;
        let (values, capabilities) =
            domain::execution::portfolio_simulation_metrics(evaluation, source, native, &result)
                .map_err(|_| StoreError::Integrity)?;
        gate = if let Some(requirements) = &policy.portfolio_metric_requirements {
            domain::evidence::evaluate_metrics(evaluation, requirements, &values, &capabilities)?
        } else {
            MetricGate {
                evidence_status: EvidenceStatus::Incomplete,
                decision: Decision::Inconclusive,
                reasons: vec!["PORTFOLIO_CRITERIA_UNDEFINED".into()],
            }
        };
        metrics = values;
        let actual = quality.datasets[0].row_count.get();
        let registered = counter(binding.try_get("row_count")?)?.get();
        source_rows = Some(quality.datasets[0].row_count);
        let mut incomplete = Vec::new();
        if bigdecimal::BigDecimal::from(registered.saturating_sub(actual))
            > policy.maximum_missing_fraction.as_decimal()
                * bigdecimal::BigDecimal::from(registered)
        {
            incomplete.push("REGISTERED_DATA_MISSING".into());
        }
        if result.returns.len() < policy.minimum_observations as usize {
            incomplete.push("INSUFFICIENT_DAILY_OBSERVATIONS".into());
        }
        if policy.require_real_data
            && (binding.try_get::<String, _>("origin")? != "REAL"
                || binding.try_get::<String, _>("dataset_origin")? != "REAL"
                || binding.try_get::<String, _>("pit_status")? != "VERIFIED"
                || binding.try_get::<String, _>("revision_policy")? != "AS_KNOWN_THEN")
        {
            incomplete.push("REAL_POINT_IN_TIME_DATA_REQUIRED".into());
        }
        match sources(
            &mut tx,
            run,
            &request,
            &parameters,
            binding.try_get("image_ref")?,
            &mut read,
        )
        .await
        {
            Ok(until) => target_until = Some(until),
            Err(StoreError::Invalid(_) | StoreError::Domain(_)) => {
                incomplete.push("CANDIDATE_SOURCE_NO_LONGER_ELIGIBLE".into())
            }
            Err(error) => return Err(error),
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
        task = Some(parameters);
    }
    let valid_until = target_until
        .and_then(|until| {
            i64::try_from(policy.validity_seconds.get())
                .ok()
                .and_then(Duration::try_seconds)
                .and_then(|ttl| completed_at.checked_add_signed(ttl))
                .map(|ttl| ttl.min(until))
        })
        .filter(|until| *until > concluded_at);
    if run.state == RunState::Succeeded && valid_until.is_none() {
        if gate.evidence_status == EvidenceStatus::Valid {
            gate.evidence_status = EvidenceStatus::Incomplete;
        }
        gate.decision = Decision::Inconclusive;
        gate.reasons
            .push("CANDIDATE_EVIDENCE_EXPIRED_OR_INELIGIBLE".into());
    }
    publication::document(&mut tx,run,report_id,"qz.candidate_evaluation",binding.try_get("origin")?,json!({"schema_version":1,"evaluation_id":evaluation,"candidate_id":candidate,"policy_id":policy_id,"run_id":run.id,"input_set_id":run.input_set_id,"evaluation_kind":"FORWARD","mode":"HOLD","execution_status":run.state,"evidence_status":gate.evidence_status,"decision":gate.decision,"reasons":gate.reasons,"native_manifest_artifact_id":manifest_id,"native_reports":reports,"native_versions":versions,"source_rows":source_rows,"concluded_at":concluded_at,"valid_until":valid_until}),&mut publish).await?;
    if let Some(until) = valid_until {
        let original = task.as_ref().ok_or(StoreError::Integrity)?;
        sources(
            &mut tx,
            run,
            &request,
            original,
            binding.try_get("image_ref")?,
            &mut read,
        )
        .await
        .map_err(|error| match error {
            StoreError::Invalid(_) | StoreError::Domain(_) => StoreError::Conflict,
            other => other,
        })?;
        if until <= now(&mut tx).await? {
            return Err(StoreError::Conflict);
        }
    }
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) VALUES($1,$2,$3,$4,$5,$6,'FORWARD',$7,$8,$9,$10,$10,$11,$12)")
        .bind(evaluation.as_uuid()).bind(run.project_id.as_uuid()).bind(candidate.as_uuid()).bind(run.input_set_id.as_uuid()).bind(policy_id.as_uuid()).bind(run.id.as_uuid()).bind(db::code(&run.state)?).bind(db::code(&gate.evidence_status)?).bind(db::code(&gate.decision)?).bind(report_id.as_uuid()).bind(concluded_at).bind(valid_until).execute(&mut *tx).await?;
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

async fn sources<R, Read>(
    tx: &mut Tx<'_>,
    run: &RunSnapshotV1,
    intent: &CandidateSimulationRequestV1,
    task: &NativeTaskParametersV1,
    image: &str,
    read: &mut R,
) -> Result<DateTime<Utc>, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let NativeTaskParametersV1::SimulateCandidate {
        candidate_id,
        candidate_available_ns,
        dataset_revision_id,
        source_selection,
        target_artifact_id,
        settings_artifact_id,
        request,
        ..
    } = task
    else {
        return Err(StoreError::Integrity);
    };
    let target = weights::target(tx, run.project_id, *candidate_id, read).await?;
    if target.available_ns != *candidate_available_ns
        || !matches!(target.input,RuntimeInputV1::Artifact{artifact_id,..} if artifact_id==*target_artifact_id)
    {
        return Err(StoreError::Integrity);
    }
    domain::execution::candidate_simulation(
        *candidate_id,
        *candidate_available_ns,
        &target.document,
        request,
    )
    .map_err(|_| StoreError::Integrity)?;
    let row=sqlx::query("SELECT b.request,t.parameters_artifact_id,s.settings,s.input_set_id AS assumptions_input FROM app.portfolio_candidates c JOIN app.portfolio_build_tasks b ON b.run_id=c.run_id JOIN app.run_native_tasks t ON t.run_id=b.run_id JOIN app.portfolio_mandates m ON m.id=c.mandate_id JOIN app.execution_assumption_sources s ON s.assumptions_id=m.execution_assumptions_id WHERE c.id=$1")
        .bind(candidate_id.as_uuid()).fetch_one(&mut **tx).await?;
    let original: PortfolioBuildRequestV1 =
        serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    if original.runtime_id != intent.runtime_id {
        return Err(StoreError::Integrity);
    }
    let bytes = validation::read_document(
        tx,
        db::id(row.try_get("parameters_artifact_id")?)?,
        None,
        "qz.native_task",
        8 * 1024 * 1024,
        read,
    )
    .await?;
    let NativeTaskParametersV1::BuildPortfolio {
        request: frozen, ..
    } = serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?
    else {
        return Err(StoreError::Integrity);
    };
    if frozen.mandate.constraints.transaction_costs_ref != *settings_artifact_id
        || db::json(&frozen.execution_settings)? != db::json(&request.settings)?
    {
        return Err(StoreError::Integrity);
    }
    let settings = crate::execution_assumptions::liquidity::document(
        tx,
        run.project_id,
        *settings_artifact_id,
        "qz.native_simulation_settings",
        1024 * 1024,
        read,
    )
    .await?;
    let settings: NativeSimulationSettingsV1 =
        serde_json::from_slice(&settings).map_err(|_| StoreError::Integrity)?;
    if db::json(&settings)? != db::json(&request.settings)?
        || row.try_get::<Value, _>("settings")? != db::json(&settings)?
    {
        return Err(StoreError::Integrity);
    }
    let datasets = crate::data_validation::dataset_bindings(
        tx,
        intent.input_set_id,
        run.project_id,
        intent.runtime_id,
        &[contracts::research::InputPurpose::Forward],
        read,
    )
    .await?;
    let [dataset] = datasets.as_slice() else {
        return Err(StoreError::Integrity);
    };
    if dataset.selection.dataset_revision_id != *dataset_revision_id
        || db::json(&dataset.metadata.quality.datasets[0].selection)? != db::json(source_selection)?
    {
        return Err(StoreError::Integrity);
    }
    domain::catalogs::execution_fees(&dataset.metadata, &settings)?;
    for member in &original.members {
        member_source(
            tx,
            run.project_id,
            frozen.mandate.required_evaluation_policy_id,
            intent.runtime_id,
            image,
            member,
            read,
        )
        .await?;
    }
    let assumptions = db::id(row.try_get("assumptions_input")?)?;
    for inputs in [original.input_set_id, assumptions] {
        crate::research::revalidate_frozen_inputs(tx, inputs, run.project_id, intent.runtime_id)
            .await?;
    }
    let until = target.document.valid_until;
    let nanos = counter(until.timestamp_nanos_opt().ok_or(StoreError::Integrity)?)?;
    if !publication::windows_current(
        tx,
        &original,
        &[assumptions, intent.input_set_id],
        nanos,
        until,
    )
    .await?
    {
        return Err(StoreError::Invalid("candidate_evidence_source_expired"));
    }
    Ok(until)
}
