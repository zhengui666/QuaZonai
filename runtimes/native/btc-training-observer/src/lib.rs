//! Training-only observer of precomputed, source-clock-qualified external bars. No orders.
use anyhow::{Result, ensure};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_common::actor::DataActor;
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage, indicator::Indicator,
    momentum::rsi::RelativeStrengthIndex,
};
use nautilus_model::{
    data::{Bar, BarType},
    identifiers::{InstrumentId, StrategyId},
    instruments::Instrument,
};
use nautilus_trading::{
    nautilus_strategy,
    strategy::{StrategyConfig, StrategyCore},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs::File, io::Write, path::PathBuf};

const NAME: &str = "btc_training_external_second_bar_signal_v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    instrument_id: String,
    start_ns: String,
    end_ns: String,
    report_path: PathBuf,
    signals_path: PathBuf,
    bars_path: PathBuf,
}

#[derive(Debug)]
struct TradeIndicatorObserver {
    core: StrategyCore,
    id: InstrumentId,
    report_path: PathBuf,
    selected_start_ns: String,
    selected_end_ns: String,
    fast: ExponentialMovingAverage,
    slow: ExponentialMovingAverage,
    rsi: RelativeStrengthIndex,
    first: Option<u64>,
    last: Option<u64>,
    initialized_at: [Option<u64>; 3],
    instrument_basis: Option<Value>,
    started: bool,
    signals: File,
    prior_signal: i8,
    emitted_signals: u64,
    bars: File,
    bar_type: BarType,
    bars_seen: u64,
}
nautilus_strategy!(TradeIndicatorObserver);

impl DataActor for TradeIndicatorObserver {
    fn on_start(&mut self) -> Result<()> {
        let basis = {
            let cache = self.cache();
            let original = cache.try_instrument(&self.id)?;
            json!({"id":original.id().to_string(), "price_precision":original.price_precision(),
                "size_precision":original.size_precision(), "ts_event_ns":original.ts_event().as_u64().to_string(),
                "ts_init_ns":original.ts_init().as_u64().to_string(),
                "scope":"ORIGINAL_LATE_DEFINITION_RETAINED_AS_STATIC_SCENARIO_NOT_PIT"})
        };
        self.instrument_basis = Some(basis);
        self.subscribe_bars(self.bar_type, None, None);
        self.started = true;
        Ok(())
    }

