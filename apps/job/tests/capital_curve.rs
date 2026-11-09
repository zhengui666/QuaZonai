//! Capital planning and the independently invokable single-point native primitive.
//! Product scheduling/recovery is owned by Pi tasks, not this Rust process.
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
            },
        },
    )
}

fn plan(catalog: &Path, request: &impl serde::Serialize) -> NativeCapitalCurveResultV1 {
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

fn assert_no_replays(report: &NativeCapitalCurveResultV1) {
    assert!(
        report.points.is_empty(),
        "planned amounts are not tested evidence"
    );
    assert_eq!(report.resources.attempted_simulations, 0);
    assert_eq!(report.resources.completed_simulations, 0);
    assert_eq!(report.resources.failed_simulations, 0);
    assert_eq!(report.resources.completed_native_events.get(), 0);
    assert_eq!(
        report.economic_capacity_status,
        NativeCapitalEconomicStatusV1::Unavailable
    );
}

#[test]
fn default_capital_operation_only_plans_and_preserves_native_minimum_provenance() {
    let (catalog, request) = input();
    let report = plan(catalog.path(), &request);
    assert_no_replays(&report);
    assert_eq!(
        report.termination,
        NativeCapitalCurveTerminationV1::PlannedOnly
    );
    assert_eq!(report.generation.planned_capitals.len(), 3);
    assert_eq!(
        report.generation.planned_capitals[0],
        "800".parse().unwrap()
    );
    assert_eq!(report.generation.minimum_sources.len(), 2);
    assert_eq!(
        serde_json::to_value(&report.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    for source in &report.generation.minimum_sources {
        assert!(source.bar_event_ns <= source.decision_ns);
        assert!(source.bar_available_ns <= source.decision_ns);
        assert!(source.instrument_available_ns <= source.decision_ns);
        assert!(source.minimum_notional.is_positive());
    }
    assert!(report
        .generation
        .planned_capitals
        .iter()
        .all(|capital| capital.is_positive() && capital <= &report.deployable_capital));
    let wire = serde_json::to_value(report).unwrap();
    assert!(wire.get("maximum_capital").is_none());
    assert!(wire.get("feasible_regions").is_none());
}

#[test]
fn planned_point_is_an_independent_native_process_input() {
    let (catalog, mut request) = input();
    request.capital_basis.equity = "1000200".parse().unwrap();
    let report = plan(catalog.path(), &request);
    assert_no_replays(&report);
    assert_eq!(report.deployable_capital, "1000000".parse().unwrap());
    let mut simulation = request.simulation.clone();
    simulation.settings.starting_capital = report.generation.planned_capitals[0].clone();
    let output = native::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            catalog.path().as_os_str(),
        ],
        &simulation,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let executed: NativeSimulationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(executed.events.get() > 0);
    assert_eq!(
        executed.consumed_target_points.get() as usize,
        simulation.target_points.len()
    );
    assert_eq!(executed.native_version, job::NAUTILUS_VERSION);
    // Executing a point cannot turn the original plan into fabricated completed evidence.
    assert_no_replays(&report);
}

#[test]
fn sampling_count_is_a_method_input_without_an_embedded_execution_loop() {
    let (catalog, mut request) = input();
    request.budget.maximum_simulations = 65;
    let report = plan(catalog.path(), &request);
    assert_eq!(report.generation.planned_capitals.len(), 65);
    assert_no_replays(&report);
}

#[test]
fn one_point_prioritizes_available_capital_without_rounding_reserves() {
    let (catalog, mut request) = input();
    request.budget.maximum_simulations = 1;
    request.capital_basis.equity = "1000.000000000000000001".parse().unwrap();
    let report = plan(catalog.path(), &request);
    assert_eq!(
        report.deployable_capital,
        "800.000000000000000001".parse().unwrap()
    );
    assert_eq!(
        report.generation.planned_capitals,
        vec![report.deployable_capital.clone()]
    );
    assert_no_replays(&report);
}

#[test]
fn unfunded_account_never_looks_up_or_manufactures_a_minimum() {
    let (catalog, mut request) = input();
    request.capital_basis.equity = "200".parse().unwrap();
    let report = plan(&catalog.path().join("does-not-exist"), &request);
    assert_eq!(
        report.termination,
        NativeCapitalCurveTerminationV1::NoDeployableCapital
    );
    assert_eq!(report.deployable_capital, "0".parse().unwrap());
    assert!(report.generation.minimum_candidate_capital.is_none());
    assert!(report.generation.planned_capitals.is_empty());
    assert_no_replays(&report);
}

#[test]
fn unavailable_catalog_is_missing_minimum_evidence_and_does_not_start_failed_jobs() {
    let (catalog, request) = input();
    let report = plan(&catalog.path().join("does-not-exist"), &request);
    assert!(report.generation.minimum_candidate_capital.is_none());
    assert!(report.generation.minimum_reason_code.is_some());
    assert_eq!(
        report.generation.lower_planning_anchor,
        "0".parse().unwrap()
    );
    assert_eq!(report.generation.planned_capitals.len(), 3);
    assert_no_replays(&report);
}

#[test]
fn cash_only_targets_have_no_trade_minimum_or_economic_capacity_claim() {
    let (catalog, mut request) = input();
    for point in &mut request.simulation.target_points {
        point.cash_weight = "1".parse().unwrap();
        for target in &mut point.targets {
            target.weight = "0".parse().unwrap();
        }
    }
    let report = plan(catalog.path(), &request);
    assert_eq!(
        report.generation.minimum_reason_code.as_deref(),
        Some("CAPITAL_MINIMUM_NO_NONZERO_ENTRY")
    );
    assert_eq!(report.generation.planned_capitals.len(), 3);
    assert_no_replays(&report);
}

#[test]
fn below_minimum_money_remains_a_planned_amount_without_extra_funding() {
    let (catalog, mut request) = input();
    request.capital_basis.equity = "200.01".parse().unwrap();
    let report = plan(catalog.path(), &request);
    assert!(
        report
            .generation
            .minimum_candidate_capital
            .as_ref()
            .unwrap()
            > &report.deployable_capital
    );
    assert_eq!(
        report.generation.lower_planning_anchor,
        "0".parse().unwrap()
    );
    assert_eq!(
        report.generation.planned_capitals[0],
        "0.01".parse().unwrap()
    );
    assert!(report
        .generation
        .planned_capitals
        .iter()
        .all(|capital| capital.is_positive() && capital <= &report.deployable_capital));
    assert_no_replays(&report);
}

#[test]
fn future_bars_cannot_supply_minimum_capital_provenance() {
    let (catalog, mut request) = input();
    request.simulation.target_points[0].asof_ns = DbCounter::ZERO;
    let report = plan(catalog.path(), &request);
    assert!(report.generation.minimum_candidate_capital.is_none());
    assert_eq!(
        report.generation.minimum_reason_code.as_deref(),
        Some("CAPITAL_MINIMUM_CAUSAL_PRICE_MISSING")
    );
    assert!(report.generation.minimum_sources.is_empty());
    assert_no_replays(&report);
}

#[test]
fn tiny_original_targets_preserve_lower_capital_exploration_without_claiming_execution() {
    let (catalog, mut request) = input();
    for point in &mut request.simulation.target_points {
        point.targets[0].weight = "0.000000000007".parse().unwrap();
        point.targets[1].weight = "0.799999999993".parse().unwrap();
        point.cash_weight = "0.2".parse().unwrap();
    }
    let report = plan(catalog.path(), &request);
    assert!(
        report
            .generation
            .minimum_candidate_capital
            .as_ref()
            .unwrap()
            > &report.deployable_capital
    );
    assert_eq!(
        report.generation.minimum_reason_code.as_deref(),
        Some("CAPITAL_MINIMUM_NOT_A_FUNDED_LOWER_ANCHOR")
    );
    assert_eq!(
        report.generation.planned_capitals[0],
        "800".parse().unwrap()
    );
    assert_eq!(
        report.generation.planned_capitals[1],
        "400".parse().unwrap()
    );
    assert_no_replays(&report);
}

#[test]
fn short_target_identity_and_original_signed_weights_are_preserved() {
    let (catalog, mut request) = input();
    for (index, point) in request.simulation.target_points.iter_mut().enumerate() {
        point.targets[1].weight = if index == 0 { "-0.2" } else { "-0.4" }.parse().unwrap();
        point.cash_weight = if index == 0 { "0.8" } else { "1.2" }.parse().unwrap();
    }
    let report = plan(catalog.path(), &request);
    assert!(report
        .generation
        .minimum_sources
        .iter()
        .any(|source| source.entry_weight.as_decimal() < &bigdecimal::BigDecimal::from(0)));
    assert_eq!(
        serde_json::to_value(&report.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    assert_no_replays(&report);
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
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        let output = native::command(
            &[
                "capital-curve".as_ref(),
                "--catalog".as_ref(),
                catalog.path().as_os_str(),
            ],
            &invalid,
        );
        assert!(!output.status.success(), "accepted {pointer}");
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
fn obsolete_execution_mode_flag_is_rejected_instead_of_running_a_hidden_batch() {
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
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--plan-only"));
}
