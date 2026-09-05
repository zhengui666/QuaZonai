//! W0 executable compatibility proof, NOT a production research/job API.
//! Every input is synthetic; none of these outputs can be used for delivery.
//! Python is embedded only in this short-lived process, never in an API process.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::{Value, json};

const GOLDEN_WEIGHTS: [f64; 2] = [0.8, 0.2];
const WEIGHT_TOLERANCE: f64 = 1e-6;

fn check_golden(weights: &[f64]) -> Result<(), &'static str> {
    if weights.len() != GOLDEN_WEIGHTS.len()
        || weights
            .iter()
            .zip(GOLDEN_WEIGHTS)
            .any(|(actual, expected)| {
                !actual.is_finite() || (actual - expected).abs() > WEIGHT_TOLERANCE
            })
    {
        return Err("native minimum-variance result differs from the analytic fixture");
    }
    Ok(())
}

fn portfolio_golden(py: Python<'_>, output: &Path) -> PyResult<Value> {
    // The sample columns are orthogonal, zero-mean and have variance ratio 1:4.
    // The analytic constrained minimum is (4/5, 1/5). This is a test oracle,
    // not a second optimizer: all estimation and optimization below is native.
    let observations = vec![
        vec![-0.01, -0.02],
        vec![-0.01, 0.02],
        vec![0.01, -0.02],
        vec![0.01, 0.02],
    ];
    let returns = py.import("numpy")?.call_method1("array", (observations,))?;
    let covariance = py
        .import("skfolio.moments")?
        .getattr("EmpiricalCovariance")?
        .call0()?;
    let prior_options = PyDict::new(py);
    prior_options.set_item("covariance_estimator", covariance)?;
    let prior = py
        .import("skfolio.prior")?
        .getattr("EmpiricalPrior")?
        .call((), Some(&prior_options))?;
    let solver_options = PyDict::new(py);
    solver_options.set_item("tol_gap_abs", 1e-11)?;
    solver_options.set_item("tol_gap_rel", 1e-11)?;
    solver_options.set_item("tol_feas", 1e-11)?;
    let options = PyDict::new(py);
    options.set_item("prior_estimator", prior)?;
    options.set_item("min_weights", 0.0)?;
    options.set_item("max_weights", 1.0)?;
    options.set_item("budget", 1.0)?;
    options.set_item("solver", "CLARABEL")?;
    options.set_item("solver_params", solver_options)?;
    options.set_item("save_problem", true)?;
    options.set_item("raise_on_failure", true)?;
    options.set_item("fallback", py.None())?;
    let optimization = py.import("skfolio.optimization")?;
    options.set_item(
        "objective_function",
        optimization
            .getattr("ObjectiveFunction")?
            .getattr("MINIMIZE_RISK")?,
    )?;
    options.set_item(
        "risk_measure",
        py.import("skfolio")?
            .getattr("RiskMeasure")?
            .getattr("VARIANCE")?,
    )?;
    let model = optimization.getattr("MeanRisk")?.call((), Some(&options))?;
    model.call_method1("fit", (&returns,))?;
    let weights: Vec<f64> = model
        .getattr("weights_")?
        .call_method0("tolist")?
        .extract()?;
    let solver_status: String = model.getattr("problem_")?.getattr("status")?.extract()?;
    if solver_status != "optimal" {
        return Err(PyValueError::new_err(
            "native solver did not report optimal",
        ));
    }
    check_golden(&weights).map_err(PyValueError::new_err)?;

    let arrow = py.import("pyarrow")?;
    let columns = PyDict::new(py);
    columns.set_item("fixture_asset", vec!["A", "B"])?;
    columns.set_item("weight", &weights)?;
    let metadata = PyDict::new(py);
    metadata.set_item("schema_name", "qz.native_probe.v1")?;
    metadata.set_item("origin", "SYNTHETIC")?;
    metadata.set_item("deliverable", "false")?;
    let table = arrow
        .call_method1("table", (columns,))?
        .call_method1("replace_schema_metadata", (metadata,))?;
    let path = output.join("native-weights.arrow");
    let path = path.to_string_lossy();
    let ipc = py.import("pyarrow.ipc")?;
    let writer = ipc.call_method1("new_file", (path.as_ref(), table.getattr("schema")?))?;
    writer.call_method1("write_table", (&table,))?;
    writer.call_method0("close")?;
    let restored = ipc
        .call_method1("open_file", (path.as_ref(),))?
        .call_method0("read_all")?;
    let equal_options = PyDict::new(py);
    equal_options.set_item("check_metadata", true)?;
    let equal: bool = table
        .call_method("equals", (&restored,), Some(&equal_options))?
        .extract()?;
    if !equal {
        return Err(PyValueError::new_err(
            "Arrow round trip lost data or metadata",
        ));
    }
    Ok(json!({
        "origin": "SYNTHETIC", "deliverable": false,
        "weights": weights, "expected_weights": GOLDEN_WEIGHTS,
        "absolute_tolerance": WEIGHT_TOLERANCE,
        "solver_status": solver_status, "solver": "CLARABEL",
        "covariance_estimator": "skfolio.moments.EmpiricalCovariance",
        "artifact": "native-weights.arrow", "arrow_round_trip_equal": equal
    }))
}

