//! Optional owner-built L2 development profile, not a default QZ runtime.
//! Runs only the already-exposed 2026-10-13 00:00..01:00 UTC window. Official
//! OBI dry_run forbids orders, including stop-time orders. This is execution
//! validation, not a profitable strategy, an untouched OOS run or account access.
use anyhow::{ensure, Result};
use job::native_configured::ConfiguredNativeRun;
use nautilus_backtest::config::{
    BacktestDataConfig, BacktestEngineConfig, BacktestRunConfig, BacktestVenueConfig,
};
use nautilus_common::logging::logger::LoggerConfig;
use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
use nautilus_model::{
    data::NautilusDataType,
    enums::{AccountType, BookType, OmsType, TimeInForce},
    identifiers::InstrumentId,
    instruments::Instrument,
};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog, config::DataCatalogConfig,
};
use nautilus_trading::examples::strategies::orderbook_imbalance::{
    OrderBookImbalance, OrderBookImbalanceConfig,
};
use serde::Deserialize;
use std::path::Path;

const NAME: &str = "l2-development-hour-static-dry-run";
const START: u64 = 1791849600000000000;
const END_EXCLUSIVE: u64 = 1791853200000000000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    instrument_id: String,
    // Caller must read the actual retained definition. No timestamp rewriting.
    instrument_ts_init_ns: String,
    instrument_ts_event_ns: String,
}

fn parameters(name: &str, value: &serde_json::Value) -> Result<Parameters> {
    ensure!(name == NAME, "DEVELOPMENT_PROFILE_UNKNOWN");
    let p: Parameters = serde_json::from_value(value.clone())?;
    ensure!(
        matches!(
            p.instrument_id.as_str(),
            "BTC-USD-PERP.HYPERLIQUID" | "ETH-USD-PERP.HYPERLIQUID"
        ),
        "DEVELOPMENT_INSTRUMENT_UNSUPPORTED"
    );
    Ok(p)
}

fn factory(root: &Path, name: &str, value: &serde_json::Value) -> Result<ConfiguredNativeRun> {
    let p = parameters(name, value)?;
    let id: InstrumentId = p.instrument_id.parse()?;
    let catalog_path = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("CATALOG_PATH_UTF8"))?;
    let catalog = ParquetDataCatalog::from_uri(catalog_path, None, Some(4096), None, None)?;
    let definitions = catalog.query_instruments_filtered(Some(&[p.instrument_id]), None, None)?;
    ensure!(
        definitions.len() == 1,
        "STATIC_SCENARIO_REQUIRES_SINGLE_DEFINITION"
    );
    let definition = &definitions[0];
    ensure!(
        definition.id() == id
            && definition.ts_init().as_u64() == p.instrument_ts_init_ns.parse::<u64>()?
            && definition.ts_event().as_u64() == p.instrument_ts_event_ns.parse::<u64>()?,
        "STATIC_SCENARIO_DEFINITION_CLOCK_MISMATCH"
    );
    // 6000 USDC is an independent no-order simulation denomination, not a
    // conversion of the user's USD budget. Rates are explicit current-tier
    // scenarios, not verified historical/account rates; no funding is claimed.
    let venue = BacktestVenueConfig::builder()
        .name("HYPERLIQUID")
        .account_type(AccountType::Margin)
        .oms_type(OmsType::Netting)
        .book_type(BookType::L2_MBP)
        .starting_balances(vec!["6000 USDC".into()])
        .base_currency("USDC".parse()?)
        .default_leverage("1".parse()?)
        .bar_execution(false)
        .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::new(
            "0.00015".parse()?,
            "0.00045".parse()?,
        )))
        .build()?;
    let data = BacktestDataConfig::builder()
        .data_type(NautilusDataType::OrderBookDelta)
        .catalog(DataCatalogConfig::new(catalog_path.into(), None, None))
        .instrument_id(id)
        .start_time(START.into())
        .end_time((END_EXCLUSIVE - 1).into())
        .optimize_file_loading(false)
        .build()?;
    let engine = BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .is_colored(false)
            .build()?,
        bypass_logging: true,
        shutdown_on_error: true,
        ..Default::default()
    };
    // Native bounds select the hour even if the source Parquet file spans more
    // hours. No synthetic QuoteTick/TradeTick or custom book assembly is used.
    let config = BacktestRunConfig::builder()
        .id(NAME.into())
        .venues(vec![venue])
        .data(vec![data])
        .engine(engine)
        .chunk_size(4096)
        .start(START.into())
        .end((END_EXCLUSIVE - 1).into())
        .raise_exception(true)
        .dispose_on_completion(false)
        .build()?;
    let strategy = OrderBookImbalance::from_config(
        OrderBookImbalanceConfig::builder()
            .instrument_id(id)
            .max_trade_size("0.01".parse().map_err(anyhow::Error::msg)?)
            .trigger_min_size("0".parse()?)
            .trigger_imbalance_ratio("0.2".parse()?)
            .min_seconds_between_triggers(30.0)
            .book_type(BookType::L2_MBP)
            .time_in_force(TimeInForce::Ioc)
            .dry_run(true)
            .build(),
    )?;
    Ok(ConfiguredNativeRun {
        config,
        install: Box::new(move |engine| engine.add_strategy(strategy)),
    })
}

fn main() {
    job::native_runner::main_with_hooks(None, Some(factory), None);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn development_profile_does_not_accept_window_or_order_overrides() {
        let p = serde_json::json!({"instrument_id":"BTC-USD-PERP.HYPERLIQUID",
            "instrument_ts_init_ns":"1794382317589870164",
            "instrument_ts_event_ns":"1794382317589870164"});
        assert!(parameters(NAME, &p).is_ok());
        assert!(parameters("other", &p).is_err());
        for key in ["start_ns", "end_ns", "dry_run", "starting_balances"] {
            let mut changed = p.clone();
            changed[key] = serde_json::json!(false);
            assert!(parameters(NAME, &changed).is_err());
        }
        let mut changed = p;
        changed["instrument_id"] = serde_json::json!("AIXBT-USD-PERP.HYPERLIQUID");
        assert!(parameters(NAME, &changed).is_err());
    }
}