    fn on_bar(&mut self, bar: &Bar) -> Result<()> {
        ensure!(bar.bar_type == self.bar_type, "UNEXPECTED_BAR");
        let open = bar.ts_event.as_u64();
        let available = bar.ts_init.as_u64();
        checked_bar_clock(open, available, self.last)?;
        self.first.get_or_insert(open);
        self.last = Some(available);
        self.bars_seen += 1;
        self.fast.handle_bar(bar);
        self.slow.handle_bar(bar);
        self.rsi.handle_bar(bar);
        for (i, ready) in [
            self.fast.initialized(),
            self.slow.initialized(),
            self.rsi.initialized(),
        ]
        .into_iter()
        .enumerate()
        {
            if ready && self.initialized_at[i].is_none() {
                self.initialized_at[i] = Some(self.bars_seen);
            }
        }
        let values = self.values()?;
        if let [Some(fast), Some(slow), Some(rsi)] = values {
            let signal = fixed_signal(fast, slow, rsi);
            if signal != self.prior_signal {
                serde_json::to_writer(
                    &mut self.signals,
                    &json!({
                    "instrument_id":self.id.to_string(),
                    "bar_open_ns":bar.ts_event.as_u64().to_string(),
                    "bar_close_ns":(bar.ts_event.as_u64()+1_000_000_000).to_string(),
                    "available_ns":bar.ts_init.as_u64().to_string(),
                    "bar_index":self.bars_seen.to_string(),
                    "signal":signal, "ema8":fast,"ema32":slow,"rsi14":rsi,
                    "scope":"AFTER_AVAILABLE_PRECOMPUTED_NONEMPTY_BAR_NO_ORDER_NO_FILL"}),
                )?;
                writeln!(self.signals)?;
                self.emitted_signals += 1;
                self.prior_signal = signal;
            }
        }
        serde_json::to_writer(
            &mut self.bars,
            &json!({"bar_type":bar.bar_type.to_string(),"ts_event":bar.ts_event.as_u64().to_string(),"ts_init":bar.ts_init.as_u64().to_string(),"open":bar.open.to_string(),"high":bar.high.to_string(),"low":bar.low.to_string(),"close":bar.close.to_string(),"volume":bar.volume.to_string(),"indicator_values":values,"scope":"OFFICIAL_CLOSED_NONEMPTY_BAR_INDICATOR_UPDATE"}),
        )?;
        writeln!(self.bars)?;
        Ok(())
    }
    fn on_stop(&mut self) -> Result<()> {
        self.unsubscribe_bars(self.bar_type, None, None);
        let report = json!({
            "scope":"FIXED_NATIVE_INDICATOR_SIGNAL_CANDIDATE_NOT_PREDICTIVE_VALIDATION",
            "selection":{"clock":"ts_init","start_inclusive_ns":self.selected_start_ns,"end_exclusive_ns":self.selected_end_ns,"native_end_inclusive":"end_exclusive-1"},
            "bars_seen":self.bars_seen.to_string(),
            "emitted_signal_transitions":self.emitted_signals.to_string(),
            "final_signal":self.prior_signal,
            "instrument_id":self.id.to_string(), "started":self.started, "on_stop_called":true,
            "first_event_ns":self.first.map(|v|v.to_string()), "last_available_ns":self.last.map(|v|v.to_string()),
            "first_initialized_nonempty_bar_index":self.initialized_at.map(|v|v.map(|n|n.to_string())),
            "last_native_indicator_values":self.values()?, "original_instrument":self.instrument_basis,
            "periods":{"ema_fast_nonempty_bars":8,"ema_slow_nonempty_bars":32,"rsi_price_changes":14},
            "notes":["Only official EMA/RSI calculations; fixed fast/slow and RSI midpoint signal comparisons. No labels, orders, position, return or performance calculations. Periods count nonempty one-second bars, not contiguous elapsed seconds. Missing seconds do not update indicators; no fills or forward fill. Signals are available only after bar callback. Bar ts_event is interval start; ts_init is max(interval end, last source node observation). This is not verified user receipt availability.",
                "6000 USDC is a no-order simulation balance, not a conversion of 6000 USD; zero fees are not verified historical costs. Window selects source-node availability ts_init in [start,end), not complete event coverage. Current inspected input is one partial-day shard, not complete hour/day coverage.",
                "Canonical account/financial fields are produced by the official engine and retained untouched; zero trading is not evidence of predictive skill or profitability.",
                "6000 USDC and zero fees are unchanged no-order scenario parameters, not verified collateral, conversion or historical costs. No orders or fills are implemented. Selection uses ts_init: the first selected bar interval may start before the selected availability window. No return or predictive validation is performed.",
                "This callback clock preserves source node observation delays but does not model user network or processing delay. The original current instrument is a hypothetical static scenario."]
        });
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&self.report_path)?;
        serde_json::to_writer(&mut file, &report)?;
        writeln!(file)?;
        Ok(())
    }
}
impl TradeIndicatorObserver {
    fn values(&self) -> Result<[Option<f64>; 3]> {
        let mut values = [None; 3];
        for (i, (ready, value)) in [
            (self.fast.initialized(), self.fast.value),
            (self.slow.initialized(), self.slow.value),
            (self.rsi.initialized(), self.rsi.value),
        ]
        .into_iter()
        .enumerate()
        {
            if ready {
                ensure!(value.is_finite(), "NONFINITE_INITIALIZED_INDICATOR");
                values[i] = Some(value);
            }
        }
        Ok(values)
    }
}
fn register(
    engine: &mut BacktestEngine,
    id: InstrumentId,
    name: &str,
    config: &Value,
) -> Result<()> {
    ensure!(name == NAME, "UNKNOWN_PRIVATE_OBSERVER");
    let params: Parameters = serde_json::from_value(config.clone())?;
    engine.add_strategy(TradeIndicatorObserver {
        core: StrategyCore::new_checked(StrategyConfig {
            strategy_id: Some(StrategyId::from("SECOND_BAR_OBSERVER-001")),
            order_id_tag: Some("001".into()),
            ..Default::default()
        })?,
        id,
        report_path: params.report_path,
        selected_start_ns: params.start_ns,
        selected_end_ns: params.end_ns,
        fast: ExponentialMovingAverage::new(8, None),
        slow: ExponentialMovingAverage::new(32, None),
        rsi: RelativeStrengthIndex::new(14, None),
        first: None,
        last: None,
        initialized_at: [None; 3],
        instrument_basis: None,
        started: false,
        signals: File::options()
            .write(true)
            .create_new(true)
            .open(&params.signals_path)?,
        prior_signal: 0,
        emitted_signals: 0,
        bars: File::options()
            .write(true)
            .create_new(true)
            .open(&params.bars_path)?,
        bar_type: format!("{}-1-SECOND-LAST-EXTERNAL", id).parse()?,
        bars_seen: 0,
    })
}
fn fixed_signal(fast: f64, slow: f64, rsi: f64) -> i8 {
    if fast > slow && rsi > 50.0 {
        1
    } else if fast < slow && rsi < 50.0 {
        -1
    } else {
        0
    }
}

