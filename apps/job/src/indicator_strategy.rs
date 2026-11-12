//! Thin signal mapping over official indicators, updated by Nautilus before on_bar.
use anyhow::{ensure, Result};
use contracts::indicator_strategy::NativeIndicatorObservationV1;
use contracts::{
    indicator_strategy::{NativeIndicatorSignalV1, NativeIndicatorStrategyRequestV1},
    portfolio::AllocationTargetV1,
    science::NativeTargetPointV1,
    DbCounter, DecimalValue, SchemaV1,
};
use nautilus_common::actor::indicators::SharedActorIndicator;
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage,
    momentum::{macd::MovingAverageConvergenceDivergence, rsi::RelativeStrengthIndex},
};
use nautilus_indicators::{
    average::vwap::VolumeWeightedAveragePrice,
    indicator::Indicator,
    momentum::stochastics::Stochastics,
    volatility::{atr::AverageTrueRange, dc::DonchianChannel},
};
use nautilus_model::data::Bar;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(crate) enum Signal {
    Ema(
        Rc<RefCell<ExponentialMovingAverage>>,
        Rc<RefCell<ExponentialMovingAverage>>,
    ),
    Rsi(Rc<RefCell<RelativeStrengthIndex>>, f64, f64),
    Macd(Rc<RefCell<MovingAverageConvergenceDivergence>>),
}

struct Observations {
    atr: Rc<RefCell<AverageTrueRange>>,
    vwap: Rc<RefCell<VolumeWeightedAveragePrice>>,
    stochastics: Rc<RefCell<Stochastics>>,
    donchian: Rc<RefCell<DonchianChannel>>,
}

pub(crate) struct IndicatorStrategy {
    same_time_remaining: BTreeMap<u64, usize>,
    pub skipped_unsettled: Rc<RefCell<usize>>,
    pub merged_same_time: Rc<RefCell<usize>>,
    observations: Option<Observations>,
    pub rows: Rc<RefCell<Vec<NativeIndicatorObservationV1>>>,
    signal: Signal,
    request: NativeIndicatorStrategyRequestV1,
    long: bool,
    pub generated: Rc<RefCell<usize>>,
}

