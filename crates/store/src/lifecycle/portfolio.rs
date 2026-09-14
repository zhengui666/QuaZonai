//! Original-source portfolio admission on the existing Cycle budget and Run queue.
use super::*;
use contracts::{
    artifacts::ArtifactAccess,
    control::OperatorOperation,
    execution::NativeTaskParametersV1,
    portfolio::*,
    research::{ArtifactInputRole, DataOrigin},
    runtime_jobs::RuntimeInputV1,
    science::*,
};
use native::{bind_task, NativeObjectPublication, NativeTaskDefinition};
use std::collections::BTreeSet;

mod approvals;
mod automated;
mod decisions;
mod evaluation;
mod handoffs;
mod publication;
mod release;
mod simulation;
mod study;
mod weights;
pub(super) use evaluation::publish as publish_evaluation;
pub(super) use publication::publish;

// FOR UPDATE also conflicts with the revocation insert's native FK key-share
// lock. FOR SHARE alone would not serialize a new revocation against admission.

const MEMBER_SOURCE: &str = "SELECT q.alpha_version_id,v.alpha_id,v.model_artifact_id,v.runtime_image_ref,e.run_id,t.parameters_artifact_id,source_cycle.brief_id,r.input_set_id FROM app.qualifications q JOIN app.alpha_versions v ON v.id=q.alpha_version_id AND v.project_id=$2 JOIN app.alphas a ON a.id=v.alpha_id AND a.active_version_id=v.id AND a.lifecycle='QUALIFIED' JOIN app.evaluations e ON e.id=q.qualifying_evaluation_id AND e.subject_alpha_version_id=v.id AND e.policy_id=q.policy_id AND e.evaluation_kind='SEALED' AND e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='PASS' AND e.valid_until>clock_timestamp() JOIN app.evaluation_publications p ON p.evaluation_id=e.id JOIN app.runs r ON r.id=e.run_id AND r.state='SUCCEEDED' JOIN app.mission_sealed_evaluations continuation ON continuation.run_id=r.id JOIN app.mission_reviews review ON review.reservation_id=continuation.review_reservation_id AND review.decision='PASS' JOIN app.artifacts report ON report.id=e.report_artifact_id AND report.id=e.method_versions_artifact_id AND report.project_id=e.project_id AND report.producer_run_id=r.id AND report.producer_attempt_id=r.active_attempt_id AND report.origin='REAL' AND report.schema_name='qz.alpha_evaluation' AND report.schema_version='1' AND report.access_class='EVALUATOR_ONLY' JOIN app.sealed_opportunities opportunity ON opportunity.attempt_id=r.active_attempt_id JOIN app.run_native_attempts original_attempt ON original_attempt.attempt_id=opportunity.attempt_id AND original_attempt.run_id=r.id JOIN app.run_native_tasks t ON t.run_id=r.id AND t.origin='REAL' JOIN app.sealed_evaluation_tasks s ON s.run_id=r.id JOIN app.evaluations original ON original.id=s.validation_evaluation_id JOIN app.runs source_run ON source_run.id=original.run_id JOIN app.research_cycles source_cycle ON source_cycle.id=source_run.cycle_id WHERE q.id=$1 AND q.policy_id=$3 AND q.granted_at<=clock_timestamp() AND q.valid_until>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM app.qualification_revocations revoked WHERE revoked.qualification_id=q.id AND revoked.effective_at<=clock_timestamp()) FOR UPDATE OF q FOR SHARE OF a";

