//! Current native decisions use the existing release/approval/handoff ledger.
use super::*;
use contracts::{delivery::PackageTargetV1, forward::ForwardEnvironmentV1, strategy_portfolio::*};

pub(super) async fn package<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    candidate_id: Id,
    release_id: Id,
    read: &mut R,
) -> Result<TargetPackageV2, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let candidate = crate::portfolio::strategy::snapshot(tx, candidate_id).await?;
    if candidate.project_id != project
        || !matches!(
            candidate.purpose,
            StrategyPortfolioPurposeV1::CurrentDecision { .. }
        )
    {
        return Err(StoreError::Invalid("strategy_current_decision_required"));
    }
    let row = sqlx::query("SELECT b.request,t.parameters_artifact_id,t.image_ref FROM app.portfolio_candidates c JOIN app.portfolio_build_tasks b ON b.run_id=c.run_id AND b.source_kind='STRATEGY_ALPHA' AND b.purpose='CURRENT_DECISION' JOIN app.run_native_tasks t ON t.run_id=c.run_id WHERE c.id=$1 AND app.strategy_candidate_producer_valid(c.id,$2,$3,$4)")
        .bind(candidate_id.as_uuid()).bind(candidate.run_id.as_uuid()).bind(candidate.accepted_attempt_id.as_uuid()).bind(candidate.report_artifact_id.as_uuid())
        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("strategy_accepted_decision_required"))?;
    let build: StrategyPortfolioBuildV1 =
        serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    if build.mandate_id != candidate.mandate_id
        || build.input_set_id != candidate.input_set_id
        || db::json(&build.purpose)? != db::json(&candidate.purpose)?
    {
        return Err(StoreError::Integrity);
    }
    let bytes = validation::read_document(
        tx,
        candidate.report_artifact_id,
        Some((candidate.run_id, candidate.accepted_attempt_id)),
        "qz.strategy_portfolio",
        contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
        read,
    )
    .await?;
    let report: NativeStrategyCompositionResultV1 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    domain::execution::strategy_composition_result(&report.request, &report)
        .map_err(|_| StoreError::Integrity)?;
    let parameters_id = db::id(row.try_get("parameters_artifact_id")?)?;
    let parameters = validation::read_document(
        tx,
        parameters_id,
        None,
        "qz.native_task",
        8 * 1024 * 1024,
        read,
    )
    .await?;
    let parameters: Value =
        serde_json::from_slice(&parameters).map_err(|_| StoreError::Integrity)?;
    if parameters.get("operation").and_then(Value::as_str) != Some("COMPOSE_STRATEGY_TARGETS")
        || parameters.get("request") != Some(&db::json(&report.request)?)
    {
        return Err(StoreError::Integrity);
    }
    let stored_mandate: Value = sqlx::query_scalar("SELECT strategy_content FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2 AND allocation_method='FIXED_TARGET_WEIGHTS'")
        .bind(candidate.mandate_id.as_uuid()).bind(project.as_uuid()).fetch_one(&mut **tx).await?;
    if stored_mandate != db::json(&report.request.mandate)? {
        return Err(StoreError::Integrity);
    }
    // Reuse build admission's original member/source checks on every delivery step.
    strategy::eligibility(tx, project, &build, &report, read).await?;
    let StrategyCompositionOutcomeV1::CurrentDecision {
        account_start,
        target,
        ..
    } = &report.outcome
    else {
        return Err(StoreError::Invalid("strategy_current_decision_required"));
    };
    let cost = sqlx::query("SELECT e.venue_capability_ref,e.cost_assumption_status,s.input_set_id,s.settings FROM app.execution_assumptions e JOIN app.execution_assumption_sources s ON s.assumptions_id=e.id AND s.project_id=$2 AND s.runtime_id=$3 WHERE e.id=$1 AND e.cost_assumption_status<>'INSUFFICIENT'")
        .bind(report.request.mandate.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(build.runtime_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("release_execution_assumptions"))?;
    if cost.try_get::<Value, _>("settings")? != db::json(&report.request.settings)? {
        return Err(StoreError::Integrity);
    }
    let mut inputs = BTreeSet::from([build.input_set_id, db::id(cost.try_get("input_set_id")?)?]);
    let source_runs: Vec<_> = report
        .request
        .members
        .iter()
        .map(|m| m.policy.source.evaluation_run_id.as_uuid())
        .collect();
    let source_inputs: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT DISTINCT input_set_id FROM app.runs WHERE id=ANY($1::uuid[]) AND project_id=$2 AND input_set_id IS NOT NULL")
        .bind(source_runs).bind(project.as_uuid()).fetch_all(&mut **tx).await?;
    for input in source_inputs {
        inputs.insert(db::id(input)?);
    }
    for &input in &inputs {
        crate::research::revalidate_frozen_inputs(tx, input, project, build.runtime_id).await?;
    }
    let mut datasets = BTreeSet::from([report.request.input_provenance.dataset_revision_id]);
    datasets.extend(
        report
            .request
            .members
            .iter()
            .map(|m| m.policy.dataset_revision_id),
    );
    let input_ids: Vec<_> = inputs.iter().map(|id| id.as_uuid()).collect();
    let input_datasets: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT DISTINCT dataset_revision_id FROM app.input_set_items WHERE input_set_id=ANY($1::uuid[]) AND dataset_revision_id IS NOT NULL")
        .bind(input_ids).fetch_all(&mut **tx).await?;
    for dataset in input_datasets {
        datasets.insert(db::id(dataset)?);
    }
    let mut until = micro_floor(nanos(target.valid_until_ns)?)?;
    let dataset_ids: Vec<_> = datasets.iter().map(|v| v.as_uuid()).collect();
    let rows = sqlx::query("SELECT d.id,g.valid_until,(SELECT min(v.effective_at) FROM app.data_use_revocations v WHERE v.grant_id=g.id) AS revoked_at FROM app.dataset_revisions d JOIN app.data_use_grants g ON g.id=d.data_use_grant_id WHERE d.id=ANY($1::uuid[]) AND g.allowed_uses IN ('RESEARCH_AND_PAPER','RESEARCH_PAPER_LIVE') AND NOT EXISTS(SELECT 1 FROM app.data_use_revocations v WHERE v.grant_id=g.id AND v.effective_at<=clock_timestamp()) FOR UPDATE OF g")
        .bind(dataset_ids).fetch_all(&mut **tx).await?;
    if rows.len() != datasets.len() {
        return Err(StoreError::Invalid("approval_data_use"));
    }
    for row in rows {
        until = source_deadline(
            until,
            row.try_get("valid_until")?,
            row.try_get("revoked_at")?,
        );
    }
    let now = now(tx).await?;
    if until <= now {
        return Err(StoreError::Invalid("release_expired"));
    }
    let package = TargetPackageV2 {
        release_id, package_schema_version: TargetPackageVersionV2::V2,
        project_id: project, candidate_id, mandate_id: candidate.mandate_id,
        source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
        source: NativeTargetDecisionSourceV1 {
            run_id: candidate.run_id, accepted_attempt_id: candidate.accepted_attempt_id,
            report_artifact_id: candidate.report_artifact_id,
            alpha_version_ids: candidate.members.iter().map(|m| m.alpha_version_id).collect(),
            input_provenance: report.request.input_provenance.clone(),
        },
        execution_environment: ForwardEnvironmentV1::Paper,
        account_start: account_start.as_ref().clone(), execution_settings: report.request.settings.clone(),
        input_revision_refs: datasets.into_iter().collect(), engine_versions: report.native_versions.clone(),
        asof: nanos(target.asof_ns)?, valid_from: now.max(micro_ceil(nanos(target.asof_ns)?)?), valid_until: until,
        base_currency: report.request.mandate.base_currency.clone(),
        capital_assumption: report.request.mandate.capital_assumption.clone(),
        targets: target.targets.iter().map(|t| PackageTargetV1 { instrument_id: t.instrument_id.clone(), target_weight: t.weight.clone(), currency: t.currency.clone() }).collect(),
        cash_weight: target.cash_weight.clone(), constraints_summary: report.request.mandate.constraints.clone(),
        exposure_tolerance: report.request.mandate.exposure_tolerance.clone(),
        cost_assumption_ref: report.request.mandate.execution_assumptions_id,
        compatible_market_capabilities: vec![cost.try_get("venue_capability_ref")?],
        limitations: vec!["Fresh Paper account only. Targets are not orders or observed positions. Native wall-clock execution does not apply historical latency.".into()],
        provenance_artifact_refs: vec![parameters_id, candidate.report_artifact_id],
    };
    domain::delivery::strategy_target_package(&package, &report, &candidate)?;
    Ok(package)
}

