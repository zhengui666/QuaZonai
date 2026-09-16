//! Native quadratic-cone risk budgeting in a frozen orthant, before constraints.
use super::native_number;
use anyhow::{ensure, Result};
use clarabel::{algebra::CscMatrix, solver::*};
use contracts::portfolio::*;
use nalgebra::DMatrix;
use std::collections::BTreeMap;

pub(super) fn solve(
    input: &AllocationInputV1,
    covariance: &[Vec<f64>],
    lower: &DMatrix<f64>,
    budget: &RiskBudgetSettingsV1,
    parameters: &AllocatorSettingsV1,
) -> Result<DefaultSolution<f64>> {
    let n = input.assets.len();
    // A scalar change of variables; final gross normalization cancels it.
    let scale = (0..n).map(|i| covariance[i][i]).fold(0.0_f64, f64::max);
    ensure!(
        scale.is_finite() && scale > 0.0,
        "RISK_BUDGET_VARIANCE_SCALE"
    );
    let by_id: BTreeMap<_, _> = budget
        .assets
        .iter()
        .map(|b| (b.instrument_id.as_str(), b))
        .collect();
    let mut rows = Vec::new();
    let mut cols = Vec::new();
    let mut values = Vec::new();
    let mut b = Vec::new();
    let mut cones = Vec::new();
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
        // SOC slack: (sign*(w_i+(Cw)_i), 2*sqrt(share_i)*t, sign*(w_i-(Cw)_i)).
        for (j, covariance) in covariance[i].iter().enumerate() {
            let identity = if i == j { 1.0 } else { 0.0 };
            rows.extend([b.len(), b.len() + 2]);
            cols.extend([j, j]);
            values.extend([
                -sign * (identity + covariance / scale),
                -sign * (identity - covariance / scale),
            ]);
        }
        rows.push(b.len() + 1);
        cols.push(n);
        values.push(-2.0 * native_number(&config.share)?.sqrt());
        b.extend([0.0; 3]);
        cones.push(SecondOrderConeT(3));
    }
    // Fix only the risk gauge ||L^T*w||<=1, not the capital or gross scale.
    b.push(1.0);
    for i in 0..n {
        for j in i..n {
            rows.push(b.len());
            cols.push(j);
            values.push(-lower[(j, i)] / scale.sqrt());
        }
        b.push(0.0);
    }
    cones.push(SecondOrderConeT(n + 1));
    let a = CscMatrix::new_from_triplets(b.len(), n + 1, rows, cols, values);
    let p = CscMatrix::zeros((n + 1, n + 1));
    let mut q = vec![0.0; n + 1];
    q[n] = -1.0;
    let tolerance = native_number(&parameters.solver_tolerance)?;
    let settings = DefaultSettingsBuilder::default()
        .verbose(false)
        .max_iter(parameters.max_iterations)
        .tol_gap_abs(tolerance)
        .tol_gap_rel(tolerance)
        .tol_feas(tolerance)
        .build()?;
    let mut solver = DefaultSolver::new(&p, &q, &a, &b, &cones, settings)?;
    solver.solve();
    Ok(solver.solution)
}