impl IndicatorStrategy {
    pub(crate) fn new(request: &NativeIndicatorStrategyRequestV1) -> Result<Self> {
        ensure!(
            request.selection.bar_types.len() == 1,
            "INDICATOR_STRATEGY_SINGLE_SERIES_REQUIRED"
        );
        ensure!(
            request.decision_start_ns >= request.selection.event_start_ns
                && request.decision_start_ns <= request.selection.decision_cutoff_ns,
            "INDICATOR_STRATEGY_DECISION_RANGE"
        );
        ensure!(
            request.target_ttl_seconds > 0,
            "INDICATOR_STRATEGY_TTL_INVALID"
        );
        ensure!(
            request.long_weight.is_nonnegative()
                && request.long_weight.as_decimal() <= &bigdecimal::BigDecimal::from(1),
            "INDICATOR_STRATEGY_WEIGHT_INVALID"
        );
        let period = |n: u32| -> Result<usize> {
            ensure!(
                (1..=16_777_216).contains(&n),
                "INDICATOR_STRATEGY_PERIOD_INVALID"
            );
            Ok(n as usize)
        };
        let signal = match request.signal {
            NativeIndicatorSignalV1::EmaCross {
                fast_period,
                slow_period,
            } => {
                ensure!(fast_period < slow_period, "INDICATOR_STRATEGY_PERIOD_ORDER");
                Signal::Ema(
                    Rc::new(RefCell::new(ExponentialMovingAverage::new(
                        period(fast_period)?,
                        None,
                    ))),
                    Rc::new(RefCell::new(ExponentialMovingAverage::new(
                        period(slow_period)?,
                        None,
                    ))),
                )
            }
            NativeIndicatorSignalV1::Rsi {
                period: n,
                entry_below,
                exit_above,
            } => {
                ensure!(
                    entry_below.is_finite()
                        && exit_above.is_finite()
                        && 0.0 <= entry_below
                        && entry_below < exit_above
                        && exit_above <= 100.0,
                    "INDICATOR_STRATEGY_RSI_THRESHOLDS"
                );
                Signal::Rsi(
                    Rc::new(RefCell::new(RelativeStrengthIndex::new(period(n)?, None))),
                    entry_below,
                    exit_above,
                )
            }
            NativeIndicatorSignalV1::Macd {
                fast_period,
                slow_period,
                signal_period,
            } => {
                ensure!(fast_period < slow_period, "INDICATOR_STRATEGY_PERIOD_ORDER");
                Signal::Macd(Rc::new(RefCell::new(
                    MovingAverageConvergenceDivergence::new(
                        period(fast_period)?,
                        period(slow_period)?,
                        Some(period(signal_period)?),
                        None,
                        None,
                    ),
                )))
            }
        };
        let observations = request
            .observations
            .as_ref()
            .map(|config| -> Result<Observations> {
                Ok(Observations {
                    atr: Rc::new(RefCell::new(AverageTrueRange::new(
                        period(config.atr_period)?,
                        None,
                        Some(true),
                        None,
                    ))),
                    vwap: Rc::new(RefCell::new(VolumeWeightedAveragePrice::new())),
                    stochastics: Rc::new(RefCell::new(Stochastics::new(
                        period(config.stochastics_k_period)?,
                        period(config.stochastics_d_period)?,
                    ))),
                    donchian: Rc::new(RefCell::new(DonchianChannel::new(period(
                        config.donchian_period,
                    )?))),
                })
            })
            .transpose()?;
        Ok(Self {
            same_time_remaining: BTreeMap::new(),
            skipped_unsettled: Rc::new(RefCell::new(0)),
            merged_same_time: Rc::new(RefCell::new(0)),
            observations,
            rows: Rc::new(RefCell::new(Vec::new())),
            signal,
            request: request.clone(),
            long: false,
            generated: Rc::new(RefCell::new(0)),
        })
    }

    /// Catalog timestamps supply batch boundaries, never future indicator values.
    pub(crate) fn set_bar_batches(&mut self, bars: &[Bar]) {
        for bar in bars {
            *self
                .same_time_remaining
                .entry(bar.ts_init.as_u64())
                .or_default() += 1;
        }
        self.same_time_remaining.retain(|_, count| *count > 1);
    }

    pub(crate) fn batch_complete(&mut self, bar: &Bar) -> bool {
        if let Some(remaining) = self.same_time_remaining.get_mut(&bar.ts_init.as_u64()) {
            *remaining -= 1;
            if *remaining > 0 {
                *self.merged_same_time.borrow_mut() += 1;
                return false;
            }
            self.same_time_remaining.remove(&bar.ts_init.as_u64());
        }
        true
    }

    pub(crate) fn batches_complete(&self) -> bool {
        self.same_time_remaining.is_empty()
    }

    pub(crate) fn can_trade_at(&self, bar: &Bar) -> bool {
        bar.ts_init.as_u64() >= self.request.decision_start_ns.get()
    }

    pub(crate) fn replay_end(&self) -> Result<u64> {
        let end = self
            .request
            .selection
            .decision_cutoff_ns
            .get()
            .checked_add(u64::from(self.request.target_ttl_seconds) * 1_000_000_000)
            .ok_or_else(|| anyhow::anyhow!("INDICATOR_STRATEGY_EXPIRY_OVERFLOW"))?;
        DbCounter::new(end).map_err(anyhow::Error::msg)?;
        Ok(end)
    }

