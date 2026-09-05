//! A one-process native compatibility probe for Issue #62 W0.
//!
//! This executable does not grant qualifications, publish releases, or implement
//! an optimizer/backtester. All financial calculations below are upstream calls.
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use pyo3::exceptions::PyAssertionError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde::Serialize;

#[derive(Serialize)]
struct ProbeReport {
    schema_version: u8,
    origin: &'static str,
    deliverable: bool,
    versions: BTreeMap<String, String>,
    minimum_variance_weights: Vec<f64>,
    nautilus_iterations: u64,
    nautilus_fill_rows: usize,
    arrow_file: &'static str,
}

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

fn native_minimum_variance(py: Python<'_>) -> PyResult<Vec<f64>> {
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
    let estimator = optimization.getattr("MeanRisk")?.call((), Some(&kwargs))?;
    estimator.call_method1("fit", (&returns,))?;
    let weights: Vec<f64> = estimator
        .getattr("weights_")?
        .call_method0("tolist")?
        .extract()?;
    verify_weights(&weights).map_err(PyAssertionError::new_err)?;
    Ok(weights)
}

fn native_backtest(py: Python<'_>) -> PyResult<(u64, usize)> {
    eprintln!("native-probe: Nautilus BacktestEngine / upstream EMA strategy");
    let logging_kwargs = PyDict::new(py);
    logging_kwargs.set_item("log_level", "ERROR")?;
    let logging = py
        .import("nautilus_trader.config")?
        .getattr("LoggingConfig")?
        .call((), Some(&logging_kwargs))?;
    let config_kwargs = PyDict::new(py);
    config_kwargs.set_item("logging", logging)?;
    let config = py
        .import("nautilus_trader.backtest.config")?
        .getattr("BacktestEngineConfig")?
        .call((), Some(&config_kwargs))?;
    let engine_kwargs = PyDict::new(py);
    engine_kwargs.set_item("config", config)?;
    let engine = py
        .import("nautilus_trader.backtest.engine")?
        .getattr("BacktestEngine")?
        .call((), Some(&engine_kwargs))?;

    // Dispose even when data loading, native execution or report extraction fails.
    let result = run_native_backtest(py, &engine);
    let disposed = engine.call_method0("dispose");
    let result = result?;
    disposed?;
    Ok(result)
}

fn run_native_backtest(py: Python<'_>, engine: &Bound<'_, PyAny>) -> PyResult<(u64, usize)> {
    let identifiers = py.import("nautilus_trader.model.identifiers")?;
    let venue = identifiers.getattr("Venue")?.call1(("XNAS",))?;
    let enums = py.import("nautilus_trader.model.enums")?;
    let objects = py.import("nautilus_trader.model.objects")?;
    let usd = py.import("nautilus_trader.model.currencies")?.getattr("USD")?;
    let starting_balance = objects.getattr("Money")?.call1((100_000.0, &usd))?;
    let venue_kwargs = PyDict::new(py);
    venue_kwargs.set_item("venue", &venue)?;
    venue_kwargs.set_item("oms_type", enums.getattr("OmsType")?.getattr("NETTING")?)?;
    venue_kwargs.set_item("account_type", enums.getattr("AccountType")?.getattr("CASH")?)?;
    venue_kwargs.set_item("base_currency", &usd)?;
    venue_kwargs.set_item("starting_balances", PyList::new(py, [starting_balance])?)?;
    engine.call_method("add_venue", (), Some(&venue_kwargs))?;

    // Upstream test-kit instruments and upstream example strategy are restricted
    // to this explicitly FIXTURE-marked compatibility probe.
    let instrument_kwargs = PyDict::new(py);
    instrument_kwargs.set_item("symbol", "AAPL")?;
    instrument_kwargs.set_item("venue", "XNAS")?;
    let instrument = py
        .import("nautilus_trader.test_kit.providers")?
        .getattr("TestInstrumentProvider")?
        .call_method("equity", (), Some(&instrument_kwargs))?;
    engine.call_method1("add_instrument", (&instrument,))?;
    let instrument_id = instrument.getattr("id")?;
    let data = py.import("nautilus_trader.model.data")?;
    let bar_type = data
        .getattr("BarType")?
        .call_method1("from_str", ("AAPL.XNAS-1-MINUTE-LAST-EXTERNAL",))?;
    let price_type = objects.getattr("Price")?;
    let volume = objects.getattr("Quantity")?.call_method1("from_int", (10_000,))?;
    let bars = PyList::empty(py);
    for index in 0_u64..64 {
        let close = if index < 32 { 100 + index } else { 163 - index };
        let price = price_type.call_method1("from_str", (format!("{close}.00"),))?;
        let high = price_type.call_method1("from_str", (format!("{close}.50"),))?;
        let low = price_type.call_method1("from_str", (format!("{}.50", close - 1),))?;
        let timestamp = 1_735_689_600_000_000_000_u64 + index * 60_000_000_000;
        bars.append(data.getattr("Bar")?.call1((
            &bar_type, &price, high, low, &price, &volume, timestamp, timestamp,
        ))?)?;
    }
    engine.call_method1("add_data", (&bars,))?;
    let strategy_module = py.import("nautilus_trader.examples.strategies.ema_cross_long_only")?;
    let strategy_kwargs = PyDict::new(py);
    strategy_kwargs.set_item("instrument_id", instrument_id)?;
    strategy_kwargs.set_item("bar_type", bar_type)?;
    strategy_kwargs.set_item("trade_size", py.import("decimal")?.getattr("Decimal")?.call1(("1",))?)?;
    strategy_kwargs.set_item("fast_ema_period", 2)?;
    strategy_kwargs.set_item("slow_ema_period", 3)?;
    let strategy_config = strategy_module
        .getattr("EMACrossLongOnlyConfig")?
        .call((), Some(&strategy_kwargs))?;
    let strategy = strategy_module.getattr("EMACrossLongOnly")?.call1((strategy_config,))?;
    engine.call_method1("add_strategy", (strategy,))?;
    engine.call_method0("run")?;
    let native_result = engine.call_method0("get_result")?;
    let iterations: u64 = native_result.getattr("iterations")?.extract()?;
    let fills = engine
        .getattr("trader")?
        .call_method0("generate_order_fills_report")?;
    let rows: usize = fills.call_method0("__len__")?.extract()?;
    if iterations != 64 || rows == 0 {
        return Err(PyAssertionError::new_err("NATIVE_BACKTEST_DID_NOT_PROCESS_FIXTURE"));
    }
    Ok((iterations, rows))
}