fn nautilus_probe(py: Python<'_>, output: &Path) -> PyResult<Value> {
    let instrument = py
        .import("nautilus_trader.test_kit.providers")?
        .getattr("TestInstrumentProvider")?
        .call_method0("adabtc_binance")?;
    let data = py.import("nautilus_trader.model.data")?;
    let objects = py.import("nautilus_trader.model.objects")?;
    let bar_type = data
        .getattr("BarType")?
        .call_method1("from_str", ("ADABTC.BINANCE-1-MINUTE-LAST-EXTERNAL",))?;
    let volume = objects
        .getattr("Quantity")?
        .call_method1("from_str", ("1000000.00000000",))?;
    let bars = PyList::empty(py);
    let prices = [
        2000, 2050, 2100, 2150, 2200, 2250, 2200, 2150, 2100, 2050, 2000, 1950, 2000, 2050, 2100,
        2150, 2200, 2250, 2300, 2350,
    ];
    for (index, number) in prices.iter().enumerate() {
        let price = objects
            .getattr("Price")?
            .call_method1("from_str", (format!("0.{number:08}"),))?;
        // The upstream EMA strategy deliberately ignores single-price bars.
        // Supply valid OHLC ranges rather than replacing its trading logic.
        let high = objects
            .getattr("Price")?
            .call_method1("from_str", (format!("0.{:08}", number + 10),))?;
        let low = objects
            .getattr("Price")?
            .call_method1("from_str", (format!("0.{:08}", number - 10),))?;
        let timestamp = 1_700_000_000_000_000_000_u64 + (index as u64 + 1) * 60_000_000_000;
        bars.append(data.getattr("Bar")?.call1((
            &bar_type, &price, &high, &low, &price, &volume, timestamp, timestamp,
        ))?)?;
    }
    let catalog_options = PyDict::new(py);
    catalog_options.set_item("path", output.join("catalog").to_string_lossy().as_ref())?;
    let catalog = py
        .import("nautilus_trader.persistence.catalog.parquet")?
        .getattr("ParquetDataCatalog")?
        .call((), Some(&catalog_options))?;
    let instruments = PyList::empty(py);
    instruments.append(&instrument)?;
    catalog.call_method1("write_data", (&instruments,))?;
    catalog.call_method1("write_data", (&bars,))?;
    let restored_bars = catalog.call_method0("bars")?;
    let restored_instruments = catalog.call_method0("instruments")?;
    if restored_bars.len()? != prices.len() || restored_instruments.len()? != 1 {
        return Err(PyValueError::new_err("native catalog round trip lost rows"));
    }

    let config = py.import("nautilus_trader.config")?;
    let logging_options = PyDict::new(py);
    logging_options.set_item("log_level", "ERROR")?;
    let logging = config
        .getattr("LoggingConfig")?
        .call((), Some(&logging_options))?;
    let engine_options = PyDict::new(py);
    engine_options.set_item("logging", logging)?;
    engine_options.set_item("run_analysis", false)?;
    let engine_config = config
        .getattr("BacktestEngineConfig")?
        .call((), Some(&engine_options))?;
    let engine = py
        .import("nautilus_trader.backtest.engine")?
        .getattr("BacktestEngine")?
        .call1((engine_config,))?;
    let result: PyResult<Value> = (|| {
        let enums = py.import("nautilus_trader.model.enums")?;
        let currency = py
            .import("nautilus_trader.model.currencies")?
            .getattr("BTC")?;
        let balances = PyList::empty(py);
        balances.append(objects.getattr("Money")?.call1((1, &currency))?)?;
        let venue_options = PyDict::new(py);
        venue_options.set_item("venue", instrument.getattr("id")?.getattr("venue")?)?;
        venue_options.set_item("oms_type", enums.getattr("OmsType")?.getattr("NETTING")?)?;
        venue_options.set_item(
            "account_type",
            enums.getattr("AccountType")?.getattr("MARGIN")?,
        )?;
        venue_options.set_item("base_currency", currency)?;
        venue_options.set_item("starting_balances", balances)?;
        engine.call_method("add_venue", (), Some(&venue_options))?;
        engine.call_method1("add_instrument", (restored_instruments.get_item(0)?,))?;
        engine.call_method1("add_data", (restored_bars,))?;
        let strategy_options = PyDict::new(py);
        strategy_options.set_item("instrument_id", instrument.getattr("id")?)?;
        strategy_options.set_item("bar_type", &bar_type)?;
        strategy_options.set_item(
            "trade_size",
            py.import("decimal")?.getattr("Decimal")?.call1(("100",))?,
        )?;
        strategy_options.set_item("fast_ema_period", 2)?;
        strategy_options.set_item("slow_ema_period", 4)?;
        strategy_options.set_item("request_bars", false)?;
        strategy_options.set_item("subscribe_trade_ticks", false)?;
        let upstream = py.import("nautilus_trader.examples.strategies.ema_cross")?;
        let strategy_config = upstream
            .getattr("EMACrossConfig")?
            .call((), Some(&strategy_options))?;
        let strategy = upstream.getattr("EMACross")?.call1((strategy_config,))?;
        engine.call_method1("add_strategy", (strategy,))?;
        engine.call_method0("run")?;
        let native = engine.call_method0("get_result")?;
        let iterations: usize = native.getattr("iterations")?.extract()?;
        let orders: usize = native.getattr("total_orders")?.extract()?;
        let events: usize = native.getattr("total_events")?.extract()?;
        if iterations != prices.len() || orders == 0 || events == 0 {
            return Err(PyValueError::new_err(format!(
                "native backtest fixture incomplete: iterations={iterations}, orders={orders}, events={events}"
            )));
        }
        Ok(json!({
            "origin": "SYNTHETIC", "deliverable": false,
            "native_strategy": "nautilus_trader.examples.strategies.ema_cross.EMACross",
            "iterations": iterations, "simulated_orders": orders, "native_events": events,
            "catalog_rows": prices.len(),
            "scope": "ABI/catalog/backtest smoke, NOT target-portfolio acceptance"
        }))
    })();
    // Dispose on both success and error; process exit is the final isolation boundary.
    let disposed = engine.call_method0("dispose");
    let result = result?;
    disposed?;
    Ok(result)
}

