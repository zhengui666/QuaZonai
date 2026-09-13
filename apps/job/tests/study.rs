//! Actual one-process rolling Wasm/Clarabel/Nautilus science on a synthetic catalog.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use contracts::science::*;
use std::fs;

#[test]
fn rolling_original_models_use_one_native_account_and_observed_weights() {
    let (catalog, request, model) = market::study();
    let objects = tempfile::tempdir().unwrap();
    for member in &request.members {
        fs::write(
            objects.path().join(member.model_artifact_id.to_string()),
            &model,
        )
        .unwrap();
    }
    fs::write(
        objects.path().join(
            request
                .mandate
                .constraints
                .transaction_costs_ref
                .to_string(),
        ),
        serde_json::to_vec(&request.execution_settings).unwrap(),
    )
    .unwrap();
    let execute = |request: &NativePortfolioStudyRequestV1| {
        native::command(
            &[
                "study-portfolio".as_ref(),
                "--catalog".as_ref(),
                catalog.path().as_os_str(),
                "--objects".as_ref(),
                objects.path().as_os_str(),
            ],
            request,
        )
    };
    // Fully invested allocation can be rejected by native margin during a price-changing rebalance.
    let mut fully_invested = request.clone();
    fully_invested.mandate.constraints.min_cash_weight = "0".parse().unwrap();
    fully_invested.mandate.constraints.max_cash_weight = "0".parse().unwrap();
    fully_invested.mandate.constraints.min_net_exposure = "1".parse().unwrap();
    fully_invested.mandate.constraints.max_net_exposure = "1".parse().unwrap();
    let denied = execute(&fully_invested);
    assert!(!denied.status.success());
    assert_eq!(denied.stderr, b"QZ_NATIVE_JOB_FAILED\n");
    // A distinct, explicit study mandate reserves cash; the engine never adjusts the frozen input.
    let output = execute(&request);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativePortfolioStudyResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.stdout.len() < 8 * 1024 * 1024);
    assert_eq!(result.frames.len(), 3);
    assert!(result.consumed_fuel.get() > 0 && result.consumed_fuel.get() <= 200_000_000);
    domain::execution::check_portfolio_study(&request, &result).unwrap();
    let rejects = |changed: NativePortfolioStudyResultV1| {
        assert!(domain::execution::check_portfolio_study(&request, &changed).is_err());
    };
    let mut changed = result.clone();
    changed.frames[1].cutoff_ns = changed.frames[0].cutoff_ns;
    rejects(changed);
    let mut changed = result.clone();
    changed.frames[0].input.forecasts.members[0].alpha_id = contracts::Id::new();
    rejects(changed);
    let mut changed = result.clone();
    changed.frames[0].input.return_history.available_ns[0] = request.source_selection.event_end_ns;
    rejects(changed);
    let mut changed = result.clone();
    changed.consumed_fuel = market::count(200_000_001);
    rejects(changed);
    let mut changed = result.clone();
    changed
        .simulation_request
        .as_mut()
        .unwrap()
        .settings
        .snapshot_interval_ms = 1_000;
    rejects(changed);
    let mut changed = result.clone();
    changed.simulation_request.as_mut().unwrap().target_points[0].valid_until_ns =
        request.source_selection.event_end_ns;
    rejects(changed);
    let mut changed = result.clone();
    changed.simulation = None;
    rejects(changed);
    let simulation = result.simulation.as_ref().unwrap();
    let replay = result.simulation_request.as_ref().unwrap();
    assert_eq!(simulation.consumed_target_points.get(), 3);
    assert!(simulation.orders.get() > 0);
    assert_eq!(
        simulation.canonical_result["accounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        simulation.returns_status,
        contracts::evidence::MetricStatus::Ok
    );
    assert!(simulation.returns.len() >= 2);
    assert!(result.frames[0]
        .input
        .assets
        .iter()
        .all(|a| a.current_weight.as_decimal() == &0.into()));
    assert_eq!(
        result.frames[0].input.capital_assumption,
        request.mandate.capital_assumption
    );
    assert_ne!(
        result.frames[1].input.capital_assumption,
        request.mandate.capital_assumption
    );
    assert!(result.frames[1]
        .input
        .assets
        .iter()
        .any(|a| a.current_weight.as_decimal() != &0.into()));
    for (frame, target) in result.frames.iter().zip(&replay.target_points) {
        assert!(frame
            .input
            .return_history
            .available_ns
            .iter()
            .all(|n| *n <= frame.cutoff_ns));
        assert!(frame
            .input
            .forecasts
            .members
            .iter()
            .all(|m| m.available_ns <= frame.cutoff_ns));
        assert_eq!(frame.allocation.targets.as_ref().unwrap(), &target.targets);
        assert_eq!(frame.input.forecasts.decision_asof_ns, target.asof_ns);
        domain::portfolio::allocation_result(&frame.input, &frame.allocation).unwrap();
    }
    domain::execution::portfolio_simulation_metrics(
        contracts::Id::new(),
        contracts::Id::new(),
        replay,
        simulation,
    )
    .unwrap();
    let mut impossible = request.clone();
    impossible.mandate.constraints.max_asset_weight = "0.4".parse().unwrap();
    let output = execute(&impossible);
    assert!(output.status.success());
    let failed: NativePortfolioStudyResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(failed.simulation.is_none() && failed.simulation_request.is_none());
    assert_eq!(failed.frames.len(), 1);
    assert_eq!(
        failed.frames[0].allocation.solver_status,
        contracts::portfolio::SolverStatus::Infeasible
    );
    assert!(
        failed.frames[0].allocation.targets.is_none()
            && failed.frames[0].allocation.cash_weight.is_none()
    );
}