fn source_deadline(
    target_until: DateTime<Utc>,
    grant_until: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    [grant_until, revoked_at]
        .into_iter()
        .flatten()
        .fold(target_until, |until, bound| until.min(bound))
}

fn micro_floor(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    DateTime::from_timestamp_micros(value.timestamp_micros()).ok_or(StoreError::Integrity)
}
fn micro_ceil(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    let floor = micro_floor(value)?;
    if floor < value {
        floor
            .checked_add_signed(chrono::Duration::microseconds(1))
            .ok_or(StoreError::Integrity)
    } else {
        Ok(floor)
    }
}

fn nanos(value: DbCounter) -> Result<DateTime<Utc>, StoreError> {
    let seconds = i64::try_from(value.get() / 1_000_000_000).map_err(|_| StoreError::Integrity)?;
    DateTime::from_timestamp(seconds, (value.get() % 1_000_000_000) as u32)
        .ok_or(StoreError::Integrity)
}

pub(super) async fn source<R, Read>(
    tx: &mut Tx<'_>,
    release: Id,
    environment: ForwardEnvironmentV1,
    read: &mut R,
) -> Result<(Id, Id, TargetPackageEnvelopeV2, DateTime<Utc>), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    if environment != ForwardEnvironmentV1::Paper {
        return Err(StoreError::Invalid("strategy_paper_only"));
    }
    let row = sqlx::query("SELECT r.*,c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1 AND r.source_kind='NATIVE_TARGET_DECISION' AND r.package_schema_version='2' AND r.execution_environment='PAPER'")
        .bind(release.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    let project = db::id(row.try_get("project_id")?)?;
    let candidate = db::id(row.try_get("candidate_id")?)?;
    crate::research::project_for_write(tx, project).await?;
    sqlx::query("SELECT id FROM app.portfolio_candidates WHERE id=$1 FOR UPDATE")
        .bind(candidate.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    let artifact = db::id(row.try_get("package_artifact_id")?)?;
    let bytes = read_package(tx, artifact, project, read).await?;
    let original: TargetPackageV2 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    if original.release_id != release
        || original.candidate_id != candidate
        || original.project_id != project
        || original.source.run_id != db::id(row.try_get("decision_run_id")?)?
        || original.source.accepted_attempt_id != db::id(row.try_get("decision_attempt_id")?)?
        || original.source.report_artifact_id
            != db::id(row.try_get("decision_report_artifact_id")?)?
        || original.valid_from != row.try_get::<DateTime<Utc>, _>("valid_from")?
        || original.valid_until != row.try_get::<DateTime<Utc>, _>("valid_until")?
        || original.asof.timestamp_micros()
            != row.try_get::<DateTime<Utc>, _>("asof")?.timestamp_micros()
        || original.compatible_market_capabilities.first()
            != Some(&row.try_get::<String, _>("market_capability_version")?)
    {
        return Err(StoreError::Integrity);
    }
    let mut until = original.valid_until;
    // The callbacks may overlap revocation/expiry; validate again before return.
    for _ in 0..2 {
        let mut current = package(tx, project, candidate, release, read).await?;
        // Keep the immutable execution window inside current authority. An
        // offer expiry is only a claim deadline; Paper must not execute an old
        // package past a newly scheduled source revocation.
        if current.valid_until < original.valid_until {
            return Err(StoreError::Invalid(
                "strategy_release_source_window_changed",
            ));
        }
        until = until.min(current.valid_until);
        current.valid_from = original.valid_from;
        current.valid_until = original.valid_until;
        if db::json(&current)? != db::json(&original)? {
            return Err(StoreError::Integrity);
        }
    }
    if read_package(tx, artifact, project, read).await? != bytes {
        return Err(StoreError::Integrity);
    }
    Ok((
        project,
        candidate,
        TargetPackageEnvelopeV2::TargetDecision(Box::new(original)),
        until,
    ))
}

async fn read_package<R, Read>(
    tx: &mut Tx<'_>,
    id: Id,
    project: Id,
    read: &mut R,
) -> Result<Vec<u8>, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='PACKAGE' AND schema_name='qz.target_package' AND schema_version='2' AND media_type='application/json' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1' AND access_class='DELIVERY'")
        .bind(id.as_uuid()).bind(project.as_uuid()).fetch_one(&mut **tx).await?;
    if !(1..=8 * 1024 * 1024).contains(&size) {
        return Err(StoreError::Integrity);
    }
    let bytes = read(id, counter(size)?).await?;
    if bytes.len() as u64 != size as u64 {
        return Err(StoreError::Integrity);
    }
    Ok(bytes)
}

