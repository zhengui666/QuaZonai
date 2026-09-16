//! The policy owns the window; the original Candidate owns the cohort, not past weights.
use super::*;
use contracts::research::{InputPurpose, PitStatus};

struct Sources {
    task: NativeTaskParametersV1,
    inputs: Vec<RuntimeInputV1>,
    input_sets: BTreeSet<Id>,
    qualifications: Vec<Id>,
    input_set: Id,
    dataset: Id,
    policy: Id,
    image: String,
    cpu: u16,
    capability: Id,
}

impl Store {
    pub async fn start_portfolio_study<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &PortfolioStudyRequestV1,
        read: R,
        publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::data::bounded_native_limits(&request.limits)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::PortfolioSimulate,
            key,
            Some(request.candidate_id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let (mut tx, run, window) =
            Box::pin(admit_study(tx, request, read, publish, "OPERATOR")).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        window.recheck(&mut tx).await?;
        let result = commands::finish(&mut tx, prepared, run, 202).await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(super) struct StudyWindow {
    qualifications: Vec<Id>,
    input_sets: BTreeSet<Id>,
}
impl StudyWindow {
    pub(super) async fn recheck(&self, tx: &mut Tx<'_>) -> Result<(), StoreError> {
        source_until(tx, &self.qualifications, &self.input_sets).await?;
        Ok(())
    }
}

// The caller holds its own original authority; no scientific or delivery bypass.
pub(super) async fn admit_study<'a, R, Read, P, Published>(
    mut tx: Tx<'a>,
    request: &PortfolioStudyRequestV1,
    mut read: R,
    mut publish: P,
    created_by: &str,
) -> Result<(Tx<'a>, RunSnapshotV1, StudyWindow), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    P: FnMut(NativeObjectPublication) -> Published,
    Published: std::future::Future<Output = Result<(), StoreError>>,
{
    domain::data::bounded_native_limits(&request.limits)?;
    let project: uuid::Uuid =
        sqlx::query_scalar("SELECT project_id FROM app.portfolio_candidates WHERE id=$1")
            .bind(request.candidate_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    let project = db::id(project)?;
    crate::research::project_for_write(&mut tx, project).await?;
    let original = sources(&mut tx, project, request, &mut read).await?;
    let bytes = serde_json::to_vec(&original.task).map_err(|_| StoreError::Integrity)?;
    let parameter = Id::new();
    let size = counter(bytes.len() as i64)?;
    publish(NativeObjectPublication {
        id: parameter,
        bytes,
    })
    .await?;
    // A callback can outlive licenses/qualifications or expose changed files.
    // Reread the same sources, not a newly selected cohort or a replacement plan.
    let mut current = sources(&mut tx, project, request, &mut read).await?;
    if db::json(&current.task)? != db::json(&original.task)?
        || db::json(&current.inputs)? != db::json(&original.inputs)?
        || current.capability != original.capability
        || current.image != original.image
    {
        return Err(StoreError::Integrity);
    }
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','REAL',$5,'REFERENCED')")
            .bind(parameter.as_uuid()).bind(project.as_uuid()).bind(parameter.to_string()).bind(size.get() as i64).bind(created_by).execute(&mut *tx).await?;
    current.inputs.push(RuntimeInputV1::Artifact {
        artifact_id: parameter,
        storage_version: "1".into(),
        byte_count: size,
        role: ArtifactInputRole::Parameters,
    });
    let (mut tx, run) = Store::enqueue_run_in_transaction(
        tx,
        &format!("portfolio-study/{}", Id::new()),
        &RunSubmission {
            cycle_id: request.cycle_id,
            input_set_id: current.input_set,
            runtime_id: request.runtime_id,
            runtime_revision: request.expected_runtime_revision,
            kind: RunKind::PortfolioSimulate,
            limits: request.limits.clone(),
        },
    )
    .await?;
    bind_task(
        &mut tx,
        &run.resource,
        NativeTaskDefinition {
            parameters_artifact_id: parameter,
            inputs: current.inputs,
            image_ref: current.image,
            cpu: current.cpu,
            capability_snapshot_artifact_id: current.capability,
            output_schemas: current.task.output_schemas(),
            origin: DataOrigin::Real,
            access: ArtifactAccess::EvaluatorOnly,
        },
    )
    .await?;
    sqlx::query("INSERT INTO app.portfolio_study_tasks(run_id,candidate_id,policy_id,dataset_revision_id,request) VALUES($1,$2,$3,$4,$5)")
            .bind(run.resource.id.as_uuid()).bind(request.candidate_id.as_uuid())
            .bind(current.policy.as_uuid()).bind(current.dataset.as_uuid()).bind(db::json(request)?)
            .execute(&mut *tx).await?;
    let window = StudyWindow {
        qualifications: current.qualifications,
        input_sets: current.input_sets,
    };
    Ok((tx, run.resource, window))
}

pub(super) async fn source_until(
    tx: &mut Tx<'_>,
    qualifications: &[Id],
    inputs: &BTreeSet<Id>,
) -> Result<DateTime<Utc>, StoreError> {
    let qualifications: Vec<_> = qualifications.iter().copied().map(Id::as_uuid).collect();
    let inputs: Vec<_> = inputs.iter().copied().map(Id::as_uuid).collect();
    let until: Option<DateTime<Utc>> = sqlx::query_scalar("WITH grants AS (SELECT DISTINCT g.id,g.valid_until FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id JOIN app.data_use_grants g ON g.id=d.data_use_grant_id WHERE i.input_set_id=ANY($2::uuid[])), deadlines AS (SELECT valid_until AS until FROM app.qualifications WHERE id=ANY($1::uuid[]) UNION ALL SELECT effective_at FROM app.qualification_revocations WHERE qualification_id=ANY($1) UNION ALL SELECT valid_until FROM grants UNION ALL SELECT r.effective_at FROM app.data_use_revocations r JOIN grants g ON g.id=r.grant_id) SELECT min(until) FROM deadlines HAVING min(until)>statement_timestamp() AND (SELECT count(*) FROM app.qualifications WHERE id=ANY($1))=cardinality($1) AND cardinality($1)>0")
        .bind(qualifications).bind(inputs).fetch_optional(&mut **tx).await?.flatten();
    until.ok_or(StoreError::Invalid("portfolio_study_source_expired"))
}

pub(super) async fn evidence_until<R, Read>(
    tx: &mut Tx<'_>,
    run: &RunSnapshotV1,
    intent: &PortfolioStudyRequestV1,
    task: &NativeTaskParametersV1,
    image: &str,
    read: &mut R,
) -> Result<DateTime<Utc>, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let current = sources(tx, run.project_id, intent, read).await?;
    if current.input_set != run.input_set_id
        || current.image != image
        || db::json(&current.task)? != db::json(task)?
    {
        return Err(StoreError::Integrity);
    }
    source_until(tx, &current.qualifications, &current.input_sets).await
}

async fn sources<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    request: &PortfolioStudyRequestV1,
    read: &mut R,
) -> Result<Sources, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row = sqlx::query("SELECT c.mandate_id,b.request,t.parameters_artifact_id,t.image_ref FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id JOIN app.runs r ON r.id=c.run_id AND r.state='SUCCEEDED' JOIN app.portfolio_build_tasks b ON b.run_id=c.run_id AND b.mandate_id=c.mandate_id JOIN app.run_native_tasks t ON t.run_id=b.run_id AND t.origin IN ('REAL','SYNTHETIC') WHERE c.id=$1 AND c.project_id=$2 AND c.evidence_status='VALID' AND c.solver_status IN ('OPTIMAL','ACCEPTABLE_INACCURATE')")
        .bind(request.candidate_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("portfolio_study_candidate"))?;
    let build: PortfolioBuildRequestV1 =
        serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
    let mandate = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2")
        .bind(row.try_get::<uuid::Uuid, _>("mandate_id")?)
        .bind(project.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    let mandate = crate::portfolio::view(&mandate)?;
    let policy_id: uuid::Uuid = sqlx::query_scalar("SELECT b.evaluation_policy_id FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN' WHERE c.id=$1 AND c.project_id=$2 AND c.state='RUNNING' FOR UPDATE OF c")
        .bind(request.cycle_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("portfolio_study_cycle"))?;
    if policy_id != mandate.content.required_evaluation_policy_id.as_uuid()
        || build.runtime_id != request.runtime_id
        || build.mandate_id != mandate.id
    {
        return Err(StoreError::Invalid("portfolio_study_source"));
    }
    let policy = crate::research::frozen_policy(tx, db::id(policy_id)?).await?;
    let plan = policy
        .portfolio_study_plan
        .as_ref()
        .ok_or(StoreError::Invalid("portfolio_study_plan_required"))?;
    if policy.project_id != project || policy.portfolio_metric_requirements.is_none() {
        return Err(StoreError::Invalid("portfolio_study_policy"));
    }
    crate::research::portfolio_study_input(tx, project, plan).await?;
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
    if frozen.mandate != mandate.content || build.members.len() != frozen.members.len() {
        return Err(StoreError::Integrity);
    }
    let cap = crate::runtime::require_capabilities(
        tx,
        request.runtime_id,
        request.expected_runtime_revision,
        RunKind::PortfolioSimulate,
    )
    .await?;
    domain::runtime::job_limits(&cap, &request.limits)?;
    let image = cap
        .image_refs
        .iter()
        .find(|i| i.job_kind == RunKind::PortfolioSimulate)
        .ok_or(DomainError::CapabilityUnavailable("portfolio_study_image"))?
        .image_ref
        .clone();
    if image != row.try_get::<String, _>("image_ref")? {
        return Err(DomainError::CapabilityUnavailable("portfolio_study_image").into());
    }
    for (name, version) in [
        ("portfolio-study", "6"),
        ("portfolio-history", "1"),
        ("portfolio-models", "4"),
        ("portfolio-cost-source", "1"),
        ("portfolio-slippage", "1"),
        ("simulation-models", "1"),
        ("nautilus", NAUTILUS_EXECUTION_VERSION),
        ("clarabel", CLARABEL_VERSION),
        ("ndarray", FIXED_ENSEMBLE_VERSION),
        ("ndarray-stats", SAMPLE_COVARIANCE_VERSION),
    ] {
        if cap.engine_versions.get(name).map(String::as_str) != Some(version) {
            return Err(DomainError::CapabilityUnavailable("portfolio_study_engines").into());
        }
    }
    crate::portfolio::risk_capability(&mandate.content, &cap)?;
    let mut datasets = crate::data_validation::dataset_bindings(
        tx,
        plan.input_set_id,
        project,
        request.runtime_id,
        &[InputPurpose::Portfolio],
        read,
    )
    .await?;
    if datasets.len() != 1 {
        return Err(StoreError::Invalid("portfolio_study_dataset"));
    }
    let dataset = datasets.remove(0);
    // Policy freezes the registered version's whole window. A valid quality
    // report for a smaller selection is not permission to trim that window.
    if dataset.selection.selection.event_start_ns
        != counter(
            dataset
                .metadata
                .event_start
                .timestamp_nanos_opt()
                .ok_or(StoreError::Integrity)?,
        )?
        || dataset.selection.selection.event_end_ns
            != counter(
                dataset
                    .metadata
                    .event_end
                    .timestamp_nanos_opt()
                    .ok_or(StoreError::Integrity)?,
            )?
    {
        return Err(StoreError::Invalid("portfolio_study_source_window"));
    }
    if dataset.origin != DataOrigin::Real
        || dataset.metadata.pit_status != PitStatus::Verified
        || dataset.metadata.revision_policy != contracts::catalogs::DataRevisionPolicy::AsKnownThen
    {
        return Err(StoreError::Invalid("portfolio_study_real_pit"));
    }
    let universe: uuid::Uuid =
        sqlx::query_scalar("SELECT universe_version_id FROM app.dataset_revisions WHERE id=$1")
            .bind(dataset.selection.dataset_revision_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
    if universe != mandate.content.universe_version_id.as_uuid() {
        return Err(StoreError::Invalid("portfolio_study_universe"));
    }
    let mut inputs = vec![dataset.input];
    let mut input_sets = BTreeSet::from([plan.input_set_id, build.input_set_id]);
    let mut available = DbCounter::ZERO;
    let mut members = Vec::new();
    for (chosen, original) in build.members.iter().zip(&frozen.members) {
        let (member, mut artifacts) = member_source(
            tx,
            project,
            policy.id,
            request.runtime_id,
            &image,
            chosen,
            read,
        )
        .await?;
        if db::json(&member)? != db::json(original)? {
            return Err(StoreError::Integrity);
        }
        let source = sqlx::query(MEMBER_SOURCE)
            .bind(chosen.qualification_id.as_uuid())
            .bind(project.as_uuid())
            .bind(policy.id.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
        let context =
            crate::cycles::execution_context(tx, db::id(source.try_get("brief_id")?)?).await?;
        // Selection used the sealed result too. Metadata is read only by Store;
        // none of these research datasets becomes a Study runtime input.
        for (id, purpose) in [
            (context.discovery_input_set_id, InputPurpose::Discovery),
            (context.validation_input_set_id, InputPurpose::Validation),
            (
                db::id(source.try_get("input_set_id")?)?,
                InputPurpose::Sealed,
            ),
        ] {
            input_sets.insert(id);
            for data in crate::data_validation::dataset_bindings(
                tx,
                id,
                project,
                request.runtime_id,
                &[purpose],
                read,
            )
            .await?
            {
                available = available.max(data.available_through_ns);
            }
        }
        members.push(member);
        inputs.append(&mut artifacts);
    }
    let costs = mandate.content.constraints.transaction_costs_ref;
    let (settings, assumption_input, assumption_image): (Value, uuid::Uuid, String) = sqlx::query_as("SELECT s.settings,s.input_set_id,e.engine_image_ref FROM app.execution_assumption_sources s JOIN app.execution_assumptions e ON e.id=s.assumptions_id AND e.fee_schedule_artifact_id=$4 WHERE s.assumptions_id=$1 AND s.project_id=$2 AND s.runtime_id=$3")
        .bind(mandate.content.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(request.runtime_id.as_uuid()).bind(costs.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Integrity)?;
    input_sets.insert(db::id(assumption_input)?);
    let cost_bytes = crate::execution_assumptions::liquidity::document(
        tx,
        project,
        costs,
        "qz.native_simulation_settings",
        1024 * 1024,
        read,
    )
    .await?;
    let settings: NativeSimulationSettingsV1 =
        serde_json::from_value(settings).map_err(|_| StoreError::Integrity)?;
    let document: NativeSimulationSettingsV1 =
        serde_json::from_slice(&cost_bytes).map_err(|_| StoreError::Integrity)?;
    if db::json(&settings)? != db::json(&document)?
        || db::json(&settings)? != db::json(&frozen.execution_settings)?
        || image != assumption_image
    {
        return Err(StoreError::Integrity);
    }
    domain::catalogs::execution_fees(&dataset.metadata, &settings)?;
    inputs.push(RuntimeInputV1::Artifact {
        artifact_id: costs,
        storage_version: "1".into(),
        byte_count: counter(cost_bytes.len() as i64)?,
        role: ArtifactInputRole::Parameters,
    });
    let rolling = crate::execution_assumptions::liquidity::rolling(
        tx,
        project,
        request.runtime_id,
        mandate.content.execution_assumptions_id,
        read,
    )
    .await?;
    if let Some((_, input)) = &rolling {
        if !matches!(input, RuntimeInputV1::Artifact {artifact_id, ..} if Some(*artifact_id) == mandate.content.constraints.liquidity_ref)
        {
            return Err(StoreError::Integrity);
        }
        if cap
            .engine_versions
            .get("portfolio-rolling-liquidity")
            .map(String::as_str)
            != Some("1")
            || cap.engine_versions.get("bar-notional").map(String::as_str) != Some("1")
        {
            return Err(DomainError::CapabilityUnavailable("portfolio_study_liquidity").into());
        }
        inputs.push(input.clone());
    }
    let mut calendar = None;
    if mandate.content.rebalance_schedule.kind == RebalanceKind::CalendarSession {
        if cap
            .engine_versions
            .get("portfolio-calendar")
            .map(String::as_str)
            != Some("2")
        {
            return Err(DomainError::CapabilityUnavailable("portfolio_study_calendar").into());
        }
        let (original, input) = crate::data_registration::registered_calendar(
            tx,
            db::id(universe)?,
            &dataset.metadata.universe,
            dataset.origin,
            read,
        )
        .await?;
        let RuntimeInputV1::Artifact { artifact_id, .. } = &input else {
            return Err(StoreError::Integrity);
        };
        calendar = Some(NativePortfolioCalendarV1 {
            artifact_id: *artifact_id,
            calendar: original,
        });
        inputs.push(input);
    }
    let evaluation_start = counter(
        plan.evaluation_start
            .timestamp_nanos_opt()
            .ok_or(StoreError::Integrity)?,
    )?;
    let instruments: Vec<_> = frozen
        .assets
        .iter()
        .map(|a| a.instrument_id.clone())
        .collect();
    let groups = domain::catalogs::portfolio_groups(
        &dataset.metadata.universe,
        &instruments,
        &mandate.content.constraints.group_bounds,
        evaluation_start,
    )?;
    let mut assets = frozen.assets.clone();
    for (asset, groups) in assets.iter_mut().zip(groups) {
        asset.current_weight = "0".parse().map_err(|_| StoreError::Integrity)?;
        asset.available_notional = None;
        asset.groups = groups;
    }
    let task = NativeTaskParametersV1::StudyPortfolio {
        schema_version: SchemaV1,
        dataset_revision_id: dataset.selection.dataset_revision_id,
        request: Box::new(NativePortfolioStudyRequestV1 {
            schema_version: SchemaV1,
            source_selection: dataset.selection.selection,
            evaluation_start_ns: evaluation_start,
            manual_cutoffs_ns: plan
                .manual_cutoffs
                .as_ref()
                .map(|values| {
                    values
                        .iter()
                        .map(|t| counter(t.timestamp_nanos_opt().ok_or(StoreError::Integrity)?))
                        .collect()
                })
                .transpose()?,
            calendar,
            research_available_through_ns: available,
            mandate: mandate.content,
            execution_settings: settings,
            rolling_liquidity: rolling.map(|(policy, _)| policy),
            assets,
            members,
        }),
    };
    if let NativeTaskParametersV1::StudyPortfolio { request, .. } = &task {
        domain::execution::portfolio_study_cutoffs(request)?;
    }
    if !task.output_schemas().iter().all(|s| {
        cap.artifact_schemas
            .iter()
            .any(|v| v.name == s.name && v.version == s.version)
    }) {
        return Err(DomainError::CapabilityUnavailable("portfolio_study_outputs").into());
    }
    // Shared models can appear in more than one original member. Native inputs
    // have unique object identities and the existing 256-input ceiling.
    let mut seen = BTreeSet::new();
    inputs.retain(|input| match input {
        RuntimeInputV1::Artifact { artifact_id, .. } => seen.insert(*artifact_id),
        _ => true,
    });
    if inputs.len() >= 256 {
        return Err(StoreError::Invalid("portfolio_study_inputs"));
    }
    for input in &input_sets {
        crate::research::revalidate_frozen_inputs(tx, *input, project, request.runtime_id).await?;
    }
    let capability: uuid::Uuid = sqlx::query_scalar(
        "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
    )
    .bind(request.runtime_id.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    Ok(Sources {
        task,
        inputs,
        input_sets,
        qualifications: build.members.iter().map(|m| m.qualification_id).collect(),
        input_set: plan.input_set_id,
        dataset: dataset.selection.dataset_revision_id,
        policy: policy.id,
        image,
        cpu: experiment::native_cpu(&request.limits, &cap)?,
        capability: db::id(capability)?,
    })
}
