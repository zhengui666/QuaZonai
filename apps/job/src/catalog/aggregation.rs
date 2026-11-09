//! Historical source clocks and precision around the official time aggregator.
//! Nautilus owns interval boundaries, OHLCV, empty intervals and timer state.
use anyhow::{ensure, Context, Result};
use nautilus_common::clock::{Clock, VirtualClock};
use nautilus_core::UnixNanos;
use nautilus_data::aggregation::{BarAggregator, TimeBarAggregator};
use nautilus_model::{
    data::{Bar, BarType, TradeTick},
    enums::{AggregationSource, BarIntervalType},
    types::{fixed::FIXED_PRECISION, Price, Quantity},
};
use std::{cell::RefCell, rc::Rc};

pub const CONVENTION: &str =
    "NAUTILUS_INTERNAL_RIGHT_OPEN_EVENT_OPEN_AVAILABLE_MAX_CLOSE_SOURCE_RECEIPT";

#[derive(Default)]
struct SourceFacts {
    available: u64,
    price_precision: u8,
    size_precision: u8,
    volume: Option<Quantity>,
}

/// One instrument's `[open, close)` bars. Native `ts_event` is the open,
/// `ts_init` is no earlier than the native close or any contributing receipt.
pub struct HistoricalTradeBars {
    aggregator: Rc<RefCell<Box<dyn BarAggregator>>>,
    clock: Rc<RefCell<VirtualClock>>,
    facts: Rc<RefCell<SourceFacts>>,
    emitted: Rc<RefCell<Vec<Bar>>>,
    timer: String,
    last_event: Option<UnixNanos>,
}

impl HistoricalTradeBars {
    pub fn new(bar_type: BarType) -> Result<Self> {
        let bar_type = BarType::new(
            bar_type.instrument_id(),
            bar_type.spec(),
            AggregationSource::Internal,
        );
        ensure!(bar_type.spec().is_time_aggregated(), "TIME_BAR_REQUIRED");
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let facts = Rc::new(RefCell::new(SourceFacts::default()));
        let source = facts.clone();
        let emitted = Rc::new(RefCell::new(Vec::new()));
        let sink = emitted.clone();
        let aggregator = TimeBarAggregator::new(
            bar_type,
            FIXED_PRECISION,
            FIXED_PRECISION,
            clock.clone(),
            move |mut bar| {
                let facts = std::mem::take(&mut *source.borrow_mut());
                bar = with_bar_precision(bar, (facts.price_precision, facts.size_precision));
                bar.ts_init = bar.ts_init.max(facts.available.into());
                sink.borrow_mut().push(bar);
            },
            false,
            true,
            BarIntervalType::RightOpen,
            None,
            nautilus_core::datetime::get_timezone("UTC")?,
            0,
            false,
        );
        Ok(Self {
            aggregator: Rc::new(RefCell::new(Box::new(aggregator))),
            clock,
            facts,
            emitted,
            timer: format!("TIME_BAR_{bar_type}"),
            last_event: None,
        })
    }

    fn advance(&mut self, at: UnixNanos) {
        let events = self.clock.borrow_mut().advance_time(at, true);
        for event in events {
            self.aggregator.borrow_mut().build_bar(&event);
        }
    }

    pub fn update(&mut self, trade: &TradeTick) -> Result<Option<Bar>> {
        ensure!(
            self.aggregator.borrow().bar_type().instrument_id() == trade.instrument_id,
            "TRADE_AGGREGATION_INSTRUMENT_INVALID"
        );
        ensure!(
            self.last_event.is_none_or(|last| trade.ts_event >= last),
            "TRADE_AGGREGATION_ORDER_INVALID"
        );
        if self.last_event.is_none() {
            let kind = self.aggregator.borrow().bar_type();
            let open = nautilus_model::data::bar::get_time_bar_start(
                trade.ts_event.to_datetime_utc(),
                &kind,
                None,
            );
            // Start at the official interval origin, including the epoch. A
            // native timer interprets a zero start as "now", so starting at the
            // first mid-interval tick could otherwise shift the epoch's grid.
            self.clock.borrow_mut().set_time(open.into());
            self.aggregator
                .borrow_mut()
                .start_timer(Some(self.aggregator.clone()));
        }
        // Right-open closes run before a trade on the boundary. Source receipt
        // time is retained separately; it never drives historical event grouping.
        self.advance(trade.ts_event);
        let size = Quantity::from_raw(trade.size.raw(), FIXED_PRECISION);
        {
            let mut facts = self.facts.borrow_mut();
            facts.available = facts.available.max(trade.ts_init.as_u64());
            facts.price_precision = facts.price_precision.max(trade.price.precision);
            facts.size_precision = facts.size_precision.max(trade.size.precision);
            facts.volume = Some(
                facts
                    .volume
                    .unwrap_or(Quantity::zero(FIXED_PRECISION))
                    .checked_add(size)
                    .context("BAR_VOLUME_RANGE")?,
            );
        }
        self.aggregator.borrow_mut().update(
            Price::from_raw(trade.price.raw(), FIXED_PRECISION),
            size,
            trade.ts_event,
        );
        self.last_event = Some(trade.ts_event);
        let mut emitted = self.emitted.borrow_mut();
        ensure!(emitted.len() <= 1, "NATIVE_AGGREGATION_EMISSION_COUNT");
        Ok(emitted.pop())
    }

