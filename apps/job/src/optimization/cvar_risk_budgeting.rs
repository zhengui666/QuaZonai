//! Native CVaR/geometric-budget cone program. Scenario duals remain in original order.
use super::native_number;
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, ToPrimitive};
use clarabel::{algebra::CscMatrix, solver::*};
use contracts::{portfolio::*, DecimalValue};
use std::collections::BTreeMap;

pub(super) fn solve(
    input: &AllocationInputV1,
    budget: &RiskBudgetSettingsV1,
    parameters: &AllocatorSettingsV1,
    confidence: &DecimalValue,
) -> Result<DefaultSolution<f64>> {
    let n = input.assets.len();
    let count = input.return_history.end_ns.len();
    let positive = budget
        .assets
        .iter()
        .filter(|a| a.share.is_positive())
        .count();
    let eta = n + positive - 1;
    let excess = eta + 1;
    let variables = excess + count;
    let scale = input
        .return_history
        .asset_returns
        .iter()
        .flatten()
        .map(|v| v.abs())
        .fold(0.0_f64, f64::max);
    ensure!(
        scale.is_finite() && scale > 0.0,
        "CVAR_RISK_BUDGET_ZERO_RISK"
    );
    let cap = domain::portfolio::cvar_tail_coefficient(count, confidence)?;
    let mut rows = Vec::new();
    let mut cols = Vec::new();
    let mut values = Vec::new();
    // First count rows are exactly the original scenario loss inequalities.
    for scenario in 0..count {
        for (i, returns) in input.return_history.asset_returns.iter().enumerate() {
            rows.push(scenario);
            cols.push(i);
            values.push(-returns[scenario] / scale);
        }
        rows.extend([scenario, scenario]);
        cols.extend([eta, excess + scenario]);
        values.extend([-1.0, -1.0]);
    }
    let mut b = vec![0.0; count];
    let mut cones = vec![NonnegativeConeT(count)];
    for scenario in 0..count {
        rows.push(b.len());
        cols.push(excess + scenario);
        values.push(-1.0);
        b.push(0.0);
    }
    cones.push(NonnegativeConeT(count));
    let mut q = vec![0.0; variables];
    q[eta] = 1.0;
    q[excess..].fill(cap);
    let by_id: BTreeMap<_, _> = budget
        .assets
        .iter()
        .map(|a| (a.instrument_id.as_str(), a))
        .collect();
    let mut previous: Option<(usize, f64)> = None;
    let mut total = BigDecimal::from(0);
    let mut auxiliary = n;
    for (i, asset) in input.assets.iter().enumerate() {
        let config = by_id[asset.instrument_id.as_str()];
        if !config.share.is_positive() {
            rows.push(b.len());
            cols.push(i);
            values.push(1.0);
            b.push(0.0);
            cones.push(ZeroConeT(1));
            continue;
        }
        let sign = if config.sign == RiskBudgetSign::Long {
            1.0
        } else {
            -1.0
        };
        let next_total = &total + config.share.as_decimal();
        if let Some((column, previous_sign)) = previous {
            let alpha = (&total / &next_total)
                .to_f64()
                .filter(|a| a.is_finite() && *a > 0.0 && *a < 1.0)
                .ok_or_else(|| anyhow::anyhow!("CVAR_BUDGET_POWER_RANGE"))?;
            rows.extend([b.len(), b.len() + 1, b.len() + 2]);
            cols.extend([column, i, auxiliary]);
            values.extend([-previous_sign, -sign, -1.0]);
            b.extend([0.0; 3]);
            cones.push(PowerConeT(alpha));
            previous = Some((auxiliary, 1.0));
            auxiliary += 1;
        } else {
            previous = Some((i, sign));
        }
        total = next_total;
    }
    let (column, sign) = previous.ok_or_else(|| anyhow::anyhow!("CVAR_RISK_BUDGET_EMPTY"))?;
    rows.push(b.len());
    cols.push(column);
    values.push(-sign);
    b.push(-1.0);
    cones.push(NonnegativeConeT(1));
    let p = CscMatrix::zeros((variables, variables));
    let a = CscMatrix::new_from_triplets(b.len(), variables, rows, cols, values);
    let tolerance = native_number(&parameters.solver_tolerance)?;
    // Objective error is second order near the optimum; contribution error is first order.
    let gap = tolerance.min(native_number(&input.exposure_tolerance)?.powi(2));
    let settings = DefaultSettingsBuilder::default()
        .verbose(false)
        .max_iter(parameters.max_iterations)
        .tol_gap_abs(gap)
        .tol_gap_rel(gap)
        .tol_feas(tolerance)
        .build()?;
    let mut solver = DefaultSolver::new(&p, &q, &a, &b, &cones, settings)?;
    solver.solve();
    Ok(solver.solution)
}
