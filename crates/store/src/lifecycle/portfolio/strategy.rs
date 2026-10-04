//! TARGET_WEIGHT admission on the existing PORTFOLIO_BUILD budget, Run and queue.
use super::*;
use contracts::{
    research::{DataPartition, InputPurpose},
    strategy_portfolio::*,
};
use std::collections::BTreeMap;

struct Resolved {
    request: NativeStrategyCompositionRequestV1,
    inputs: Vec<RuntimeInputV1>,
    origin: DataOrigin,
}

async fn artifact(
    tx: &mut Tx<'_>,
    project: Id,
    id: Id,
    role: ArtifactInputRole,
    schema: &str,
) -> Result<(RuntimeInputV1, DataOrigin), StoreError> {
    let row=sqlx::query("SELECT byte_count,storage_version,origin FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind=$3 AND schema_name=$4 AND schema_version='1' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1' AND access_class IN ('RESEARCH','EVALUATOR_ONLY')")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(role.code()).bind(schema).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("strategy_artifact_source"))?;
    let size = counter(row.try_get("byte_count")?)?;
    let maximum = if role == ArtifactInputRole::Report {
        contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES
    } else {
        2 * 1024 * 1024
    };
    if size == DbCounter::ZERO || size.get() > maximum {
        return Err(StoreError::Invalid("strategy_artifact_size"));
    }
    Ok((
        RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: row.try_get("storage_version")?,
            byte_count: size,
            role,
        },
        db::enum_value(&row, "origin")?,
    ))
}
async fn read_input<T, R, Read>(input: &RuntimeInputV1, read: &mut R) -> Result<T, StoreError>
where
    T: serde::de::DeserializeOwned,
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let RuntimeInputV1::Artifact {
        artifact_id,
        byte_count,
        ..
    } = input
    else {
        return Err(StoreError::Integrity);
    };
    let bytes = read(*artifact_id, *byte_count).await?;
    if bytes.len() as u64 != byte_count.get() {
        return Err(StoreError::Integrity);
    }
    serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)
}

pub(super) fn purpose(value: &StrategyPortfolioPurposeV1) -> &'static str {
    match value {
        StrategyPortfolioPurposeV1::HistoricalReplay {} => "HISTORICAL_REPLAY",
        StrategyPortfolioPurposeV1::CurrentDecision { .. } => "CURRENT_DECISION",
    }
}

/// Collect every source before the first runtime/grant lock. Current decisions
/// reuse historical policy observations under the current Forward use purpose.
async fn authorize_sources(
    tx: &mut Tx<'_>,
    project: Id,
    build: &StrategyPortfolioBuildV1,
) -> Result<(), StoreError> {
    let input = crate::research::input(tx, build.input_set_id).await?;
    if input.header.project_id != project {
        return Err(StoreError::Integrity);
    }
    let purpose = input.header.purpose;
    let mut uses = crate::research::frozen_dataset_uses(tx, build.input_set_id, project).await?;
    for chosen in &build.members {
        let row = sqlx::query("SELECT strategy_policy FROM app.alpha_versions WHERE id=$1 AND project_id=$2 AND output_kind='TARGET_WEIGHT'")
            .bind(chosen.alpha_version_id.as_uuid()).bind(project.as_uuid())
            .fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("strategy_alpha_required"))?;
        let policy: FrozenTargetPolicyV1 = serde_json::from_value(row.try_get("strategy_policy")?)
            .map_err(|_| StoreError::Integrity)?;
        let original: uuid::Uuid =
            sqlx::query_scalar("SELECT input_set_id FROM app.runs WHERE id=$1 AND project_id=$2")
                .bind(policy.source.evaluation_run_id.as_uuid())
                .bind(project.as_uuid())
                .fetch_one(&mut **tx)
                .await?;
        uses.extend(crate::research::frozen_dataset_uses(tx, db::id(original)?, project).await?);
        for id in &policy.feature_artifact_ids {
            let feature = crate::recorded_features::resolve(
                tx,
                project,
                *id,
                Some(policy.dataset_revision_id),
            )
            .await?;
            if let Some(binding) = feature.binding {
                uses.push(
                    crate::recorded_features::dataset_use(
                        tx,
                        binding.dataset_revision_id,
                        purpose,
                        Some(input.header.decision_cutoff),
                    )
                    .await?,
                );
            }
        }
    }
    let assumption: uuid::Uuid = sqlx::query_scalar("SELECT s.input_set_id FROM app.portfolio_mandates m JOIN app.execution_assumption_sources s ON s.assumptions_id=m.execution_assumptions_id AND s.project_id=m.project_id AND s.runtime_id=$3 WHERE m.id=$1 AND m.project_id=$2")
        .bind(build.mandate_id.as_uuid()).bind(project.as_uuid()).bind(build.runtime_id.as_uuid())
        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("strategy_execution_assumptions"))?;
    uses.extend(crate::research::frozen_dataset_uses(tx, db::id(assumption)?, project).await?);
    crate::research::validate_dataset_uses(tx, &uses, Some(build.runtime_id)).await
}

