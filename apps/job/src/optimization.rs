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