const TRAINING_START: u64 = 1778284800000000000;
const TRAINING_END: u64 = 1778889600000000000; // 2026-05-16 00:00 UTC exclusive
const BTC: &str = "BTC-USD-PERP.HYPERLIQUID";
const SECOND: u64 = 1_000_000_000;

fn checked_window(params: &Parameters) -> Result<(u64, u64)> {
    ensure!(params.instrument_id == BTC, "BTC_TRAINING_ONLY");
    let start: u64 = params.start_ns.parse()?;
    let end: u64 = params.end_ns.parse()?;
    ensure!(
        TRAINING_START <= start && start < end && end <= TRAINING_END,
        "OUTSIDE_EXPOSED_TRAINING_WINDOW"
    );
    ensure!(
        start % SECOND == 0 && end % SECOND == 0,
        "WINDOW_MUST_USE_SECOND_BOUNDARIES"
    );
    Ok((start, end))
}
fn checked_bar_clock(open: u64, available: u64, last_available: Option<u64>) -> Result<()> {
    let close = open
        .checked_add(SECOND)
        .ok_or_else(|| anyhow::anyhow!("BAR_CLOCK_OVERFLOW"))?;
    ensure!(
        open % SECOND == 0 && available >= close,
        "BAR_AVAILABLE_BEFORE_INTERVAL_END"
    );
    ensure!(
        last_available.is_none_or(|last| available >= last),
        "AVAILABILITY_ORDER_REGRESSION"
    );
    Ok(())
}

/// Explicit source-node availability selection; this is not a point-in-time contract claim.
pub mod training_factory {
    use super::*;
    use nautilus_backtest::config::{
        BacktestDataConfig, BacktestEngineConfig, BacktestRunConfig, BacktestVenueConfig,
    };
    use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
    use nautilus_model::{
        data::NautilusDataType,
        enums::{AccountType, BookType, OmsType},
    };
    use nautilus_persistence::config::DataCatalogConfig;

