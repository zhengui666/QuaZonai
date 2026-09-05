//! Native compatibility probe. All outputs are FIXTURE and non-deliverable.
use std::collections::BTreeMap;
use std::path::Path;

use pyo3::exceptions::PyAssertionError;
use pyo3::prelude::*;
use serde::Serialize;

mod arrow;
mod backtest;
mod optimization;

#[derive(Serialize)]
pub struct ProbeReport {
    schema_version: u8,
    origin: &'static str,
    deliverable: bool,
    versions: BTreeMap<String, String>,
    minimum_variance_weights: Vec<f64>,
    nautilus_iterations: u64,
    nautilus_fill_rows: usize,
    arrow_file: &'static str,
}

pub fn probe(directory: &Path) -> PyResult<ProbeReport> {
    Python::with_gil(|py| {
        let metadata = py.import("importlib.metadata")?;
        let mut versions = BTreeMap::new();
        for (distribution, expected) in [
            ("nautilus-trader", "1.231.0"),
            ("skfolio", "1.0.3"),
            ("cvxpy-base", "1.7.2"),
            ("clarabel", "0.11.1"),
            ("pyarrow", "25.0.0"),
        ] {
            let actual: String = metadata
                .call_method1("version", (distribution,))?
                .extract()?;
            if actual != expected {
                return Err(PyAssertionError::new_err(format!(
                    "VERSION_MISMATCH:{distribution}"
                )));
            }
            versions.insert(distribution.to_owned(), actual);
        }
        versions.insert("pyo3".to_owned(), "0.25.1".to_owned());
        let python_version: String = py
            .import("platform")?
            .call_method0("python_version")?
            .extract()?;
        if python_version != "3.12.12" {
            return Err(PyAssertionError::new_err("VERSION_MISMATCH:python"));
        }
        versions.insert("python".to_owned(), python_version);
        let weights = optimization::native_minimum_variance(py)?;
        let (iterations, fill_rows) = backtest::native_backtest(py)?;
        arrow::write_arrow(py, directory, &weights)?;
        Ok(ProbeReport {
            schema_version: 1,
            origin: "FIXTURE",
            deliverable: false,
            versions,
            minimum_variance_weights: weights,
            nautilus_iterations: iterations,
            nautilus_fill_rows: fill_rows,
            arrow_file: "native-weights.arrow",
        })
    })
}