fn verify(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // No overwrite or reuse of a previous evidence directory.
    fs::create_dir(output)?;
    let evidence = Python::attach(|py| -> PyResult<Value> {
        let metadata = py.import("importlib.metadata")?;
        let mut versions = BTreeMap::new();
        for distribution in [
            "nautilus-trader",
            "skfolio",
            "pyarrow",
            "optuna",
            "cvxpy-base",
            "clarabel",
            "numpy",
        ] {
            let version: String = metadata
                .call_method1("version", (distribution,))?
                .extract()?;
            versions.insert(distribution, version);
        }
        let python: String = py
            .import("platform")?
            .call_method0("python_version")?
            .extract()?;
        let portfolio = portfolio_golden(py, output)?;
        let nautilus = nautilus_probe(py, output)?;
        Ok(json!({
            "schema_version": 1, "kind": "W0_NATIVE_COMPATIBILITY_PROBE",
            "origin": "SYNTHETIC", "deliverable": false,
            "python": python, "pyo3": "0.29.2", "engine_versions": versions,
            "portfolio_golden": portfolio, "nautilus": nautilus
        }))
    })?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("evidence.json"))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &evidence)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "verify-native" {
        eprintln!("Usage: qz-job verify-native <new-output-directory>");
        return ExitCode::from(2);
    }
    match verify(Path::new(&args[2])) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("W0 verification failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check_golden;

    #[test]
    fn oracle_rejects_missing_nonfinite_and_wrong_results() {
        assert!(check_golden(&[0.8, 0.2]).is_ok());
        for values in [
            vec![],
            vec![0.8],
            vec![f64::NAN, 0.2],
            vec![f64::INFINITY, 0.2],
            vec![1.0, 0.0],
        ] {
            assert!(check_golden(&values).is_err());
        }
    }
}
