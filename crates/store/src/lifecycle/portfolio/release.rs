//! Freeze original target-only bytes before recording a Release. No approval.
use super::*;
use contracts::delivery::{
    PackageOriginV1, PackageTargetV1, ReleaseCreateV1, ReleaseViewV1, TargetPackageV1,
};

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
        let rows = sqlx::query("SELECT r.*,c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND ($2::uuid IS NULL OR r.id<$2) ORDER BY r.id DESC LIMIT $3").bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| v.id))
    }

    pub async fn release(&self, actor: &Actor, id: Id) -> Result<ReleaseViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT r.*,c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1")
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
        mut read: R,
        mut publish: P,
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
        let original = package(&mut tx, project, request, release, &mut read).await?;
        let bytes = serde_json::to_vec(&original).map_err(|_| StoreError::Integrity)?;
        let size = i64::try_from(bytes.len()).map_err(|_| StoreError::Integrity)?;
        let artifact = Id::new();
        publish(NativeObjectPublication {
            id: artifact,
            bytes,
        })
        .await?;
        // Recheck files and all original authority/source windows after publication.
        let mut current = package(&mut tx, project, request, release, &mut read).await?;
        current.valid_from = original.valid_from;
        if db::json(&current)? != db::json(&original)? {
            return Err(StoreError::Conflict);
        }
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        if original.valid_until <= now(&mut tx).await? {
            return Err(StoreError::Conflict);
        }
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PACKAGE','application/json','qz.target_package','1','LOCAL',$3,'1',$4,'DELIVERY','REAL','OPERATOR','REFERENCED')")
            .bind(artifact.as_uuid()).bind(project.as_uuid()).bind(artifact.to_string()).bind(size).execute(&mut *tx).await?;
        let market = &original.compatible_market_capabilities[0];
        let created_at = sqlx::query_scalar("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment) VALUES($1,$2,$3,'1',$4,$5,$6,$7,$8,$9,'REAL') RETURNING created_at")
            .bind(release.as_uuid()).bind(request.candidate_id.as_uuid()).bind(artifact.as_uuid())
            .bind(original.mandate_id.as_uuid()).bind(request.evaluation_id.as_uuid()).bind(market)
            .bind(original.asof).bind(original.valid_from).bind(original.valid_until).fetch_one(&mut *tx).await?;
        let view = ReleaseViewV1 {
            id: release,
            project_id: project,
            candidate_id: request.candidate_id,
            mandate_id: original.mandate_id,
            evaluation_id: request.evaluation_id,
            package_artifact_id: artifact,
            package_schema_version: original.package_schema_version,
            market_capability_version: market.clone(),
            asof: original.asof,
            valid_from: original.valid_from,
            valid_until: original.valid_until,
            environment: PackageOriginV1::Real,
            created_at,
        };
        let result = commands::finish(&mut tx, prepared, view, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(super) async fn package<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    intent: &ReleaseCreateV1,
    release: Id,
    read: &mut R,
) -> Result<TargetPackageV1, StoreError>
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
    if source.origin != DataOrigin::Real || candidate.header.origin != DataOrigin::Real {
        return Err(StoreError::Invalid("release_real_candidate"));
    }
    let binding = sqlx::query("SELECT t.parameters_artifact_id,t.image_ref,a.id AS report_id FROM app.run_native_tasks t JOIN app.runs r ON r.id=t.run_id AND r.state='SUCCEEDED' JOIN app.run_native_outputs o ON o.attempt_id=r.active_attempt_id JOIN app.artifacts a ON a.id=o.artifact_id AND a.producer_run_id=r.id AND a.producer_attempt_id=r.active_attempt_id AND a.schema_name='qz.native_portfolio' AND a.schema_version='1' WHERE t.run_id=$1")
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
    if frozen.mandate != mandate.content
        || frozen.members.len() != build.members.len()
        || binding.try_get::<String, _>("image_ref")? != row.try_get::<String, _>("image_ref")?
    {
        return Err(StoreError::Integrity);
    }
    let report_id = db::id(binding.try_get("report_id")?)?;
    let bytes = validation::read_document(
        tx,
        report_id,
        None,
        "qz.native_portfolio",
        contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
        read,
    )
    .await?;
    let report: NativePortfolioBuildResultV1 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    domain::execution::portfolio_build_result(&frozen, &report)
        .map_err(|_| StoreError::Integrity)?;
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
        8 * 1024 * 1024,
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
    let package = TargetPackageV1 {
        release_id: release,
        package_schema_version: contracts::settings::PackageSchemaVersion::V1,
        environment_origin: PackageOriginV1::Real,
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
        ],
    };
    domain::delivery::target_package(&package, &source.document, &mandate, &candidate)?;
    Ok(package)
}
