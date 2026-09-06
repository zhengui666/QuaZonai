//! Rust-only native compatibility execution. FIXTURE outputs are not deliverable.
use anyhow::Result;
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};
mod arrow;
mod backtest;
mod native_version;
mod optimization;
pub use native_version::verified_codex_version;
mod report;
pub use report::write_probe_report;

#[derive(Serialize)]
pub struct ProbeReport {
    schema_version: u8,
    origin: &'static str,
    deliverable: bool,
    python_runtime: bool,
    versions: BTreeMap<&'static str, &'static str>,
    minimum_variance_weights: Vec<f64>,
    nautilus_iterations: usize,
    nautilus_order_count: usize,
    nautilus_event_count: usize,
    arrow_file: &'static str,
}
pub fn probe(directory: &Path) -> Result<ProbeReport> {
    let weights = optimization::native_minimum_variance()?;
    let (iterations, orders, events) = backtest::native_backtest()?;
    arrow::write_arrow(directory, &weights)?;
    Ok(ProbeReport {
        schema_version: 1,
        origin: "FIXTURE",
        deliverable: false,
        python_runtime: false,
        versions: BTreeMap::from([
            ("nautilus-backtest", "0.63.0"),
            ("clarabel", "0.11.1"),
            ("arrow-ipc", "56.2.0"),
        ]),
        minimum_variance_weights: weights,
        nautilus_iterations: iterations,
        nautilus_order_count: orders,
        nautilus_event_count: events,
        arrow_file: "native-weights.arrow",
    })
}
