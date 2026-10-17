//! Conditional source-quantity liquidity through the official native FillModel.
//! Books only preview the fee allocator's shared pool; native fee callbacks are
//! the sole consumers. Market IOC prevents the native L1 remainder extrapolation.
use super::*;
use nautilus_execution::models::fill::{DefaultFillModel, FillModel};
use nautilus_model::{
    data::order::BookOrder,
    enums::{BookType, OrderSide, OrderType, TimeInForce},
    orderbook::OrderBook,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct RecordedLiquidityObserver(Rc<RefCell<Option<String>>>);

impl RecordedLiquidityObserver {
    fn observe<T>(&self, result: Result<T>) -> Result<T> {
        if let Err(error) = &result {
            let mut first = self.0.borrow_mut();
            if first.is_none() {
                *first = Some(format!("{error:#}"));
            }
        }
        result
    }

    pub(crate) fn check(&self) -> Result<()> {
        match self.0.borrow().as_ref() {
            Some(error) => anyhow::bail!("{error}"),
            None => Ok(()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct RecordedLiquidityModel {
    fees: RecordedFeeModel,
    delegate: DefaultFillModel,
    observer: RecordedLiquidityObserver,
}

impl RecordedLiquidityModel {
    pub(crate) fn new(fees: RecordedFeeModel, delegate: DefaultFillModel) -> Self {
        Self {
            fees,
            delegate,
            observer: RecordedLiquidityObserver::default(),
        }
    }

    pub(crate) fn observer(&self) -> RecordedLiquidityObserver {
        self.observer.clone()
    }

    fn book(
        &self,
        instrument: &InstrumentAny,
        order: &OrderAny,
        best_bid: Price,
        best_ask: Price,
    ) -> Result<Option<OrderBook>> {
        ensure!(
            order.order_type() == OrderType::Market
                && order.time_in_force() == TimeInForce::Ioc
                && !order.is_quote_quantity()
                && order.instrument_id() == instrument.id()
                && !order.leaves_qty().is_undefined()
                && !order.leaves_qty().is_zero(),
            "RECORDED_LIQUIDITY_MARKET_IOC_REQUIRED"
        );
        let (side, price) = match order.order_side() {
            OrderSide::Buy => (OrderSide::Sell, best_ask),
            OrderSide::Sell => (OrderSide::Buy, best_bid),
        };
        ensure!(
            !price.is_undefined() && price.as_decimal() > Decimal::ZERO,
            "RECORDED_LIQUIDITY_PRICE_INVALID"
        );
        instrument.try_normalize_price(price)?;
        let now = self.fees.clock.borrow().timestamp_ns();
        let key = self.fees.key(instrument);
        let state = self.fees.state.borrow();
        let bucket = state
            .buckets
            .get(&key)
            .context("RECORDED_LIQUIDITY_CLOCK_UNMATCHED")?;
        let available: BigDecimal = bucket
            .records
            .iter()
            .zip(&bucket.used)
            .map(|(record, used)| record.quantity.as_decimal() - used)
            .sum();
        ensure!(
            available > BigDecimal::from(0),
            "RECORDED_LIQUIDITY_EXHAUSTED: instrument={} fill_ns={} bar_end_ns={}",
            key.0,
            now,
            key.1
        );
        let quantity = available.min(BigDecimal::from_str(
            &order.leaves_qty().as_decimal().to_string(),
        )?);
        let size = Quantity::from_decimal_dp(
            Decimal::from_str_exact(&quantity.to_plain_string())?,
            instrument.size_precision(),
        )?;
        ensure!(
            !size.is_zero() && BigDecimal::from_str(&size.as_decimal().to_string())? == quantity,
            "RECORDED_LIQUIDITY_QUANTITY_GRID"
        );
        instrument.try_normalize_qty(size)?;
        let mut book = OrderBook::new(instrument.id(), BookType::L2_MBP);
        book.add(BookOrder::new(side, price, size, 1), 0, 0, now);
        // Never return an empty book: upstream would fall back to default fills.
        Ok(Some(book))
    }
}

impl FillModel for RecordedLiquidityModel {
    fn is_limit_filled(&mut self) -> Result<bool> {
        self.observer.observe(self.delegate.is_limit_filled())
    }

    fn is_slipped(&mut self) -> Result<bool> {
        // MatchingEngine applies the one adverse tick after our book fill.
        self.observer.observe(self.delegate.is_slipped())
    }

    fn get_orderbook_for_fill_simulation(
        &mut self,
        instrument: &InstrumentAny,
        order: &OrderAny,
        best_bid: Price,
        best_ask: Price,
    ) -> Result<Option<OrderBook>> {
        self.observer
            .observe(self.book(instrument, order, best_bid, best_ask))
    }
}

#[cfg(test)]
mod tests;