fn write_arrow(py: Python<'_>, directory: &Path, weights: &[f64]) -> PyResult<()> {
    eprintln!("native-probe: Arrow IPC round trip");
    let arrow = py.import("pyarrow")?;
    let columns = PyDict::new(py);
    columns.set_item("fixture_asset", vec!["FIXTURE_A", "FIXTURE_B"])?;
    columns.set_item("weight", weights.to_vec())?;
    columns.set_item("origin", vec!["FIXTURE", "FIXTURE"])?;
    let table = arrow.getattr("Table")?.call_method1("from_pydict", (columns,))?;
    let path = directory.join("native-weights.arrow");
    let path = path.to_str().ok_or_else(|| PyAssertionError::new_err("INVALID_OUTPUT_PATH"))?;
    let sink = arrow.getattr("OSFile")?.call1((path, "wb"))?;
    let ipc = py.import("pyarrow.ipc")?;
    let result = (|| {
        let writer = ipc.getattr("new_file")?.call1((&sink, table.getattr("schema")?))?;
        let write_result = writer.call_method1("write_table", (&table,));
        let close_result = writer.call_method0("close");
        write_result?;
        close_result?;
        PyResult::Ok(())
    })();
    let closed = sink.call_method0("close");
    result?;
    closed?;
    let reader = ipc.getattr("open_file")?.call1((path,))?;
    let roundtrip = reader.call_method0("read_all")?;
    if !table.call_method1("equals", (&roundtrip,))?.extract::<bool>()? {
        return Err(PyAssertionError::new_err("ARROW_ROUNDTRIP_MISMATCH"));
    }
    Ok(())
}

fn probe(directory: &Path) -> PyResult<ProbeReport> {
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
            let actual: String = metadata.call_method1("version", (distribution,))?.extract()?;
            if actual != expected {
                return Err(PyAssertionError::new_err(format!("VERSION_MISMATCH:{distribution}")));
            }
            versions.insert(distribution.to_owned(), actual);
        }
        versions.insert("pyo3".to_owned(), "0.25.1".to_owned());
        versions.insert("python".to_owned(), py.import("platform")?.call_method0("python_version")?.extract()?);
        let weights = native_minimum_variance(py)?;
        let (iterations, fill_rows) = native_backtest(py)?;
        write_arrow(py, directory, &weights)?;
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

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 || args[1] != "verify-native" || args[2] != "--output" {
        return Err("usage: qz-job verify-native --output NEW_DIRECTORY".into());
    }
    let directory = Path::new(&args[3]);
    // Refuse pre-existing paths and accidental overwrites. This local operator
    // command is not an Agent-controlled path or a published artifact service.
    fs::create_dir(directory)?;
    let report = probe(directory)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("native-probe.json"))?;
    serde_json::to_writer_pretty(&mut file, &report)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!("native compatibility probe completed; origin=FIXTURE; deliverable=false");
    Ok(())
}

fn main() {
    if run().is_err() {
        // Never copy an upstream exception/traceback into public job output.
        eprintln!("QZ_NATIVE_PROBE_FAILED (see the last completed probe stage)");
        std::process::exit(1);
    }
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
