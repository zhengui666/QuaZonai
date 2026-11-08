//! Complete published L2 snapshots, streamed on their original receipt clock.
use anyhow::{ensure, Context, Result};
use nautilus_model::{
    data::{DataBatch, NautilusDataType, OrderBookDelta, OrderBookDeltas},
    enums::{BookAction, OrderSide, RecordFlag},
    identifiers::InstrumentId,
    types::Quantity,
};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog, catalog::types::CatalogQuery,
};
use rust_decimal::Decimal;
use std::{collections::VecDeque, path::Path};

pub(super) struct Snapshot {
    pub deltas: OrderBookDeltas,
    pub midpoint: Option<Decimal>,
    pub bid_depth: Decimal,
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
        let midpoint = match (bid, ask) {
            (Some(bid), Some(ask)) => {
                ensure!(bid <= ask, "ECONOMIC_BOOK_CROSSED");
                Some((bid + ask) / Decimal::TWO)
            }
            _ => None,
        };
        Ok(Self {
            deltas: batch,
            midpoint,
            bid_depth,
            ask_depth,
        })
    }
    pub fn received(&self) -> u64 {
        self.deltas.ts_init.as_u64()
    }
    pub fn capacity(&self, buy: bool) -> Decimal {
        if buy {
            self.ask_depth
        } else {
            self.bid_depth
        }
    }
    pub fn price(&self, buy: bool) -> Option<Decimal> {
        let side = if buy { OrderSide::Sell } else { OrderSide::Buy };
        self.deltas
            .deltas
            .iter()
            .find(|row| row.action == BookAction::Add && row.order.side == Some(side))
            .map(|row| row.order.price.as_decimal())
    }
    pub fn levels(&self, buy: bool) -> impl Iterator<Item = (Decimal, Decimal)> + '_ {
        let side = if buy { OrderSide::Sell } else { OrderSide::Buy };
        self.deltas
            .deltas
            .iter()
            .filter(move |row| row.action == BookAction::Add && row.order.side == Some(side))
            .map(|row| (row.order.price.as_decimal(), row.order.size.as_decimal()))
    }
    pub fn floor_sizes(self, increment: Quantity) -> Result<Self> {
        let mut deltas = self.deltas;
        floor_book_sizes(&mut deltas, increment)?;
        Self::new(deltas)
    }
}

pub(super) fn floor_book_sizes(snapshot: &mut OrderBookDeltas, increment: Quantity) -> Result<()> {
    for delta in &mut snapshot.deltas {
        delta.order.size = delta
            .order
            .size
            .checked_floor_to_increment(increment)
            .context("TICK_ECONOMIC_BOOK_SIZE_INCREMENT_UNSUPPORTED")?;
    }
    snapshot
        .deltas
        .retain(|delta| delta.action != BookAction::Add || delta.order.size.raw() > 0);
    if let Some(last) = snapshot.deltas.last_mut() {
        last.flags |= RecordFlag::F_LAST as u8;
    }
    Ok(())
}

pub(super) struct Books(Source);
enum Source {
    Native(NativeBooks),
    Published(super::source_books::RawBooks),
}
impl Books {
    pub fn open(root: &Path, id: InstrumentId, start: u64, end: u64) -> Result<Self> {
        Ok(Self(if root.join("book-snapshots.source.json").is_file() {
            Source::Published(super::source_books::RawBooks::open(root, id, start, end)?)
        } else {
            Source::Native(NativeBooks::open(root, id, start, end)?)
        }))
    }
    pub fn next(&mut self) -> Result<Option<Snapshot>> {
        match &mut self.0 {
            Source::Native(source) => source.next(),
            Source::Published(source) => source.next(),
        }
    }
    pub fn checkpoint_counts(&self) -> (u64, u64) {
        match &self.0 {
            Source::Native(_) => (0, 0),
            Source::Published(source) => source.checkpoint_counts(),
        }
    }
}

