//! Compileable owner module for a no-order CustomData callback validation.
//! Export from the owner library with `pub mod research;`.
use std::{path::Path, fmt};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use nautilus_backtest::{config::{BacktestRunConfig, BacktestVenueConfig}, engine::BacktestEngine};
use nautilus_common::actor::DataActor;
use nautilus_core::UnixNanos;
use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
use nautilus_model::{data::{CustomData, Data, DataType, HasTsInit}, enums::{AccountType, BookType, OmsType}, identifiers::StrategyId};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use nautilus_trading::{nautilus_strategy, strategy::{StrategyConfig, StrategyCore}};

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().with_context(|| format!("expected string: {key}"))
}
fn bounds(value: &Value) -> Result<(u64, u64)> {
    let start = text(value, "start_ns")?.parse()?;
    let end = text(value, "end_ns")?.parse()?;
    ensure!(start <= end, "start_ns must not exceed end_ns");
    Ok((start, end))
}

pub fn config(catalog: &Path, name: &str, value: &Value) -> Result<BacktestRunConfig> {
    ensure!(name == "custom-callback-check", "unknown configured run");
    ensure!(catalog.canonicalize()? == Path::new(text(value, "catalog")?).canonicalize()?, "catalog must equal the bound training catalog");
    let (start, end) = bounds(value)?;
    // Explicitly hypothetical, inert venue: this example never submits orders.
    // These values are NOT the user's balance or a realistic fee assumption.
    let venue = BacktestVenueConfig::builder()
        .name("CALLBACK_SIM").oms_type(OmsType::Netting)
        .account_type(AccountType::Cash).book_type(BookType::L1_MBP)
        .starting_balances(vec!["1 USD".into()])
        .bar_execution(false)
        .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
        .build()?;
    Ok(BacktestRunConfig::builder()
        .venues(vec![venue]).data(Vec::new())
        .start(UnixNanos::from(start)).end(UnixNanos::from(end))
        .raise_exception(true).dispose_on_completion(false).build()?)
}

pub fn install(engine: &mut BacktestEngine, name: &str, value: &Value) -> Result<()> {
    ensure!(name == "custom-callback-check", "unknown configured run");
    let (start, end) = bounds(value)?;
    let mut catalog = ParquetDataCatalog::new(Path::new(text(value, "catalog")?), None, None, None, None);
    // The generated launcher must register this actual source type before config/install.
    let data = catalog.query_custom_data_dynamic("ChainticksFillObservation", None,
        Some(start.into()), Some(end.into()), None, None, true)?;
    let data_type = match data.first() {
        Some(Data::Custom(row)) => row.data_type.clone(),
        _ => anyhow::bail!("no registered CustomData in the declared training window"),
    };
    for row in &data {
        ensure!((start..=end).contains(&row.ts_init().as_u64()), "record outside declared window");
    }
    let expected = data.len();
    engine.add_strategy(CallbackCheck {
        core: StrategyCore::new(StrategyConfig {
            strategy_id: Some(StrategyId::from("CALLBACK-001")),
            order_id_tag: Some("001".into()), ..Default::default()
        }), data_type, received: 0, expected,
    })?;
    engine.add_data(data, None, true, true)?;
    Ok(())
}

struct CallbackCheck { core: StrategyCore, data_type: DataType, received: usize, expected: usize }
impl fmt::Debug for CallbackCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackCheck").field("received", &self.received).finish()
    }
}
nautilus_strategy!(CallbackCheck);
impl DataActor for CallbackCheck {
    fn on_start(&mut self) -> Result<()> {
        self.subscribe_data(self.data_type.clone(), None, None);
        Ok(())
    }
    fn on_data(&mut self, data: &CustomData) -> Result<()> {
        // Uses the registered dynamic type; do not redeclare a look-alike source struct.
        let row: Value = serde_json::from_str(&data.data.to_json()?)?;
        ensure!(row["classification"].is_string(), "missing source classification");
        self.received += 1;
        // Real research must explicitly consume the source classification and clocks.
        // No signal, orders, fee-adjusted return or profitability is implemented here.
        Ok(())
    }
    fn on_stop(&mut self) -> Result<()> {
        ensure!(self.received == self.expected, "callback count differs from loaded records");
        eprintln!("CUSTOM_CALLBACK_CHECK received={} expected={} orders_submitted=0", self.received, self.expected);
        Ok(())
    }
}
