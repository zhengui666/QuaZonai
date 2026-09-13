//! Controlled numerical response, adopted through the original native protocol.
//! These values do not claim a solver run, market forecast or REAL acceptance.
use super::*;
use contracts::{portfolio::*, runtime_jobs::*, science::*, Revision};
use store::lifecycle::native::{NativeJob, NativePayloads};

pub(super) async fn complete(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    job: &NativeJob,
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
    let NativeTaskParametersV1::BuildPortfolio { request, .. } =
        serde_json::from_slice(&f.read(job.spec.parameters_artifact_id, size).await.unwrap())
            .unwrap()
    else {
        panic!("original portfolio task");
    };
    let mut input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mandate = &request.mandate;
    input.objective = mandate.objective;
    input.risk = mandate.risk_measure;
    input.base_currency = mandate.base_currency.clone();
    input.capital_assumption = mandate.capital_assumption.clone();
    input.current_cash_weight = request.current_weights.cash_weight.clone();
    input.exposure_tolerance = mandate.exposure_tolerance.clone();
    input.constraints = mandate.constraints.clone();
    input.optimizer = mandate.optimizer.clone();
    input.alpha_ensemble = mandate.alpha_ensemble.clone();
    input.covariance_estimator = mandate.covariance_estimator.clone();
    input.assets = request.assets.clone();
    let forecasts = &mut input.forecasts;
    forecasts.base_currency = mandate.base_currency.clone();
    forecasts.decision_asof_ns = request.selection.decision_cutoff_ns;
    forecasts.forecast_asof_ns =
        DbCounter::new(request.selection.event_end_ns.get() - 1_000_000_000).unwrap();
    forecasts.max_input_age_seconds = mandate.rebalance_schedule.max_input_age_seconds;
    forecasts.instrument_ids = request
        .assets
        .iter()
        .map(|a| a.instrument_id.clone())
        .collect();
    forecasts.bar_types = request.selection.bar_types.clone();
    forecasts.horizon_value = DbCounter::new(u64::from(
        request.members[0].parameters.label_horizon_observations,
    ))
    .unwrap();
    for (actual, original) in forecasts.members.iter_mut().zip(&request.members) {
        actual.alpha_id = original.alpha_id;
        actual.alpha_version_id = original.alpha_version_id;
        actual.ensemble_weight = original.ensemble_weight.clone();
        actual.horizon_value = forecasts.horizon_value;
        actual.base_currency = forecasts.base_currency.clone();
        actual.asof_ns = forecasts.forecast_asof_ns;
        actual.available_ns = forecasts.forecast_asof_ns;
        actual.instrument_ids = forecasts.instrument_ids.clone();
        actual.bar_types = forecasts.bar_types.clone();
        actual.forecasts = vec![0.1; request.assets.len()];
    }
    let history = &mut input.return_history;
    history.base_currency = forecasts.base_currency.clone();
    history.horizon_value = forecasts.horizon_value;
    history.instrument_ids = forecasts.instrument_ids.clone();
    history.bar_types = forecasts.bar_types.clone();
    history.end_ns = (1..=5)
        .rev()
        .map(|i| DbCounter::new(forecasts.forecast_asof_ns.get() - i * 300_000_000_000).unwrap())
        .collect();
    history.available_ns = history.end_ns.clone();
    history.asset_returns = vec![vec![0.01, -0.01, 0.02, -0.02, 0.01]; request.assets.len()];
    let report = NativePortfolioBuildResultV1 {
        schema_version: SchemaV1,
        input,
        consumed_fuel: DbCounter::ZERO,
        allocation: AllocationResultV1 {
            schema_version: SchemaV1,
            solver_status: SolverStatus::Optimal,
            reason_code: None,
            targets: Some(request.current_weights.weights.clone()),
            cash_weight: Some(request.current_weights.cash_weight.clone()),
            iterations: 1,
            cvar_risk_budget_witness: None,
            objective_value: Some(0.0001),
            primal_residual: Some(0.0),
            dual_residual: Some(0.0),
        },
    };
    domain::execution::portfolio_build_result(&request, &report).unwrap();
    assert!(store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap());
    let now = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let bytes = serde_json::to_vec(&report).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::Report,
        schema: job.spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        media_type: "application/json".into(),
    };
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: lease.run.id,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id.clone(),
        input_set_id: job.spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: [("controlled-protocol-response".into(), "1".into())].into(),
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
    store
        .publish_native_result(
            lease.run.id,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            NativePayloads::Verified(vec![(output, bytes)]),
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
