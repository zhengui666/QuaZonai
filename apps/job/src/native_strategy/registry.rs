//! Compiled upstream constructors and the static user-crate registration boundary.
use anyhow::{ensure, Result};
use contracts::native_strategy::NativeCompiledStrategyV1;
use nautilus_backtest::engine::BacktestEngine;
use nautilus_model::{enums::BookType, identifiers::InstrumentId};
use nautilus_trading::examples::strategies::{
    ema_cross::{EmaCross, EmaCrossConfig},
    grid_mm::{GridMarketMaker, GridMarketMakerConfig},
    hurst_vpin_directional::{HurstVpinDirectional, HurstVpinDirectionalConfig},
    orderbook_imbalance::{OrderBookImbalance, OrderBookImbalanceConfig},
};

pub enum CompiledNativeStrategy {
    EmaCross(EmaCross),
    GridMarketMaker(Box<GridMarketMaker>),
    HurstVpinDirectional(Box<HurstVpinDirectional>),
    OrderBookImbalance(OrderBookImbalance),
}

/// No dynamic code loading and no alternate implementation of the strategies.
pub fn build_strategy(
    id: InstrumentId,
    config: &NativeCompiledStrategyV1,
) -> Result<CompiledNativeStrategy> {
    Ok(match config {
        NativeCompiledStrategyV1::Registered { .. } => {
            anyhow::bail!("NATIVE_REGISTERED_STRATEGY_REQUIRES_USER_RUNNER")
        }
        NativeCompiledStrategyV1::EmaCross {
            base,
            trade_size,
            fast_period,
            slow_period,
        } => {
            ensure!(
                *fast_period > 0 && fast_period < slow_period,
                "NATIVE_EMA_PERIODS"
            );
            let mut config = EmaCrossConfig::builder()
                .instrument_id(id)
                .trade_size(trade_size.parse().map_err(anyhow::Error::msg)?)
                .fast_period(*fast_period)
                .slow_period(*slow_period)
                .build();
            if let Some(base) = base {
                config.base = serde_json::from_value(base.clone())?;
            }
            CompiledNativeStrategy::EmaCross(EmaCross::from_config(config))
        }
        NativeCompiledStrategyV1::GridMarketMaker {
            base,
            trade_size,
            num_levels,
            grid_step_bps,
            skew_factor,
            max_position,
            requote_threshold_bps,
            expire_time_secs,
            on_cancel_resubmit,
        } => {
            let mut config = GridMarketMakerConfig::builder()
                .instrument_id(id)
                .maybe_trade_size(
                    trade_size
                        .as_deref()
                        .map(str::parse)
                        .transpose()
                        .map_err(anyhow::Error::msg)?,
                )
                .num_levels(*num_levels)
                .grid_step_bps(*grid_step_bps)
                .skew_factor(*skew_factor)
                .max_position(max_position.parse().map_err(anyhow::Error::msg)?)
                .requote_threshold_bps(*requote_threshold_bps)
                .maybe_expire_time_secs(*expire_time_secs)
                .on_cancel_resubmit(*on_cancel_resubmit)
                .build();
            if let Some(base) = base {
                config.base = serde_json::from_value(base.clone())?;
            }
            CompiledNativeStrategy::GridMarketMaker(Box::new(GridMarketMaker::new(config)))
        }
        NativeCompiledStrategyV1::HurstVpinDirectional {
            base,
            bar_type,
            trade_size,
            hurst_window,
            hurst_lags,
            hurst_enter,
            hurst_exit,
            vpin_window,
            vpin_threshold,
            max_holding_secs,
        } => {
            let mut config = HurstVpinDirectionalConfig::builder()
                .instrument_id(id)
                .bar_type(bar_type.parse().map_err(anyhow::Error::msg)?)
                .trade_size(trade_size.parse().map_err(anyhow::Error::msg)?)
                .hurst_window(*hurst_window)
                .hurst_lags(hurst_lags.clone())
                .hurst_enter(*hurst_enter)
                .hurst_exit(*hurst_exit)
                .vpin_window(*vpin_window)
                .vpin_threshold(*vpin_threshold)
                .max_holding_secs(*max_holding_secs)
                .build();
            if let Some(base) = base {
                config.base = serde_json::from_value(base.clone())?;
            }
            CompiledNativeStrategy::HurstVpinDirectional(Box::new(
                HurstVpinDirectional::new_checked(config)?,
            ))
        }
        NativeCompiledStrategyV1::OrderBookImbalance {
            base,
            max_trade_size,
            trigger_min_size,
            trigger_imbalance_ratio,
            min_seconds_between_triggers,
            time_in_force,
            dry_run,
        } => {
            let mut config = OrderBookImbalanceConfig::builder()
                .instrument_id(id)
                .max_trade_size(max_trade_size.parse().map_err(anyhow::Error::msg)?)
                .trigger_min_size(trigger_min_size.parse()?)
                .trigger_imbalance_ratio(trigger_imbalance_ratio.parse()?)
                .min_seconds_between_triggers(*min_seconds_between_triggers)
                .book_type(BookType::L2_MBP)
                .time_in_force(time_in_force.parse()?)
                .dry_run(*dry_run)
                .build();
            if let Some(base) = base {
                config.base = serde_json::from_value(base.clone())?;
            }
            CompiledNativeStrategy::OrderBookImbalance(OrderBookImbalance::from_config(config)?)
        }
    })
}

