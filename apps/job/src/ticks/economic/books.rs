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
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
};

pub(in crate::ticks) struct Snapshot {
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

/// Original received-book observations shared by research and economic prediction.
#[derive(Clone)]
pub(in crate::ticks) struct BookFeatures([contracts::science::FeatureValueV1; 4]);

impl BookFeatures {
    pub fn from_snapshot(snapshot: &Snapshot) -> Result<Self> {
        use contracts::science::{FeatureMissingReasonV1, FeatureValueV1};
        use nautilus_indicators::{book::imbalance::BookImbalanceRatio, indicator::Indicator};
        use nautilus_model::{enums::BookType, orderbook::OrderBook};

        let mut book = OrderBook::new(snapshot.deltas.instrument_id, BookType::L2_MBP);
        book.apply_deltas(&snapshot.deltas)?;
        let mut ratio = BookImbalanceRatio::new();
        ratio.handle_book(&book);
        // Upstream retains its previous ratio when a side is absent.
        let bid = book.best_bid_size().is_some();
        let ask = book.best_ask_size().is_some();
        let values = [
            (bid && ask).then_some(ratio.value),
            book.spread()
                .zip(book.midpoint())
                .map(|(spread, mid)| spread / mid * 10_000.0),
            bid.then(|| book.bids(Some(5)).map(|level| level.exposure()).sum()),
            ask.then(|| book.asks(Some(5)).map(|level| level.exposure()).sum()),
        ];
        let feature = |value: Option<f64>| -> Result<FeatureValueV1> {
            ensure!(value.is_none_or(f64::is_finite), "TICK_FEATURE_NONFINITE");
            Ok(FeatureValueV1 {
                value,
                missing_reason: value
                    .is_none()
                    .then_some(FeatureMissingReasonV1::SourceMissing),
                event_ns: Some(super::super::counter(snapshot.deltas.ts_event.as_u64())?),
                observed_available_ns: Some(super::super::counter(snapshot.received())?),
                effective_available_ns: Some(super::super::counter(snapshot.received())?),
                sequence: Some(super::super::counter(snapshot.deltas.sequence)?),
            })
        };
        let [ratio, spread, bid, ask] = values;
        Ok(Self([
            feature(ratio)?,
            feature(spread)?,
            feature(bid)?,
            feature(ask)?,
        ]))
    }

    pub fn append(
        features: &mut Vec<contracts::science::FeatureValueV1>,
        frame: Option<&Self>,
        decision: u64,
    ) {
        use contracts::science::{FeatureMissingReasonV1, FeatureValueV1};
        if let Some(frame) =
            frame.filter(|frame| frame.0[0].effective_available_ns.unwrap().get() <= decision)
        {
            features.extend_from_slice(&frame.0);
        } else {
            features.extend((0..4).map(|_| FeatureValueV1 {
                value: None,
                missing_reason: Some(FeatureMissingReasonV1::NotYetAvailable),
                event_ns: None,
                observed_available_ns: None,
                effective_available_ns: None,
                sequence: None,
            }));
        }
    }
}

pub(in crate::ticks) struct FeatureBooks {
    reader: Books,
    next: Option<Snapshot>,
    latest: Option<BookFeatures>,
}

impl FeatureBooks {
    pub fn open(root: &Path, id: InstrumentId, start: u64, end: u64) -> Result<Self> {
        ensure!(
            root.join("book-snapshots.source.json").is_file(),
            "TICK_BOOK_FEATURE_SOURCE_REQUIRED"
        );
        let mut reader = Books::open(root, id, start, end)?;
        let next = reader.next()?;
        Ok(Self {
            reader,
            next,
            latest: None,
        })
    }