impl Store {
    pub async fn start_portfolio_build<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &PortfolioBuildRequestV1,
        mut read: R,
        mut publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::portfolio::build_selection(request)?;
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
        let row = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1")
            .bind(request.mandate_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let mandate = crate::portfolio::view(&row)?;
        let project = mandate.project_id;
        crate::research::project_for_write(&mut tx, project).await?;
        let cycle = sqlx::query("SELECT b.evaluation_policy_id FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN' WHERE c.id=$1 AND c.project_id=$2 AND c.state='RUNNING' FOR UPDATE OF c")
            .bind(request.cycle_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("portfolio_cycle"))?;
        if cycle.try_get::<uuid::Uuid, _>("evaluation_policy_id")?
            != mandate.content.required_evaluation_policy_id.as_uuid()
        {
            return Err(StoreError::Invalid("portfolio_policy"));
        }
        let policy = mandate.content.required_evaluation_policy_id;
        let cap = crate::runtime::require_capabilities(
            &mut tx,
            request.runtime_id,
            request.expected_runtime_revision,
            RunKind::PortfolioBuild,
        )
        .await?;
        domain::runtime::job_limits(&cap, &request.limits)?;
        let image = cap
            .image_refs
            .iter()
            .find(|i| i.job_kind == RunKind::PortfolioBuild)
            .ok_or(DomainError::CapabilityUnavailable("portfolio_image"))?
            .image_ref
            .clone();
        for (name, version) in [
            ("portfolio-models", "4"),
            ("portfolio-weights", "1"),
            ("portfolio-cost-source", "1"),
            ("clarabel", CLARABEL_VERSION),
            ("ndarray", FIXED_ENSEMBLE_VERSION),
            ("ndarray-stats", SAMPLE_COVARIANCE_VERSION),
        ] {
            if cap.engine_versions.get(name).map(String::as_str) != Some(version) {
                return Err(DomainError::CapabilityUnavailable("portfolio_native_models").into());
            }
        }
        crate::portfolio::risk_capability(&mandate.content, &cap)?;
        let mut datasets = crate::data_validation::dataset_bindings(
            &mut tx,
            request.input_set_id,
            project,
            request.runtime_id,
            &[contracts::research::InputPurpose::Forward],
            &mut read,
        )
        .await?;
        if datasets.len() != 1 {
            return Err(StoreError::Invalid("portfolio_forward_dataset"));
        }
        let dataset = datasets.remove(0);
        if dataset.origin != DataOrigin::Real
            || dataset.metadata.pit_status != contracts::research::PitStatus::Verified
            || dataset.metadata.revision_policy
                != contracts::catalogs::DataRevisionPolicy::AsKnownThen
        {
            return Err(StoreError::Invalid("portfolio_real_pit_required"));
        }
        let universe: uuid::Uuid =
            sqlx::query_scalar("SELECT universe_version_id FROM app.dataset_revisions WHERE id=$1")
                .bind(dataset.selection.dataset_revision_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if universe != mandate.content.universe_version_id.as_uuid() {
            return Err(StoreError::Invalid("portfolio_universe"));
        }

        let resolved = weights::resolve(&mut tx, project, request, &mut read).await?;
        let mut origin = resolved.origin;
        let weights = resolved.content;
        let weights_deadline = weights.valid_until_ns;
        let mut derived = None;
        let weights_input = if let Some(artifact) = resolved.artifact {
            artifact
        } else {
            let id = Id::new();
            let bytes = serde_json::to_vec(&weights).map_err(|_| StoreError::Integrity)?;
            let size = counter(bytes.len() as i64)?;
            derived = Some(NativeObjectPublication { id, bytes });
            RuntimeInputV1::Artifact {
                artifact_id: id,
                storage_version: "1".into(),
                byte_count: size,
                role: ArtifactInputRole::Report,
            }
        };
        let RuntimeInputV1::Artifact {
            artifact_id: weights_id,
            ..
        } = &weights_input
        else {
            return Err(StoreError::Integrity);
        };
        let weights_id = *weights_id;
        let mut inputs = vec![dataset.input, weights_input];
        let mut members = Vec::with_capacity(request.members.len());
        let mut selected = BTreeSet::new();
        for member in &request.members {
            let (original, mut artifacts) = member_source(
                &mut tx,
                project,
                mandate.content.required_evaluation_policy_id,
                request.runtime_id,
                &image,
                member,
                &mut read,
            )
            .await?;
            if !selected.insert(original.alpha_version_id) {
                return Err(StoreError::Invalid("portfolio_duplicate_version"));
            }
            members.push(original);
            inputs.append(&mut artifacts);
        }
        let groups = domain::catalogs::portfolio_groups(
            &dataset.metadata.universe,
            &weights
                .weights
                .iter()
                .map(|w| w.instrument_id.clone())
                .collect::<Vec<_>>(),
            &mandate.content.constraints.group_bounds,
            dataset.selection.selection.decision_cutoff_ns,
        )?;
        let assumption = sqlx::query("SELECT s.settings,s.input_set_id,e.engine_image_ref,e.cost_assumption_status FROM app.execution_assumption_sources s JOIN app.execution_assumptions e ON e.id=s.assumptions_id WHERE s.assumptions_id=$1 AND s.project_id=$2 AND s.runtime_id=$3 AND e.fee_schedule_artifact_id=$4")
            .bind(mandate.content.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(request.runtime_id.as_uuid())
            .bind(mandate.content.constraints.transaction_costs_ref.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("portfolio_execution_source"))?;
        let settings: NativeSimulationSettingsV1 =
            serde_json::from_value(assumption.try_get("settings")?)
                .map_err(|_| StoreError::Integrity)?;
        let costs = mandate.content.constraints.transaction_costs_ref;
        // These are declared native model parameters, not market observations.
        // Their original fee/data bindings are owned by execution_assumption_sources.
        let cost_bytes: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='PARAMETERS' AND schema_name='qz.native_simulation_settings' AND schema_version='1' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1' AND access_class='RESEARCH' AND origin='SYNTHETIC'")
            .bind(costs.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
        let cost_bytes = counter(cost_bytes)?;
        if cost_bytes == DbCounter::ZERO || cost_bytes.get() > 1024 * 1024 {
            return Err(StoreError::Integrity);
        }
        let original = read(costs, cost_bytes).await?;
        let original_settings: NativeSimulationSettingsV1 =
            serde_json::from_slice(&original).map_err(|_| StoreError::Integrity)?;
        if original.len() as u64 != cost_bytes.get()
            || db::json(&original_settings)? != db::json(&settings)?
        {
            return Err(StoreError::Integrity);
        }
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            db::id(assumption.try_get("input_set_id")?)?,
            project,
            request.runtime_id,
        )
        .await?;
        domain::portfolio::simulation_settings(&settings)?;
        domain::catalogs::execution_fees(&dataset.metadata, &settings)?;
        let NativeModelRefV1::NautilusDefaultFill {
            parameters: fill, ..
        } = &settings.fill_model
        else {
            return Err(StoreError::Integrity);
        };
        if fill.prob_slippage.is_positive()
            && cap
                .engine_versions
                .get("portfolio-slippage")
                .map(String::as_str)
                != Some("1")
        {
            return Err(DomainError::CapabilityUnavailable("portfolio_slippage").into());
        }
        if assumption.try_get::<String, _>("cost_assumption_status")? != "CONSERVATIVE_ASSUMPTION"
            || settings.base_currency != mandate.content.base_currency
            || settings.starting_capital != mandate.content.capital_assumption
        {
            return Err(DomainError::CapabilityUnavailable("portfolio_all_in_cost_source").into());
        }
        let rolling = crate::execution_assumptions::liquidity::rolling(
            &mut tx,
            project,
            request.runtime_id,
            mandate.content.execution_assumptions_id,
            &mut read,
        )
        .await?;
        if let Some((_, input)) = &rolling {
            if cap
                .engine_versions
                .get("portfolio-build-rolling")
                .map(String::as_str)
                != Some("1")
            {
                return Err(DomainError::CapabilityUnavailable("portfolio_build_rolling").into());
            }
            inputs.push(input.clone());
        }
        let liquidity = if rolling.is_none() {
            crate::execution_assumptions::liquidity::frozen(
                &mut tx,
                project,
                request.runtime_id,
                mandate.content.execution_assumptions_id,
                &mut read,
            )
            .await?
        } else {
            None
        };
        if let Some(source) = &liquidity {
            if cap
                .engine_versions
                .get("portfolio-liquidity")
                .map(String::as_str)
                != Some("1")
            {
                return Err(DomainError::CapabilityUnavailable("portfolio_liquidity").into());
            }
            origin = crate::data_validation::combine_origin(origin, source.origin);
            inputs.push(source.input.clone());
        }
        let assets = weights
            .weights
            .iter()
            .zip(groups)
            .map(|(w, groups)| {
                let rate = settings
                    .fee_rates
                    .iter()
                    .find(|r| r.instrument_id == w.instrument_id)
                    .ok_or(StoreError::Invalid("portfolio_cost_asset"))?;
                Ok(AllocationAssetV1 {
                    instrument_id: w.instrument_id.clone(),
                    currency: w.currency.clone(),
                    current_weight: w.weight.clone(),
                    transaction_cost_rate: rate.taker.clone(),
                    available_notional: liquidity
                        .as_ref()
                        .map(|source| {
                            source
                                .report
                                .datasets
                                .iter()
                                .find(|q| {
                                    q.dataset_revision_id
                                        == source.binding.source.dataset_revision_id
                                })
                                .and_then(|q| q.last_bar_notionals.as_ref())
                                .and_then(|values| {
                                    values.iter().find(|v| v.instrument_id == w.instrument_id)
                                })
                                .map(|v| v.notional_value.clone())
                                .ok_or(StoreError::Invalid("portfolio_liquidity_asset"))
                        })
                        .transpose()?,
                    groups,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let native = NativePortfolioBuildRequestV1 {
            schema_version: SchemaV1,
            selection: dataset.selection.selection,
            mandate: mandate.content,
            current_weights_artifact_id: weights_id,
            current_weights: weights,
            execution_settings: settings,
            assets,
            bar_liquidity: liquidity.as_ref().map(|s| s.binding.clone()),
            rolling_liquidity: rolling.as_ref().map(|(policy, _)| policy.clone()),
            members,
        };
        domain::execution::portfolio_build_request(&native)?;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: costs,
            storage_version: "1".into(),
            byte_count: cost_bytes,
            role: ArtifactInputRole::Parameters,
        });
        if let Some(source) = &liquidity {
            domain::execution::portfolio_build_liquidity(&native, &source.report)?;
        }
        let target_until = publication::target_window(
            native.selection.decision_cutoff_ns.get(),
            native.mandate.rebalance_schedule.target_ttl_seconds,
        )?
        .1;
        let task = NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: dataset.selection.dataset_revision_id,
            request: Box::new(native),
        };
        if image != assumption.try_get::<String, _>("engine_image_ref")? {
            return Err(DomainError::CapabilityUnavailable("portfolio_frozen_image").into());
        }
        let schemas = task.output_schemas();
        if !schemas.iter().all(|s| {
            cap.artifact_schemas
                .iter()
                .any(|v| v.name == s.name && v.version == s.version)
        }) {
            return Err(DomainError::CapabilityUnavailable("portfolio_outputs").into());
        }
        let cpu = experiment::native_cpu(&request.limits, &cap)?;
        let capability: uuid::Uuid = sqlx::query_scalar(
            "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
        )
        .bind(request.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let parameter = Id::new();
        if let Some(object) = derived {
            let id = object.id;
            let size = counter(object.bytes.len() as i64)?;
            publish(object).await?;
            sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.portfolio_current_weights','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY',$5,'OPERATOR','REFERENCED')")
                .bind(id.as_uuid()).bind(project.as_uuid()).bind(id.to_string()).bind(size.get() as i64).bind(db::code(&origin)?).execute(&mut *tx).await?;
        }
        let bytes = serde_json::to_vec(&task).map_err(|_| StoreError::Integrity)?;
        let size = counter(bytes.len() as i64)?;
        publish(NativeObjectPublication {
            id: parameter,
            bytes,
        })
        .await?;
        // Recheck current source eligibility after potentially slow publication.
        for member in &request.members {
            member_source(
                &mut tx,
                project,
                policy,
                request.runtime_id,
                &image,
                member,
                &mut read,
            )
            .await?;
        }
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            request.input_set_id,
            project,
            request.runtime_id,
        )
        .await?;
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            db::id(assumption.try_get("input_set_id")?)?,
            project,
            request.runtime_id,
        )
        .await?;
        let checked = now(&mut tx).await?;
        let checked_ns = u64::try_from(checked.timestamp_nanos_opt().ok_or(StoreError::Integrity)?)
            .map_err(|_| StoreError::Integrity)?;
        if weights_deadline.get() <= checked_ns {
            return Err(StoreError::Invalid("portfolio_weights_expired"));
        }
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY',$5,'OPERATOR','REFERENCED')")
            .bind(parameter.as_uuid()).bind(project.as_uuid()).bind(parameter.to_string()).bind(size.get() as i64).bind(db::code(&origin)?).execute(&mut *tx).await?;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: parameter,
            storage_version: "1".into(),
            byte_count: size,
            role: ArtifactInputRole::Parameters,
        });
        let mut artifacts = BTreeSet::new();
        inputs.retain(|i| match i {
            RuntimeInputV1::Artifact { artifact_id, .. } => artifacts.insert(*artifact_id),
            _ => true,
        });
        let (mut tx, run) = Store::enqueue_run_in_transaction(
            tx,
            &format!("portfolio-build/{}", Id::new()),
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
                parameters_artifact_id: parameter,
                inputs,
                image_ref: image,
                cpu,
                capability_snapshot_artifact_id: db::id(capability)?,
                output_schemas: schemas,
                origin,
                access: ArtifactAccess::EvaluatorOnly,
            },
        )
        .await?;
        let (snapshot, candidate) = match request.current_weights_source {
            PortfolioBuildWeightsV1::ForwardSnapshot { snapshot_id } => {
                (Some(snapshot_id.as_uuid()), None)
            }
            PortfolioBuildWeightsV1::LastTarget { candidate_id } => {
                (None, Some(candidate_id.as_uuid()))
            }
        };
        sqlx::query("INSERT INTO app.portfolio_build_tasks(run_id,mandate_id,snapshot_id,last_target_candidate_id,request) VALUES($1,$2,$3,$4,$5)")
            .bind(run.resource.id.as_uuid()).bind(request.mandate_id.as_uuid()).bind(snapshot).bind(candidate).bind(db::json(request)?).execute(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        if !publication::windows_current(
            &mut tx,
            request,
            &[db::id(assumption.try_get("input_set_id")?)?],
            weights_deadline,
            target_until,
        )
        .await?
        {
            return Err(StoreError::Invalid("portfolio_source_expired"));
        }
        let result = commands::finish(&mut tx, prepared, run.resource, 202).await?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn member_source<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    policy: Id,
    runtime: Id,
    image: &str,
    member: &PortfolioMemberSelectionV1,
    read: &mut R,
) -> Result<(NativePortfolioAlphaV1, Vec<RuntimeInputV1>), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row = sqlx::query(MEMBER_SOURCE)
        .bind(member.qualification_id.as_uuid())
        .bind(project.as_uuid())
        .bind(policy.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::Invalid("portfolio_qualification"))?;
    let run = db::id(row.try_get("run_id")?)?;
    if row.try_get::<String, _>("runtime_image_ref")? != image {
        return Err(DomainError::CapabilityUnavailable("portfolio_alpha_image").into());
    }
    native::sealed::validate_binding(tx, run).await?;
    let context = crate::cycles::execution_context(tx, db::id(row.try_get("brief_id")?)?).await?;
    for input in [
        context.discovery_input_set_id,
        context.validation_input_set_id,
        db::id(row.try_get("input_set_id")?)?,
    ] {
        crate::research::revalidate_frozen_inputs(tx, input, project, runtime).await?;
        let real: bool = sqlx::query_scalar("SELECT count(*)>0 AND bool_and(d.origin='REAL' AND d.pit_status='VERIFIED' AND d.revision_policy='AS_KNOWN_THEN') FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id WHERE i.input_set_id=$1")
            .bind(input.as_uuid()).fetch_one(&mut **tx).await?;
        if !real {
            return Err(StoreError::Invalid("portfolio_qualification_sources"));
        }
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
    let NativeTaskParametersV1::EvaluateSealedAlpha {
        model_artifact_id,
        calibration_artifact_id,
        request,
        ..
    } = serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?
    else {
        return Err(StoreError::Integrity);
    };
    if model_artifact_id.as_uuid() != row.try_get::<uuid::Uuid, _>("model_artifact_id")? {
        return Err(StoreError::Integrity);
    }
    let mut inputs = vec![];
    for id in std::iter::once(model_artifact_id).chain(calibration_artifact_id) {
        let artifact = sqlx::query("SELECT byte_count,storage_version FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='MODEL' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND access_class IN ('RESEARCH','EVALUATOR_ONLY')")
            .bind(id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Integrity)?;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: artifact.try_get("storage_version")?,
            byte_count: counter(artifact.try_get("byte_count")?)?,
            role: ArtifactInputRole::Model,
        });
    }
    Ok((
        NativePortfolioAlphaV1 {
            alpha_id: db::id(row.try_get("alpha_id")?)?,
            alpha_version_id: db::id(row.try_get("alpha_version_id")?)?,
            model_artifact_id,
            calibration_artifact_id,
            target_kind: request.target_kind,
            ensemble_weight: member.ensemble_weight.clone(),
            parameters: request.forecast.parameters,
        },
        inputs,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;

    #[sqlx::test(migrations = "../../migrations")]
    async fn postgres_prepares_original_qualification_selection_without_forging_evidence(
        pool: sqlx::PgPool,
    ) {
        pool.prepare(MEMBER_SOURCE).await.unwrap();
    }
}
