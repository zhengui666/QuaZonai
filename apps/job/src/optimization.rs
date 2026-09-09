//! Native Clarabel compatibility golden; no handwritten optimization algorithm.
use anyhow::{ensure, Result};
use clarabel::{algebra::CscMatrix, solver::*};

fn verify_weights(weights: &[f64]) -> Result<()> {
    ensure!(weights.len() == 2, "NATIVE_RESULT_DIMENSION");
    ensure!(
        weights.iter().all(|x| x.is_finite()),
        "NONFINITE_NATIVE_RESULT"
    );
    // Independent hand calculation, a test oracle only: diag(1,4), sum=1,
    // long-only => minimum variance (4/5,1/5). Never a fallback result.
    ensure!(
        (weights[0] - 0.8).abs() <= 1e-5 && (weights[1] - 0.2).abs() <= 1e-5,
        "NATIVE_NUMERICAL_GOLDEN_MISMATCH"
    );
    Ok(())
}

pub(crate) fn native_minimum_variance() -> Result<Vec<f64>> {
    let p = CscMatrix::from(&[[2.0, 0.0], [0.0, 8.0]]);
    let q = [0.0, 0.0];
    // Equality sum(x)=1; x >= 0, using native zero/nonnegative cones.
    let a = CscMatrix::from(&[[1.0, 1.0], [-1.0, 0.0], [0.0, -1.0]]);
    let b = [1.0, 0.0, 0.0];
    let cones = [ZeroConeT(1), NonnegativeConeT(2)];
    let settings = DefaultSettingsBuilder::default()
        .verbose(false)
        .tol_gap_abs(1e-12)
        .tol_gap_rel(1e-12)
        .tol_feas(1e-12)
        .build()?;
    let mut solver = DefaultSolver::new(&p, &q, &a, &b, &cones, settings)?;
    solver.solve();
    ensure!(
        solver.solution.status == SolverStatus::Solved,
        "NATIVE_SOLVER_NOT_OPTIMAL"
    );
    verify_weights(&solver.solution.x)?;
    Ok(solver.solution.x)
}

// The production adapter constructs one native convex program. It does not
// duplicate Clarabel's optimization algorithm or silently regularize bad inputs.
use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use contracts::{
    portfolio::{
        AllocationInputV1, AllocationObjective, AllocationResultV1, AllocationRisk,
        AllocationTargetV1, SolverStatus as AllocationStatus,
    },
    DecimalValue, SchemaV1,
};
use nalgebra::{linalg::Cholesky, DMatrix};
use std::{collections::BTreeMap, str::FromStr};

fn native_number(value: &DecimalValue) -> Result<f64> {
    let number = value
        .as_decimal()
        .to_f64()
        .ok_or_else(|| anyhow::anyhow!("ALLOCATION_NUMBER_RANGE"))?;
    ensure!(number.is_finite(), "ALLOCATION_NUMBER_RANGE");
    ensure!(
        value.as_decimal() == &BigDecimal::from(0) || number != 0.0,
        "ALLOCATION_NUMBER_UNDERFLOW"
    );
    Ok(number)
}
fn public_weight(value: f64) -> Result<DecimalValue> {
    ensure!(value.is_finite(), "NONFINITE_NATIVE_WEIGHT");
    BigDecimal::from_str(&value.to_string())?
        .with_scale_round(18, RoundingMode::HalfEven)
        .to_plain_string()
        .parse()
        .map_err(|_| anyhow::anyhow!("NATIVE_WEIGHT_RANGE"))
}
fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