    pub fn finish(mut self) -> Result<Vec<Bar>> {
        if self.last_event.is_some() {
            let close = self
                .clock
                .borrow()
                .next_time_ns(&self.timer)
                .context("NATIVE_AGGREGATION_TIMER_MISSING")?;
            self.advance(close);
            self.aggregator.borrow_mut().stop();
        }
        let emitted = std::mem::take(&mut *self.emitted.borrow_mut());
        Ok(emitted)
    }
}

pub fn with_bar_precision(mut bar: Bar, precision: (u8, u8)) -> Bar {
    for price in [&mut bar.open, &mut bar.high, &mut bar.low, &mut bar.close] {
        *price = Price::from_raw(price.raw(), precision.0);
    }
    bar.volume = Quantity::from_raw(bar.volume.raw(), precision.1);
    bar
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{enums::AggressorSide, identifiers::TradeId};

    fn trade(event: u64, received: u64, price: &str, quantity: &str) -> TradeTick {
        TradeTick::new_checked(
            "TOKEN/QUOTE.VENUE".parse().unwrap(),
            price.parse().unwrap(),
            quantity.parse().unwrap(),
            AggressorSide::Buy,
            TradeId::new(event.to_string()),
            event.into(),
            received.into(),
        )
        .unwrap()
    }

    #[test]
    fn native_right_open_boundary_never_exposes_a_close_before_its_receipt() {
        let mut bars =
            HistoricalTradeBars::new("TOKEN/QUOTE.VENUE-1-SECOND-LAST-INTERNAL".parse().unwrap())
                .unwrap();
        assert!(bars.update(&trade(0, 100, "1.2", "2")).unwrap().is_none());
        assert!(bars
            .update(&trade(999_999_999, 3_000_000_000, "1.234", "0.125"))
            .unwrap()
            .is_none());
        let first = bars
            .update(&trade(1_000_000_000, 3_000_000_000, "1.3", "4"))
            .unwrap()
            .unwrap();
        assert_eq!(first.ts_event.as_u64(), 0);
        assert_eq!(first.ts_init.as_u64(), 3_000_000_000);
        assert!(first.bar_type.is_internally_aggregated());
        assert_eq!(first.open, "1.200".parse::<Price>().unwrap());
        assert_eq!(first.close.precision, 3);
        assert_eq!(first.volume, "2.125".parse::<Quantity>().unwrap());
        let final_bars = bars.finish().unwrap();
        assert_eq!(final_bars.len(), 1);
        assert_eq!(final_bars[0].ts_event.as_u64(), 1_000_000_000);
        assert_eq!(final_bars[0].ts_init.as_u64(), 3_000_000_000);
    }

    #[test]
    fn empty_intervals_are_not_filled_and_event_order_is_not_silently_dropped() {
        let mut bars =
            HistoricalTradeBars::new("TOKEN/QUOTE.VENUE-1-SECOND-LAST-INTERNAL".parse().unwrap())
                .unwrap();
        bars.update(&trade(1, 1, "1", "1")).unwrap();
        let first = bars
            .update(&trade(3_000_000_000, 3_000_000_000, "2", "2"))
            .unwrap()
            .unwrap();
        assert_eq!(first.ts_event.as_u64(), 0);
        assert!(bars
            .update(&trade(2_000_000_000, 3_000_000_000, "3", "3"))
            .is_err());
        let final_bars = bars.finish().unwrap();
        assert_eq!(final_bars.len(), 1);
        assert_eq!(final_bars[0].ts_event.as_u64(), 3_000_000_000);
        assert_eq!(final_bars[0].ts_init.as_u64(), 4_000_000_000);
    }
}
