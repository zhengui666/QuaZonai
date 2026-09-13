//! One immutable Candidate per original terminal Run; never an approval.
use super::*;

const WINDOWS_CURRENT: &str = "SELECT (SELECT count(*)=cardinality($1::uuid[]) AND coalesce(bool_and(valid_until>statement_timestamp()),false) FROM app.qualifications WHERE id=ANY($1)) AND NOT EXISTS(SELECT 1 FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id JOIN app.data_use_grants g ON g.id=d.data_use_grant_id WHERE i.input_set_id=ANY($2::uuid[]) AND g.valid_until<=statement_timestamp()) AND $3::timestamptz>statement_timestamp() AND $4::numeric>extract(epoch FROM statement_timestamp())*1000000000";
use contracts::{
    evidence::EvidenceStatus,
    runtime_jobs::{JobSpecV1, ResultManifestV1},
};

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
    let binding = sqlx::query("SELECT b.request,t.parameters_artifact_id,t.image_ref,t.origin FROM app.portfolio_build_tasks b JOIN app.run_native_tasks t ON t.run_id=b.run_id WHERE b.run_id=$1")
        .bind(run.id.as_uuid()).fetch_one(&mut *tx).await?;
    let prior: Option<uuid::Uuid> = sqlx::query_scalar("SELECT c.id FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id WHERE c.run_id=$1")
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
    let request: PortfolioBuildRequestV1 =
        serde_json::from_value(binding.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    domain::portfolio::build_selection(&request).map_err(|_| StoreError::Integrity)?;
    let parameter = db::id(binding.try_get("parameters_artifact_id")?)?;
    let bytes = validation::read_document(
        &mut tx,
        parameter,
        None,
        "qz.native_task",
        8 * 1024 * 1024,
        &mut read,
    )
    .await?;
    let task: NativeTaskParametersV1 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    let NativeTaskParametersV1::BuildPortfolio {
        request: frozen, ..
    } = &task
    else {
        return Err(StoreError::Integrity);
    };
    domain::execution::portfolio_build_request(frozen).map_err(|_| StoreError::Integrity)?;
    let mandate = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1")
        .bind(request.mandate_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
    let mandate = crate::portfolio::view(&mandate)?;
    if mandate.project_id != run.project_id || mandate.content != frozen.mandate {
        return Err(StoreError::Integrity);
    }
    if request.input_set_id != run.input_set_id
        || Some(request.cycle_id) != run.cycle_id
        || request.members.len() != frozen.members.len()
    {
        return Err(StoreError::Integrity);
    }
    let terminal = sqlx::query("SELECT a.result_manifest_artifact_id FROM app.run_terminal_receipts t LEFT JOIN app.run_attempts a ON a.id=t.attempt_id WHERE t.run_id=$1 AND t.terminal_state=$2 AND t.attempt_id IS NOT DISTINCT FROM $3")
        .bind(run.id.as_uuid()).bind(db::code(&run.state)?).bind(run.active_attempt_id.map(Id::as_uuid)).fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
    let manifest_id = db::optional_id(&terminal, "result_manifest_artifact_id")?;
    let mut native_report = None;
    let mut result: Option<NativePortfolioBuildResultV1> = None;
    if run.state == RunState::Succeeded {
        let attempt = run.active_attempt_id.ok_or(StoreError::Integrity)?;
        let original = sqlx::query("SELECT n.spec_json,a.created_at FROM app.run_native_attempts n JOIN app.run_attempts a ON a.id=n.attempt_id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL WHERE n.run_id=$1 AND n.attempt_id=$2")
            .bind(run.id.as_uuid()).bind(attempt.as_uuid()).fetch_one(&mut *tx).await?;
        let spec: JobSpecV1 = serde_json::from_value(original.try_get("spec_json")?)
            .map_err(|_| StoreError::Integrity)?;
        if spec.parameters_artifact_id != parameter {
            return Err(StoreError::Integrity);
        }
        domain::execution::task(&spec, &task).map_err(|_| StoreError::Integrity)?;
        let raw = validation::read_document(
            &mut tx,
            manifest_id.ok_or(StoreError::Integrity)?,
            Some((run.id, attempt)),
            "qz.job_result",
            domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES,
            &mut read,
        )
        .await?;
        let manifest: ResultManifestV1 =
            serde_json::from_slice(&raw).map_err(|_| StoreError::Integrity)?;
        let checked_at = now(&mut tx).await?;
        domain::runtime_jobs::manifest(
            &manifest,
            &spec,
            original.try_get("created_at")?,
            checked_at,
        )
        .map_err(|_| StoreError::Integrity)?;
        let outputs: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT a.id FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.producer_run_id=$2 AND a.producer_attempt_id=$1 AND a.kind='REPORT' AND a.schema_name='qz.native_portfolio' AND a.schema_version='1' AND a.access_class='EVALUATOR_ONLY'")
            .bind(attempt.as_uuid()).bind(run.id.as_uuid()).fetch_all(&mut *tx).await?;
        let [id] = outputs.as_slice() else {
            return Err(StoreError::Integrity);
        };
        let id = db::id(*id)?;
        let raw = validation::read_document(
            &mut tx,
            id,
            Some((run.id, attempt)),
            "qz.native_portfolio",
            contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
            &mut read,
        )
        .await?;
        let report: NativePortfolioBuildResultV1 =
            serde_json::from_slice(&raw).map_err(|_| StoreError::Integrity)?;
        domain::execution::portfolio_build_result(frozen, &report)
            .map_err(|_| StoreError::Integrity)?;
        native_report = Some(id);
        result = Some(report);
    }
    let (asof, until) = target_window(
        frozen.selection.decision_cutoff_ns.get(),
        frozen.mandate.rebalance_schedule.target_ttl_seconds,
    )?;
    let mut status = EvidenceStatus::Incomplete;
    let mut reason = Some(
        if run.state == RunState::Cancelled {
            "PORTFOLIO_CANCELLED"
        } else {
            "PORTFOLIO_EXECUTION_FAILED"
        }
        .to_owned(),
    );
    let mut targets = None;
    let mut cash = None;
    let solver = result
        .as_ref()
        .map_or(SolverStatus::Failed, |r| r.allocation.solver_status);
    if let Some(report) = &result {
        reason = report.allocation.reason_code.clone();
        status = EvidenceStatus::Valid;
        match eligibility(
            &mut tx,
            run.project_id,
            &request,
            frozen,
            binding.try_get("image_ref")?,
            until,
            &mut read,
        )
        .await
        {
            Ok(()) => {
                targets = report.allocation.targets.clone();
                cash = report.allocation.cash_weight.clone();
            }
            Err(StoreError::Invalid(_) | StoreError::Domain(_)) => {
                status = EvidenceStatus::Invalid;
                reason = Some("PORTFOLIO_SOURCE_NO_LONGER_ELIGIBLE".into());
            }
            Err(error) => return Err(error),
        }
    }
    let candidate = Id::new();
    let diagnostics = Id::new();
    let target_artifact = if let Some(targets) = &targets {
        let id = Id::new();
        document(&mut tx,run,id,"qz.portfolio_targets",binding.try_get("origin")?,
            json!({"schema_version":1,"candidate_id":candidate,"base_currency":frozen.mandate.base_currency,"asof":asof,"valid_until":until,"cash_weight":cash,"targets":targets}),&mut publish).await?;
        Some(id)
    } else {
        None
    };
    document(&mut tx,run,diagnostics,"qz.portfolio_candidate",binding.try_get("origin")?,
        json!({"schema_version":1,"candidate_id":candidate,"run_id":run.id,"execution_status":run.state,"solver_status":solver,"evidence_status":status,"reason_code":reason,"native_report_artifact_id":native_report,"native_manifest_artifact_id":manifest_id,"target_artifact_id":target_artifact}),&mut publish).await?;
    if targets.is_some() {
        eligibility(
            &mut tx,
            run.project_id,
            &request,
            frozen,
            binding.try_get("image_ref")?,
            until,
            &mut read,
        )
        .await
        .map_err(|e| match e {
            StoreError::Invalid(_) | StoreError::Domain(_) => StoreError::Conflict,
            other => other,
        })?;
    }
    sqlx::query("INSERT INTO app.portfolio_candidates(id,project_id,mandate_id,input_set_id,decision_asof,run_id,solver_status,evidence_status,reason_code,forecast_artifact_id,diagnostics_artifact_id,target_artifact_id,cash_weight,current_weights_source,current_weights_artifact_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,'FORWARD_SNAPSHOT',$14)")
        .bind(candidate.as_uuid()).bind(run.project_id.as_uuid()).bind(request.mandate_id.as_uuid()).bind(run.input_set_id.as_uuid()).bind(asof).bind(run.id.as_uuid()).bind(db::code(&solver)?).bind(db::code(&status)?).bind(reason).bind(native_report.map(Id::as_uuid)).bind(diagnostics.as_uuid()).bind(target_artifact.map(Id::as_uuid)).bind(cash.as_ref().map(|v|v.as_decimal())).bind(frozen.current_weights_artifact_id.as_uuid()).execute(&mut *tx).await?;
    for (chosen, member) in request.members.iter().zip(&frozen.members) {
        let calibration: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT calibration_id FROM app.alpha_versions WHERE id=$1 AND project_id=$2",
        )
        .bind(member.alpha_version_id.as_uuid())
        .bind(run.project_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO app.candidate_alphas(candidate_id,alpha_version_id,qualification_id,ensemble_weight,calibration_id,forecast_unit,coverage_fraction) VALUES($1,$2,$3,$4,$5,'RETURN_PER_HORIZON',$6)")
            .bind(candidate.as_uuid()).bind(member.alpha_version_id.as_uuid()).bind(chosen.qualification_id.as_uuid()).bind(member.ensemble_weight.as_decimal()).bind(calibration)
            .bind(if result.is_some() {bigdecimal::BigDecimal::from(1)} else {bigdecimal::BigDecimal::from(0)}).execute(&mut *tx).await?;
    }
    for target in targets.into_iter().flatten() {
        sqlx::query("INSERT INTO app.candidate_targets(candidate_id,instrument_id,target_weight,currency,asof,valid_until) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(candidate.as_uuid()).bind(target.instrument_id).bind(target.weight.as_decimal()).bind(target.currency).bind(asof).bind(until).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Some(CommandResult {
        schema_version: SchemaV1,
        replayed: false,
        resource: candidate,
    }))
}