async fn resolve<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    build: &StrategyPortfolioBuildV1,
    mandate: &StrategyMandateContentV1,
    image: &str,
    read: &mut R,
) -> Result<Resolved, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    domain::execution::strategy::build(build)?;
    domain::execution::strategy::mandate(mandate)?;
    authorize_sources(tx, project, build).await?;
    let historical = matches!(
        build.purpose,
        StrategyPortfolioPurposeV1::HistoricalReplay {}
    );
    let purposes = if historical {
        vec![InputPurpose::Portfolio, InputPurpose::Validation]
    } else {
        vec![InputPurpose::Forward]
    };
    let mut datasets = crate::data_validation::dataset_bindings(
        tx,
        build.input_set_id,
        project,
        build.runtime_id,
        &purposes,
        read,
    )
    .await?;
    if datasets.len() != 1 {
        return Err(StoreError::Invalid("strategy_execution_dataset"));
    }
    let dataset = datasets.remove(0);
    if !matches!(&dataset.input,RuntimeInputV1::Dataset{role,..} if *role==if historical{DataPartition::Validation}else{DataPartition::Forward})
        || !dataset.selection.settlements.is_empty()
    {
        return Err(StoreError::Invalid("strategy_execution_partition"));
    }
    if !historical {
        let checked = now(tx)
            .await?
            .timestamp_nanos_opt()
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(StoreError::Integrity)?;
        let available = dataset.available_through_ns.get();
        if available > checked
            || checked - available > u64::from(mandate.max_input_age_seconds) * 1_000_000_000
        {
            return Err(StoreError::Invalid("strategy_current_input_expired"));
        }
    }
    let universe: uuid::Uuid =
        sqlx::query_scalar("SELECT universe_version_id FROM app.dataset_revisions WHERE id=$1")
            .bind(dataset.selection.dataset_revision_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
    if universe != mandate.universe_version_id.as_uuid() {
        return Err(StoreError::Invalid("strategy_universe"));
    }
    let assumption=sqlx::query("SELECT s.settings,s.input_set_id,e.engine_image_ref,e.base_currency,e.starting_capital,e.fee_schedule_artifact_id,e.liquidity_artifact_id,e.participation_limit,e.cost_assumption_status FROM app.execution_assumption_sources s JOIN app.execution_assumptions e ON e.id=s.assumptions_id WHERE s.assumptions_id=$1 AND s.project_id=$2 AND s.runtime_id=$3")
        .bind(mandate.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(build.runtime_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("strategy_execution_assumptions"))?;
    let settings: NativeSimulationSettingsV1 =
        serde_json::from_value(assumption.try_get("settings")?)
            .map_err(|_| StoreError::Integrity)?;
    if assumption.try_get::<String, _>("engine_image_ref")? != image
        || assumption.try_get::<String, _>("base_currency")? != mandate.base_currency
        || assumption.try_get::<bigdecimal::BigDecimal, _>("starting_capital")?
            != *mandate.capital_assumption.as_decimal()
        || db::id(assumption.try_get("fee_schedule_artifact_id")?)?
            != mandate.constraints.transaction_costs_ref
        || db::optional_id(&assumption, "liquidity_artifact_id")?
            != mandate.constraints.liquidity_ref
        || assumption
            .try_get::<Option<bigdecimal::BigDecimal>, _>("participation_limit")?
            .is_some()
        || assumption.try_get::<String, _>("cost_assumption_status")? == "INSUFFICIENT"
    {
        return Err(StoreError::Invalid("strategy_frozen_execution"));
    }
    crate::research::revalidate_frozen_inputs(
        tx,
        db::id(assumption.try_get("input_set_id")?)?,
        project,
        build.runtime_id,
    )
    .await?;
    let (cost_input, _) = artifact(
        tx,
        project,
        mandate.constraints.transaction_costs_ref,
        ArtifactInputRole::Parameters,
        "qz.native_simulation_settings",
    )
    .await?;
    let original_settings: NativeSimulationSettingsV1 = read_input(&cost_input, read).await?;
    if db::json(&settings)? != db::json(&original_settings)? {
        return Err(StoreError::Integrity);
    }
    domain::catalogs::execution_fees(&dataset.metadata, &settings)?;
    let mut inputs = vec![dataset.input.clone(), cost_input];
    let mut origin = dataset.origin;
    let mut members = Vec::with_capacity(build.members.len());
    let mut feature_artifact_origins = BTreeMap::new();
    let mut feature_source_bindings = BTreeMap::new();
    let mut selected = dataset.selection.selection.clone();
    let mut original_clocks = None;
    for chosen in &build.members {
        let resolved = super::super::external_experiment::resolve_strategy_alpha(
            tx,
            project,
            chosen.alpha_version_id,
            read,
        )
        .await?;
        let policy = resolved.version.policy;
        if resolved.runtime_id != build.runtime_id || policy.runtime_image_ref != image {
            return Err(StoreError::Invalid("strategy_alpha_runtime"));
        }
        let source_input: uuid::Uuid =
            sqlx::query_scalar("SELECT input_set_id FROM app.runs WHERE id=$1 AND project_id=$2")
                .bind(policy.source.evaluation_run_id.as_uuid())
                .bind(project.as_uuid())
                .fetch_one(&mut **tx)
                .await?;
        crate::research::revalidate_frozen_inputs(
            tx,
            db::id(source_input)?,
            project,
            build.runtime_id,
        )
        .await?;
        let fold = domain::execution::strategy::source(&policy, &resolved.report)?;
        if historical {
            let clocks = fold
                .decisions
                .iter()
                .map(|d| (d.event_ns, d.decision_ns))
                .collect::<Vec<_>>();
            if original_clocks
                .as_ref()
                .is_some_and(|original| *original != clocks)
            {
                return Err(StoreError::Invalid("strategy_historical_alignment"));
            }
            if members.is_empty() {
                selected = fold.simulation_request.selection.clone();
                original_clocks = Some(clocks);
            }
            if selected.event_start_ns != fold.simulation_request.selection.event_start_ns
                || selected.event_end_ns != fold.simulation_request.selection.event_end_ns
                || selected.decision_cutoff_ns
                    != fold.simulation_request.selection.decision_cutoff_ns
                || selected.bar_types != fold.simulation_request.selection.bar_types
            {
                return Err(StoreError::Invalid("strategy_historical_window"));
            }
        } else if selected.event_start_ns > policy.initialization.first_event_ns
            || selected.decision_cutoff_ns
                <= fold
                    .decisions
                    .last()
                    .ok_or(StoreError::Integrity)?
                    .decision_ns
        {
            return Err(StoreError::Invalid("strategy_current_warmup_catalog"));
        }
        let mut feature_ids = policy.feature_artifact_ids.clone();
        if let StrategyPortfolioPurposeV1::CurrentDecision { member_inputs, .. } = &build.purpose {
            feature_ids.extend(
                member_inputs
                    .iter()
                    .find(|i| i.alpha_version_id == chosen.alpha_version_id)
                    .ok_or(StoreError::Invalid("strategy_member_inputs"))?
                    .feature_artifact_ids
                    .iter()
                    .copied(),
            );
        }
        let mut original_parts = Vec::new();
        let mut new_parts = Vec::new();
        for (index, id) in feature_ids.iter().enumerate() {
            let expected_dataset = if index < policy.feature_artifact_ids.len() {
                policy.dataset_revision_id
            } else {
                dataset.selection.dataset_revision_id
            };
            let feature =
                crate::recorded_features::resolve(tx, project, *id, Some(expected_dataset)).await?;
            let part = crate::recorded_features::read(&feature, read).await?;
            if index < policy.feature_artifact_ids.len() {
                original_parts.push(part);
            } else {
                new_parts.push(part);
            }
            origin = crate::data_validation::combine_origin(origin, feature.origin);
            feature_artifact_origins.insert(*id, feature.origin);
            if let Some(binding) = feature.binding {
                feature_source_bindings.insert(*id, binding);
            }
            inputs.push(feature.input);
        }
        domain::execution::features::bind_observations(
            &original_parts,
            &policy.feature_schema,
            DataPartition::Validation,
        )?;
        if !historical {
            domain::execution::features::bind_observations(
                &new_parts,
                &policy.feature_schema,
                DataPartition::Forward,
            )?;
        }
        inputs.push(
            artifact(
                tx,
                project,
                policy.model_artifact_id,
                ArtifactInputRole::Model,
                "qz.wasm_model",
            )
            .await?
            .0,
        );
        inputs.push(
            artifact(
                tx,
                project,
                policy.source.report_artifact_id,
                ArtifactInputRole::Report,
                "qz.experiment_evaluation",
            )
            .await?
            .0,
        );
        members.push(NativeStrategyMemberV1 {
            alpha_id: resolved.version.alpha_id,
            alpha_version_id: chosen.alpha_version_id,
            ensemble_weight: chosen.ensemble_weight.clone(),
            policy,
            feature_artifact_ids: feature_ids,
        });
    }
    let scope = &dataset.selection.selection;
    if selected.event_start_ns < scope.event_start_ns
        || selected.event_end_ns > scope.event_end_ns
        || selected.decision_cutoff_ns > scope.decision_cutoff_ns
        || selected.bar_types != scope.bar_types
        || selected.maximum_rows > scope.maximum_rows
    {
        return Err(StoreError::Invalid("strategy_catalog_scope"));
    }
    if let StrategyPortfolioPurposeV1::CurrentDecision { account_start, .. } = &build.purpose {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.downstream_integrations WHERE id=$1 AND enabled AND environments IN ('PAPER','BOTH'))").bind(account_start.downstream_id.as_uuid()).fetch_one(&mut **tx).await?;
        if !exists {
            return Err(StoreError::Invalid("strategy_paper_downstream"));
        }
    }
    let request = NativeStrategyCompositionRequestV1 {
        schema_version: SchemaV1,
        selection: selected,
        mandate: mandate.clone(),
        settings,
        purpose: build.purpose.clone(),
        input_provenance: StrategyInputProvenanceV1 {
            feature_source_bindings: (!feature_source_bindings.is_empty())
                .then_some(feature_source_bindings),
            dataset_revision_id: dataset.selection.dataset_revision_id,
            market_data_origin: dataset.origin,
            pit_status: dataset.metadata.pit_status,
            revision_policy: dataset.metadata.revision_policy,
            feature_artifact_origins,
        },
        members,
        total_fuel: DbCounter::new(1_000_000_000).map_err(|_| StoreError::Integrity)?,
    };
    domain::execution::strategy_composition_request(&request)?;
    let mut seen = BTreeSet::new();
    inputs.retain(|i| match i {
        RuntimeInputV1::Artifact { artifact_id, .. } => seen.insert(*artifact_id),
        _ => true,
    });
    // Immutable object reads can cross a grant expiry or a future-effective
    // revocation. Recheck every original/current source after the final read,
    // including the second resolution used for admission and eligibility.
    authorize_sources(tx, project, build).await?;
    Ok(Resolved {
        request,
        inputs,
        origin,
    })
}

