//! Shared compiled upstream constructors for backtest and explicitly configured account hosts.
use anyhow::{ensure, Result};
use contracts::native_strategy::NativeCompiledStrategyV1;
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
        NativeCompiledStrategyV1::HurstVpinDirectional { base, bar_type, trade_size, hurst_window, hurst_lags,
            hurst_enter, hurst_exit, vpin_window, vpin_threshold, max_holding_secs } => {
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
            if let Some(base) = base { config.base = serde_json::from_value(base.clone())?; }
            CompiledNativeStrategy::HurstVpinDirectional(Box::new(HurstVpinDirectional::new_checked(config)?))
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