    pub(crate) fn indicators(&self) -> Vec<SharedActorIndicator> {
        let mut indicators: Vec<SharedActorIndicator> = match &self.signal {
            Signal::Ema(fast, slow) => vec![fast.clone(), slow.clone()],
            Signal::Rsi(rsi, ..) => vec![rsi.clone()],
            Signal::Macd(macd) => vec![macd.clone()],
        };
        if let Some(values) = &self.observations {
            indicators.extend([
                values.atr.clone() as SharedActorIndicator,
                values.vwap.clone(),
                values.stochastics.clone(),
                values.donchian.clone(),
            ]);
        }
        indicators
    }

    pub(crate) fn observe(&self, bar: &Bar) -> Result<()> {
        if let Some(values) = &self.observations {
            let atr = values.atr.borrow();
            let vwap = values.vwap.borrow();
            let stochastics = values.stochastics.borrow();
            let donchian = values.donchian.borrow();
            self.rows.borrow_mut().push(NativeIndicatorObservationV1 {
                event_ns: DbCounter::new(bar.ts_event.as_u64()).map_err(anyhow::Error::msg)?,
                available_ns: DbCounter::new(bar.ts_init.as_u64()).map_err(anyhow::Error::msg)?,
                atr: atr.initialized().then_some(atr.value),
                vwap: vwap.initialized().then_some(vwap.value),
                stochastics_k: stochastics.initialized().then_some(stochastics.value_k),
                stochastics_d: stochastics.initialized().then_some(stochastics.value_d),
                donchian_upper: donchian.initialized().then_some(donchian.upper),
                donchian_middle: donchian.initialized().then_some(donchian.middle),
                donchian_lower: donchian.initialized().then_some(donchian.lower),
            });
        }
        Ok(())
    }

    pub(crate) fn initialized(&self) -> bool {
        // Observation-only indicators must not delay the selected strategy's readiness.
        match &self.signal {
            Signal::Ema(fast, slow) => fast.borrow().initialized() && slow.borrow().initialized(),
            Signal::Rsi(rsi, ..) => rsi.borrow().initialized(),
            Signal::Macd(macd) => macd.borrow().initialized(),
        }
    }

    /// Called only after the actor's registered indicators report initialized.
    pub(crate) fn target(&mut self, bar: &Bar) -> Result<Option<NativeTargetPointV1>> {
        let now = bar.ts_init.as_u64();
        // Do not let prefix-only RSI observations change the initial flat position.
        if now < self.request.decision_start_ns.get() {
            return Ok(None);
        }
        self.long = match &self.signal {
            Signal::Ema(fast, slow) => fast.borrow().value > slow.borrow().value,
            Signal::Rsi(rsi, entry, exit) => {
                let value = rsi.borrow().value;
                if value < *entry {
                    true
                } else if value > *exit {
                    false
                } else {
                    self.long
                }
            }
            Signal::Macd(macd) => {
                let macd = macd.borrow();
                macd.value > macd.signal
            }
        };
        let weight = if self.long {
            self.request.long_weight.clone()
        } else {
            DecimalValue::zero()
        };
        let cash = (bigdecimal::BigDecimal::from(1) - weight.as_decimal())
            .to_plain_string()
            .parse::<DecimalValue>()
            .map_err(anyhow::Error::msg)?;
        let expiry = now
            .checked_add(u64::from(self.request.target_ttl_seconds) * 1_000_000_000)
            .ok_or_else(|| anyhow::anyhow!("INDICATOR_STRATEGY_EXPIRY_OVERFLOW"))?;
        *self.generated.borrow_mut() += 1;
        Ok(Some(NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: DbCounter::new(now).map_err(anyhow::Error::msg)?,
            valid_until_ns: DbCounter::new(expiry).map_err(anyhow::Error::msg)?,
            rebalance_instrument_ids: None,
            targets: vec![AllocationTargetV1 {
                instrument_id: bar.bar_type.instrument_id().to_string(),
                currency: self.request.settings.base_currency.clone(),
                weight,
            }],
            cash_weight: cash,
        }))
    }
}
