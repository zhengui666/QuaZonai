//! Controlled original-protocol reports, not actual solver execution or market evidence.
use super::*;
use contracts::{portfolio::*, runtime_jobs::*, science::*};
use store::lifecycle::native::{NativeJob, NativePayloads};

pub(super) async fn complete(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    job: &NativeJob,
    infeasible: bool,
) {
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let NativeTaskParametersV1::StudyPortfolio {
        dataset_revision_id,
        request,
        ..
    } = serde_json::from_slice(&f.read(job.spec.parameters_artifact_id, size).await.unwrap())
        .unwrap()
    else {
        panic!("original Study");
    };
    let (id,size): (uuid::Uuid,i64) = sqlx::query_as("SELECT a.id,a.byte_count FROM app.portfolio_study_tasks s JOIN app.portfolio_candidates c ON c.id=s.candidate_id JOIN app.artifacts a ON a.producer_run_id=c.run_id AND a.schema_name='qz.native_portfolio' WHERE s.run_id=$1").bind(lease.run.id.as_uuid()).fetch_one(pool).await.unwrap();
    let original: NativePortfolioBuildResultV1 = serde_json::from_slice(
        &f.read(
            id.to_string().try_into().unwrap(),
            DbCounter::new(size as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let mut frames = Vec::new();
    let cutoffs = domain::execution::portfolio_study_cutoffs(&request).unwrap();
    for cutoff in cutoffs {
        let mut input = original.input.clone();
        input.capital_assumption = request.mandate.capital_assumption.clone();
        input.current_cash_weight = "1".parse().unwrap();
        input.forecasts.decision_asof_ns = cutoff;
        let asof = DbCounter::new(cutoff.get() - 1_000_000_000).unwrap();
        input.forecasts.forecast_asof_ns = asof;
        for member in &mut input.forecasts.members {
            member.asof_ns = asof;
            member.available_ns = asof;
        }
        input.return_history.end_ns = (1..=5)
            .rev()
            .map(|i| DbCounter::new(asof.get() - i * 300_000_000_000).unwrap())
            .collect();
        input.return_history.available_ns = input.return_history.end_ns.clone();
        let mut slippage_references = original.slippage_references.clone();
        for reference in &mut slippage_references {
            reference.event_ns = asof;
            reference.available_ns = asof;
        }
        let mut bar_notionals = original.bar_notionals.clone();
        for value in &mut bar_notionals {
            value.event_ns = asof;
            value.available_ns = asof;
        }
        let mut selection = request.source_selection.clone();
        selection.event_end_ns = cutoff;
        selection.decision_cutoff_ns = cutoff;
        let assets = domain::execution::portfolio_study_liquidity_assets(
            &request,
            cutoff,
            asof,
            cutoff,
            &bar_notionals,
        )
        .unwrap();
        input.assets = domain::execution::portfolio_costs(
            &selection,
            &request.mandate,
            &request.execution_settings,
            &assets,
            &slippage_references,
        )
        .unwrap();
        let mut allocation = original.allocation.clone();
        if infeasible {
            allocation.solver_status = SolverStatus::Infeasible;
            allocation.reason_code = Some("CONTROLLED_INFEASIBLE".into());
            allocation.targets = None;
            allocation.cash_weight = None;
            allocation.objective_value = None;
        }
        frames.push(NativePortfolioStudyFrameV1 {
            cutoff_ns: cutoff,
            input,
            allocation,
            slippage_references,
            bar_notionals,
        });
        if infeasible {
            break;
        }
    }
    let simulation_request = (!infeasible).then(|| {
        let mut selection = request.source_selection.clone();
        selection.event_start_ns = request.evaluation_start_ns;
        NativeSimulationRequestV1 {
            schema_version: SchemaV1,
            selection,
            settings: request.execution_settings.clone(),
            target_points: frames
                .iter()
                .map(|frame| NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    asof_ns: frame.input.forecasts.decision_asof_ns,
                    valid_until_ns: DbCounter::new(
                        frame.cutoff_ns.get()
                            + u64::from(request.mandate.rebalance_schedule.target_ttl_seconds)
                                * 1_000_000_000,
                    )
                    .unwrap(),
                    targets: frame.allocation.targets.clone().unwrap(),
                    cash_weight: frame.allocation.cash_weight.clone().unwrap(),
                })
                .collect(),
        }
    });
    let report = NativePortfolioStudyResultV1 {
        schema_version: SchemaV1,
        consumed_fuel: DbCounter::new(frames.len() as u64).unwrap(),
        frames,
        simulation: simulation_request.as_ref().map(simulation_result::intraday),
        simulation_request,
    };
    domain::execution::check_portfolio_study(&request, &report).unwrap();
    let mut history = Vec::new();
    contracts::portfolio_history::write(
        &mut history,
        &contracts::portfolio_history::batch(&request, &report).unwrap(),
    )
    .unwrap();
    let (metadata,size): (uuid::Uuid,i64) = sqlx::query_as("SELECT a.id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1").bind(dataset_revision_id.as_uuid()).fetch_one(pool).await.unwrap();
    let metadata: contracts::catalogs::RuntimeCatalogMetadataV1 = serde_json::from_slice(
        &f.read(
            metadata.to_string().try_into().unwrap(),
            DbCounter::new(size as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut quality = metadata.quality;
    quality.checked_at = now;
    quality.datasets[0].dataset_revision_id = dataset_revision_id;
    quality.datasets[0].selection = request.source_selection.clone();
    let outputs: Vec<_> = job
        .spec
        .requested_output_schemas
        .iter()
        .map(|schema| {
            let bytes = match schema.name.as_str() {
                "qz.data_quality" => serde_json::to_vec(&quality).unwrap(),
                "qz.portfolio_study" => serde_json::to_vec(&report).unwrap(),
                "qz.portfolio_history" => history.clone(),
                _ => panic!("fixed Study outputs"),
            };
            let contract = native_output_contract(&schema.name, &schema.version).unwrap();
            (
                RuntimeOutputV1 {
                    kind: contract.kind,
                    schema: schema.clone(),
                    storage_ref: Id::new(),
                    storage_version: contracts::Revision::INITIAL,
                    byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
                    media_type: contract.media_type.into(),
                },
                bytes,
            )
        })
        .collect();
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: lease.run.id,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id.clone(),
        input_set_id: job.spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: [
            ("controlled-protocol-response".into(), "1".into()),
            ("portfolio-study".into(), "6".into()),
            ("portfolio-history".into(), "1".into()),
            ("portfolio-rolling-liquidity".into(), "1".into()),
            ("nautilus".into(), "0.63.0".into()),
        ]
        .into(),
        started_at: Some(now),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::ZERO,
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: DbCounter::new(outputs.iter().map(|(o, _)| o.byte_count.get()).sum())
                .unwrap(),
        },
        artifacts: outputs.iter().map(|(o, _)| o.clone()).collect(),
        error: None,
    };
    store
        .publish_native_result(
            lease.run.id,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            NativePayloads::Verified(outputs),
            |id, size| f.read(id, size),
            |batch| {
                std::future::ready(batch.into_iter().try_for_each(|object| {
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }))
            },
        )
        .await
        .unwrap();
}
