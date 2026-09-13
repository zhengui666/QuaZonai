//! Original Candidate hold admission; success is not Evaluation or Release authority.
use super::*;

impl Store {
    pub async fn start_candidate_simulation<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &CandidateSimulationRequestV1,
        mut read: R,
        mut publish: P,
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
        let row = sqlx::query("SELECT c.project_id,c.mandate_id,b.request,t.parameters_artifact_id,t.image_ref FROM app.portfolio_candidates c JOIN app.portfolio_build_tasks b ON b.run_id=c.run_id AND b.mandate_id=c.mandate_id JOIN app.run_native_tasks t ON t.run_id=b.run_id WHERE c.id=$1")
            .bind(request.candidate_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let project = db::id(row.try_get("project_id")?)?;
        crate::research::project_for_write(&mut tx, project).await?;
        let original: PortfolioBuildRequestV1 =
            serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
        let mandate =
            sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2")
                .bind(row.try_get::<uuid::Uuid, _>("mandate_id")?)
                .bind(project.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let mandate = crate::portfolio::view(&mandate)?;
        let policy: uuid::Uuid = sqlx::query_scalar("SELECT b.evaluation_policy_id FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN' WHERE c.id=$1 AND c.project_id=$2 AND c.state='RUNNING' FOR UPDATE OF c")
            .bind(request.cycle_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("candidate_simulation_cycle"))?;
        if policy != mandate.content.required_evaluation_policy_id.as_uuid()
            || original.runtime_id != request.runtime_id
            || original.mandate_id != mandate.id
        {
            return Err(StoreError::Invalid("candidate_simulation_source"));
        }
        let source = weights::target(&mut tx, project, request.candidate_id, &mut read).await?;
        let bytes = validation::read_document(
            &mut tx,
            db::id(row.try_get("parameters_artifact_id")?)?,
            None,
            "qz.native_task",
            8 * 1024 * 1024,
            &mut read,
        )
        .await?;
        let NativeTaskParametersV1::BuildPortfolio {
            request: frozen, ..
        } = serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?
        else {
            return Err(StoreError::Integrity);
        };
        if frozen.mandate != mandate.content {
            return Err(StoreError::Integrity);
        }
        let cap = crate::runtime::require_capabilities(
            &mut tx,
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
            .ok_or(DomainError::CapabilityUnavailable(
                "candidate_simulation_image",
            ))?
            .image_ref
            .clone();
        if image != row.try_get::<String, _>("image_ref")?
            || cap
                .engine_versions
                .get("candidate-simulation")
                .map(String::as_str)
                != Some("1")
            || cap
                .engine_versions
                .get("simulation-models")
                .map(String::as_str)
                != Some("1")
        {
            return Err(DomainError::CapabilityUnavailable("candidate_simulation_source").into());
        }
        let mut datasets = crate::data_validation::dataset_bindings(
            &mut tx,
            request.input_set_id,
            project,
            request.runtime_id,
            &[DataPartition::Forward],
            &mut read,
        )
        .await?;
        if datasets.len() != 1 {
            return Err(StoreError::Invalid("candidate_simulation_dataset"));
        }
        let dataset = datasets.remove(0);
        if dataset.origin != DataOrigin::Real
            || dataset.metadata.pit_status != contracts::research::PitStatus::Verified
            || dataset.metadata.revision_policy
                != contracts::catalogs::DataRevisionPolicy::AsKnownThen
        {
            return Err(StoreError::Invalid("candidate_simulation_real_pit"));
        }
        let universe: uuid::Uuid =
            sqlx::query_scalar("SELECT universe_version_id FROM app.dataset_revisions WHERE id=$1")
                .bind(dataset.selection.dataset_revision_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if universe != mandate.content.universe_version_id.as_uuid() {
            return Err(StoreError::Invalid("candidate_simulation_universe"));
        }
        let costs = mandate.content.constraints.transaction_costs_ref;
        let (settings, assumption_inputs, assumption_image): (Value, uuid::Uuid, String) = sqlx::query_as("SELECT s.settings,s.input_set_id,e.engine_image_ref FROM app.execution_assumption_sources s JOIN app.execution_assumptions e ON e.id=s.assumptions_id AND e.fee_schedule_artifact_id=$4 WHERE s.assumptions_id=$1 AND s.project_id=$2 AND s.runtime_id=$3")
            .bind(mandate.content.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(request.runtime_id.as_uuid()).bind(costs.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
        let cost_bytes = crate::execution_assumptions::liquidity::document(
            &mut tx,
            project,
            costs,
            "qz.native_simulation_settings",
            1024 * 1024,
            &mut read,
        )
        .await?;
        let settings: NativeSimulationSettingsV1 =
            serde_json::from_value(settings).map_err(|_| StoreError::Integrity)?;
        let document: NativeSimulationSettingsV1 =
            serde_json::from_slice(&cost_bytes).map_err(|_| StoreError::Integrity)?;
        if db::json(&document)? != db::json(&settings)?
            || db::json(&settings)? != db::json(&frozen.execution_settings)?
            || image != assumption_image
        {
            return Err(StoreError::Integrity);
        }
        domain::catalogs::execution_fees(&dataset.metadata, &settings)?;
        let until = counter(
            source
                .document
                .valid_until
                .timestamp_nanos_opt()
                .ok_or(StoreError::Integrity)?,
        )?;
        let mut selection = dataset.selection.selection;
        if selection.event_start_ns > source.available_ns {
            return Err(StoreError::Invalid("candidate_simulation_start_coverage"));
        }
        selection.event_start_ns = source.available_ns;
        selection.event_end_ns = selection.event_end_ns.min(until);
        let native = NativeSimulationRequestV1 {
            schema_version: SchemaV1,
            selection,
            settings,
            target_points: vec![NativeTargetPointV1 {
                schema_version: SchemaV1,
                asof_ns: source.available_ns,
                valid_until_ns: until,
                targets: source.document.targets.clone(),
                cash_weight: source.document.cash_weight.clone(),
            }],
        };
        domain::execution::candidate_simulation(
            request.candidate_id,
            source.available_ns,
            &source.document,
            &native,
        )?;
        let task = NativeTaskParametersV1::SimulateCandidate {
            schema_version: SchemaV1,
            candidate_id: request.candidate_id,
            candidate_available_ns: source.available_ns,
            dataset_revision_id: dataset.selection.dataset_revision_id,
            target_artifact_id: match &source.input {
                RuntimeInputV1::Artifact { artifact_id, .. } => *artifact_id,
                _ => return Err(StoreError::Integrity),
            },
            settings_artifact_id: costs,
            request: Box::new(native),
        };
        let schemas = task.output_schemas();
        if !schemas.iter().all(|s| {
            cap.artifact_schemas
                .iter()
                .any(|v| v.name == s.name && v.version == s.version)
        }) {
            return Err(DomainError::CapabilityUnavailable("candidate_simulation_outputs").into());
        }
        let cpu = experiment::native_cpu(&request.limits, &cap)?;
        let capability: uuid::Uuid = sqlx::query_scalar(
            "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
        )
        .bind(request.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let parameter = Id::new();
        let bytes = serde_json::to_vec(&task).map_err(|_| StoreError::Integrity)?;
        let size = counter(bytes.len() as i64)?;
        publish(NativeObjectPublication {
            id: parameter,
            bytes,
        })
        .await?;
        for member in &original.members {
            member_source(
                &mut tx,
                project,
                mandate.content.required_evaluation_policy_id,
                request.runtime_id,
                &image,
                member,
                &mut read,
            )
            .await?;
        }
        for input in [
            request.input_set_id,
            original.input_set_id,
            db::id(assumption_inputs)?,
        ] {
            crate::research::revalidate_frozen_inputs(&mut tx, input, project, request.runtime_id)
                .await?;
        }
        let reread = weights::target(&mut tx, project, request.candidate_id, &mut read).await?;
        if db::json(&reread.document)? != db::json(&source.document)?
            || reread.available_ns != source.available_ns
            || crate::execution_assumptions::liquidity::document(
                &mut tx,
                project,
                costs,
                "qz.native_simulation_settings",
                1024 * 1024,
                &mut read,
            )
            .await?
                != cost_bytes
        {
            return Err(StoreError::Integrity);
        }
        let checked = now(&mut tx).await?;
        if checked >= source.document.valid_until {
            return Err(StoreError::Invalid("candidate_simulation_expired"));
        }
        let origin = crate::data_validation::combine_origin(source.origin, dataset.origin);
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY',$5,'OPERATOR','REFERENCED')")
            .bind(parameter.as_uuid()).bind(project.as_uuid()).bind(parameter.to_string()).bind(size.get() as i64).bind(db::code(&origin)?).execute(&mut *tx).await?;
        let inputs = vec![
            dataset.input,
            source.input,
            RuntimeInputV1::Artifact {
                artifact_id: costs,
                storage_version: "1".into(),
                byte_count: counter(cost_bytes.len() as i64)?,
                role: ArtifactInputRole::Parameters,
            },
            RuntimeInputV1::Artifact {
                artifact_id: parameter,
                storage_version: "1".into(),
                byte_count: size,
                role: ArtifactInputRole::Parameters,
            },
        ];
        let (mut tx, run) = Store::enqueue_run_in_transaction(
            tx,
            &format!("candidate-simulate/{}", Id::new()),
            &RunSubmission {
                cycle_id: request.cycle_id,
                input_set_id: request.input_set_id,
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
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        if !publication::windows_current(
            &mut tx,
            &original,
            db::id(assumption_inputs)?,
            until,
            source.document.valid_until,
        )
        .await?
        {
            return Err(StoreError::Invalid("candidate_simulation_source_expired"));
        }
        let result = commands::finish(&mut tx, prepared, run.resource, 202).await?;
        tx.commit().await?;
        Ok(result)
    }
}
