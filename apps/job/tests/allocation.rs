//! Native Clarabel solves, checked against independent small analytical answers.
//! Example instruments and assumptions are SYNTHETIC, not qualification evidence.
use bigdecimal::ToPrimitive;
use contracts::{portfolio::*, DecimalValue, Id};

fn input() -> AllocationInputV1 {
    serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap()
}
fn decimal(value: &str) -> DecimalValue {
    value.parse().unwrap()
}
fn weights(request: &AllocationInputV1) -> Vec<f64> {
    let result = job::allocate(request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Optimal, "{result:?}");
    domain::portfolio::allocation_result(request, &result).unwrap();
    result
        .targets
        .unwrap()
        .iter()
        .map(|v| v.weight.as_decimal().to_f64().unwrap())
        .collect()
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-5, "{actual} != {expected}");
}

#[test]
fn native_minimum_risk_uses_the_whole_covariance_not_equal_weights() {
    let request = input();
    let result = weights(&request);
    near(result[0], 0.8);
    near(result[1], 0.2);
    let mut changed = request;
    changed.covariance = vec![vec![4.0, 0.0], vec![0.0, 1.0]];
    let changed = weights(&changed);
    near(changed[0], 0.2);
    near(changed[1], 0.8);
}

#[test]
fn native_utility_and_transaction_costs_change_the_optimum() {
    let mut request = input();
    request.objective = AllocationObjective::MaxUtility;
    let result = weights(&request);
    // f(x)=x²+4(1-x)²-0.1x-0.2(1-x), derivative 10x-7.9.
    near(result[0], 0.79);
    request.objective = AllocationObjective::MinRisk;
    for asset in &mut request.assets {
        asset.transaction_cost_rate = decimal("0.5");
    }
    // Both legs cost 0.5*|x-0.5|: derivative 10x-8+1 for x>=0.5.
    near(weights(&request)[0], 0.7);
}

#[test]
fn asset_overrides_and_group_bounds_enter_the_native_problem() {
    let mut request = input();
    request.constraints.asset_overrides.push(AssetBoundV1 {
        instrument_id: request.assets[0].instrument_id.clone(),
        min: decimal("0"),
        max: decimal("0.6"),
    });
    near(weights(&request)[0], 0.6);
    request.constraints.group_bounds.push(GroupBoundV1 {
        group_id: "sector-a".into(),
        min: decimal("0.3"),
        max: decimal("0.4"),
    });
    near(weights(&request)[0], 0.4);
}

#[test]
fn cash_and_net_exposure_are_one_shared_capital_constraint() {
    let mut request = input();
    request.constraints.min_cash_weight = decimal("0.2");
    request.constraints.max_cash_weight = decimal("0.2");
    request.constraints.min_net_exposure = decimal("0.8");
    request.constraints.max_net_exposure = decimal("0.8");
    let result = job::allocate(&request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Optimal);
    near(
        result.cash_weight.unwrap().as_decimal().to_f64().unwrap(),
        0.2,
    );
    let result = weights(&request);
    near(result[0], 0.64);
    near(result[1], 0.16);
}

#[test]
fn turnover_and_real_notional_participation_bound_both_legs() {
    let mut request = input();
    request.constraints.max_turnover_per_rebalance = decimal("0.1");
    near(weights(&request)[0], 0.55);
    request.constraints.max_turnover_per_rebalance = decimal("2");
    request.constraints.max_participation = Some(decimal("0.1"));
    request.constraints.liquidity_ref = Some(Id::new());
    for asset in &mut request.assets {
        asset.available_notional = Some(decimal("500000"));
    }
    near(weights(&request)[0], 0.55);
    for asset in &mut request.assets {
        asset.available_notional = Some(decimal("0"));
    }
    near(weights(&request)[0], 0.5);
}

#[test]
fn signed_exposures_obey_native_gross_and_net_limits() {
    let mut request = input();
    request.objective = AllocationObjective::MaxUtility;
    request.risk_aversion = decimal("0.1");
    request.constraints.long_only = false;
    request.constraints.min_asset_weight = decimal("-1");
    request.constraints.max_asset_weight = decimal("2");
    request.constraints.max_gross_exposure = decimal("1.4");
    request.assets[0].expected_return = 5.0;
    let result = weights(&request);
    near(result[0], 1.2);
    near(result[1], -0.2);
}

#[test]
fn native_infeasibility_never_publishes_a_certificate_as_targets() {
    let mut request = input();
    request.constraints.max_asset_weight = decimal("0.1");
    let result = job::allocate(&request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Infeasible);
    assert!(result.targets.is_none());
    assert!(result.cash_weight.is_none());
    assert!(result.reason_code.is_some());
    domain::portfolio::allocation_result(&request, &result).unwrap();
}

#[test]
fn no_iteration_or_accuracy_failure_becomes_a_fallback_allocation() {
    let mut request = input();
    request.settings.max_iterations = 1;
    let result = job::allocate(&request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Failed);
    assert!(result.targets.is_none() && result.cash_weight.is_none());
}

#[test]
fn invalid_or_unsupported_inputs_are_not_silently_repaired() {
    let mut request = input();
    request.covariance[0][1] = 0.1;
    assert!(job::allocate(&request).is_err());
    request.covariance = vec![vec![1.0, 2.0], vec![2.0, 1.0]];
    assert!(job::allocate(&request).is_err());
    request = input();
    request.covariance[0][0] = f64::NAN;
    assert!(job::allocate(&request).is_err());
    assert!(serde_json::to_value(&request).is_err());
    request = input();
    request.assets[1].instrument_id = request.assets[0].instrument_id.clone();
    assert!(job::allocate(&request).is_err());
    request = input();
    request.assets[0].transaction_cost_rate = decimal("-0.1");
    assert!(job::allocate(&request).is_err());
    request = input();
    request.constraints.max_participation = Some(decimal("0.1"));
    assert!(job::allocate(&request).is_err());
    request = input();
    request.current_cash_weight = decimal("0.1");
    assert!(job::allocate(&request).is_err());
    request = input();
    request.risk = AllocationRisk::Cvar;
    assert!(job::allocate(&request).is_err());
    request = input();
    request.objective = AllocationObjective::RiskBudgeting;
    assert!(job::allocate(&request).is_err());
    request = input();
    request.constraints.max_ex_ante_risk = Some(decimal("0.1"));
    assert!(job::allocate(&request).is_err());
}

#[test]
fn publication_rechecks_exact_decimal_targets_and_original_identities() {
    let request = input();
    let original = job::allocate(&request).unwrap();
    let mut result = original.clone();
    result.targets.as_mut().unwrap()[0].weight = decimal("2");
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original.clone();
    result.targets.as_mut().unwrap().swap(0, 1);
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original.clone();
    result.targets.as_mut().unwrap()[0].currency = "EUR".into();
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original.clone();
    result.cash_weight = None;
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original;
    result.solver_status = SolverStatus::Infeasible;
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
}

#[test]
fn native_cli_consumes_exact_json_without_issuing_delivery_authority() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("allocate")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!(
            "../../../tests/contracts/allocation-input.json"
        ))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: AllocationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Optimal);
    domain::portfolio::allocation_result(&input(), &result).unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("approval"));
}
