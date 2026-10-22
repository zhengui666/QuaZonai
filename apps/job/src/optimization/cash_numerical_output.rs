//! Explicit finite representation protocol for an already solved CASH QP.
//! This is a certified segment search, not another optimizer or a cash buffer.
use super::*;
use contracts::portfolio::{
    NativeCashAnchorKindV1, NativeCashNumericalOutcomeV1, NativeCashNumericalOutputV1,
};
use domain::portfolio::{
    allocation_point_feasibility, cash_numerical_anchor, cash_numerical_objective,
    cash_numerical_objective_delta, cash_numerical_segment,
};

const GRID: u64 = 1_000_000_000_000_000_000;

fn fraction(ticks: u64) -> Result<DecimalValue> {
    BigDecimal::new(ticks.into(), 18)
        .normalized()
        .to_plain_string()
        .parse()
        .map_err(|_| anyhow::anyhow!("CASH_NUMERICAL_FRACTION_RANGE"))
}

pub(super) fn apply(
    input: &AllocationInputV1,
    native: &DefaultSolution<f64>,
    result: &mut AllocationResultV1,
) -> Result<()> {
    let settings = domain::portfolio::optimizer_settings(&input.optimizer)?;
    let protocol = settings
        .cash_numerical_output
        .ok_or_else(|| anyhow::anyhow!("CASH_NUMERICAL_PROTOCOL_REQUIRED"))?;
    ensure!(
        native.x.len() == 3 * input.assets.len() + 1
            && native.x.iter().all(|x| x.is_finite())
            && native.obj_val.is_finite()
            && native.r_prim.is_finite()
            && native.r_dual.is_finite(),
        "CASH_NUMERICAL_NATIVE_RESULT"
    );
    let raw_targets = result.targets.as_ref().unwrap().clone();
    let raw_cash = result.cash_weight.as_ref().unwrap().clone();
    let raw_point_objective = cash_numerical_objective(input, &raw_targets)?;
    let raw_error = allocation_point_feasibility(input, &raw_targets, &raw_cash, None)
        .err()
        .map(|e| e.to_string());
    let mut trace = NativeCashNumericalOutputV1 {
        protocol,
        outcome: NativeCashNumericalOutcomeV1::Unchanged,
        raw_x: native.x.clone(),
        raw_native_status: format!("{:?}", native.status),
        raw_objective_value: native.obj_val,
        raw_point_objective_value: raw_point_objective,
        raw_primal_residual: native.r_prim,
        raw_dual_residual: native.r_dual,
        raw_targets: raw_targets.clone(),
        raw_cash_weight: raw_cash,
        raw_constraint_error: raw_error,
        anchor: None,
        segment_fraction: None,
        objective_delta: Some(0.0),
        objective_delta_exact: Some("0".into()),
        objective_change_limit: settings.solver_tolerance.clone(),
        search_steps: 0,
        reason_code: None,
    };
    if trace.raw_constraint_error.is_none() {
        // Even unchanged weights use their true economic objective. The native
        // epigraph objective is retained separately, including any t slack.
        result.objective_value = Some(raw_point_objective);
        result.numerical_output = Some(trace);
        return Ok(());
    }

    let mut found_anchor = false;
    for kind in [
        NativeCashAnchorKindV1::CurrentWeights,
        NativeCashAnchorKindV1::AllCash,
    ] {
        let Ok(anchor) = cash_numerical_anchor(input, kind) else {
            continue;
        };
        found_anchor = true;
        // Every candidate, including the identity-cash point at t=0, must
        // satisfy the original full validator. Capital identity alone is not
        // permission to rewrite an infeasible solver result.
        let zero = fraction(0)?;
        let (mut targets, mut cash) = cash_numerical_segment(&raw_targets, &anchor, &zero)?;
        trace.search_steps += 1;
        let mut selected_fraction = zero;
        if allocation_point_feasibility(input, &targets, &cash, None).is_err() {
            let mut lower = 0;
            let mut upper = GRID;
            targets = anchor.targets.clone();
            cash = anchor.cash_weight.clone();
            selected_fraction = fraction(upper)?;
            // A certified upper point is always retained. Decimal rounding can
            // create holes, so this does not claim the nearest feasible point.
            // At most 60 bisections on the fixed 10^-18 representation grid.
            while upper - lower > 1 {
                let middle = lower + (upper - lower) / 2;
                let t = fraction(middle)?;
                let (candidate, candidate_cash) =
                    cash_numerical_segment(&raw_targets, &anchor, &t)?;
                trace.search_steps += 1;
                if allocation_point_feasibility(input, &candidate, &candidate_cash, None).is_ok() {
                    upper = middle;
                    targets = candidate;
                    cash = candidate_cash;
                    selected_fraction = t;
                } else {
                    lower = middle;
                }
            }
        }
        let objective = cash_numerical_objective(input, &targets)?;
        let delta = cash_numerical_objective_delta(input, &raw_targets, &targets)?;
        if delta.abs() > *settings.solver_tolerance.as_decimal() {
            continue;
        }
        trace.outcome = NativeCashNumericalOutcomeV1::Corrected;
        trace.anchor = Some(anchor);
        trace.segment_fraction = Some(selected_fraction);
        trace.objective_delta = Some(
            delta
                .to_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| anyhow::anyhow!("CASH_NUMERICAL_DELTA_RANGE"))?,
        );
        trace.objective_delta_exact = Some(delta.normalized().to_plain_string());
        result.targets = Some(targets);
        result.cash_weight = Some(cash);
        result.objective_value = Some(objective);
        result.numerical_output = Some(trace);
        return Ok(());
    }
    let reason = if found_anchor {
        "CASH_NUMERICAL_OBJECTIVE_CHANGE_LIMIT"
    } else {
        "CASH_NUMERICAL_NO_FEASIBLE_ANCHOR"
    };
    trace.outcome = NativeCashNumericalOutcomeV1::Rejected;
    trace.reason_code = Some(reason.into());
    trace.objective_delta = None;
    trace.objective_delta_exact = None;
    result.solver_status = AllocationStatus::Failed;
    result.reason_code = Some(reason.into());
    result.targets = None;
    result.cash_weight = None;
    result.numerical_output = Some(trace);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::portfolio::{GroupBoundV1, NativeModelRefV1};

    // Structural candidate fixtures test the representation adapter's failure
    // behavior, not an assertion that Clarabel produced these altered points.
    // The separate integration regression executes the actual captured QP.
    fn candidate() -> (AllocationInputV1, DefaultSolution<f64>, AllocationResultV1) {
        let mut input: AllocationInputV1 = serde_json::from_str(include_str!(
            "../../tests/fixtures/cash-funding-boundary-input.json"
        ))
        .unwrap();
        let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut input.optimizer else {
            unreachable!()
        };
        parameters.cash_numerical_output = Some(
            contracts::portfolio::NativeCashNumericalOutputProtocolV1::ValidatedFeasibleSegmentV1,
        );
        input.assets[0].current_weight = "0".parse().unwrap();
        input.current_cash_weight = "1".parse().unwrap();
        let mut native = DefaultSolution::new(4, 0);
        native.x = vec![1.0, 0.0, 1.0, 1.0];
        native.status = SolverStatus::Solved;
        native.r_prim = 0.0;
        native.r_dual = 0.0;
        let targets = vec![AllocationTargetV1 {
            instrument_id: input.assets[0].instrument_id.clone(),
            currency: input.assets[0].currency.clone(),
            weight: "1".parse().unwrap(),
        }];
        native.obj_val = cash_numerical_objective(&input, &targets).unwrap();
        let mut result = native_outcome(&native, false);
        result.targets = Some(targets);
        result.cash_weight = Some("0".parse().unwrap());
        (input, native, result)
    }

    #[test]
    fn cash_numerical_requires_a_certified_anchor_for_every_original_constraint() {
        for variant in 0..4 {
            let (mut input, native, mut result) = candidate();
            match variant {
                0 => input.constraints.min_net_exposure = "0.5".parse().unwrap(),
                1 => {
                    input.assets[0].groups = vec!["g".into()];
                    input.constraints.group_bounds = vec![GroupBoundV1 {
                        group_id: "g".into(),
                        min: "0.5".parse().unwrap(),
                        max: "1".parse().unwrap(),
                    }];
                }
                2 => {
                    input.assets[0].current_weight = "1".parse().unwrap();
                    input.current_cash_weight = "0".parse().unwrap();
                    input.constraints.max_turnover_per_rebalance = "0".parse().unwrap();
                    // Force a raw violation; unchanged fully invested HOLD
                    // would otherwise legitimately require no repair.
                    result.cash_weight = Some("-0.0000000001".parse().unwrap());
                }
                _ => {
                    input.assets[0].current_weight = "0.6".parse().unwrap();
                    input.current_cash_weight = "0.4".parse().unwrap();
                    input.constraints.max_turnover_per_rebalance = "0.1".parse().unwrap();
                    input.constraints.max_ex_ante_risk =
                        Some("0.000000000000000001".parse().unwrap());
                }
            }
            let mut native = native;
            if variant == 2 {
                native.x[1] = -1e-10;
            }
            domain::portfolio::allocation_input(&input)
                .unwrap_or_else(|error| panic!("variant {variant}: {error}"));
            apply(&input, &native, &mut result).unwrap();
            assert_eq!(
                result.solver_status,
                AllocationStatus::Failed,
                "variant {variant}"
            );
            assert_eq!(
                result.reason_code.as_deref(),
                Some("CASH_NUMERICAL_NO_FEASIBLE_ANCHOR")
            );
            assert!(result.targets.is_none());
            domain::portfolio::allocation_result(&input, &result).unwrap();
        }
    }

    #[test]
    fn cash_numerical_rejects_feasible_correction_exceeding_original_objective_budget() {
        let (input, native, mut result) = candidate();
        apply(&input, &native, &mut result).unwrap();
        assert_eq!(result.solver_status, AllocationStatus::Failed);
        assert_eq!(
            result.reason_code.as_deref(),
            Some("CASH_NUMERICAL_OBJECTIVE_CHANGE_LIMIT")
        );
        assert!(result.targets.is_none());
        let trace = result.numerical_output.as_ref().unwrap();
        assert_eq!(trace.outcome, NativeCashNumericalOutcomeV1::Rejected);
        assert!(trace.search_steps > 0 && trace.search_steps <= 122);
        domain::portfolio::allocation_result(&input, &result).unwrap();
    }
}
