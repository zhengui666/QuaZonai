//! Freeze original target-only bytes before recording a Release. No approval.
use super::*;
use contracts::delivery::{
    ForecastEvaluationSourceV2, ForecastReleaseSourceV2, ForecastTargetPackageV2, PackageOriginV1,
    PackageTargetV1, ReleaseCreateV1, ReleaseViewV1,
};
use std::future::Future;

/// Active delivery may never replay an immutable V1 receipt as new authority.
/// Keep this separate from historical views, which continue to show version 1.
pub(super) async fn require_v2(tx: &mut Tx<'_>, release: Id) -> Result<(), StoreError> {
    let version: String =
        sqlx::query_scalar("SELECT package_schema_version FROM app.releases WHERE id=$1")
            .bind(release.as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    if version != "2" {
        return Err(domain::DomainError::CapabilityUnavailable("target_package_version").into());
    }
    Ok(())
}

fn view(row: &PgRow) -> Result<ReleaseViewV1, StoreError> {
    Ok(ReleaseViewV1 {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        candidate_id: db::id(row.try_get("candidate_id")?)?,
        mandate_id: db::id(row.try_get("mandate_id")?)?,
        evaluation_id: db::id(row.try_get("evaluation_id")?)?,
        package_artifact_id: db::id(row.try_get("package_artifact_id")?)?,
        package_schema_version: db::enum_value(row, "package_schema_version")?,
        market_capability_version: row.try_get("market_capability_version")?,
        asof: row.try_get("asof")?,
        valid_from: row.try_get("valid_from")?,
        valid_until: row.try_get("valid_until")?,
        environment: db::enum_value(row, "environment")?,
        created_at: row.try_get("created_at")?,
    })
}

impl Store {
    pub async fn releases(
        &self,
        actor: &Actor,
        project: Id,
        query: &contracts::control::ListQuery,
    ) -> Result<contracts::control::Page<ReleaseViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT r.*,c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND r.source_kind='FORECAST_EVALUATION' AND ($2::uuid IS NULL OR r.id<$2) ORDER BY r.id DESC LIMIT $3").bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| v.id))
    }

    pub async fn release(&self, actor: &Actor, id: Id) -> Result<ReleaseViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT r.*,c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1 AND r.source_kind='FORECAST_EVALUATION'")
            .bind(id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let project = db::id(row.try_get("project_id")?)?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        let view = view(&row)?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn create_release<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &ReleaseCreateV1,
        read: R,
        publish: P,
    ) -> Result<CommandResult<ReleaseViewV1>, StoreError>
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
        if let Some(replay) = prepared.replay::<ReleaseViewV1>()? {
            require_v2(&mut tx, replay.resource.id).await?;
            tx.commit().await?;
            return Ok(replay);
        }
        let (mut tx, view) =
            Box::pin(freeze_release(tx, request, read, publish, "OPERATOR")).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        if view.valid_until <= now(&mut tx).await? {
            return Err(StoreError::Conflict);
        }
        let result = commands::finish(&mut tx, prepared, view, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(super) async fn freeze_release<'a, R, Read, P, Published>(
    mut tx: Tx<'a>,
    request: &ReleaseCreateV1,
    mut read: R,
    mut publish: P,
    created_by: &str,
) -> Result<(Tx<'a>, ReleaseViewV1), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    P: FnMut(NativeObjectPublication) -> Published,
    Published: std::future::Future<Output = Result<(), StoreError>>,
{
    let project: uuid::Uuid =
        sqlx::query_scalar("SELECT project_id FROM app.portfolio_candidates WHERE id=$1")
            .bind(request.candidate_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    let project = db::id(project)?;
    crate::research::project_for_write(&mut tx, project).await?;
    let release = Id::new();
    let original = package(&mut tx, project, request, release, &mut read).await?;
    let bytes = serde_json::to_vec(&original).map_err(|_| StoreError::Integrity)?;
    let size = i64::try_from(bytes.len()).map_err(|_| StoreError::Integrity)?;
    let artifact = Id::new();
    if let Err(error) = publish(NativeObjectPublication {
        id: artifact,
        bytes,
    })
    .await
    {
        // Drop only queues SQLx's rollback. Release the project lock before
        // reporting failure so immediate SKIP LOCKED retries can make progress.
        tx.rollback().await?;
        return Err(error);
    }
    // Recheck files and all original authority/source windows after publication.
    let mut current = package(&mut tx, project, request, release, &mut read).await?;
    current.valid_from = original.valid_from;
    if db::json(&current)? != db::json(&original)? {
        return Err(StoreError::Conflict);
    }
    if original.valid_until <= now(&mut tx).await? {
        return Err(StoreError::Conflict);
    }
    let origin = match original.environment_origin {
        PackageOriginV1::Real => "REAL",
        PackageOriginV1::Synthetic => "SYNTHETIC",
        _ => return Err(StoreError::Integrity),
    };
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PACKAGE','application/json','qz.target_package','2','LOCAL',$3,'1',$4,'DELIVERY',$6,$5,'REFERENCED')")
            .bind(artifact.as_uuid()).bind(project.as_uuid()).bind(artifact.to_string()).bind(size).bind(created_by).bind(origin).execute(&mut *tx).await?;
    let market = &original.compatible_market_capabilities[0];
    let created_at = sqlx::query_scalar("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment,source_kind,execution_environment,paper_initial_weights_artifact_id) VALUES($1,$2,$3,'2',$4,$5,$6,$7,$8,$9,$11,'FORECAST_EVALUATION',$10,$12) RETURNING created_at")
            .bind(release.as_uuid()).bind(request.candidate_id.as_uuid()).bind(artifact.as_uuid())
            .bind(original.mandate_id.as_uuid()).bind(request.evaluation_id.as_uuid()).bind(market)
            .bind(original.asof).bind(original.valid_from).bind(original.valid_until)
            .bind(db::code(&original.source.build_environment)?).bind(origin)
            .bind(original.current_weights.paper_initialization.as_ref().map(|root|root.artifact_id.as_uuid())).fetch_one(&mut *tx).await?;
    let view = ReleaseViewV1 {
        id: release,
        project_id: project,
        candidate_id: request.candidate_id,
        mandate_id: original.mandate_id,
        evaluation_id: request.evaluation_id,
        package_artifact_id: artifact,
        package_schema_version: contracts::settings::PackageSchemaVersion::V2,
        market_capability_version: market.clone(),
        asof: original.asof,
        valid_from: original.valid_from,
        valid_until: original.valid_until,
        environment: original.environment_origin,
        created_at,
    };
    Ok((tx, view))
}

pub(super) fn package<'a, 'tx: 'a, R, Read>(
    tx: &'a mut Tx<'tx>,
    project: Id,
    intent: &'a ReleaseCreateV1,
    release: Id,
    read: &'a mut R,
) -> impl Future<Output = Result<ForecastTargetPackageV2, StoreError>> + 'a + use<'a, 'tx, R, Read>
where
    R: FnMut(Id, DbCounter) -> Read + 'a,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>> + 'a,
{
    // Construct and move the large state machine before it is polled, so this
    // constructor's temporary stack frame unwinds before lifecycle validation.
    // Keep the same borrowed transaction, callbacks and eligibility checks.
    Box::pin(package_inner(tx, project, intent, release, read))
}

async fn package_inner<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    intent: &ReleaseCreateV1,
    release: Id,
    read: &mut R,
) -> Result<ForecastTargetPackageV2, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let candidate = crate::portfolio::candidates::snapshot(tx, intent.candidate_id).await?;
    let row = sqlx::query("SELECT e.*,t.image_ref,b.request,a.producer_attempt_id FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id JOIN app.portfolio_study_tasks s ON s.run_id=e.run_id AND s.candidate_id=e.subject_candidate_id AND s.policy_id=e.policy_id JOIN app.runs r ON r.id=e.run_id AND r.state='SUCCEEDED' AND r.project_id=e.project_id AND r.input_set_id=e.input_set_id JOIN app.run_native_tasks t ON t.run_id=r.id AND t.origin='REAL' JOIN app.artifacts a ON a.id=e.report_artifact_id AND a.id=e.method_versions_artifact_id AND a.project_id=e.project_id AND a.producer_run_id=r.id AND a.producer_attempt_id=r.active_attempt_id AND a.origin='REAL' JOIN app.portfolio_build_tasks b ON b.run_id=$4 WHERE e.id=$1 AND e.project_id=$2 AND e.subject_candidate_id=$3 AND e.evaluation_kind='PORTFOLIO' AND e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='PASS' AND e.valid_until>clock_timestamp()")
        .bind(intent.evaluation_id.as_uuid()).bind(project.as_uuid()).bind(intent.candidate_id.as_uuid()).bind(candidate.header.run_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("release_portfolio_evaluation"))?;
    let build: PortfolioBuildRequestV1 =
        serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    domain::portfolio::build_selection(&build)?;
    let mandate = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2")
        .bind(candidate.header.mandate_id.as_uuid())
        .bind(project.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    let mandate = crate::portfolio::view(&mandate)?;
    if row.try_get::<uuid::Uuid, _>("policy_id")?
        != mandate.content.required_evaluation_policy_id.as_uuid()
        || build.mandate_id != mandate.id
        || build.input_set_id != candidate.header.input_set_id
    {
        return Err(StoreError::Integrity);
    }
    let policy =
        crate::research::frozen_policy(tx, mandate.content.required_evaluation_policy_id).await?;
    let plan = policy
        .portfolio_study_plan
        .as_ref()
        .ok_or(StoreError::Invalid("release_study_plan"))?;
    if Some(plan.input_set_id.as_uuid()) != row.try_get::<Option<uuid::Uuid>, _>("input_set_id")?
        || policy.portfolio_metric_requirements.is_none()
    {
        return Err(StoreError::Integrity);
    }
    crate::research::portfolio_study_input(tx, project, plan).await?;
    crate::research::revalidate_frozen_inputs(tx, plan.input_set_id, project, build.runtime_id)
        .await?;
    let source = weights::target(tx, project, intent.candidate_id, read).await?;
    let paper = source.paper_initialization.is_some();
    if (!paper
        && (source.origin != DataOrigin::Real || candidate.header.origin != DataOrigin::Real))
        || (paper
            && (source.origin != DataOrigin::Synthetic
                || candidate.header.origin != DataOrigin::Synthetic
                || build.environment != contracts::forward::ForwardEnvironmentV1::Paper))
    {
        return Err(StoreError::Invalid("release_real_candidate"));
    }
    let binding = sqlx::query("SELECT t.parameters_artifact_id,t.image_ref,r.active_attempt_id,a.id AS report_id FROM app.run_native_tasks t JOIN app.runs r ON r.id=t.run_id AND r.state='SUCCEEDED' JOIN app.run_attempts attempt ON attempt.id=r.active_attempt_id AND attempt.run_id=r.id AND attempt.dispatch_state='TERMINAL' AND attempt.accepted_at IS NOT NULL JOIN app.run_terminal_receipts terminal ON terminal.run_id=r.id AND terminal.attempt_id=attempt.id AND terminal.terminal_state='SUCCEEDED' JOIN app.run_native_outputs o ON o.attempt_id=r.active_attempt_id JOIN app.artifacts a ON a.id=o.artifact_id AND a.producer_run_id=r.id AND a.producer_attempt_id=r.active_attempt_id AND a.schema_name='qz.native_portfolio' AND a.schema_version='1' WHERE t.run_id=$1")
        .bind(candidate.header.run_id.as_uuid()).fetch_all(&mut **tx).await?;
    let [binding] = binding.as_slice() else {
        return Err(StoreError::Integrity);
    };
    let parameters = db::id(binding.try_get("parameters_artifact_id")?)?;
    let bytes = validation::read_document(
        tx,
        parameters,
        None,
        "qz.native_task",
        read,
    )
    .await?;
    // Delivery revalidation reaches this decode through a deep lifecycle poll
    // chain. Decode only owned bytes off that stack; keep SQL with this task.
    let (forward_dataset, frozen) = tokio::task::spawn_blocking(move || {
        let NativeTaskParametersV1::BuildPortfolio {
            dataset_revision_id,
            request,
            ..
        } = serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?
        else {
            return Err(StoreError::Integrity);
        };
        Ok((dataset_revision_id, request))
    })
    .await
    .map_err(|_| StoreError::Integrity)??;
    if frozen.mandate != mandate.content
        || frozen.members.len() != build.members.len()
        || binding.try_get::<String, _>("image_ref")? != row.try_get::<String, _>("image_ref")?
    {
        return Err(StoreError::Integrity);
    }
    if frozen.current_weights.paper_initialization != source.paper_initialization {
        return Err(StoreError::Integrity);
    }
    paper_initial::validate(tx, project, &build, &frozen.current_weights).await?;
    let report_id = db::id(binding.try_get("report_id")?)?;
    let bytes = validation::read_document(
        tx,
        report_id,
        None,
        "qz.native_portfolio",
        read,
    )
    .await?;
    // Decode and validate on the blocking stack, then keep the report boxed.
    // Its inline JoinHandle output and poll storage would otherwise return
    // to the nested lifecycle stack; source checks stay with this transaction.
    let (frozen, report) = tokio::task::spawn_blocking(move || {
        let report: NativePortfolioBuildResultV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        domain::execution::portfolio_build_result(&frozen, &report)
            .map_err(|_| StoreError::Integrity)?;
        Ok::<_, StoreError>((frozen, Box::new(report)))
    })
    .await
    .map_err(|_| StoreError::Integrity)??;
    publication::eligibility(
        tx,
        project,
        &build,
        (&frozen, &report),
        binding.try_get("image_ref")?,
        source.document.valid_until,
        read,
    )
    .await?;
    // ForwardSnapshot uses its RESEARCH artifact, while LastTarget freezes a
    // derived EVALUATOR_ONLY weights document. Check that original object too;
    // a matching embedded task value alone does not attest its referenced bytes.
    let weights_size: i64 = sqlx::query_scalar(
        "SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='REPORT' AND schema_name='qz.portfolio_current_weights' AND schema_version='1' AND media_type='application/json' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1' AND access_class IN ('RESEARCH','EVALUATOR_ONLY')",
    )
    .bind(frozen.current_weights_artifact_id.as_uuid())
    .bind(project.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    if weights_size == 0 {
        return Err(StoreError::Integrity);
    }
    let weights_bytes = read(frozen.current_weights_artifact_id, counter(weights_size)?).await?;
    let original_weights: PortfolioCurrentWeightsV1 =
        serde_json::from_slice(&weights_bytes).map_err(|_| StoreError::Integrity)?;
    if weights_bytes.len() as u64 != weights_size as u64
        || original_weights != frozen.current_weights
    {
        return Err(StoreError::Integrity);
    }
    // Freeze the original task's dataset identity, not just equivalent selection
    // coordinates. The same immutable input set must still resolve that ID.
    let bindings = crate::data_validation::dataset_bindings(
        tx,
        build.input_set_id,
        project,
        build.runtime_id,
        &[contracts::research::InputPurpose::Forward],
        read,
    )
    .await?;
    let [forward_binding] = bindings.as_slice() else {
        return Err(StoreError::Integrity);
    };
    if forward_binding.selection.dataset_revision_id != forward_dataset
        || forward_binding.selection.selection != frozen.selection
    {
        return Err(StoreError::Integrity);
    }
    let forward_metadata = db::id(
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT native_metadata_artifact_id FROM app.dataset_registration_evidence WHERE dataset_revision_id=$1",
        )
        .bind(forward_dataset.as_uuid())
        .fetch_one(&mut **tx)
        .await?,
    )?;
    let instrument_ids: BTreeSet<String> = frozen
        .assets
        .iter()
        .map(|asset| asset.instrument_id.clone())
        .chain(
            frozen
                .current_weights
                .weights
                .iter()
                .map(|weight| weight.instrument_id.clone()),
        )
        .chain(
            source
                .document
                .targets
                .iter()
                .map(|target| target.instrument_id.clone()),
        )
        .collect();
    let forward_dataset_bundle = domain::delivery::freeze_forward_dataset(
        forward_dataset,
        forward_metadata,
        &forward_binding.selection.selection,
        &forward_binding.metadata,
        &instrument_ids.into_iter().collect::<Vec<_>>(),
    )?;
    let cost = sqlx::query("SELECT e.venue_capability_ref,e.cost_assumption_status,s.input_set_id FROM app.execution_assumptions e JOIN app.execution_assumption_sources s ON s.assumptions_id=e.id AND s.project_id=$2 AND s.runtime_id=$3 WHERE e.id=$1 AND e.cost_assumption_status<>'INSUFFICIENT'")
        .bind(mandate.content.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(build.runtime_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("release_execution_assumptions"))?;
    let mut inputs = BTreeSet::from([
        build.input_set_id,
        plan.input_set_id,
        db::id(cost.try_get("input_set_id")?)?,
    ]);
    for chosen in &build.members {
        let member = sqlx::query(MEMBER_SOURCE)
            .bind(chosen.qualification_id.as_uuid())
            .bind(project.as_uuid())
            .bind(policy.id.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
        let context =
            crate::cycles::execution_context(tx, db::id(member.try_get("brief_id")?)?).await?;
        inputs.extend([
            context.discovery_input_set_id,
            context.validation_input_set_id,
            db::id(member.try_get("input_set_id")?)?,
        ]);
    }
    let input_ids: Vec<_> = inputs.iter().map(|id| id.as_uuid()).collect();
    let data = sqlx::query("SELECT DISTINCT d.id,d.origin,d.pit_status,d.revision_policy FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id WHERE i.input_set_id=ANY($1::uuid[]) ORDER BY d.id")
        .bind(&input_ids).fetch_all(&mut **tx).await?;
    if data.is_empty() {
        return Err(StoreError::Invalid("release_real_sources"));
    }
    for dataset in &data {
        if dataset.try_get::<String, _>("origin")? != "REAL"
            || dataset.try_get::<String, _>("pit_status")? != "VERIFIED"
            || dataset.try_get::<String, _>("revision_policy")? != "AS_KNOWN_THEN"
        {
            return Err(StoreError::Invalid("release_real_sources"));
        }
    }
    let evaluation_report = db::id(row.try_get("report_artifact_id")?)?;
    let bytes = validation::read_document(
        tx,
        evaluation_report,
        Some((
            db::id(row.try_get("run_id")?)?,
            db::id(row.try_get("producer_attempt_id")?)?,
        )),
        "qz.candidate_evaluation",
        read,
    )
    .await?;
    let evidence: Value = serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    if evidence.get("evaluation_id") != Some(&db::json(&intent.evaluation_id)?)
        || evidence.get("candidate_id") != Some(&db::json(&intent.candidate_id)?)
        || evidence.get("evaluation_kind").and_then(Value::as_str) != Some("PORTFOLIO")
        || evidence.get("decision").and_then(Value::as_str) != Some("PASS")
    {
        return Err(StoreError::Integrity);
    }
    let qualifications = candidate
        .members
        .iter()
        .map(|m| m.qualification_id)
        .collect::<Vec<_>>();
    let mut until = study::source_until(tx, &qualifications, &inputs)
        .await?
        .min(row.try_get("valid_until")?)
        .min(source.document.valid_until);
    let weights_until = DateTime::<Utc>::from_timestamp_micros(
        i64::try_from(frozen.current_weights.valid_until_ns.get() / 1000)
            .map_err(|_| StoreError::Integrity)?,
    )
    .ok_or(StoreError::Integrity)?;
    until = until.min(weights_until);
    if let Some(rolling) = &frozen.rolling_liquidity {
        until = until.min(crate::execution_assumptions::liquidity::expiry(
            &report.bar_notionals,
            rolling.maximum_age_seconds,
        )?);
    }
    let liquidity_until: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT bar_liquidity_valid_until FROM app.execution_assumption_sources WHERE assumptions_id=$1")
        .bind(mandate.content.execution_assumptions_id.as_uuid()).fetch_optional(&mut **tx).await?.flatten();
    if let Some(deadline) = liquidity_until {
        until = until.min(deadline);
    }
    let current = now(tx).await?;
    if until <= current {
        return Err(StoreError::Invalid("release_expired"));
    }
    let package = ForecastTargetPackageV2 {
        release_id: release,
        package_schema_version: contracts::strategy_portfolio::TargetPackageVersionV2::V2,
        source_kind: ForecastReleaseSourceV2::ForecastEvaluation,
        source: ForecastEvaluationSourceV2 {
            build_run_id: candidate.header.run_id,
            build_accepted_attempt_id: db::id(binding.try_get("active_attempt_id")?)?,
            build_parameters_artifact_id: parameters,
            build_report_artifact_id: report_id,
            build_input_set_id: build.input_set_id,
            build_environment: build.environment,
            forward_dataset_revision_id: forward_dataset,
            forward_metadata_artifact_id: forward_metadata,
            current_weights_artifact_id: frozen.current_weights_artifact_id,
        },
        forward_dataset: forward_dataset_bundle.clone(),
        current_weights: frozen.current_weights.clone(),
        execution_settings: frozen.execution_settings.clone(),
        environment_origin: if paper {
            PackageOriginV1::Synthetic
        } else {
            PackageOriginV1::Real
        },
        project_id: project,
        candidate_id: intent.candidate_id,
        mandate_id: mandate.id,
        qualification_refs: qualifications,
        evaluation_refs: vec![intent.evaluation_id],
        input_revision_refs: data
            .iter()
            .map(|d| db::id(d.try_get("id")?))
            .collect::<Result<_, StoreError>>()?,
        engine_versions: serde_json::from_value(
            evidence
                .get("native_versions")
                .cloned()
                .ok_or(StoreError::Integrity)?,
        )
        .map_err(|_| StoreError::Integrity)?,
        asof: source.document.asof,
        valid_from: current.max(source.document.asof),
        valid_until: until,
        base_currency: mandate.content.base_currency.clone(),
        capital_assumption: mandate.content.capital_assumption.clone(),
        current_weights_source: candidate.header.current_weights_source,
        targets: source
            .document
            .targets
            .iter()
            .map(|t| PackageTargetV1 {
                instrument_id: t.instrument_id.clone(),
                target_weight: t.weight.clone(),
                currency: t.currency.clone(),
            })
            .collect(),
        cash_weight: source.document.cash_weight.clone(),
        constraints_summary: mandate.content.constraints.clone(),
        exposure_tolerance: mandate.content.exposure_tolerance.clone(),
        cost_assumption_ref: mandate.content.execution_assumptions_id,
        compatible_market_capabilities: vec![cost.try_get("venue_capability_ref")?],
        limitations: vec![format!(
            "Cost assumptions: {}. Targets are not orders or account positions.",
            cost.try_get::<String, _>("cost_assumption_status")?
        )],
        provenance_artifact_refs: vec![
            candidate
                .header
                .target_artifact_id
                .ok_or(StoreError::Integrity)?,
            candidate.header.diagnostics_artifact_id,
            parameters,
            report_id,
            evaluation_report,
            frozen.current_weights_artifact_id,
            forward_metadata,
        ],
    };
    domain::delivery::target_package(&package, &source.document, &mandate, &candidate)?;
    domain::delivery::forecast_source_binding(&package, &build, &frozen, &forward_dataset_bundle)?;
    Ok(package)
}

fn envelope_view(
    row: &PgRow,
) -> Result<contracts::strategy_portfolio::ReleaseViewEnvelopeV2, StoreError> {
    use contracts::{forward::ForwardEnvironmentV1, strategy_portfolio::*};
    match row.try_get::<String, _>("source_kind")?.as_str() {
        "FORECAST_EVALUATION" => return Ok(ReleaseViewEnvelopeV2::Forecast(view(row)?)),
        "NATIVE_TARGET_DECISION" => {}
        _ => return Err(StoreError::Integrity),
    }
    let candidate: StrategyPortfolioCandidateV1 =
        serde_json::from_value(row.try_get("strategy_detail")?)
            .map_err(|_| StoreError::Integrity)?;
    Ok(ReleaseViewEnvelopeV2::TargetDecision(
        StrategyReleaseViewV1 {
            schema_version: SchemaV1,
            id: db::id(row.try_get("id")?)?,
            project_id: db::id(row.try_get("project_id")?)?,
            candidate_id: db::id(row.try_get("candidate_id")?)?,
            mandate_id: db::id(row.try_get("mandate_id")?)?,
            package_artifact_id: db::id(row.try_get("package_artifact_id")?)?,
            package_schema_version: TargetPackageVersionV2::V2,
            source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
            source: NativeTargetDecisionSourceV1 {
                run_id: db::id(row.try_get("decision_run_id")?)?,
                accepted_attempt_id: db::id(row.try_get("decision_attempt_id")?)?,
                report_artifact_id: db::id(row.try_get("decision_report_artifact_id")?)?,
                alpha_version_ids: candidate
                    .members
                    .iter()
                    .map(|m| m.alpha_version_id)
                    .collect(),
                input_provenance: candidate.input_provenance,
            },
            execution_environment: ForwardEnvironmentV1::Paper,
            market_capability_version: row.try_get("market_capability_version")?,
            asof: candidate.decision_asof,
            valid_from: row.try_get("valid_from")?,
            valid_until: row.try_get("valid_until")?,
            created_at: row.try_get("created_at")?,
        },
    ))
}

impl Store {
    pub async fn releases_envelope(
        &self,
        actor: &Actor,
        project: Id,
        query: &contracts::control::ListQuery,
    ) -> Result<
        contracts::control::Page<contracts::strategy_portfolio::ReleaseViewEnvelopeV2>,
        StoreError,
    > {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT r.*,c.project_id,c.strategy_detail FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND ($2::uuid IS NULL OR r.id<$2) ORDER BY r.id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(envelope_view)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| match v {
            contracts::strategy_portfolio::ReleaseViewEnvelopeV2::Forecast(v) => v.id,
            contracts::strategy_portfolio::ReleaseViewEnvelopeV2::TargetDecision(v) => v.id,
        }))
    }

    pub async fn release_envelope(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<contracts::strategy_portfolio::ReleaseViewEnvelopeV2, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT r.*,c.project_id,c.strategy_detail FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1")
            .bind(id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(row.try_get("project_id")?)?).await?;
        let result = envelope_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn create_release_envelope<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &contracts::strategy_portfolio::ReleaseCreateEnvelopeV2,
        read: R,
        publish: P,
    ) -> Result<CommandResult<contracts::strategy_portfolio::ReleaseViewEnvelopeV2>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        use contracts::strategy_portfolio::{ReleaseCreateEnvelopeV2, ReleaseViewEnvelopeV2};
        match request {
            ReleaseCreateEnvelopeV2::Forecast(request) => {
                let result = self
                    .create_release(actor, key, request, read, publish)
                    .await?;
                Ok(CommandResult {
                    schema_version: result.schema_version,
                    replayed: result.replayed,
                    resource: ReleaseViewEnvelopeV2::Forecast(result.resource),
                })
            }
            ReleaseCreateEnvelopeV2::TargetDecision(request) => {
                let result = self
                    .create_strategy_release(actor, key, request, read, publish)
                    .await?;
                Ok(CommandResult {
                    schema_version: result.schema_version,
                    replayed: result.replayed,
                    resource: ReleaseViewEnvelopeV2::TargetDecision(result.resource),
                })
            }
        }
    }
}