    pub fn config(root: &std::path::Path, name: &str, value: &Value) -> Result<BacktestRunConfig> {
        ensure!(name == NAME, "UNKNOWN_PRIVATE_OBSERVER");
        let params: Parameters = serde_json::from_value(value.clone())?;
        let (start, end) = checked_window(&params)?;
        let catalog =
            nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog::from_uri(
                root.to_str()
                    .ok_or_else(|| anyhow::anyhow!("CATALOG_PATH"))?,
                None,
                Some(4096),
                None,
                None,
            )?;
        let definitions = catalog.query_instruments_filtered(Some(&[BTC.into()]), None, None)?;
        ensure!(
            definitions.len() == 1
                && definitions[0].id().to_string() == BTC
                && definitions[0].ts_init().as_u64() == 1794382317589870164
                && definitions[0].ts_event().as_u64() == 1794382317589870164,
            "STATIC_SCENARIO_DEFINITION_NOT_EXACT_OR_UNIQUE"
        );
        let venue = BacktestVenueConfig::builder()
            .name("HYPERLIQUID")
            .account_type(AccountType::Margin)
            .oms_type(OmsType::Netting)
            .book_type(BookType::L1_MBP)
            .starting_balances(vec!["6000 USDC".into()])
            .base_currency("USDC".parse()?)
            .bar_execution(false)
            .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
            .build()?;
        let data = BacktestDataConfig::builder()
            .data_type(NautilusDataType::Bar)
            .catalog(DataCatalogConfig::new(
                root.to_string_lossy().into_owned(),
                None,
                None,
            ))
            .bar_types(vec![format!("{BTC}-1-SECOND-LAST-EXTERNAL")])
            .start_time(start.into())
            .end_time((end - 1).into())
            .optimize_file_loading(false)
            .build()?;
        let mut engine = BacktestEngineConfig::default();
        engine.shutdown_on_error = true;
        BacktestRunConfig::builder()
            .id("btc-training-external-second-bar-observer".into())
            .venues(vec![venue])
            .data(vec![data])
            .engine(engine)
            .chunk_size(4096)
            .start(start.into())
            .end((end - 1).into())
            .raise_exception(true)
            .dispose_on_completion(false)
            .build()
            .map_err(Into::into)
    }
    pub fn install(engine: &mut BacktestEngine, name: &str, value: &Value) -> Result<()> {
        ensure!(name == NAME, "UNKNOWN_PRIVATE_OBSERVER");
        let params: Parameters = serde_json::from_value(value.clone())?;
        checked_window(&params)?;
        register(engine, BTC.parse()?, name, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> Parameters {
        serde_json::from_value(json!({"instrument_id":BTC,"start_ns":TRAINING_START.to_string(),"end_ns":(TRAINING_START+3600*SECOND).to_string(),"report_path":"/unused/report","signals_path":"/unused/signals","bars_path":"/unused/bars"})).unwrap()
    }
    #[test]
    fn explicit_training_window_only() {
        assert!(checked_window(&params()).is_ok());
        let mut p = params();
        p.instrument_id = "ETH-USD-PERP.HYPERLIQUID".into();
        assert!(checked_window(&p).is_err());
        let mut p = params();
        p.start_ns = (TRAINING_START - SECOND).to_string();
        assert!(checked_window(&p).is_err());
        let mut p = params();
        p.end_ns = (TRAINING_END + SECOND).to_string();
        assert!(checked_window(&p).is_err());
        let mut p = params();
        p.end_ns = p.start_ns.clone();
        assert!(checked_window(&p).is_err());
    }
    #[test]
    fn bar_is_closed_and_available_in_order() {
        assert!(checked_bar_clock(TRAINING_START, TRAINING_START + SECOND, None).is_ok());
        assert!(checked_bar_clock(TRAINING_START, TRAINING_START + SECOND - 1, None).is_err());
        assert!(
            checked_bar_clock(
                TRAINING_START,
                TRAINING_START + SECOND,
                Some(TRAINING_START + 2 * SECOND)
            )
            .is_err()
        );
        assert!(checked_bar_clock(u64::MAX, u64::MAX, None).is_err());
    }
    #[test]
    fn official_rsi_percent_scale_signal() {
        assert_eq!(fixed_signal(2.0, 1.0, 60.0), 1);
        assert_eq!(fixed_signal(1.0, 2.0, 40.0), -1);
        assert_eq!(fixed_signal(1.0, 1.0, 60.0), 0);
        assert_eq!(fixed_signal(2.0, 1.0, 50.0), 0);
    }
    #[test]
    fn reject_trading_settings() {
        let v = json!({"instrument_id":BTC,"start_ns":TRAINING_START.to_string(),"end_ns":TRAINING_END.to_string(),"report_path":"/unused/report","signals_path":"/unused/signals","bars_path":"/unused/bars","trade_size":"1"});
        assert!(serde_json::from_value::<Parameters>(v).is_err());
    }
}
