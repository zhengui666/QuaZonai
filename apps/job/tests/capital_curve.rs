//! Real one-job-per-process scientific evidence. Synthetic market fixtures are
//! never a profitability, production account, or economic capacity acceptance.
#[path = "support/market_catalog.rs"]
mod fixed_market;
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;

use contracts::{capital_curve::*, science::NativeSimulationResultV1, DbCounter, SchemaV1};
use std::path::Path;

fn input() -> (tempfile::TempDir, NativeCapitalCurveRequestV1) {
    let (catalog, simulation) = market::market("0.002", 20);
    (
        catalog,
        NativeCapitalCurveRequestV1 {
            schema_version: SchemaV1,
            simulation,
            capital_basis: NativeCapitalBasisV1 {
                currency: "USD".into(),
                equity: "1000".parse().unwrap(),
                reserved: "100".parse().unwrap(),
                owner_liquidity_reserve: "100".parse().unwrap(),
            },
            budget: NativeCapitalCurveBudgetV1 {
                maximum_simulations: 3,
                maximum_elapsed_ms: DbCounter::new(120_000).unwrap(),
            },
        },
    )
}

fn curve(catalog: &Path, request: &impl serde::Serialize) -> NativeCapitalCurveResultV1 {
    let output = native::command(
        &[
            "capital-curve".as_ref(),
            "--catalog".as_ref(),
            catalog.as_os_str(),
        ],
        request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn every_automatic_capital_point_matches_a_separate_native_replay() {
    let (catalog, mut request) = input();
    request.capital_basis.equity = "1000200".parse().unwrap();
    let result = curve(catalog.path(), &request);
    assert_eq!(
        serde_json::to_value(&result.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    assert_eq!(result.deployable_capital, "1000000".parse().unwrap());
    assert_eq!(result.points.len(), 3);
    assert_eq!(result.points[0].capital, result.deployable_capital);
    assert_eq!(result.resources.attempted_simulations, 3);
    assert_eq!(
        result.resources.completed_simulations + result.resources.failed_simulations,
        3
    );
    assert!(
        result.resources.completed_simulations > 0,
        "{:?}",
        result.points
    );
    assert_eq!(
        result.economic_capacity_status,
        NativeCapitalEconomicStatusV1::Unavailable
    );
    let minimum = result
        .generation
        .minimum_candidate_capital
        .as_ref()
        .unwrap();
    assert_eq!(result.points[1].capital, *minimum);
    assert!(
        result.points[2].capital > *minimum && result.points[2].capital < result.deployable_capital
    );
    for source in &result.generation.minimum_sources {
        assert!(source.bar_event_ns <= source.decision_ns);
        assert!(source.bar_available_ns <= source.decision_ns);
        assert!(source.instrument_available_ns <= source.decision_ns);
        assert!(source.minimum_notional.is_positive());
    }
    let mut events = 0;
    for point in &result.points {
        assert_eq!(point.kind, NativeCapitalPointKindV1::TestedPoint);
        assert_eq!(point.lower, point.capital);
        assert_eq!(point.upper, point.capital);
        assert!(point.capital.is_positive() && point.capital <= result.deployable_capital);
        let mut simulation = request.simulation.clone();
        simulation.settings.starting_capital = point.capital.clone();
        let standalone = native::command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                catalog.path().as_os_str(),
            ],
            &simulation,
        );
        if let Some(actual) = &point.result {
            assert_eq!(point.status, NativeCapitalReplayStatusV1::Completed);
            assert!(point.reason_code.is_none());
            assert!(standalone.status.success());
            let expected: NativeSimulationResultV1 =
                serde_json::from_slice(&standalone.stdout).unwrap();
            assert_eq!(actual.events, expected.events);
            assert_eq!(actual.orders, expected.orders);
            assert_eq!(actual.positions, expected.positions);
            assert_eq!(
                actual.consumed_target_points,
                expected.consumed_target_points
            );
            assert_eq!(
                serde_json::to_value(&actual.statistics).unwrap(),
                serde_json::to_value(&expected.statistics).unwrap()
            );
            assert_eq!(
                serde_json::to_value(&actual.returns).unwrap(),
                serde_json::to_value(&expected.returns).unwrap()
            );
            events += actual.events.get();
        } else {
            assert_eq!(point.status, NativeCapitalReplayStatusV1::Failed);
            assert!(point.reason_code.is_some());
            assert!(!standalone.status.success());
        }
    }
    assert_eq!(events, result.resources.completed_native_events.get());
    let wire = serde_json::to_value(&result).unwrap();
    assert!(wire.get("maximum_capital").is_none());
    assert!(wire.get("feasible_regions").is_none());
}

#[test]
fn small_capital_lot_failures_consume_budget_and_are_not_removed_from_the_curve() {
    let (catalog, request) = input();
    let result = curve(catalog.path(), &request);
    assert_eq!(result.resources.attempted_simulations, 3);
    assert_eq!(result.resources.failed_simulations, 3);
    assert_eq!(result.resources.completed_simulations, 0);
    assert_eq!(result.points.len(), 3);
    assert!(result.points.iter().all(|point| point.result.is_none()
        && point.reason_code.as_deref()
            == Some("NATIVE_TARGET_REPLAY_FAILED: SIMULATION_LOT_ROUNDING_EXCEEDS_TOLERANCE")));
    assert_eq!(
        result.economic_capacity_status,
        NativeCapitalEconomicStatusV1::Unavailable
    );
}

#[test]
fn actual_lot_rounding_can_reject_an_interior_amount_between_two_completed_replays() {
    let (catalog, mut simulation) = fixed_market::market_direction(
        "0",
        20,
        0.0,
        "10000000",
        false,
        [
            market::execution_models::fill(),
            market::execution_models::fee(),
            market::execution_models::latency(1_000_000),
        ],
    );
    simulation.target_points.truncate(1);
    let (_, mut request) = input();
    request.simulation = simulation;
    request.capital_basis.equity = "700".parse().unwrap();
    let result = curve(catalog.path(), &request);
    assert_eq!(
        result
            .points
            .iter()
            .map(|point| point.capital.clone())
            .collect::<Vec<_>>(),
        ["500", "5", "252.5"].map(|amount| amount.parse().unwrap())
    );
    assert_eq!(
        result.points[0].status,
        NativeCapitalReplayStatusV1::Completed
    );
    assert_eq!(
        result.points[1].status,
        NativeCapitalReplayStatusV1::Completed
    );
    assert_eq!(result.points[2].status, NativeCapitalReplayStatusV1::Failed);
    assert_eq!(
        result.points[2].reason_code.as_deref(),
        Some("NATIVE_TARGET_REPLAY_FAILED: SIMULATION_LOT_ROUNDING_EXCEEDS_TOLERANCE")
    );
    assert_eq!(result.resources.completed_simulations, 2);
    assert_eq!(result.resources.failed_simulations, 1);
    assert!(result.points.iter().all(|point| point.lower == point.upper));
}

#[test]
fn one_run_budget_prioritizes_real_available_capital_and_does_not_round_reserves() {
    let (catalog, mut request) = input();
    request.budget.maximum_simulations = 1;
    request.capital_basis.equity = "1000.000000000000000001".parse().unwrap();
    let result = curve(catalog.path(), &request);
    assert_eq!(
        result.deployable_capital,
        "800.000000000000000001".parse().unwrap()
    );
    assert_eq!(result.points.len(), 1);
    assert_eq!(result.points[0].capital, result.deployable_capital);
    assert_eq!(result.resources.attempted_simulations, 1);
}

#[test]
fn unfunded_account_uses_no_native_replay_and_never_manufactures_a_minimum() {
    let (catalog, mut request) = input();
    request.capital_basis.equity = "200".parse().unwrap();
    let result = curve(&catalog.path().join("does-not-exist"), &request);
    assert_eq!(
        result.termination,
        NativeCapitalCurveTerminationV1::NoDeployableCapital
    );
    assert_eq!(result.deployable_capital, "0".parse().unwrap());
    assert!(result.points.is_empty());
    assert_eq!(result.resources.attempted_simulations, 0);
    assert!(result.generation.minimum_candidate_capital.is_none());
}

#[test]
fn failed_catalog_is_counted_without_fabricated_result_or_unbounded_retries() {
    let (catalog, request) = input();
    let result = curve(&catalog.path().join("does-not-exist"), &request);
    assert!(result.generation.minimum_candidate_capital.is_none());
    assert!(result.generation.minimum_reason_code.is_some());
    assert_eq!(result.points.len(), 3);
    assert_eq!(result.resources.attempted_simulations, 3);
    assert_eq!(result.resources.failed_simulations, 3);
    assert_eq!(
        result.generation.lower_planning_anchor,
        "0".parse().unwrap()
    );
    assert_eq!(result.resources.completed_simulations, 0);
    assert_eq!(result.points[0].status, NativeCapitalReplayStatusV1::Failed);
    assert!(result.points[0].result.is_none());
    assert!(result.points[0].reason_code.is_some());
}

#[test]
fn cash_only_replay_has_no_trade_minimum_and_is_not_called_economic_capacity() {
    let (catalog, mut request) = input();
    for point in &mut request.simulation.target_points {
        point.cash_weight = "1".parse().unwrap();
        for target in &mut point.targets {
            target.weight = "0".parse().unwrap();
        }
    }
    let result = curve(catalog.path(), &request);
    assert_eq!(
        result.generation.minimum_reason_code.as_deref(),
        Some("CAPITAL_MINIMUM_NO_NONZERO_ENTRY")
    );
    assert_eq!(result.points.len(), 3);
    assert!(result
        .points
        .iter()
        .all(|point| point.result.as_ref().unwrap().orders.get() == 0));
    assert_eq!(
        result.economic_capacity_status,
        NativeCapitalEconomicStatusV1::Unavailable
    );
}

#[test]
fn below_minimum_capital_remains_actual_point_without_funding_unavailable_money() {
    let (catalog, mut request) = input();
    request.capital_basis.equity = "200.01".parse().unwrap();
    let result = curve(catalog.path(), &request);
    assert!(
        result
            .generation
            .minimum_candidate_capital
            .as_ref()
            .unwrap()
            > &result.deployable_capital
    );
    assert_eq!(result.points.len(), 3);
    assert_eq!(
        result.generation.lower_planning_anchor,
        "0".parse().unwrap()
    );
    assert!(result
        .points
        .iter()
        .all(|point| point.capital.is_positive() && point.capital <= result.deployable_capital));
    assert_eq!(result.points[0].capital, "0.01".parse().unwrap());
    assert_eq!(result.points[0].upper, result.points[0].lower);
    assert_eq!(
        result.economic_capacity_status,
        NativeCapitalEconomicStatusV1::Unavailable
    );
}

#[test]
fn future_bars_cannot_supply_minimum_capital_provenance() {
    let (catalog, mut request) = input();
    request.simulation.target_points[0].asof_ns = DbCounter::ZERO;
    let result = curve(catalog.path(), &request);
    assert!(result.generation.minimum_candidate_capital.is_none());
    assert_eq!(
        result.generation.minimum_reason_code.as_deref(),
        Some("CAPITAL_MINIMUM_CAUSAL_PRICE_MISSING")
    );
    assert!(result.generation.minimum_sources.is_empty());
    assert_eq!(result.points.len(), 3);
}

#[test]
fn tiny_original_target_does_not_prevent_budgeted_lower_capital_exploration() {
    let (catalog, mut request) = input();
    for point in &mut request.simulation.target_points {
        point.targets[0].weight = "0.000000000007".parse().unwrap();
        point.targets[1].weight = "0.799999999993".parse().unwrap();
        point.cash_weight = "0.2".parse().unwrap();
    }
    let output = native::command(
        &[
            "capital-curve".as_ref(),
            "--plan-only".as_ref(),
            "--catalog".as_ref(),
            catalog.path().as_os_str(),
        ],
        &request,
    );
    assert!(output.status.success());
    let plan: NativeCapitalCurveResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(plan.generation.minimum_candidate_capital.as_ref().unwrap() > &plan.deployable_capital);
    assert_eq!(
        plan.generation.minimum_reason_code.as_deref(),
        Some("CAPITAL_MINIMUM_NOT_A_FUNDED_LOWER_ANCHOR")
    );
    assert_eq!(plan.generation.lower_planning_anchor, "0".parse().unwrap());
    assert_eq!(
        plan.generation.planned_capitals.len(),
        request.budget.maximum_simulations as usize
    );
    assert_eq!(plan.generation.planned_capitals[0], "800".parse().unwrap());
    assert_eq!(plan.generation.planned_capitals[1], "400".parse().unwrap());
    assert!(plan
        .generation
        .planned_capitals
        .iter()
        .all(|capital| capital.is_positive() && capital <= &plan.deployable_capital));
    assert!(plan.points.is_empty());
    assert_eq!(plan.resources.attempted_simulations, 0);
    let replay = curve(catalog.path(), &request);
    assert_eq!(
        replay.resources.attempted_simulations,
        request.budget.maximum_simulations
    );
    assert_eq!(replay.points.len(), plan.generation.planned_capitals.len());
    assert!(replay
        .points
        .iter()
        .all(|point| point.capital.is_positive() && point.lower == point.upper));
}

#[test]
fn invalid_money_budget_identity_and_external_grids_are_rejected() {
    let (catalog, request) = input();
    let original = serde_json::to_value(&request).unwrap();
    for (pointer, value) in [
        (
            "/capital_basis/equity",
            serde_json::json!("199.999999999999999999"),
        ),
        ("/capital_basis/reserved", serde_json::json!("-1")),
        ("/capital_basis/currency", serde_json::json!("EUR")),
        ("/budget/maximum_simulations", serde_json::json!(0)),
        ("/budget/maximum_simulations", serde_json::json!(65)),
        ("/budget/maximum_elapsed_ms", serde_json::json!("0")),
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(
            !native::command(
                &[
                    "capital-curve".as_ref(),
                    "--catalog".as_ref(),
                    catalog.path().as_os_str()
                ],
                &invalid
            )
            .status
            .success(),
            "accepted {pointer}"
        );
    }
    let mut grid = original;
    grid["capitals"] = serde_json::json!(["100", "200"]);
    assert!(!native::command(
        &[
            "capital-curve".as_ref(),
            "--catalog".as_ref(),
            catalog.path().as_os_str()
        ],
        &grid
    )
    .status
    .success());
}

#[test]
fn plan_only_uses_native_lot_provenance_without_claiming_or_charging_a_replay() {
    let (catalog, request) = input();
    let output = native::command(
        &[
            "capital-curve".as_ref(),
            "--plan-only".as_ref(),
            "--catalog".as_ref(),
            catalog.path().as_os_str(),
        ],
        &request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let planned: NativeCapitalCurveResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        planned.termination,
        NativeCapitalCurveTerminationV1::PlannedOnly
    );
    assert_eq!(planned.resources.attempted_simulations, 0);
    assert_eq!(planned.resources.completed_simulations, 0);
    assert_eq!(planned.resources.failed_simulations, 0);
    assert_eq!(planned.resources.completed_native_events.get(), 0);
    assert!(
        planned.points.is_empty(),
        "planned amounts are not TESTED_POINT evidence"
    );
    assert_eq!(planned.generation.planned_capitals.len(), 3);
    assert_eq!(
        planned.generation.planned_capitals[0],
        "800".parse().unwrap()
    );
    assert_eq!(planned.generation.minimum_sources.len(), 2);
    let measured = curve(catalog.path(), &request);
    assert_eq!(
        serde_json::to_value(&planned.generation).unwrap(),
        serde_json::to_value(&measured.generation).unwrap()
    );
    assert_eq!(
        planned.economic_capacity_status,
        NativeCapitalEconomicStatusV1::Unavailable
    );
}

#[test]
fn margin_short_targets_keep_the_original_native_execution_semantics() {
    let (catalog, mut request) = input();
    request.capital_basis.equity = "1000200".parse().unwrap();
    request.budget.maximum_simulations = 1;
    for (index, point) in request.simulation.target_points.iter_mut().enumerate() {
        point.targets[1].weight = if index == 0 { "-0.2" } else { "-0.4" }.parse().unwrap();
        point.cash_weight = if index == 0 { "0.8" } else { "1.2" }.parse().unwrap();
    }
    let result = curve(catalog.path(), &request);
    assert!(result
        .generation
        .minimum_sources
        .iter()
        .any(|source| source.entry_weight.as_decimal() < &bigdecimal::BigDecimal::from(0)));
    let actual = result.points[0]
        .result
        .as_ref()
        .expect("original margin short replay must remain supported");
    let standalone = native::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            catalog.path().as_os_str(),
        ],
        &request.simulation,
    );
    assert!(standalone.status.success());
    let expected: NativeSimulationResultV1 = serde_json::from_slice(&standalone.stdout).unwrap();
    assert_eq!(actual.orders, expected.orders);
    assert_eq!(
        serde_json::to_value(&actual.statistics).unwrap(),
        serde_json::to_value(&expected.statistics).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&result.request.simulation.target_points).unwrap(),
        serde_json::to_value(&request.simulation.target_points).unwrap()
    );
}

