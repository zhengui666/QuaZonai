//! Validate complete original L2 snapshots before native catalog publication.
use anyhow::{ensure, Context, Result};
use nautilus_model::{data::OrderBookDeltas, enums::{BookAction, OrderSide, RecordFlag}};
use rust_decimal::Decimal;

#[derive(Clone)]
pub(in crate::ticks) struct Snapshot {
    pub deltas: OrderBookDeltas,
    #[cfg(test)]
    pub midpoint: Option<Decimal>,
    #[cfg(test)]
    pub bid_depth: Decimal,
    #[cfg(test)]
    pub ask_depth: Decimal,
}
impl Snapshot {
    pub fn new(batch: OrderBookDeltas) -> Result<Self> {
        let deltas = &batch.deltas;
        ensure!(
            deltas
                .first()
                .is_some_and(|row| row.action == BookAction::Clear)
                && deltas
                    .last()
                    .is_some_and(|row| row.flags & RecordFlag::F_LAST as u8 != 0),
            "ECONOMIC_BOOK_COMPLETE_SNAPSHOT_REQUIRED"
        );
        for (index, row) in deltas.iter().enumerate() {
            ensure!(
                row.instrument_id == batch.instrument_id
                    && row.ts_init == batch.ts_init
                    && row.ts_event == batch.ts_event
                    && (index == 0 || row.action == BookAction::Add)
                    && (index + 1 == deltas.len() || row.flags & RecordFlag::F_LAST as u8 == 0),
                "ECONOMIC_BOOK_SNAPSHOT_INVALID"
            );
        }
        let mut bid = None::<Decimal>;
        let mut ask = None::<Decimal>;
        let mut bid_depth = Decimal::ZERO;
        let mut ask_depth = Decimal::ZERO;
        let mut last_bid = None;
        let mut last_ask = None;
        for row in deltas.iter().skip(1) {
            let price = row.order.price.as_decimal();
            let size = row.order.size.as_decimal();
            ensure!(
                price > Decimal::ZERO && size > Decimal::ZERO,
                "ECONOMIC_BOOK_LEVEL_INVALID"
            );
            match row.order.side {
                Some(OrderSide::Buy) => {
                    ensure!(
                        last_bid.is_none_or(|last| last > price),
                        "ECONOMIC_BOOK_BID_ORDER_INVALID"
                    );
                    last_bid = Some(price);
                    bid.get_or_insert(price);
                    bid_depth = bid_depth
                        .checked_add(size)
                        .context("ECONOMIC_BOOK_DECIMAL_RANGE")?;
                }
                Some(OrderSide::Sell) => {
                    ensure!(
                        last_ask.is_none_or(|last| last < price),
                        "ECONOMIC_BOOK_ASK_ORDER_INVALID"
                    );
                    last_ask = Some(price);
                    ask.get_or_insert(price);
                    ask_depth = ask_depth
                        .checked_add(size)
                        .context("ECONOMIC_BOOK_DECIMAL_RANGE")?;
                }
                _ => anyhow::bail!("ECONOMIC_BOOK_SIDE_INVALID"),
            }
        }
        let _midpoint = match (bid, ask) {
            (Some(bid), Some(ask)) => {
                ensure!(bid <= ask, "ECONOMIC_BOOK_CROSSED");
                Some((bid + ask) / Decimal::TWO)
            }
            _ => None,
        };
        Ok(Self {
            deltas: batch,
            #[cfg(test)]
            midpoint: _midpoint,
            #[cfg(test)]
            bid_depth,
            #[cfg(test)]
            ask_depth,
        })
    }
    #[cfg(test)]
    pub fn received(&self) -> u64 {
        self.deltas.ts_init.as_u64()
    }
    #[cfg(test)]
    pub fn price(&self, buy: bool) -> Option<Decimal> {
        let side = if buy { OrderSide::Sell } else { OrderSide::Buy };
        self.deltas
            .deltas
            .iter()
            .find(|row| row.action == BookAction::Add && row.order.side == Some(side))
            .map(|row| row.order.price.as_decimal())
    }
}