struct NativeBooks {
    session: crate::catalog::FileBatchSession,
    rows: VecDeque<OrderBookDelta>,
    instrument_id: InstrumentId,
    previous: Option<u64>,
}
impl NativeBooks {
    pub fn open(root: &Path, id: InstrumentId, start: u64, end: u64) -> Result<Self> {
        let catalog = ParquetDataCatalog::new(root, None, None, None, None);
        let query = CatalogQuery::new(NautilusDataType::OrderBookDelta)
            .with_identifiers(Some(vec![id.to_string()]))
            .with_range(Some(start.into()), Some((end - 1).into()));
        let session = crate::catalog::file_batch_session(&catalog, query, None, 4096)?;
        Ok(Self {
            session,
            rows: VecDeque::new(),
            instrument_id: id,
            previous: None,
        })
    }
    fn row(&mut self) -> Result<Option<OrderBookDelta>> {
        loop {
            if let Some(row) = self.rows.pop_front() {
                return Ok(Some(row));
            }
            match self.session.next_batch()? {
                Some(DataBatch::BookDelta(rows)) => self.rows.extend(rows.iter().copied()),
                Some(_) => anyhow::bail!("ECONOMIC_BOOK_DATA_TYPE_INVALID"),
                None => return Ok(None),
            }
        }
    }
    pub fn next(&mut self) -> Result<Option<Snapshot>> {
        let Some(first) = self.row()? else {
            return Ok(None);
        };
        ensure!(
            first.instrument_id == self.instrument_id && first.action == BookAction::Clear,
            "ECONOMIC_BOOK_COMPLETE_SNAPSHOT_REQUIRED"
        );
        let at = first.ts_init.as_u64();
        ensure!(
            self.previous.is_none_or(|previous| previous < at),
            "ECONOMIC_BOOK_RECEIPT_ORDER_INVALID"
        );
        let mut deltas = vec![first];
        while deltas.last().unwrap().flags & RecordFlag::F_LAST as u8 == 0 {
            let next = self.row()?.context("ECONOMIC_BOOK_INCOMPLETE_SNAPSHOT")?;
            ensure!(
                next.instrument_id == self.instrument_id
                    && next.ts_init == first.ts_init
                    && next.ts_event == first.ts_event
                    && next.action == BookAction::Add
                    && deltas.len() < 4096,
                "ECONOMIC_BOOK_SNAPSHOT_INVALID"
            );
            deltas.push(next);
        }
        self.previous = Some(at);
        Snapshot::new(OrderBookDeltas::new_checked(self.instrument_id, deltas)?).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        data::BookOrder,
        types::{Price, Quantity},
    };

    fn snapshot() -> OrderBookDeltas {
        let id = InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID");
        let mut rows = vec![OrderBookDelta::clear(id, 0, 100.into(), 200.into())];
        for (side, price, size) in [
            (OrderSide::Buy, "99.00", "2.0000"),
            (OrderSide::Sell, "101.00", "3.0000"),
        ] {
            rows.push(OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(side, Price::from(price), Quantity::from(size), 0),
                RecordFlag::F_SNAPSHOT as u8,
                0,
                100.into(),
                200.into(),
            ));
        }
        rows.last_mut().unwrap().flags |= RecordFlag::F_LAST as u8;
        OrderBookDeltas::new(id, rows)
    }
    #[test]
    fn complete_snapshot_preserves_clocks_depth_and_empty_reset() {
        let frame = Snapshot::new(snapshot()).unwrap();
        assert_eq!(frame.received(), 200);
        assert_eq!(frame.deltas.ts_event.as_u64(), 100);
        assert_eq!(frame.midpoint, Some(Decimal::from(100)));
        assert_eq!(frame.capacity(false), Decimal::from(2));
        assert_eq!(frame.capacity(true), Decimal::from(3));
        let mut empty = frame.deltas.deltas[0];
        empty.flags |= RecordFlag::F_LAST as u8;
        assert!(
            Snapshot::new(OrderBookDeltas::new(empty.instrument_id, vec![empty]))
                .unwrap()
                .midpoint
                .is_none()
        );
        let mut broken = snapshot();
        broken.deltas[1].ts_init = 201.into();
        assert!(Snapshot::new(broken).is_err());
        let mut incomplete = snapshot();
        incomplete.deltas.last_mut().unwrap().flags &= !(RecordFlag::F_LAST as u8);
        assert!(Snapshot::new(incomplete).is_err());
    }
}