#[test]
fn genuine_clarabel_targets_keep_their_frozen_tolerance_without_renormalization() {
    let (catalog, simulation) = market::market("0", 20);
    let (_, mut request) = input();
    request.simulation = simulation;
    request.capital_basis.equity = "1000200".parse().unwrap();
    request.budget.maximum_simulations = 1;
    let mut allocation: contracts::portfolio::AllocationInputV1 = serde_json::from_str(
        include_str!("../../../tests/contracts/allocation-input.json"),
    )
    .unwrap();
    let ids: Vec<_> = request
        .simulation
        .settings
        .fee_rates
        .iter()
        .map(|rate| rate.instrument_id.clone())
        .collect();
    allocation.forecasts.instrument_ids = ids.clone();
    allocation.forecasts.bar_types = request.simulation.selection.bar_types.clone();
    allocation.return_history.instrument_ids = ids.clone();
    allocation.return_history.bar_types = request.simulation.selection.bar_types.clone();
    for member in &mut allocation.forecasts.members {
        member.instrument_ids = ids.clone();
        member.bar_types = request.simulation.selection.bar_types.clone();
    }
    for (asset, id) in allocation.assets.iter_mut().zip(ids) {
        asset.instrument_id = id;
    }
    let solved = job::allocate(&allocation).unwrap();
    assert_eq!(
        solved.solver_status,
        contracts::portfolio::SolverStatus::Optimal
    );
    let targets = solved.targets.unwrap();
    let cash = solved.cash_weight.unwrap();
    let sum = targets
        .iter()
        .fold(cash.as_decimal().clone(), |sum, target| {
            sum + target.weight.as_decimal()
        });
    assert_ne!(
        sum,
        bigdecimal::BigDecimal::from(1),
        "real decimal solver output must exercise the boundary"
    );
    assert!(
        (sum - bigdecimal::BigDecimal::from(1)).abs()
            <= *allocation.exposure_tolerance.as_decimal()
    );
    request.simulation.settings.exposure_tolerance = allocation.exposure_tolerance;
    for point in &mut request.simulation.target_points {
        point.targets = targets.clone();
        point.cash_weight = cash.clone();
    }
    let report = curve(catalog.path(), &request);
    let actual = report.points[0]
        .result
        .as_ref()
        .expect("valid original solver tolerance must reach native replay");
    let standalone = native::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            catalog.path().as_os_str(),
        ],
        &request.simulation,
    );
    assert!(standalone.status.success());
    let expected: NativeSimulationResultV1 = serde_json::from_slice(&standalone.stdout).unwrap();
    assert_eq!(
        serde_json::to_value(&actual.statistics).unwrap(),
        serde_json::to_value(&expected.statistics).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&report.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    request.simulation.target_points[0].cash_weight = "0.01".parse().unwrap();
    assert!(
        !native::command(
            &[
                "capital-curve".as_ref(),
                "--catalog".as_ref(),
                catalog.path().as_os_str()
            ],
            &request
        )
        .status
        .success(),
        "out-of-tolerance input must still fail"
    );
}