pub(super) fn target_window(
    decision_ns: u64,
    ttl: u32,
) -> Result<(DateTime<Utc>, DateTime<Utc>), StoreError> {
    let deadline = decision_ns
        .checked_add(u64::from(ttl) * 1_000_000_000)
        .ok_or(StoreError::Integrity)?;
    // Never round a target's validity outward at PostgreSQL's microsecond boundary.
    let asof = DateTime::<Utc>::from_timestamp_micros(
        i64::try_from(decision_ns.div_ceil(1000)).map_err(|_| StoreError::Integrity)?,
    )
    .ok_or(StoreError::Integrity)?;
    let until = DateTime::<Utc>::from_timestamp_micros(
        i64::try_from(deadline / 1000).map_err(|_| StoreError::Integrity)?,
    )
    .ok_or(StoreError::Integrity)?;
    if until <= asof {
        return Err(StoreError::Integrity);
    }
    Ok((asof, until))
}

async fn eligibility<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    request: &PortfolioBuildRequestV1,
    frozen: &NativePortfolioBuildRequestV1,
    image: &str,
    until: DateTime<Utc>,
    read: &mut R,
) -> Result<(), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let now = now(tx).await?;
    if until <= now
        || frozen.current_weights.valid_until_ns.get()
            <= u64::try_from(now.timestamp_nanos_opt().ok_or(StoreError::Integrity)?)
                .map_err(|_| StoreError::Integrity)?
    {
        return Err(StoreError::Invalid("portfolio_expired"));
    }
    let source: Option<uuid::Uuid> = sqlx::query_scalar("SELECT d.id FROM app.forward_weight_snapshots s JOIN app.downstream_integrations d ON d.id=s.downstream_id WHERE s.id=$1 AND s.project_id=$2 AND s.report_artifact_id=$3 AND s.environment=$4 AND d.enabled AND (d.environments='BOTH' OR d.environments=s.environment) FOR SHARE OF d")
        .bind(request.current_weights_snapshot_id.as_uuid()).bind(project.as_uuid()).bind(frozen.current_weights_artifact_id.as_uuid()).bind(db::code(&request.environment)?).fetch_optional(&mut **tx).await?;
    if source.is_none() {
        return Err(StoreError::Invalid("portfolio_weights_source"));
    }
    crate::research::revalidate_frozen_inputs(
        tx,
        request.input_set_id,
        project,
        request.runtime_id,
    )
    .await?;
    let costs: uuid::Uuid = sqlx::query_scalar("SELECT input_set_id FROM app.execution_assumption_sources WHERE assumptions_id=$1 AND project_id=$2 AND runtime_id=$3")
        .bind(frozen.mandate.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(request.runtime_id.as_uuid()).fetch_one(&mut **tx).await?;
    crate::research::revalidate_frozen_inputs(tx, db::id(costs)?, project, request.runtime_id)
        .await?;
    for (chosen, original) in request.members.iter().zip(&frozen.members) {
        let (current, _) = member_source(
            tx,
            project,
            frozen.mandate.required_evaluation_policy_id,
            request.runtime_id,
            image,
            chosen,
            read,
        )
        .await?;
        if db::json(&current)? != db::json(original)? {
            return Err(StoreError::Invalid("portfolio_original_member"));
        }
    }
    if !windows_current(
        tx,
        request,
        db::id(costs)?,
        frozen.current_weights.valid_until_ns,
        until,
    )
    .await?
    {
        return Err(StoreError::Invalid("portfolio_source_expired"));
    }
    Ok(())
}