/// Execute a frozen input using the same Clarabel linked by the compatibility probe.
/// The caller still owns provenance, qualification, independent simulation and approval.
pub fn allocate(input: &AllocationInputV1) -> Result<AllocationResultV1> {
    domain::portfolio::allocation_input(input)?;
    ensure!(
        input.risk == AllocationRisk::Variance
            && input.objective != AllocationObjective::RiskBudgeting,
        "UNSUPPORTED_ALLOCATION_OBJECTIVE"
    );
    ensure!(
        input.constraints.max_ex_ante_risk.is_none(),
        "UNSUPPORTED_EX_ANTE_RISK_BOUND"
    );
    let n = input.assets.len();
    let covariance = DMatrix::from_fn(n, n, |row, col| input.covariance[row][col]);
    // Exact symmetry is part of the native estimator output contract. Do not use
    // one triangle of an inconsistent matrix or hide it with averaging/jitter.
    ensure!(
        (0..n).all(|i| (0..n).all(|j| covariance[(i, j)] == covariance[(j, i)])),
        "ASYMMETRIC_COVARIANCE"
    );
    ensure!(
        Cholesky::new(covariance).is_some(),
        "NONPOSITIVE_DEFINITE_COVARIANCE"
    );
    let variables = 3 * n + 1;
    let cash = n;
    let gross = n + 1;
    let traded = 2 * n + 1;
    let aversion = native_number(&input.risk_aversion)?;
    let mut p_rows = Vec::new();
    let mut p_cols = Vec::new();
    let mut p_values = Vec::new();
    for col in 0..n {
        for row in 0..=col {
            let value = 2.0 * aversion * input.covariance[row][col];
            ensure!(value.is_finite(), "COVARIANCE_SCALING_OVERFLOW");
            if value != 0.0 {
                p_rows.push(row);
                p_cols.push(col);
                p_values.push(value);
            }
        }
    }
    let p = CscMatrix::new_from_triplets(variables, variables, p_rows, p_cols, p_values);
    let mut q = vec![0.0; variables];
    for (i, asset) in input.assets.iter().enumerate() {
        if input.objective == AllocationObjective::MaxUtility {
            q[i] = -asset.expected_return;
        }
        q[traded + i] = native_number(&asset.transaction_cost_rate)?;
    }
    let constraints = &input.constraints;
    // Each row is a linear bound A*x <= b; only the first row is equality.
    let mut a_rows = Vec::new();
    let mut a_cols = Vec::new();
    let mut a_values = Vec::new();
    let mut b = Vec::new();
    let mut add = |terms: &[(usize, f64)], upper: f64| {
        let row = b.len();
        for &(col, value) in terms {
            if value != 0.0 {
                a_rows.push(row);
                a_cols.push(col);
                a_values.push(value);
            }
        }
        b.push(upper);
    };
    let capital_terms: Vec<_> = (0..=cash).map(|i| (i, 1.0)).collect();
    add(&capital_terms, 1.0);
    add(&[(cash, 1.0)], native_number(&constraints.max_cash_weight)?);
    add(
        &[(cash, -1.0)],
        -native_number(&constraints.min_cash_weight)?,
    );
    let overrides: BTreeMap<_, _> = constraints
        .asset_overrides
        .iter()
        .map(|bound| (bound.instrument_id.as_str(), bound))
        .collect();
    for (i, asset) in input.assets.iter().enumerate() {
        let (low, high) = overrides
            .get(asset.instrument_id.as_str())
            .map(|bound| (&bound.min, &bound.max))
            .unwrap_or((&constraints.min_asset_weight, &constraints.max_asset_weight));
        add(&[(i, 1.0)], native_number(high)?);
        add(&[(i, -1.0)], -native_number(low)?);
        add(&[(i, 1.0), (gross + i, -1.0)], 0.0);
        add(&[(i, -1.0), (gross + i, -1.0)], 0.0);
        let current = native_number(&asset.current_weight)?;
        add(&[(i, 1.0), (traded + i, -1.0)], current);
        add(&[(i, -1.0), (traded + i, -1.0)], -current);
        if let Some(participation) = &constraints.max_participation {
            let available = asset
                .available_notional
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("LIQUIDITY_REQUIRED"))?;
            let exact = available.as_decimal() * participation.as_decimal()
                / input.capital_assumption.as_decimal();
            let bound = exact
                .to_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| anyhow::anyhow!("PARTICIPATION_RANGE"))?;
            add(&[(traded + i, 1.0)], bound);
        }
    }
    add(
        &(0..n).map(|i| (gross + i, 1.0)).collect::<Vec<_>>(),
        native_number(&constraints.max_gross_exposure)?,
    );
    add(
        &(0..n).map(|i| (traded + i, 1.0)).collect::<Vec<_>>(),
        native_number(&constraints.max_turnover_per_rebalance)?,
    );
    add(
        &(0..n).map(|i| (i, 1.0)).collect::<Vec<_>>(),
        native_number(&constraints.max_net_exposure)?,
    );
    add(
        &(0..n).map(|i| (i, -1.0)).collect::<Vec<_>>(),
        -native_number(&constraints.min_net_exposure)?,
    );
    for group in &constraints.group_bounds {
        let terms: Vec<_> = input
            .assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| asset.groups.contains(&group.group_id))
            .map(|(i, _)| (i, 1.0))
            .collect();
        add(&terms, native_number(&group.max)?);
        add(
            &terms
                .iter()
                .map(|&(i, value)| (i, -value))
                .collect::<Vec<_>>(),
            -native_number(&group.min)?,
        );
    }
    ensure!(
        b.iter().chain(&a_values).chain(&q).all(|v| v.is_finite()),
        "NONFINITE_CONSTRAINT_MATRIX"
    );
    let a = CscMatrix::new_from_triplets(b.len(), variables, a_rows, a_cols, a_values);
    let cones = [ZeroConeT(1), NonnegativeConeT(b.len() - 1)];
    let tolerance = native_number(&input.settings.solver_tolerance)?;
    let settings = DefaultSettingsBuilder::default()
        .verbose(false)
        .max_iter(input.settings.max_iterations)
        .tol_gap_abs(tolerance)
        .tol_gap_rel(tolerance)
        .tol_feas(tolerance)
        .build()?;
    let mut solver = DefaultSolver::new(&p, &q, &a, &b, &cones, settings)?;
    solver.solve();
    let native = &solver.solution;
    let (status, reason) = match native.status {
        SolverStatus::Solved => (AllocationStatus::Optimal, None),
        SolverStatus::AlmostSolved if input.settings.accept_inaccurate => {
            (AllocationStatus::AcceptableInaccurate, None)
        }
        SolverStatus::PrimalInfeasible | SolverStatus::AlmostPrimalInfeasible => (
            AllocationStatus::Infeasible,
            Some("NATIVE_PRIMAL_INFEASIBLE"),
        ),
        SolverStatus::DualInfeasible | SolverStatus::AlmostDualInfeasible => {
            (AllocationStatus::Unbounded, Some("NATIVE_DUAL_INFEASIBLE"))
        }
        SolverStatus::AlmostSolved => (
            AllocationStatus::Failed,
            Some("INACCURATE_RESULT_NOT_AUTHORIZED"),
        ),
        SolverStatus::MaxIterations => (AllocationStatus::Failed, Some("NATIVE_ITERATION_LIMIT")),
        SolverStatus::MaxTime => (AllocationStatus::Failed, Some("NATIVE_TIME_LIMIT")),
        _ => (AllocationStatus::Failed, Some("NATIVE_SOLVER_FAILED")),
    };
    let mut result = AllocationResultV1 {
        schema_version: SchemaV1,
        solver_status: status,
        reason_code: reason.map(str::to_owned),
        targets: None,
        cash_weight: None,
        iterations: native.iterations,
        objective_value: finite(native.obj_val),
        primal_residual: finite(native.r_prim),
        dual_residual: finite(native.r_dual),
    };
    if reason.is_none() {
        let converted = (|| -> Result<()> {
            ensure!(native.x.len() == variables, "NATIVE_RESULT_DIMENSION");
            result.targets = Some(
                input
                    .assets
                    .iter()
                    .enumerate()
                    .map(|(i, asset)| {
                        Ok(AllocationTargetV1 {
                            instrument_id: asset.instrument_id.clone(),
                            currency: asset.currency.clone(),
                            weight: public_weight(native.x[i])?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
            result.cash_weight = Some(public_weight(native.x[cash])?);
            domain::portfolio::allocation_result(input, &result)?;
            Ok(())
        })();
        if converted.is_err() {
            result.solver_status = AllocationStatus::Failed;
            result.reason_code = Some("POST_SOLVE_CONSTRAINT_REJECTED".into());
            result.targets = None;
            result.cash_weight = None;
        }
    }
    domain::portfolio::allocation_result(input, &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_solver_matches_independent_reference() {
        verify_weights(&native_minimum_variance().unwrap()).unwrap();
    }
    #[test]
    fn golden_rejects_fallback_or_corrupt_results() {
        for weights in [
            vec![],
            vec![1.0],
            vec![1.0, 0.0],
            vec![0.5, 0.5],
            vec![f64::NAN, 0.2],
            vec![f64::INFINITY, 0.2],
            vec![0.8, 0.2, 0.0],
        ] {
            assert!(verify_weights(&weights).is_err());
        }
    }
    #[test]
    fn native_solver_reports_infeasible_constraints_without_weights() {
        let p = CscMatrix::from(&[[2.0]]);
        let a = CscMatrix::from(&[[1.0], [-1.0]]);
        let mut solver = DefaultSolver::new(
            &p,
            &[0.0],
            &a,
            &[0.0, -1.0],
            &[NonnegativeConeT(2)],
            DefaultSettingsBuilder::default()
                .verbose(false)
                .build()
                .unwrap(),
        )
        .unwrap();
        solver.solve();
        assert_eq!(solver.solution.status, SolverStatus::PrimalInfeasible);
        // Clarabel returns a certificate, NOT portfolio weights, on infeasibility.
    }
}
