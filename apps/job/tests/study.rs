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
    let mut manual = request.clone();
    let mut cutoffs = domain::execution::portfolio_study_cutoffs(&request).unwrap();
    cutoffs[1] = market::count(cutoffs[1].get() - 60_000_000_000);
    manual.manual_cutoffs_ns = Some(cutoffs.clone());
    assert!(domain::execution::portfolio_study_cutoffs(&manual).is_err());
    manual.mandate.rebalance_schedule.kind = contracts::portfolio::RebalanceKind::Manual;
    manual.mandate.rebalance_schedule.interval_seconds = None;
    // The irregular second gap requires its own explicit, frozen longer TTL.
    assert!(domain::execution::portfolio_study_cutoffs(&manual).is_err());
    manual.mandate.rebalance_schedule.target_ttl_seconds = 86_460;
    assert_eq!(
        domain::execution::portfolio_study_cutoffs(&manual).unwrap(),
        cutoffs
    );
    let output = execute(&manual);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manual_result: NativePortfolioStudyResultV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    domain::execution::check_portfolio_study(&manual, &manual_result).unwrap();
    assert_eq!(
        manual_result
            .frames
            .iter()
            .map(|f| f.cutoff_ns)
            .collect::<Vec<_>>(),
        cutoffs
    );
    assert_eq!(
        manual_result
            .simulation
            .as_ref()
            .unwrap()
            .consumed_target_points
            .get(),
        3
    );
    assert!(domain::execution::check_portfolio_study(&manual, &result).is_err());
    for invalid in [
        None,
        Some(vec![]),
        Some(vec![cutoffs[0]]),
        Some(vec![cutoffs[0], cutoffs[0]]),
        Some(vec![cutoffs[1], cutoffs[2]]),
        Some(vec![cutoffs[0], request.source_selection.event_end_ns]),
        Some(vec![cutoffs[0]; 257]),
    ] {
        let mut changed = manual.clone();
        changed.manual_cutoffs_ns = invalid;
        assert!(domain::execution::portfolio_study_cutoffs(&changed).is_err());
    }
    let mut uncovered_end = manual.clone();
    uncovered_end.manual_cutoffs_ns.as_mut().unwrap().pop();
    assert!(domain::execution::portfolio_study_cutoffs(&uncovered_end).is_err());
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

#[test]
fn rolling_liquidity_uses_original_policy_and_each_known_native_bar() {
    let run = |catalog: &std::path::Path,
               request: &NativePortfolioStudyRequestV1,
               model: &[u8],
               policy: &NativeRollingBarLiquidityPolicyV1| {
        let objects = tempfile::tempdir().unwrap();
        for member in &request.members {
            fs::write(
                objects.path().join(member.model_artifact_id.to_string()),
                model,
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
        fs::write(
            objects.path().join(
                request
                    .mandate
                    .constraints
                    .liquidity_ref
                    .unwrap()
                    .to_string(),
            ),
            serde_json::to_vec(policy).unwrap(),
        )
        .unwrap();
        native::command(
            &[
                "study-portfolio".as_ref(),
                "--catalog".as_ref(),
                catalog.as_os_str(),
                "--objects".as_ref(),
                objects.path().as_os_str(),
            ],
            request,
        )
    };
    let (catalog, request, model) = market::study_liquidity("10000000", "0.4");
    let policy = request.rolling_liquidity.as_ref().unwrap();
    let output = run(catalog.path(), &request, &model, policy);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativePortfolioStudyResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result.simulation.is_some());
    assert_eq!(result.frames.len(), 3);
    assert_ne!(
        result.frames[0].bar_notionals[0].notional_value,
        result.frames[1].bar_notionals[0].notional_value
    );
    for frame in &result.frames {
        assert_eq!(frame.bar_notionals.len(), request.assets.len());
        for (value, asset) in frame.bar_notionals.iter().zip(&frame.input.assets) {
            assert_eq!(
                Some(&value.notional_value),
                asset.available_notional.as_ref()
            );
            assert!(value.available_ns <= frame.cutoff_ns && value.event_ns < frame.cutoff_ns);
        }
    }
    let mut changed = result.clone();
    changed.frames[0].bar_notionals[0].available_ns = request.source_selection.event_end_ns;
    assert!(domain::execution::check_portfolio_study(&request, &changed).is_err());
    let mut changed = result.clone();
    changed.frames[0].bar_notionals[0].notional_value = "1".parse().unwrap();
    assert!(domain::execution::check_portfolio_study(&request, &changed).is_err());
    let mut expired = request.clone();
    expired
        .rolling_liquidity
        .as_mut()
        .unwrap()
        .maximum_age_seconds = 60;
    let first = &result.frames[0];
    domain::execution::portfolio_study_liquidity_assets(
        &expired,
        first.cutoff_ns,
        first.input.forecasts.forecast_asof_ns,
        first.cutoff_ns,
        &first.bar_notionals,
    )
    .unwrap();
    assert!(domain::execution::portfolio_study_liquidity_assets(
        &expired,
        first.cutoff_ns,
        first.input.forecasts.forecast_asof_ns,
        first.input.forecasts.decision_asof_ns,
        &first.bar_notionals
    )
    .is_err());
    // Same original file does not authorize changing the policy's frozen copy.
    assert!(!run(catalog.path(), &expired, &model, policy)
        .status
        .success());
    // This distinct original policy is fresh at cutoff, but expires before the native bar executes.
    assert!(!run(
        catalog.path(),
        &expired,
        &model,
        expired.rolling_liquidity.as_ref().unwrap()
    )
    .status
    .success());
    for (volume, participation) in [("0", "0.4"), ("10000000", "0.000001")] {
        let (catalog, request, model) = market::study_liquidity(volume, participation);
        let output = run(
            catalog.path(),
            &request,
            &model,
            request.rolling_liquidity.as_ref().unwrap(),
        );
        assert!(output.status.success());
        let result: NativePortfolioStudyResultV1 = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result.frames.len(), 1);
        assert_eq!(
            result.frames[0].allocation.solver_status,
            contracts::portfolio::SolverStatus::Infeasible
        );
        assert!(result.simulation.is_none() && result.frames[0].allocation.targets.is_none());
        if volume == "0" {
            assert!(result.frames[0]
                .bar_notionals
                .iter()
                .all(|v| v.notional_value == contracts::DecimalValue::zero()));
        }
    }
}
