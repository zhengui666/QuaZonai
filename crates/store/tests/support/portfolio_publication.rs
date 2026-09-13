//! Actual PG/PGMQ, cancellation and object publication. Source rows are controlled
//! relationship fixtures; they do not claim to pass scientific admission or REAL qualification.
use super::*;
#[path = "../../../../tests/support/portfolio.rs"]
mod numerical;
#[path = "../../../../tests/support/runtime_observation.rs"]
mod observation;
use contracts::{
    execution::NativeTaskParametersV1, forward::ForwardEnvironmentV1, lifecycle::*, portfolio::*,
    runs::RunKind, science::PortfolioWeightsSourceV1, DbCounter, Revision, SchemaV1,
};
use integrations::artifacts::ArtifactStore;
use serde_json::json;
use sqlx::Row;
use std::sync::{Arc, Mutex};
use store::{lifecycle::RunSubmission, StoreError};

async fn prepared(pool: &PgPool, objects: &ArtifactStore, cancel: bool) -> (store::Store, Id) {
    let (store, operator) = research_support::operator(pool).await;
    let f = fixture(pool, budget()).await;
    let first = alpha(pool, &f).await;
    let second = alpha(pool, &f).await;
    let runtime = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'controlled runtime','https://runtime.example','SYSTEM_CA','fixture',ARRAY['PORTFOLIO_BUILD'],'1',true)")
        .bind(runtime.as_uuid()).execute(pool).await.unwrap();
    observation::ready(pool, runtime).await;
    let capability: uuid::Uuid = sqlx::query_scalar(
        "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
    )
    .bind(runtime.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: DbCounter::new(100).unwrap(),
        wall_seconds: 300,
        memory_mib: 1024,
        output_bytes: DbCounter::new(65_536).unwrap(),
    };
    let run = store
        .enqueue_run(
            "cancelled-build",
            &RunSubmission {
                cycle_id: f.cycle,
                input_set_id: f.input_set,
                runtime_id: runtime,
                runtime_revision: Revision::INITIAL,
                kind: RunKind::PortfolioBuild,
                limits: limits.clone(),
            },
        )
        .await
        .unwrap()
        .resource;
    let allocation: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut native = numerical::request(&allocation);
    let (universe,policy,assumptions):(uuid::Uuid,uuid::Uuid,uuid::Uuid)=sqlx::query_as("SELECT universe_version_id,evaluation_policy_id,execution_assumptions_id FROM app.research_briefs WHERE project_id=$1")
        .bind(f.project.as_uuid()).fetch_one(pool).await.unwrap();
    native.mandate.universe_version_id = universe.to_string().try_into().unwrap();
    native.mandate.required_evaluation_policy_id = policy.to_string().try_into().unwrap();
    native.mandate.execution_assumptions_id = assumptions.to_string().try_into().unwrap();
    let costs_id = Id::new();
    native.mandate.constraints.transaction_costs_ref = costs_id;
    let costs = serde_json::to_vec(&native.execution_settings).unwrap();
    objects.put(costs_id, &costs).unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_simulation_settings','1','LOCAL',$3,'1',$4,'RESEARCH','SYNTHETIC','IMPORT','REFERENCED')")
        .bind(costs_id.as_uuid()).bind(f.project.as_uuid()).bind(costs_id.to_string()).bind(costs.len() as i64).execute(pool).await.unwrap();
    for (member, source) in native.members.iter_mut().zip([&first, &second]) {
        let alpha: uuid::Uuid =
            sqlx::query_scalar("SELECT alpha_id FROM app.alpha_versions WHERE id=$1")
                .bind(source.version.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
        member.alpha_id = alpha.to_string().try_into().unwrap();
        member.alpha_version_id = source.version;
        member.model_artifact_id = f.artifact;
    }
    let mandate = Id::new();
    let m = &native.mandate;
    sqlx::query("INSERT INTO app.portfolio_mandates(id,project_id,version,objective,risk_measure,base_currency,capital_assumption,universe_version_id,covariance_estimator,alpha_ensemble,optimizer,constraints,rebalance_schedule,required_evaluation_policy_id,execution_assumptions_id,exposure_tolerance) VALUES($1,$2,1,'MIN_RISK','VARIANCE',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(mandate.as_uuid()).bind(f.project.as_uuid()).bind(&m.base_currency).bind(m.capital_assumption.as_decimal()).bind(universe)
        .bind(json!(m.covariance_estimator)).bind(json!(m.alpha_ensemble)).bind(json!(m.optimizer)).bind(json!(m.constraints)).bind(json!(m.rebalance_schedule)).bind(policy).bind(assumptions).bind(m.exposure_tolerance.as_decimal()).execute(pool).await.unwrap();
    let downstream = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'controlled paper source','https://downstream.example','fixture',ARRAY['1'],'PAPER',true)")
        .bind(downstream.as_uuid()).execute(pool).await.unwrap();
    let snapshot = Id::new();
    native.current_weights.source = PortfolioWeightsSourceV1::ForwardSnapshot {
        downstream_id: downstream,
        external_message_id: "controlled-message".into(),
    };
    let weights = serde_json::to_vec(&native.current_weights).unwrap();
    let weights_id = native.current_weights_artifact_id;
    objects.put(weights_id, &weights).unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.portfolio_current_weights','1','LOCAL',$3,'1',$4,'RESEARCH','SYNTHETIC','IMPORT','REFERENCED')")
        .bind(weights_id.as_uuid()).bind(f.project.as_uuid()).bind(weights_id.to_string()).bind(weights.len() as i64).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.forward_weight_snapshots(id,project_id,downstream_id,environment,external_message_id,report_artifact_id,content) VALUES($1,$2,$3,'PAPER','controlled-message',$4,$5)")
        .bind(snapshot.as_uuid()).bind(f.project.as_uuid()).bind(downstream.as_uuid()).bind(weights_id.as_uuid()).bind(json!(native.current_weights)).execute(pool).await.unwrap();
    let request = PortfolioBuildRequestV1 {
        schema_version: SchemaV1,
        cycle_id: f.cycle,
        mandate_id: mandate,
        input_set_id: f.input_set,
        runtime_id: runtime,
        expected_runtime_revision: Revision::INITIAL,
        current_weights_source: PortfolioBuildWeightsV1::ForwardSnapshot {
            snapshot_id: snapshot,
        },
        environment: ForwardEnvironmentV1::Paper,
        members: vec![
            PortfolioMemberSelectionV1 {
                qualification_id: first.qualification,
                ensemble_weight: native.members[0].ensemble_weight.clone(),
            },
            PortfolioMemberSelectionV1 {
                qualification_id: second.qualification,
                ensemble_weight: native.members[1].ensemble_weight.clone(),
            },
        ],
        limits,
    };
    let task = NativeTaskParametersV1::BuildPortfolio {
        schema_version: SchemaV1,
        dataset_revision_id: Id::new(),
        request: Box::new(native),
    };
    let parameter = Id::new();
    let bytes = serde_json::to_vec(&task).unwrap();
    objects.put(parameter, &bytes).unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','FIXTURE','OPERATOR','REFERENCED')")
        .bind(parameter.as_uuid()).bind(f.project.as_uuid()).bind(parameter.to_string()).bind(bytes.len() as i64).execute(pool).await.unwrap();
    let mut inputs = vec![contracts::runtime_jobs::RuntimeInputV1::Artifact {
        artifact_id: weights_id,
        storage_version: "1".into(),
        byte_count: DbCounter::new(weights.len() as u64).unwrap(),
        role: contracts::research::ArtifactInputRole::Report,
    }];
    if !cancel {
        // Structural source bindings only: this fixture deliberately does not
        // dispatch or claim native_input / scientific admission validation.
        let NativeTaskParametersV1::BuildPortfolio {
            dataset_revision_id,
            request,
            ..
        } = &task
        else {
            unreachable!()
        };
        inputs.extend([
            contracts::runtime_jobs::RuntimeInputV1::Artifact {
                artifact_id: costs_id,
                storage_version: "1".into(),
                byte_count: DbCounter::new(costs.len() as u64).unwrap(),
                role: contracts::research::ArtifactInputRole::Parameters,
            },
            contracts::runtime_jobs::RuntimeInputV1::Dataset {
                revision_id: *dataset_revision_id,
                registered_ref: "controlled-not-dispatched".into(),
                storage_version: "1".into(),
                role: contracts::research::DataPartition::Forward,
            },
            contracts::runtime_jobs::RuntimeInputV1::Artifact {
                artifact_id: request.members[0].model_artifact_id,
                storage_version: "1".into(),
                byte_count: DbCounter::new(1).unwrap(),
                role: contracts::research::ArtifactInputRole::Model,
            },
            contracts::runtime_jobs::RuntimeInputV1::Artifact {
                artifact_id: parameter,
                storage_version: "1".into(),
                byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
                role: contracts::research::ArtifactInputRole::Parameters,
            },
        ]);
    }
    sqlx::query("INSERT INTO app.run_native_tasks(run_id,parameters_artifact_id,input_bindings,image_ref,cpu,capability_snapshot_artifact_id,output_schemas,origin,access_class) VALUES($1,$2,$5,'example.invalid/fixture@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',1,$3,$4,'FIXTURE','EVALUATOR_ONLY')")
        .bind(run.id.as_uuid()).bind(parameter.as_uuid()).bind(capability).bind(json!(task.output_schemas())).bind(json!(inputs)).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.portfolio_build_tasks(run_id,mandate_id,snapshot_id,request) VALUES($1,$2,$3,$4)")
        .bind(run.id.as_uuid()).bind(mandate.as_uuid()).bind(snapshot.as_uuid()).bind(json!(request)).execute(pool).await.unwrap();
    if cancel {
        store
            .cancel_run(
                &operator,
                "cancel-controlled-build",
                run.id,
                &RunCancelV1 {
                    schema_version: SchemaV1,
                    expected_revision: run.revision,
                },
            )
            .await
            .unwrap();
    }
    (store, run.id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_build_publication_rolls_back_then_replays_one_targetless_candidate(
    pool: PgPool,
) {
    let directory = tempfile::tempdir().unwrap();
    let objects = Arc::new(ArtifactStore::open(&directory.path().join("objects")).unwrap());
    let (store, run) = prepared(&pool, &objects, true).await;
    let message = store::lifecycle::RunMessage {
        message_id: sqlx::query_scalar(
            "SELECT msg_id FROM pgmq.q_runs WHERE message->>'run_id'=$1",
        )
        .bind(run.to_string())
        .fetch_one(&pool)
        .await
        .unwrap(),
        run_id: run,
        read_count: 1,
    };
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let allocated = Mutex::new(None);
    let failed = store
        .publish_scientific_result(
            run,
            |id, size| {
                let objects = objects.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            |object| {
                *allocated.lock().unwrap() = Some(object.id);
                let objects = objects.clone();
                async move {
                    objects.put(object.id, &object.bytes).unwrap();
                    Err(StoreError::Integrity)
                }
            },
        )
        .await;
    assert!(matches!(failed, Err(StoreError::Integrity)));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.portfolio_candidates WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let orphan = allocated.into_inner().unwrap().unwrap();
    let cleanup = objects.clone();
    assert!(store
        .discard_unpublished_native_object(run, orphan, move |id| async move {
            cleanup
                .discard_unpublished(id)
                .map_err(|_| StoreError::Integrity)
        })
        .await
        .unwrap());
    let publish = || {
        store.publish_scientific_result(
            run,
            |id, size| {
                let objects = objects.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            |object| {
                let objects = objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
    };
    let (first, second) = tokio::join!(publish(), publish());
    let (first, second) = (first.unwrap().unwrap(), second.unwrap().unwrap());
    assert_eq!(first.resource, second.resource);
    assert_ne!(first.replayed, second.replayed);
    let replay = store
        .publish_scientific_result(
            run,
            |_, _| async { panic!("replay must not read files") },
            |_| async { panic!("replay must not publish") },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    let (state,status,reason,targets,members,diagnostics):(String,String,String,i64,i64,uuid::Uuid)=sqlx::query_as("SELECT c.solver_status,c.evidence_status,c.reason_code,(SELECT count(*) FROM app.candidate_targets t WHERE t.candidate_id=c.id),(SELECT count(*) FROM app.candidate_alphas m WHERE m.candidate_id=c.id),c.diagnostics_artifact_id FROM app.portfolio_candidates c WHERE c.id=$1 AND c.target_artifact_id IS NULL AND c.cash_weight IS NULL")
        .bind(first.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        (
            state.as_str(),
            status.as_str(),
            reason.as_str(),
            targets,
            members
        ),
        ("FAILED", "INCOMPLETE", "PORTFOLIO_CANCELLED", 0, 2)
    );
    let bytes: i64 =
        sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND origin='FIXTURE'")
            .bind(diagnostics)
            .fetch_one(&pool)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(
        &objects
            .read(
                diagnostics.to_string().try_into().unwrap(),
                DbCounter::new(bytes as u64).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(report["execution_status"], "CANCELLED");
    assert!(report["target_artifact_id"].is_null());
    store.acknowledge_run(&message).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn successful_original_report_with_expired_target_retains_solver_but_cannot_deliver(
    pool: PgPool,
) {
    use contracts::{runtime_jobs::*, science::NativePortfolioBuildResultV1};
    use store::lifecycle::{native::NativePayloads, ClaimResult, RunMessage};
    let directory = tempfile::tempdir().unwrap();
    let objects = ArtifactStore::open(&directory.path().join("objects")).unwrap();
    let (store, run) = prepared(&pool, &objects, false).await;
    let message = RunMessage {
        message_id: sqlx::query_scalar(
            "SELECT msg_id FROM pgmq.q_runs WHERE message->>'run_id'=$1",
        )
        .bind(run.to_string())
        .fetch_one(&pool)
        .await
        .unwrap(),
        run_id: run,
        read_count: 1,
    };
    let ClaimResult::Leased(lease) = store
        .claim_run(&message, "controlled-publication", 60)
        .await
        .unwrap()
    else {
        panic!("expected lease")
    };
    let definition = sqlx::query("SELECT t.*,a.limits FROM app.run_native_tasks t JOIN app.run_admissions a ON a.run_id=t.run_id WHERE t.run_id=$1")
        .bind(run.as_uuid()).fetch_one(&pool).await.unwrap();
    let limits: JobLimitsV1 = serde_json::from_value(definition.get("limits")).unwrap();
    let parameter: uuid::Uuid = definition.get("parameters_artifact_id");
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: lease.run.current_attempt_no,
        owner_epoch: lease.fence.owner_epoch,
        external_job_id: domain::runtime_jobs::external_id(run, lease.run.current_attempt_no)
            .unwrap(),
        job_kind: RunKind::PortfolioBuild,
        image_ref: definition.get("image_ref"),
        input_set_id: lease.run.input_set_id,
        inputs: serde_json::from_value(definition.get("input_bindings")).unwrap(),
        parameters_artifact_id: parameter.to_string().try_into().unwrap(),
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: limits.cpu_seconds,
            memory_mib: limits.memory_mib,
            wall_seconds: limits.wall_seconds,
            output_bytes: limits.output_bytes,
        },
        deadline_at: lease.run.deadline_at,
        requested_output_schemas: serde_json::from_value(definition.get("output_schemas")).unwrap(),
    };
    // Frozen protocol fixture, not a successful native_job/input transfer. We
    // test the real terminal adoption/publication boundary with controlled output.
    domain::runtime_jobs::spec_shape(&spec).unwrap();
    sqlx::query(
        "INSERT INTO app.run_native_attempts(attempt_id,run_id,spec_json) VALUES($1,$2,$3)",
    )
    .bind(lease.fence.attempt_id.as_uuid())
    .bind(run.as_uuid())
    .bind(json!(spec))
    .execute(&pool)
    .await
    .unwrap();
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(parameter)
        .fetch_one(&pool)
        .await
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &objects
            .read(
                spec.parameters_artifact_id,
                DbCounter::new(size as u64).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    domain::execution::task(&spec, &task).unwrap();
    let NativeTaskParametersV1::BuildPortfolio { request, .. } = task else {
        unreachable!()
    };
    let mut input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    input.constraints = request.mandate.constraints.clone();
    for (actual, original) in input.forecasts.members.iter_mut().zip(&request.members) {
        actual.alpha_id = original.alpha_id;
        actual.alpha_version_id = original.alpha_version_id;
    }
    let report = NativePortfolioBuildResultV1 {
        schema_version: SchemaV1,
        allocation: AllocationResultV1 {
            schema_version: SchemaV1,
            solver_status: SolverStatus::Optimal,
            reason_code: None,
            targets: Some(request.current_weights.weights.clone()),
            cash_weight: Some(request.current_weights.cash_weight.clone()),
            iterations: 1,
            cvar_risk_budget_witness: None,
            objective_value: Some(0.625),
            primal_residual: Some(0.0),
            dual_residual: Some(0.0),
        },
        input,
        consumed_fuel: DbCounter::ZERO,
    };
    domain::execution::portfolio_build_result(&request, &report).unwrap();
    assert!(store.begin_run_dispatch(run, &lease.fence).await.unwrap());
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let bytes = serde_json::to_vec(&report).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::Report,
        schema: spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        media_type: "application/json".into(),
    };
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: spec.attempt_no,
        external_job_id: spec.external_job_id.clone(),
        input_set_id: spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: [
            ("controlled-fixture".into(), "1".into()),
            ("portfolio-cost-source".into(), "1".into()),
        ]
        .into(),
        started_at: Some(now),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::ZERO,
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: output.byte_count,
        },
        artifacts: vec![output.clone()],
        error: None,
    };
    let objects = &objects;
    store
        .publish_native_result(
            run,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            NativePayloads::Verified(vec![(output, bytes)]),
            |id, size| async move { objects.read(id, size).map_err(|_| StoreError::Integrity) },
            |batch| async move {
                for object in batch {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)?;
                }
                Ok(())
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let native_report: uuid::Uuid = sqlx::query_scalar("SELECT a.id FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.schema_name='qz.native_portfolio'")
        .bind(lease.fence.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
    let mut mismatched = report.clone();
    mismatched.input.forecasts.members[0].alpha_version_id = Id::new();
    let mismatched = serde_json::to_vec(&mismatched).unwrap();
    let rejected = store
        .publish_scientific_result(
            run,
            |id, size| {
                let mismatched = &mismatched;
                async move {
                    if id.as_uuid() == native_report {
                        Ok(mismatched.clone())
                    } else {
                        objects.read(id, size).map_err(|_| StoreError::Integrity)
                    }
                }
            },
            |_| async { panic!("mismatched original report must fail before publication") },
        )
        .await;
    assert!(matches!(rejected, Err(StoreError::Integrity)));
    let candidates: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.portfolio_candidates WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(candidates, 0);
    let published = store
        .publish_scientific_result(
            run,
            |id, size| async move { objects.read(id, size).map_err(|_| StoreError::Integrity) },
            |object| async move {
                objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap()
        .unwrap();
    let row = sqlx::query("SELECT c.*,r.state AS execution_state FROM app.portfolio_candidates c JOIN app.runs r ON r.id=c.run_id WHERE c.id=$1")
        .bind(published.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("execution_state"), "SUCCEEDED");
    assert_eq!(row.get::<String, _>("solver_status"), "OPTIMAL");
    assert_eq!(row.get::<String, _>("evidence_status"), "INVALID");
    assert_eq!(
        row.get::<String, _>("reason_code"),
        "PORTFOLIO_SOURCE_NO_LONGER_ELIGIBLE"
    );
    assert!(row
        .get::<Option<uuid::Uuid>, _>("forecast_artifact_id")
        .is_some());
    assert!(row
        .get::<Option<uuid::Uuid>, _>("target_artifact_id")
        .is_none());
    assert!(row
        .get::<Option<bigdecimal::BigDecimal>, _>("cash_weight")
        .is_none());
    let targets: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.candidate_targets WHERE candidate_id=$1")
            .bind(published.resource.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(targets, 0);
    store.acknowledge_run(&message).await.unwrap();
}
