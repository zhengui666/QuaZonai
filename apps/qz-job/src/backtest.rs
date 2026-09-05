//! Fixed-fixture native-engine probe, not the production portfolio adapter.
use pyo3::exceptions::PyAssertionError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

pub(crate) fn native_backtest(py: Python<'_>) -> PyResult<(u64, usize)> {
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
    let usd = py
        .import("nautilus_trader.model.currencies")?
        .getattr("USD")?;
    let starting_balance = objects.getattr("Money")?.call1((100_000.0, &usd))?;
    let venue_kwargs = PyDict::new(py);
    venue_kwargs.set_item("venue", &venue)?;
    venue_kwargs.set_item("oms_type", enums.getattr("OmsType")?.getattr("NETTING")?)?;
    venue_kwargs.set_item(
        "account_type",
        enums.getattr("AccountType")?.getattr("CASH")?,
    )?;
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
    let volume = objects
        .getattr("Quantity")?
        .call_method1("from_int", (10_000,))?;
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
    strategy_kwargs.set_item(
        "trade_size",
        py.import("decimal")?.getattr("Decimal")?.call1(("1",))?,
    )?;
    strategy_kwargs.set_item("fast_ema_period", 2)?;
    strategy_kwargs.set_item("slow_ema_period", 3)?;
    let strategy_config = strategy_module
        .getattr("EMACrossLongOnlyConfig")?
        .call((), Some(&strategy_kwargs))?;
    let strategy = strategy_module
        .getattr("EMACrossLongOnly")?
        .call1((strategy_config,))?;
    engine.call_method1("add_strategy", (strategy,))?;
    engine.call_method0("run")?;
    let native_result = engine.call_method0("get_result")?;
    let iterations: u64 = native_result.getattr("iterations")?.extract()?;
    let fills = engine
        .getattr("trader")?
        .call_method0("generate_order_fills_report")?;
    let rows: usize = fills.call_method0("__len__")?.extract()?;
    if iterations != 64 || rows == 0 {
        return Err(PyAssertionError::new_err(
            "NATIVE_BACKTEST_DID_NOT_PROCESS_FIXTURE",
        ));
    }
    Ok((iterations, rows))
}