    pub fn advance(&mut self, received: u64) -> Result<Option<&BookFeatures>> {
        let mut selected = None;
        while self
            .next
            .as_ref()
            .is_some_and(|frame| frame.received() <= received)
        {
            let frame = self.next.take().unwrap();
            // Preserve feature clock validation for every consumed snapshot.
            for clock in [
                frame.deltas.ts_event.as_u64(),
                frame.received(),
                frame.deltas.sequence,
            ] {
                super::super::counter(clock)?;
            }
            selected = Some(frame);
            self.next = self.reader.next()?;
        }
        if let Some(frame) = selected {
            self.latest = Some(BookFeatures::from_snapshot(&frame)?);
        }
        Ok(self.latest.as_ref())
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

pub(in crate::ticks) struct Books(Source);
enum Source {
    Native(NativeBooks),
    Published(super::source_books::RawBooks),
}

pub(super) enum BookReaders {
    Serial(Vec<Books>),
    Parallel(ParallelBooks),
}
impl BookReaders {
    pub fn open(
        root: &Path,
        ids: &[InstrumentId],
        start: u64,
        end: u64,
        cores: usize,
    ) -> Result<Self> {
        ensure!(cores > 0, "ECONOMIC_BOOK_CORES_INVALID");
        if cores == 1 || thread::available_parallelism().map_or(1, usize::from) == 1 {
            return ids
                .iter()
                .map(|&id| Books::open(root, id, start, end))
                .collect::<Result<Vec<_>>>()
                .map(Self::Serial);
        }
        Ok(Self::Parallel(ParallelBooks::open(
            root, ids, start, end, cores,
        )?))
    }
    pub fn next(&mut self, index: usize) -> Result<Option<Snapshot>> {
        match self {
            Self::Serial(readers) => readers[index].next(),
            Self::Parallel(readers) => readers.next(index),
        }
    }
    pub fn checkpoint_counts(&self) -> (u64, u64) {
        match self {
            Self::Serial(readers) => readers.iter().fold((0, 0), |total, reader| {
                let item = reader.checkpoint_counts();
                (total.0 + item.0, total.1 + item.1)
            }),
            Self::Parallel(readers) => readers.checkpoint_counts(),
        }
    }
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

type ReadResult = (Result<Option<Snapshot>>, (u64, u64));
enum ReadBook {
    Open {
        index: usize,
        id: InstrumentId,
        reply: mpsc::Sender<ReadResult>,
    },
    Next {
        index: usize,
        reply: mpsc::Sender<ReadResult>,
    },
}

/// Prefetch one frame per instrument; the caller retains the global event order.
pub(super) struct ParallelBooks {
    workers: Vec<mpsc::Sender<ReadBook>>,
    pending: Vec<Option<mpsc::Receiver<ReadResult>>>,
    counts: Vec<(u64, u64)>,
    cancelled: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}
impl ParallelBooks {
    pub fn open(
        root: &Path,
        ids: &[InstrumentId],
        start: u64,
        end: u64,
        cores: usize,
    ) -> Result<Self> {
        let source = root
            .join("book-snapshots.source.json")
            .is_file()
            .then(|| super::source_books::source(root))
            .transpose()?;
        let count = thread::available_parallelism()
            .map_or(1, usize::from)
            .min(cores)
            .saturating_sub(1)
            .min(ids.len())
            .max(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::with_capacity(count);
        let mut threads = Vec::with_capacity(count);
        for _ in 0..count {
            let (sender, receiver) = mpsc::channel();
            let root = root.to_path_buf();
            let cancelled = Arc::clone(&cancelled);
            let source = source.clone();
            threads.push(thread::Builder::new().spawn(move || {
                let mut readers = BTreeMap::<usize, Books>::new();
                while let Ok(job) = receiver.recv() {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    match job {
                        ReadBook::Open { index, id, reply } => {
                            let result = source
                                .as_ref()
                                .map_or_else(
                                    || Books::open(&root, id, start, end),
                                    |source| {
                                        super::source_books::RawBooks::open_indexed(
                                            Arc::clone(source),
                                            id,
                                            start,
                                            end,
                                        )
                                        .map(|reader| Books(Source::Published(reader)))
                                    },
                                )
                                .and_then(|mut reader| {
                                    let next = reader.next();
                                    let counts = reader.checkpoint_counts();
                                    readers.insert(index, reader);
                                    next.map(|frame| (frame, counts))
                                });
                            let _ = reply.send(match result {
                                Ok((frame, counts)) => (Ok(frame), counts),
                                Err(error) => (Err(error), (0, 0)),
                            });
                        }
                        ReadBook::Next { index, reply } => {
                            let result = match readers.get_mut(&index) {
                                Some(reader) => {
                                    let next = reader.next();
                                    (next, reader.checkpoint_counts())
                                }
                                None => (
                                    Err(anyhow::anyhow!("ECONOMIC_BOOK_WORKER_READER_MISSING")),
                                    (0, 0),
                                ),
                            };
                            let _ = reply.send(result);
                        }
                    }
                }
            })?);
            workers.push(sender);
        }
        let mut result = Self {
            workers,
            pending: Vec::with_capacity(ids.len()),
            counts: vec![(0, 0); ids.len()],
            cancelled,
            threads,
        };
        for (index, &id) in ids.iter().enumerate() {
            let (reply, receiver) = mpsc::channel();
            result.workers[index % count]
                .send(ReadBook::Open { index, id, reply })
                .map_err(|_| anyhow::anyhow!("ECONOMIC_BOOK_WORKER_STOPPED"))?;
            result.pending.push(Some(receiver));
        }
        Ok(result)
    }
    pub fn next(&mut self, index: usize) -> Result<Option<Snapshot>> {
        let receiver = self.pending[index]
            .take()
            .context("ECONOMIC_BOOK_WORKER_READER_FINISHED")?;
        let (result, counts) = receiver
            .recv()
            .map_err(|_| anyhow::anyhow!("ECONOMIC_BOOK_WORKER_STOPPED"))?;
        self.counts[index] = counts;
        let frame = result?;
        if frame.is_some() {
            let (reply, receiver) = mpsc::channel();
            self.workers[index % self.workers.len()]
                .send(ReadBook::Next { index, reply })
                .map_err(|_| anyhow::anyhow!("ECONOMIC_BOOK_WORKER_STOPPED"))?;
            self.pending[index] = Some(receiver);
        }
        Ok(frame)
    }
    pub fn checkpoint_counts(&self) -> (u64, u64) {
        self.counts
            .iter()
            .fold((0, 0), |total, item| (total.0 + item.0, total.1 + item.1))
    }
}
impl Drop for ParallelBooks {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.workers.clear();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
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
    fn native_features_preserve_clocks_and_mask_missing_sides() {
        use contracts::science::FeatureMissingReasonV1;
        let frame = Snapshot::new(snapshot()).unwrap();
        let original = BookFeatures::from_snapshot(&frame).unwrap();
        let mut features = Vec::new();
        BookFeatures::append(&mut features, Some(&original), 199);
        assert!(features.iter().all(|f| f.value.is_none()
            && f.missing_reason == Some(FeatureMissingReasonV1::NotYetAvailable)));
        features.clear();
        BookFeatures::append(&mut features, Some(&original), 250);
        assert_eq!(
            features.iter().map(|f| f.value).collect::<Vec<_>>(),
            [Some(2.0 / 3.0), Some(200.0), Some(198.0), Some(303.0)]
        );
        assert!(features
            .iter()
            .all(|f| f.event_ns.unwrap().get() == 100
                && f.effective_available_ns.unwrap().get() == 200));
        let mut one_sided = snapshot();
        one_sided.deltas.pop();
        one_sided.deltas.last_mut().unwrap().flags |= RecordFlag::F_LAST as u8;
        let frame = Snapshot::new(OrderBookDeltas::new(
            one_sided.instrument_id,
            one_sided.deltas,
        ))
        .unwrap();
        let missing = BookFeatures::from_snapshot(&frame).unwrap();
        features.clear();
        BookFeatures::append(&mut features, Some(&missing), 250);
        for index in [0, 1, 3] {
            assert_eq!(features[index].value, None);
            assert_eq!(
                features[index].missing_reason,
                Some(FeatureMissingReasonV1::SourceMissing)
            );
        }
        assert_eq!(features[2].value, Some(198.0));
    }

    #[test]
    fn native_depth_uses_only_five_levels_and_ratio_uses_top_level() {
        let template = snapshot();
        let mut deltas = vec![template.deltas[0]];
        for (side, start, step, size) in [(OrderSide::Buy, 99, -1, 2), (OrderSide::Sell, 101, 1, 3)]
        {
            for index in 0..6 {
                let mut delta = template.deltas[1];
                delta.order = BookOrder::new(
                    side,
                    Price::from((start + index * step).to_string().as_str()),
                    Quantity::from(size.to_string().as_str()),
                    0,
                );
                delta.flags &= !(RecordFlag::F_LAST as u8);
                deltas.push(delta);
            }
        }
        deltas.last_mut().unwrap().flags |= RecordFlag::F_LAST as u8;
        let frame = Snapshot::new(OrderBookDeltas::new(template.instrument_id, deltas)).unwrap();
        let features = BookFeatures::from_snapshot(&frame).unwrap();
        assert_eq!(features.0[0].value, Some(2.0 / 3.0));
        assert_eq!(features.0[2].value, Some(970.0));
        assert_eq!(features.0[3].value, Some(1545.0));
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