impl Store {
    pub async fn start_strategy_portfolio_build<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &StrategyPortfolioBuildV1,
        mut read: R,
        mut publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::execution::strategy::build(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::PortfolioBuild,
            key,
            Some(request.mandate_id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let row=sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND allocation_method='FIXED_TARGET_WEIGHTS'").bind(request.mandate_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let mandate = crate::portfolio::strategy::mandate(&row)?;
        let project = mandate.project_id;
        crate::research::project_for_write(&mut tx, project).await?;
        let cycle:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.research_cycles WHERE id=$1 AND project_id=$2 AND state='RUNNING')").bind(request.cycle_id.as_uuid()).bind(project.as_uuid()).fetch_one(&mut *tx).await?;
        if !cycle {
            return Err(StoreError::Invalid("strategy_cycle"));
        }
        authorize_sources(&mut tx, project, request).await?;
        let cap = crate::runtime::require_capabilities(
            &mut tx,
            request.runtime_id,
            request.expected_runtime_revision,
            RunKind::PortfolioBuild,
        )
        .await?;
        domain::runtime::job_limits(&cap, &request.limits)?;
        if cap
            .engine_versions
            .get("strategy-composition")
            .map(String::as_str)
            != Some("1")
        {
            return Err(DomainError::CapabilityUnavailable("strategy_composition").into());
        }
        let image = cap
            .image_refs
            .iter()
            .find(|i| i.job_kind == RunKind::PortfolioBuild)
            .ok_or(DomainError::CapabilityUnavailable("strategy_image"))?
            .image_ref
            .clone();
        let resolved = resolve(
            &mut tx,
            project,
            request,
            &mandate.content,
            &image,
            &mut read,
        )
        .await?;
        let task = NativeTaskParametersV1::ComposeStrategyTargets {
            schema_version: SchemaV1,
            dataset_revision_id: resolved.request.input_provenance.dataset_revision_id,
            request: Box::new(resolved.request.clone()),
        };
        let schemas = task.output_schemas();
        if !schemas.iter().all(|s| {
            cap.artifact_schemas
                .iter()
                .any(|v| v.name == s.name && v.version == s.version)
        }) {
            return Err(DomainError::CapabilityUnavailable("strategy_outputs").into());
        }
        let cpu = experiment::native_cpu(&request.limits, &cap)?;
        let capability: uuid::Uuid = sqlx::query_scalar(
            "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
        )
        .bind(request.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let id = Id::new();
        let bytes = serde_json::to_vec(&task).map_err(|_| StoreError::Integrity)?;
        let size = counter(bytes.len() as i64)?;
        publish(NativeObjectPublication { id, bytes }).await?;
        let checked = resolve(
            &mut tx,
            project,
            request,
            &mandate.content,
            &image,
            &mut read,
        )
        .await?;
        if db::json(&checked.request)? != db::json(&resolved.request)? {
            return Err(StoreError::Conflict);
        }
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY',$5,'OPERATOR','REFERENCED')").bind(id.as_uuid()).bind(project.as_uuid()).bind(id.to_string()).bind(size.get() as i64).bind(db::code(&resolved.origin)?).execute(&mut *tx).await?;
        let mut inputs = resolved.inputs;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: size,
            role: ArtifactInputRole::Parameters,
        });
        let (mut tx, run) = Store::enqueue_run_in_transaction(
            tx,
            &format!("strategy-build/{}", Id::new()),
            &RunSubmission {
                cycle_id: request.cycle_id,
                input_set_id: request.input_set_id,
                runtime_id: request.runtime_id,
                runtime_revision: request.expected_runtime_revision,
                kind: RunKind::PortfolioBuild,
                limits: request.limits.clone(),
            },
        )
        .await?;
        bind_task(
            &mut tx,
            &run.resource,
            NativeTaskDefinition {
                parameters_artifact_id: id,
                inputs,
                image_ref: image,
                cpu,
                capability_snapshot_artifact_id: db::id(capability)?,
                output_schemas: schemas,
                origin: resolved.origin,
                access: ArtifactAccess::EvaluatorOnly,
            },
        )
        .await?;
        sqlx::query("INSERT INTO app.portfolio_build_tasks(run_id,mandate_id,request,source_kind,purpose) VALUES($1,$2,$3,'STRATEGY_ALPHA',$4)").bind(run.resource.id.as_uuid()).bind(request.mandate_id.as_uuid()).bind(db::json(request)?).bind(purpose(&request.purpose)).execute(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, run.resource, 202).await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(super) async fn eligibility<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    build: &StrategyPortfolioBuildV1,
    report: &NativeStrategyCompositionResultV1,
    read: &mut R,
) -> Result<(), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    domain::execution::strategy_composition_result(&report.request, report)?;
    super::strategy_publication::source_bindings(tx, project, report, read).await?;
    let row=sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2 AND allocation_method='FIXED_TARGET_WEIGHTS'").bind(build.mandate_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("strategy_mandate"))?;
    let mandate = crate::portfolio::strategy::mandate(&row)?;
    if db::json(&mandate.content)? != db::json(&report.request.mandate)? {
        return Err(StoreError::Integrity);
    }
    let image = &report.request.members[0].policy.runtime_image_ref;
    let resolved = resolve(tx, project, build, &mandate.content, image, read).await?;
    if db::json(&resolved.request)? != db::json(&report.request)? {
        return Err(StoreError::Integrity);
    }
    if let StrategyCompositionOutcomeV1::CurrentDecision { target, .. } = &report.outcome {
        let now = now(tx)
            .await?
            .timestamp_nanos_opt()
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(StoreError::Integrity)?;
        if target.asof_ns.get() > now
            || target.valid_until_ns.get() <= now
            || now - target.asof_ns.get()
                > u64::from(mandate.content.max_input_age_seconds) * 1_000_000_000
        {
            return Err(StoreError::Invalid("strategy_current_expired"));
        }
    }
    Ok(())
}