/// No file callback follows this final snapshot of qualification and license clocks.
/// Original training/license windows are already bounded by qualification.valid_until.
pub(super) async fn windows_current(
    tx: &mut Tx<'_>,
    request: &PortfolioBuildRequestV1,
    costs: Id,
    weights_until: DbCounter,
    target_until: DateTime<Utc>,
) -> Result<bool, StoreError> {
    let qualifications: Vec<_> = request
        .members
        .iter()
        .map(|m| m.qualification_id.as_uuid())
        .collect();
    let inputs = [request.input_set_id.as_uuid(), costs.as_uuid()];
    Ok(sqlx::query_scalar(WINDOWS_CURRENT)
        .bind(qualifications)
        .bind(inputs.as_slice())
        .bind(target_until)
        .bind(weights_until.get() as i64)
        .fetch_one(&mut **tx)
        .await?)
}

async fn document<P, Published>(
    tx: &mut Tx<'_>,
    run: &RunSnapshotV1,
    id: Id,
    schema: &str,
    origin: &str,
    value: Value,
    publish: &mut P,
) -> Result<(), StoreError>
where
    P: FnMut(NativeObjectPublication) -> Published,
    Published: std::future::Future<Output = Result<(), StoreError>>,
{
    let bytes = serde_json::to_vec(&value).map_err(|_| StoreError::Integrity)?;
    let size = i64::try_from(bytes.len()).map_err(|_| StoreError::Integrity)?;
    publish(NativeObjectPublication { id, bytes }).await?;
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'REPORT','application/json',$5,'1','LOCAL',$6,'1',$7,'EVALUATOR_ONLY',$8,'RUNTIME','REFERENCED')")
        .bind(id.as_uuid()).bind(run.project_id.as_uuid()).bind(run.id.as_uuid()).bind(run.active_attempt_id.map(Id::as_uuid)).bind(schema).bind(id.to_string()).bind(size).bind(origin).execute(&mut **tx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[sqlx::test(migrations = "../../migrations")]
    async fn postgres_checks_the_final_window_without_inventing_qualification(pool: sqlx::PgPool) {
        let future = Utc::now() + Duration::hours(1);
        let qualifies: bool = sqlx::query_scalar(WINDOWS_CURRENT)
            .bind(Vec::<uuid::Uuid>::new())
            .bind(Vec::<uuid::Uuid>::new())
            .bind(future)
            .bind(future.timestamp_nanos_opt().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!qualifies);
    }
    #[test]
    fn target_window_never_extends_native_validity() {
        for (ns, start) in [(1000, 1000), (1001, 2000), (1999, 2000)] {
            let (asof, until) = target_window(ns, 1).unwrap();
            assert_eq!(asof.timestamp_nanos_opt().unwrap(), start);
            assert!(until.timestamp_nanos_opt().unwrap() as u64 <= ns + 1_000_000_000);
            assert!(until > asof);
        }
        assert!(target_window(1001, 0).is_err());
        assert!(target_window(u64::MAX, 1).is_err());
    }
}
