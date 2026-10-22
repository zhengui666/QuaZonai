//! Numerical regression from the actual synthetic-account native study frame.
//! The complete captured input is unchanged; only the explicit method opt-in is
//! added in the corrected branch. This does not establish production eligibility.
use super::*;
use bigdecimal::BigDecimal;

fn captured() -> AllocationInputV1 {
    serde_json::from_str(include_str!("../fixtures/cash-funding-boundary-input.json")).unwrap()
}

fn opt_in(input: &mut AllocationInputV1) {
    let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut input.optimizer else {
        panic!("unexpected optimizer")
    };
    parameters.cash_numerical_output =
        Some(NativeCashNumericalOutputProtocolV1::ValidatedFeasibleSegmentV1);
}

#[test]
fn captured_feasible_cash_boundary_is_strictly_rejected_without_opt_in_and_certified_with_it() {
    let mut request = captured();
    let original = request.clone();
    let rejected = job::allocate(&request).unwrap();
    assert_eq!(rejected.solver_status, SolverStatus::Failed);
    assert_eq!(
        rejected.reason_code.as_deref(),
        Some("POST_SOLVE_CONSTRAINT_REJECTED")
    );
    assert!(rejected.targets.is_none());
    assert!(rejected.numerical_output.is_none());

    opt_in(&mut request);
    let result = job::allocate(&request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Optimal, "{result:?}");
    domain::portfolio::allocation_result(&request, &result).unwrap();
    let trace = result.numerical_output.as_ref().unwrap();
    assert_eq!(trace.outcome, NativeCashNumericalOutcomeV1::Corrected);
    assert_eq!(
        trace.raw_constraint_error.as_deref(),
        Some("invalid contract field: cash_funding_borrowing")
    );
    assert_eq!(
        trace.anchor.as_ref().unwrap().kind,
        NativeCashAnchorKindV1::CurrentWeights
    );
    assert!(trace.search_steps <= 61);
    assert_eq!(request.capital_assumption, original.capital_assumption);
    assert_eq!(request.constraints, original.constraints);
    assert_eq!(request.assets, original.assets);
    assert_eq!(request.return_history, original.return_history);

    let weight = result.targets.as_ref().unwrap()[0].weight.as_decimal();
    let cash = result.cash_weight.as_ref().unwrap().as_decimal();
    let current = request.assets[0].current_weight.as_decimal();
    let fee = request.assets[0].transaction_cost_rate.as_decimal();
    let cost = (weight - current).abs() * fee;
    assert_eq!(weight + cash, BigDecimal::from(1));
    assert!(cash - &cost >= BigDecimal::from(0));
    assert!(BigDecimal::from(1) - weight - &cost >= BigDecimal::from(0));
    // Independent scalar funded-buy boundary, without division or rounding:
    // w*(1+fee) <= 1+fee*current. The derivative of this scalar objective is
    // negative near the upper bound; a corrected optimum must remain nearby.
    assert!(weight * (BigDecimal::from(1) + fee) <= BigDecimal::from(1) + fee * current);
    assert!(
        (weight - decimal("0.999999990801800095").as_decimal()).abs()
            <= *decimal("0.000000000000000001").as_decimal()
    );
    let rows = &request.return_history.asset_returns[0];
    let mean = rows.iter().sum::<f64>() / rows.len() as f64;
    let variance = rows.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (rows.len() - 1) as f64;
    let forecast: f64 = request
        .forecasts
        .members
        .iter()
        .map(|m| m.ensemble_weight.as_decimal().to_f64().unwrap() * m.forecasts[0])
        .sum();
    let w = weight.to_f64().unwrap();
    let oracle = variance * w * w + cost.to_f64().unwrap() - forecast * w;
    assert!((result.objective_value.unwrap() - oracle).abs() < 1e-16);
    let exact_delta: BigDecimal = trace
        .objective_delta_exact
        .as_ref()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(trace.objective_delta, exact_delta.to_f64());
    assert!(exact_delta.abs() <= *decimal("0.0000000001").as_decimal());
    // The original native epigraph value/residuals remain original evidence.
    assert_eq!(trace.raw_objective_value, rejected.objective_value.unwrap());
    assert_eq!(result.primal_residual, rejected.primal_residual);
    assert_eq!(result.dual_residual, rejected.dual_residual);
    assert_eq!(result.iterations, rejected.iterations);
}

#[test]
fn cash_numerical_trace_rejects_changed_points_status_and_objective_budget() {
    let mut request = captured();
    opt_in(&mut request);
    let result = job::allocate(&request).unwrap();
    domain::portfolio::allocation_result(&request, &result).unwrap();
    for change in 0..11 {
        let mut bad = result.clone();
        let trace = bad.numerical_output.as_mut().unwrap();
        match change {
            0 => trace.raw_x[0] += 0.1,
            1 => trace.raw_native_status = "AlmostSolved".into(),
            2 => trace.raw_point_objective_value += 0.01,
            3 => trace.anchor.as_mut().unwrap().cash_weight = decimal("0.3"),
            4 => trace.segment_fraction = Some(decimal("0.5")),
            5 => trace.objective_delta = Some(0.0),
            6 => trace.objective_change_limit = decimal("0.1"),
            7 => trace.raw_constraint_error = None,
            8 => bad.objective_value = Some(0.0),
            9 => bad.cash_weight = Some(decimal("0.2")),
            _ => bad.numerical_output = None,
        }
        assert!(
            domain::portfolio::allocation_result(&request, &bad).is_err(),
            "change {change}"
        );
    }
    let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut request.optimizer else {
        unreachable!()
    };
    parameters.accept_inaccurate = true;
    let mut relabeled = result.clone();
    relabeled
        .numerical_output
        .as_mut()
        .unwrap()
        .raw_native_status = "AlmostSolved".into();
    // Permission to accept an inaccurate point is not permission to relabel
    // the native status as an exact Optimal result.
    assert!(domain::portfolio::allocation_result(&request, &relabeled).is_err());
    relabeled.solver_status = SolverStatus::AcceptableInaccurate;
    domain::portfolio::allocation_result(&request, &relabeled).unwrap();
}

#[test]
fn cash_numerical_unchanged_points_and_absent_protocol_keep_original_targets() {
    for fee in ["0", "0.1"] {
        let mut request = cash_funding_input("0", "1", fee);
        request.constraints.min_cash_weight = decimal("0.2");
        let original = job::allocate(&request).unwrap();
        assert_eq!(original.solver_status, SolverStatus::Optimal);
        assert!(!serde_json::to_value(&original)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("numerical_output"));
        opt_in(&mut request);
        let updated = job::allocate(&request).unwrap();
        domain::portfolio::allocation_result(&request, &updated).unwrap();
        assert_eq!(updated.targets, original.targets);
        assert_eq!(updated.cash_weight, original.cash_weight);
        assert_eq!(
            updated.numerical_output.as_ref().unwrap().outcome,
            NativeCashNumericalOutcomeV1::Unchanged
        );
    }
    let mut not_cash = input();
    opt_in(&mut not_cash);
    assert_eq!(
        domain::portfolio::allocation_input(&not_cash)
            .unwrap_err()
            .to_string(),
        "invalid contract field: cash_numerical_output_requires_cash_funding"
    );
}
