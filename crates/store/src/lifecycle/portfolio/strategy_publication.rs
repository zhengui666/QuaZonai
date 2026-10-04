//! Persist accepted computation facts. Historical clocks stay historical; expiry
//! never turns an accepted native result into a fake forecast or qualification.
use super::*;
use contracts::{
    runtime_jobs::{JobSpecV1, ResultManifestV1},
    strategy_portfolio::*,
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
    if !run.state.is_terminal() {
        return Err(StoreError::Conflict);
    }
    let prior:Option<uuid::Uuid>=sqlx::query_scalar("SELECT c.id FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id WHERE c.run_id=$1 AND c.source_kind='STRATEGY_ALPHA'").bind(run.id.as_uuid()).fetch_optional(&mut *tx).await?;
    if let Some(id) = prior {
        tx.commit().await?;
        return Ok(Some(CommandResult {
            schema_version: SchemaV1,
            replayed: true,
            resource: db::id(id)?,
        }));
    }
    if run.state != RunState::Succeeded {
        // The existing terminal receipt is the failure/cancellation outcome. No
        // target-bearing Candidate can be manufactured from a failed execution.
        tx.commit().await?;
        return Ok(None);
    }
    let attempt = run.active_attempt_id.ok_or(StoreError::Integrity)?;
    let binding=sqlx::query("SELECT b.request,b.mandate_id,b.purpose,t.parameters_artifact_id,t.origin,t.image_ref FROM app.portfolio_build_tasks b JOIN app.run_native_tasks t ON t.run_id=b.run_id WHERE b.run_id=$1 AND b.source_kind='STRATEGY_ALPHA'").bind(run.id.as_uuid()).fetch_one(&mut *tx).await?;
    let build: StrategyPortfolioBuildV1 =
        serde_json::from_value(binding.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    domain::execution::strategy::build(&build).map_err(|_| StoreError::Integrity)?;
    if build.input_set_id != run.input_set_id
        || Some(build.cycle_id) != run.cycle_id
        || build.mandate_id.as_uuid() != binding.try_get::<uuid::Uuid, _>("mandate_id")?
        || strategy::purpose(&build.purpose) != binding.try_get::<String, _>("purpose")?
    {
        return Err(StoreError::Integrity);
    }
    let parameter = db::id(binding.try_get("parameters_artifact_id")?)?;
    let raw = validation::read_document(
        &mut tx,
        parameter,
        None,
        "qz.native_task",
        8 * 1024 * 1024,
        &mut read,
    )
    .await?;
    let task: NativeTaskParametersV1 =
        serde_json::from_slice(&raw).map_err(|_| StoreError::Integrity)?;
    let NativeTaskParametersV1::ComposeStrategyTargets {
        request: frozen, ..
    } = &task
    else {
        return Err(StoreError::Integrity);
    };
    if db::json(&frozen.purpose)? != db::json(&build.purpose)?
        || frozen.members.len() != build.members.len()
        || frozen
            .members
            .iter()
            .zip(&build.members)
            .any(|(actual, chosen)| {
                actual.alpha_version_id != chosen.alpha_version_id
                    || actual.ensemble_weight != chosen.ensemble_weight
            })
        || frozen.members.iter().any(|member| {
            member.policy.runtime_image_ref
                != binding
                    .try_get::<String, _>("image_ref")
                    .unwrap_or_default()
        })
    {
        return Err(StoreError::Integrity);
    }
    let mandate=sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2 AND allocation_method='FIXED_TARGET_WEIGHTS'").bind(build.mandate_id.as_uuid()).bind(run.project_id.as_uuid()).fetch_one(&mut *tx).await?;
    if db::json(&crate::portfolio::strategy::mandate(&mandate)?.content)?
        != db::json(&frozen.mandate)?
    {
        return Err(StoreError::Integrity);
    }
    let original=sqlx::query("SELECT n.spec_json,a.created_at,a.result_manifest_artifact_id FROM app.run_native_attempts n JOIN app.run_attempts a ON a.id=n.attempt_id AND a.run_id=n.run_id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL JOIN app.run_terminal_receipts terminal ON terminal.run_id=n.run_id AND terminal.attempt_id=a.id AND terminal.terminal_state='SUCCEEDED' WHERE n.run_id=$1 AND n.attempt_id=$2").bind(run.id.as_uuid()).bind(attempt.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
    let spec: JobSpecV1 = serde_json::from_value(original.try_get("spec_json")?)
        .map_err(|_| StoreError::Integrity)?;
    if spec.parameters_artifact_id != parameter {
        return Err(StoreError::Integrity);
    }
    domain::execution::task(&spec, &task).map_err(|_| StoreError::Integrity)?;
    let manifest_id = db::id(original.try_get("result_manifest_artifact_id")?)?;
    let manifest: ResultManifestV1 = serde_json::from_slice(
        &validation::read_document(
            &mut tx,
            manifest_id,
            Some((run.id, attempt)),
            "qz.job_result",
            domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES,
            &mut read,
        )
        .await?,
    )
    .map_err(|_| StoreError::Integrity)?;
    domain::runtime_jobs::manifest(
        &manifest,
        &spec,
        original.try_get("created_at")?,
        now(&mut tx).await?,
    )
    .map_err(|_| StoreError::Integrity)?;
    if manifest
        .engine_versions
        .get("strategy-composition")
        .map(String::as_str)
        != Some("1")
    {
        return Err(StoreError::Integrity);
    }
    let outputs=sqlx::query("SELECT a.id,o.remote_storage_ref,a.byte_count FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id AND a.project_id=$3 AND a.producer_run_id=$2 AND a.producer_attempt_id=$1 AND a.kind='REPORT' AND a.schema_name='qz.strategy_portfolio' AND a.schema_version='1' AND a.access_class='EVALUATOR_ONLY' WHERE o.attempt_id=$1").bind(attempt.as_uuid()).bind(run.id.as_uuid()).bind(run.project_id.as_uuid()).fetch_all(&mut *tx).await?;
    let [output] = outputs.as_slice() else {
        return Err(StoreError::Integrity);
    };
    let report_id = db::id(output.try_get("id")?)?;
    let remote = db::id(output.try_get("remote_storage_ref")?)?;
    let [expected] = manifest.artifacts.as_slice() else {
        return Err(StoreError::Integrity);
    };
    if expected.storage_ref != remote
        || expected.byte_count != counter(output.try_get("byte_count")?)?
    {
        return Err(StoreError::Integrity);
    }
    let raw = validation::read_document(
        &mut tx,
        report_id,
        Some((run.id, attempt)),
        "qz.strategy_portfolio",
        contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
        &mut read,
    )
    .await?;
    let report: NativeStrategyCompositionResultV1 =
        serde_json::from_slice(&raw).map_err(|_| StoreError::Integrity)?;
    domain::execution::output_bindings(
        &task,
        None,
        manifest.started_at.ok_or(StoreError::Integrity)?,
        manifest.finished_at,
        &[(expected.clone(), raw)],
    )
    .map_err(|_| StoreError::Integrity)?;
    domain::execution::strategy_composition_result(frozen, &report)
        .map_err(|_| StoreError::Integrity)?;
    source_bindings(&mut tx, run.project_id, &report, &mut read).await?;
    let target = match &report.outcome {
        StrategyCompositionOutcomeV1::HistoricalReplay {
            simulation_request, ..
        } => simulation_request
            .target_points
            .last()
            .ok_or(StoreError::Integrity)?,
        StrategyCompositionOutcomeV1::CurrentDecision { target, .. } => target,
    };
    let (asof, until) = window(target)?;
    let exact_asof = DateTime::from_timestamp_nanos(
        i64::try_from(target.asof_ns.get()).map_err(|_| StoreError::Integrity)?,
    );
    let decision_row = DateTime::from_timestamp_micros(exact_asof.timestamp_micros())
        .ok_or(StoreError::Integrity)?;
    let id = Id::new();
    let target_id = Id::new();
    let created = now(&mut tx).await?;
    let detail = StrategyPortfolioCandidateV1 {
        schema_version: SchemaV1,
        source_kind: StrategyPortfolioSourceV1::StrategyAlpha,
        id,
        project_id: run.project_id,
        mandate_id: build.mandate_id,
        input_set_id: run.input_set_id,
        run_id: run.id,
        accepted_attempt_id: attempt,
        report_artifact_id: report_id,
        allocation_method: StrategyAllocationMethodV1::FixedTargetWeights,
        purpose: build.purpose.clone(),
        input_provenance: frozen.input_provenance.clone(),
        members: build.members.clone(),
        targets: target.targets.clone(),
        cash_weight: target.cash_weight.clone(),
        decision_asof: exact_asof,
        created_at: created,
    };
    document(&mut tx,run,target_id,"qz.portfolio_targets",binding.try_get("origin")?,json!({"schema_version":1,"candidate_id":id,"base_currency":frozen.mandate.base_currency,"asof":asof,"valid_until":until,"cash_weight":target.cash_weight,"targets":target.targets}),&mut publish).await?;
    sqlx::query("INSERT INTO app.portfolio_candidates(id,created_at,project_id,mandate_id,input_set_id,decision_asof,run_id,solver_status,evidence_status,diagnostics_artifact_id,target_artifact_id,cash_weight,current_weights_source,source_kind,purpose,strategy_detail) VALUES($1,$2,$3,$4,$5,$6,$7,NULL,'VALID',$8,$9,$10,'NONE','STRATEGY_ALPHA',$11,$12)").bind(id.as_uuid()).bind(created).bind(run.project_id.as_uuid()).bind(build.mandate_id.as_uuid()).bind(run.input_set_id.as_uuid()).bind(decision_row).bind(run.id.as_uuid()).bind(report_id.as_uuid()).bind(target_id.as_uuid()).bind(target.cash_weight.as_decimal()).bind(strategy::purpose(&build.purpose)).bind(db::json(&detail)?).execute(&mut *tx).await?;
    for member in &build.members {
        sqlx::query("INSERT INTO app.candidate_alphas(candidate_id,alpha_version_id,ensemble_weight,coverage_fraction,source_kind) VALUES($1,$2,$3,1,'STRATEGY_ALPHA')").bind(id.as_uuid()).bind(member.alpha_version_id.as_uuid()).bind(member.ensemble_weight.as_decimal()).execute(&mut *tx).await?;
    }
    for item in &target.targets {
        sqlx::query("INSERT INTO app.candidate_targets(candidate_id,instrument_id,target_weight,currency,asof,valid_until) VALUES($1,$2,$3,$4,$5,$6)").bind(id.as_uuid()).bind(&item.instrument_id).bind(item.weight.as_decimal()).bind(&item.currency).bind(asof).bind(until).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Some(CommandResult {
        schema_version: SchemaV1,
        replayed: false,
        resource: id,
    }))
}

fn window(point: &NativeTargetPointV1) -> Result<(DateTime<Utc>, DateTime<Utc>), StoreError> {
    let start =
        i64::try_from(point.asof_ns.get().div_ceil(1000)).map_err(|_| StoreError::Integrity)?;
    let end =
        i64::try_from(point.valid_until_ns.get() / 1000).map_err(|_| StoreError::Integrity)?;
    if start >= end {
        return Err(StoreError::Integrity);
    }
    Ok((
        DateTime::from_timestamp_micros(start).ok_or(StoreError::Integrity)?,
        DateTime::from_timestamp_micros(end).ok_or(StoreError::Integrity)?,
    ))
}

/// Both publication and subsequent delivery bind current continuation counts and
/// clocks to the accepted original fold, without requiring a second model run.
pub(super) async fn source_bindings<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    report: &NativeStrategyCompositionResultV1,
    read: &mut R,
) -> Result<(), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let mut sources = Vec::with_capacity(report.request.members.len());
    for member in &report.request.members {
        sources.push(
            super::super::external_experiment::read_strategy_source(
                tx,
                project,
                &member.policy,
                read,
            )
            .await?,
        );
    }
    let folds = report
        .request
        .members
        .iter()
        .zip(&sources)
        .map(|(member, source)| domain::execution::strategy::source(&member.policy, source))
        .collect::<Result<Vec<_>, _>>()?;
    match &report.outcome {
        StrategyCompositionOutcomeV1::HistoricalReplay {
            simulation_request, ..
        } => {
            if folds
                .iter()
                .any(|fold| fold.decisions.len() != simulation_request.target_points.len())
            {
                return Err(StoreError::Integrity);
            }
            let first = folds[0];
            for fold in &folds {
                if fold
                    .decisions
                    .iter()
                    .zip(&first.decisions)
                    .any(|(a, b)| a.event_ns != b.event_ns || a.decision_ns != b.decision_ns)
                    || fold.simulation_request.selection.event_start_ns
                        != report.request.selection.event_start_ns
                    || fold.simulation_request.selection.event_end_ns
                        != report.request.selection.event_end_ns
                    || fold.simulation_request.selection.decision_cutoff_ns
                        != report.request.selection.decision_cutoff_ns
                    || fold.simulation_request.selection.bar_types
                        != report.request.selection.bar_types
                {
                    return Err(StoreError::Integrity);
                }
            }
            for (index, point) in simulation_request.target_points.iter().enumerate() {
                let expected = domain::execution::strategy::blend(
                    &report.request.members,
                    &folds
                        .iter()
                        .map(|fold| &fold.simulation_request.target_points[index])
                        .collect::<Vec<_>>(),
                    report.request.mandate.target_ttl_seconds,
                )?;
                if db::json(&expected)? != db::json(point)? {
                    return Err(StoreError::Integrity);
                }
            }
        }
        StrategyCompositionOutcomeV1::CurrentDecision {
            target,
            predictions_per_member,
            ..
        } => {
            for (member, fold) in report.request.members.iter().zip(folds) {
                let predictions = *predictions_per_member
                    .get(&member.alpha_version_id)
                    .ok_or(StoreError::Integrity)?;
                if predictions.get() > u64::from(report.request.selection.maximum_rows) {
                    return Err(StoreError::Integrity);
                }
                domain::execution::strategy::current_continuation(fold, target, predictions)
                    .map_err(|_| StoreError::Integrity)?;
            }
        }
    }
    Ok(())
}