/// Static user-crate registration. Implementations construct their own official Nautilus
/// Strategy type and call engine.add_strategy; QZ does not implement strategy behavior.
/// The callback runs once for every fresh backtest/WF engine, with original configuration.
pub type StrategyRegistration = fn(
    engine: &mut BacktestEngine,
    instrument_id: InstrumentId,
    name: &str,
    config: &serde_json::Value,
) -> Result<()>;

pub fn add_strategy(
    engine: &mut BacktestEngine,
    id: InstrumentId,
    config: &NativeCompiledStrategyV1,
    registration: Option<StrategyRegistration>,
) -> Result<()> {
    if let NativeCompiledStrategyV1::Registered { name, config } = config {
        let register = registration
            .ok_or_else(|| anyhow::anyhow!("NATIVE_REGISTERED_STRATEGY_REQUIRES_USER_RUNNER"))?;
        register(engine, id, name, config)?;
        ensure!(
            !engine.kernel().trader.borrow().strategy_ids().is_empty(),
            "NATIVE_REGISTERED_STRATEGY_NOT_INSTALLED"
        );
        return Ok(());
    }
    match build_strategy(id, config)? {
        CompiledNativeStrategy::EmaCross(strategy) => engine.add_strategy(strategy),
        CompiledNativeStrategy::HurstVpinDirectional(strategy) => engine.add_strategy(*strategy),
        CompiledNativeStrategy::GridMarketMaker(strategy) => engine.add_strategy(*strategy),
        CompiledNativeStrategy::OrderBookImbalance(strategy) => engine.add_strategy(strategy),
    }
}

#[cfg(test)]
mod registration_tests {
    use super::*;

    #[test]
    fn registration_receives_original_parameters_and_adds_an_official_strategy() {
        fn register(
            engine: &mut BacktestEngine,
            id: InstrumentId,
            name: &str,
            config: &serde_json::Value,
        ) -> Result<()> {
            assert_eq!(name, "user::ema");
            assert_eq!(config["trade_size"], "1.00000000");
            let native = EmaCrossConfig::builder()
                .instrument_id(id)
                .trade_size(
                    config["trade_size"]
                        .as_str()
                        .unwrap()
                        .parse()
                        .map_err(anyhow::Error::msg)?,
                )
                .fast_period(2)
                .slow_period(3)
                .build();
            engine.add_strategy(EmaCross::from_config(native))
        }
        let mut engine = BacktestEngine::new(Default::default()).unwrap();
        let config = NativeCompiledStrategyV1::Registered {
            name: "user::ema".into(),
            config: serde_json::json!({"trade_size": "1.00000000"}),
        };
        add_strategy(
            &mut engine,
            "BTC.SIM".parse().unwrap(),
            &config,
            Some(register),
        )
        .unwrap();
    }

    #[test]
    fn unregistered_user_strategy_never_falls_back_to_a_builtin() {
        let request = NativeCompiledStrategyV1::Registered {
            name: "user::strategy".into(),
            config: serde_json::json!({"quantity": "0.000000000000000001"}),
        };
        let error = build_strategy("BTC.SIM".parse().unwrap(), &request)
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "NATIVE_REGISTERED_STRATEGY_REQUIRES_USER_RUNNER"
        );
        let encoded = serde_json::to_value(&request).unwrap();
        let decoded: NativeCompiledStrategyV1 = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
    }
}
