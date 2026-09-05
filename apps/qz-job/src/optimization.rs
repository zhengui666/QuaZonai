//! Calls upstream skfolio; contains no production optimizer implementation.
use pyo3::exceptions::PyAssertionError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn verify_weights(weights: &[f64]) -> Result<(), &'static str> {
    // Independent, hand-solvable golden: diagonal covariance proportional to
    // diag(1, 4), budget 1, long only => native optimum (4/5, 1/5).
    // This formula is a test oracle, never a production optimizer fallback.
    if weights.len() != 2
        || weights.iter().any(|w| !w.is_finite())
        || (weights[0] - 0.8).abs() > 1e-5
        || (weights[1] - 0.2).abs() > 1e-5
    {
        return Err("NATIVE_NUMERICAL_GOLDEN_MISMATCH");
    }
    Ok(())
}

pub(crate) fn native_minimum_variance(py: Python<'_>) -> PyResult<Vec<f64>> {
    eprintln!("native-probe: skfolio MeanRisk / CLARABEL");
    let samples: Vec<Vec<f64>> = (0..20)
        .flat_map(|_| {
            [
                vec![-0.01, -0.02],
                vec![-0.01, 0.02],
                vec![0.01, -0.02],
                vec![0.01, 0.02],
            ]
        })
        .collect();
    let numpy = py.import("numpy")?;
    let returns = numpy.getattr("array")?.call1((samples,))?;
    let optimization = py.import("skfolio.optimization")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item(
        "objective_function",
        optimization
            .getattr("ObjectiveFunction")?
            .getattr("MINIMIZE_RISK")?,
    )?;
    kwargs.set_item(
        "risk_measure",
        py.import("skfolio")?
            .getattr("RiskMeasure")?
            .getattr("VARIANCE")?,
    )?;
    kwargs.set_item("min_weights", 0.0)?;
    kwargs.set_item("max_weights", 1.0)?;
    kwargs.set_item("budget", 1.0)?;
    kwargs.set_item("solver", "CLARABEL")?;
    // The fixture covariance is O(1e-4). Scaling the native objective by a
    // positive constant preserves its minimizer and avoids weak absolute-gap
    // accuracy in an otherwise correctly solved near-flat objective.
    kwargs.set_item("scale_objective", 10_000.0)?;
    kwargs.set_item("save_problem", true)?;
    let solver_params = PyDict::new(py);
    for name in ["tol_gap_abs", "tol_gap_rel", "tol_feas"] {
        solver_params.set_item(name, 1e-12)?;
    }
    kwargs.set_item("solver_params", solver_params)?;
    let estimator = optimization.getattr("MeanRisk")?.call((), Some(&kwargs))?;
    estimator.call_method1("fit", (&returns,))?;
    let status: String = estimator
        .getattr("problem_")?
        .getattr("status")?
        .extract()?;
    if status != "optimal" {
        return Err(PyAssertionError::new_err("NATIVE_SOLVER_NOT_OPTIMAL"));
    }
    let weights: Vec<f64> = estimator
        .getattr("weights_")?
        .call_method0("tolist")?
        .extract()?;
    verify_weights(&weights).map_err(PyAssertionError::new_err)?;
    Ok(weights)
}

#[cfg(test)]
mod tests {
    use super::verify_weights;

    #[test]
    fn golden_accepts_native_reference_with_documented_tolerance() {
        assert!(verify_weights(&[0.8, 0.2]).is_ok());
        assert!(verify_weights(&[0.800001, 0.199999]).is_ok());
    }

    #[test]
    fn golden_rejects_hidden_fallback_and_non_finite_results() {
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
}