impl Store {
    pub async fn create_strategy_release<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &StrategyReleaseCreateV1,
        mut read: R,
        mut publish: P,
    ) -> Result<CommandResult<StrategyReleaseViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ReleaseCreate,
            key,
            Some(request.candidate_id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.portfolio_candidates WHERE id=$1")
                .bind(request.candidate_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let project = db::id(project)?;
        crate::research::project_for_write(&mut tx, project).await?;
        let release = Id::new();
        let original = package(&mut tx, project, request.candidate_id, release, &mut read).await?;
        let bytes = serde_json::to_vec(&original).map_err(|_| StoreError::Integrity)?;
        let size = i64::try_from(bytes.len()).map_err(|_| StoreError::Integrity)?;
        let artifact = Id::new();
        publish(NativeObjectPublication {
            id: artifact,
            bytes,
        })
        .await?;
        let mut current =
            package(&mut tx, project, request.candidate_id, release, &mut read).await?;
        current.valid_from = original.valid_from;
        if db::json(&current)? != db::json(&original)? {
            return Err(StoreError::Conflict);
        }
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        if original.valid_until <= now(&mut tx).await? {
            return Err(StoreError::Invalid("release_expired"));
        }
        // Provenance describes the inputs. PAPER remains the execution environment.
        let origin: String = sqlx::query_scalar("SELECT origin FROM app.artifacts WHERE id=$1")
            .bind(original.source.report_artifact_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let environment = if origin == "REAL" { "REAL" } else { "DEMO" };
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PACKAGE','application/json','qz.target_package','2','LOCAL',$3,'1',$4,'DELIVERY',$5,'OPERATOR','REFERENCED')")
            .bind(artifact.as_uuid()).bind(project.as_uuid()).bind(artifact.to_string()).bind(size).bind(origin).execute(&mut *tx).await?;
        let row = sqlx::query("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment,source_kind,decision_run_id,decision_attempt_id,decision_report_artifact_id,execution_environment) VALUES($1,$2,$3,'2',$4,NULL,$5,$6,$7,$8,$9,'NATIVE_TARGET_DECISION',$10,$11,$12,'PAPER') RETURNING created_at")
            .bind(release.as_uuid()).bind(request.candidate_id.as_uuid()).bind(artifact.as_uuid()).bind(original.mandate_id.as_uuid()).bind(&original.compatible_market_capabilities[0])
            .bind(original.asof).bind(original.valid_from).bind(original.valid_until).bind(environment).bind(original.source.run_id.as_uuid()).bind(original.source.accepted_attempt_id.as_uuid()).bind(original.source.report_artifact_id.as_uuid()).fetch_one(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        if original.valid_until <= now(&mut tx).await? {
            return Err(StoreError::Invalid("release_expired"));
        }
        let result = commands::finish(
            &mut tx,
            prepared,
            StrategyReleaseViewV1 {
                schema_version: SchemaV1,
                id: release,
                project_id: project,
                candidate_id: request.candidate_id,
                mandate_id: original.mandate_id,
                package_artifact_id: artifact,
                package_schema_version: TargetPackageVersionV2::V2,
                source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
                source: original.source,
                execution_environment: ForwardEnvironmentV1::Paper,
                market_capability_version: original.compatible_market_capabilities[0].clone(),
                asof: original.asof,
                valid_from: original.valid_from,
                valid_until: original.valid_until,
                created_at: row.try_get("created_at")?,
            },
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

    #[test]
    fn permanent_grants_preserve_target_deadline_and_scheduled_revocations_bound_it() {
        let target = DateTime::<Utc>::from_timestamp(100, 0).unwrap();
        let revocation = DateTime::<Utc>::from_timestamp(80, 0).unwrap();
        let grant = DateTime::<Utc>::from_timestamp(90, 0).unwrap();
        assert_eq!(source_deadline(target, None, None), target);
        assert_eq!(source_deadline(target, None, Some(revocation)), revocation);
        assert_eq!(
            source_deadline(target, Some(grant), Some(revocation)),
            revocation
        );
        assert_eq!(source_deadline(target, Some(grant), None), grant);
    }

    #[test]
    fn database_window_rounds_inward_without_retiming_the_decision() {
        let original = nanos(DbCounter::new(43_000_000_001).unwrap()).unwrap();
        let expiry = nanos(DbCounter::new(53_000_000_001).unwrap()).unwrap();
        assert_eq!(original.timestamp_nanos_opt(), Some(43_000_000_001));
        assert_eq!(
            micro_ceil(original).unwrap().timestamp_nanos_opt(),
            Some(43_000_001_000)
        );
        assert_eq!(
            micro_floor(expiry).unwrap().timestamp_nanos_opt(),
            Some(53_000_000_000)
        );
        assert!(micro_ceil(original).unwrap() >= original);
        assert!(micro_floor(expiry).unwrap() <= expiry);
    }
}
