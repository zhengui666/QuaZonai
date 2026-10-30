use anyhow::{ensure, Context, Result};
use nautilus_model::{
    data::{
        Bar, Data, DataBatch, FundingRateUpdate, HasTsInit, IndexPriceUpdate, MarkPriceUpdate,
        NautilusDataType, OrderBookDelta, QuoteTick, TradeTick,
    },
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog, catalog::types::CatalogQuery,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

const BATCH_RECORDS: usize = 100_000;

pub(super) struct Writer {
    catalog: ParquetDataCatalog,
    pending: BTreeMap<(String, String), Vec<Data>>,
    last_written: BTreeMap<(String, String), u64>,
    pending_records: usize,
    pub(super) counts: BTreeMap<String, BTreeMap<String, u64>>,
    root: PathBuf,
    verify_only: bool,
    max_output_bytes: Option<u64>,
}

fn identity(data: &Data) -> Result<(String, String)> {
    let kind = match data {
        Data::Trade(_) => "trades",
        Data::Bar(_) => "bars",
        Data::BookDelta(_) => "book_deltas",
        Data::Quote(_) => "quotes",
        Data::FundingRate(_) => "funding_rates",
        Data::MarkPrice(_) => "mark_prices",
        Data::IndexPrice(_) => "index_prices",
        _ => anyhow::bail!("NATIVE_RECORD_UNSUPPORTED"),
    };
    let id = if let Data::Bar(bar) = data {
        bar.bar_type.to_string()
    } else {
        data.instrument_id().to_string()
    };
    Ok((kind.to_owned(), id))
}

fn metadata(data: &Data) -> HashMap<String, String> {
    match data {
        Data::Trade(d) => {
            TradeTick::get_metadata(&d.instrument_id, d.price.precision, d.size.precision)
        }
        Data::Bar(d) => Bar::get_metadata(&d.bar_type, d.open.precision, d.volume.precision),
        Data::BookDelta(d) => OrderBookDelta::get_metadata(
            &d.instrument_id,
            d.order.price.precision,
            d.order.size.precision,
        ),
        Data::Quote(d) => QuoteTick::get_metadata(
            &d.instrument_id,
            d.bid_price.precision,
            d.bid_size.precision,
        ),
        Data::FundingRate(d) => FundingRateUpdate::get_metadata(&d.instrument_id),
        Data::MarkPrice(d) => MarkPriceUpdate::get_metadata(&d.instrument_id, d.value.precision),
        Data::IndexPrice(d) => IndexPriceUpdate::get_metadata(&d.instrument_id, d.value.precision),
        _ => unreachable!("identity rejects unsupported records"),
    }
}

impl Writer {
    #[cfg(test)]
    pub(super) fn new(output: &Path, instruments: &[InstrumentAny]) -> Result<Self> {
        Self::configured(output, instruments, false, None)
    }

    pub(super) fn configured(
        output: &Path,
        instruments: &[InstrumentAny],
        verify_only: bool,
        max_output_bytes: Option<u64>,
    ) -> Result<Self> {
        if !verify_only {
            fs::create_dir(output).context("OUTPUT_MUST_BE_NEW")?;
        }
        let root = output.join("catalog");
        if !verify_only {
            fs::create_dir(&root)?;
        }
        let catalog = ParquetDataCatalog::from_uri(
            root.to_str().context("CATALOG_PATH_ENCODING")?,
            None,
            Some(4096),
            None,
            None,
        )?;
        if verify_only {
            job::catalog::currencies::load(&root)?;
            let saved: serde_json::Value =
                serde_json::from_reader(fs::File::open(root.join("currencies.json"))?)?;
            ensure!(
                saved == serde_json::to_value(job::catalog::currencies::definitions(instruments))?,
                "RESUME_CURRENCY_DEFINITIONS_CHANGED"
            );
        } else {
            job::catalog::currencies::write(&root, instruments)?;
        }
        let mut instruments = instruments.to_vec();
        instruments.sort_by_key(|i| (i.id(), Instrument::ts_init(i)));
        if !verify_only {
            catalog.write_instruments(instruments.clone())?;
        }
        let mut readback = catalog.query_instruments_filtered(None, None, None)?;
        readback.sort_by_key(|i| (i.id(), Instrument::ts_init(i)));
        ensure!(
            serde_json::to_value(readback)? == serde_json::to_value(instruments)?,
            "INSTRUMENT_READBACK_MISMATCH"
        );
        Ok(Self {
            catalog,
            pending: BTreeMap::new(),
            last_written: BTreeMap::new(),
            pending_records: 0,
            counts: BTreeMap::new(),
            root: output.to_owned(),
            verify_only,
            max_output_bytes,
        })
    }

    pub(super) fn push(&mut self, data: Data) -> Result<()> {
        let key = identity(&data)?;
        if let Some(last) = self.last_written.get(&key) {
            ensure!(
                data.ts_init().as_u64() > *last,
                "SOURCE_ORDER_OR_OVERLAP_INVALID"
            );
        }
        self.pending.entry(key).or_default().push(data);
        self.pending_records += 1;
        if self.pending_records >= BATCH_RECORDS {
            self.flush(false)?;
            ensure!(
                self.max_output_bytes.is_none() || self.pending_records <= BATCH_RECORDS,
                "TRADE_INPUT_COHORT_LIMIT"
            );
        }
        Ok(())
    }

    fn flush(&mut self, final_batch: bool) -> Result<()> {
        let pending = std::mem::take(&mut self.pending);
        self.pending_records = 0;
        for (key, mut items) in pending {
            items.sort_by_key(HasTsInit::ts_init);
            // Keep equal-time records together: native file intervals must be disjoint.
            let last = items.last().context("EMPTY_NATIVE_BATCH")?.ts_init();
            let split = if final_batch {
                items.len()
            } else {
                items.partition_point(|item| item.ts_init() < last)
            };
            let retained = items.split_off(split);
            if !retained.is_empty() {
                self.pending_records += retained.len();
                self.pending.insert(key.clone(), retained);
            }
            if items.is_empty() {
                continue;
            }
            if matches!(items[0], Data::Bar(_)) {
                // Equal-availability BARs share one native file; preserve raw values at a common precision.
                for group in items.chunk_by_mut(|a, b| a.ts_init() == b.ts_init()) {
                    let precision = group.iter().fold((0_u8, 0_u8), |(p, q), data| {
                        let Data::Bar(bar) = data else { unreachable!() };
                        (p.max(bar.open.precision), q.max(bar.volume.precision))
                    });
                    for data in group {
                        let Data::Bar(bar) = data else { unreachable!() };
                        *bar = super::records::with_bar_precision(*bar, precision);
                    }
                }
            }
            // Split contiguous native encodings, not all equal precisions across intervening versions.
            for items in items.chunk_by(|a, b| metadata(a) == metadata(b)) {
                if !self.verify_only {
                    self.catalog.write_data_enum(items, None, None, None)?;
                }
                let start = Some(items[0].ts_init());
                let end = Some(items.last().unwrap().ts_init());
                let ids = Some(vec![key.1.clone()]);
                let query = CatalogQuery::new(NautilusDataType::from_data(&items[0]))
                    .with_identifiers(ids)
                    .with_range(start, end);
                let mut session = job::catalog::file_batch_session(
                    &self.catalog,
                    query,
                    None,
                    items.len().saturating_add(1).min(4096),
                )?;
                let mut observed = 0;
                while let Some(batch) = session.next_batch()? {
                    for index in 0..batch.len() {
                        // Compare as rows arrive, including the first excess row;
                        // never collect an unrestricted readback before checking it.
                        let expected = items
                            .get(observed)
                            .context("NATIVE_RECORD_READBACK_MISMATCH")?;
                        let actual = batch
                            .get(index)
                            .context("NATIVE_RECORD_READBACK_MISMATCH")?;
                        ensure!(
                            serde_json::to_value(actual.to_owned_data())?
                                == serde_json::to_value(expected)?,
                            "NATIVE_RECORD_READBACK_MISMATCH"
                        );
                        observed += 1;
                    }
                }
                ensure!(observed == items.len(), "NATIVE_RECORD_READBACK_MISMATCH");
                self.last_written
                    .insert(key.clone(), items.last().unwrap().ts_init().as_u64());
                *self
                    .counts
                    .entry(key.0.clone())
                    .or_default()
                    .entry(items[0].instrument_id().to_string())
                    .or_default() += items.len() as u64;
            }
        }
        if let Some(limit) = self.max_output_bytes {
            ensure!(
                directory_bytes(&self.root)? <= limit,
                "TRADE_INPUT_OUTPUT_BYTE_LIMIT"
            );
        }
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        self.flush(true)
    }

    pub(super) fn check_output_limit_for(&self, report: &impl serde::Serialize) -> Result<()> {
        if let Some(limit) = self.max_output_bytes {
            let report_bytes = serde_json::to_vec_pretty(report)?.len() as u64 + 1;
            ensure!(
                directory_bytes(&self.root)?
                    .checked_add(report_bytes.saturating_mul(2))
                    .is_some_and(|size| size <= limit),
                "TRADE_INPUT_OUTPUT_BYTE_LIMIT"
            );
        }
        Ok(())
    }

    pub(super) fn verify_trade_count(&mut self, count: u64) -> Result<()> {
        let mut actual = 0_u64;
        let mut session = job::catalog::file_batch_session(
            &self.catalog,
            CatalogQuery::new(NautilusDataType::TradeTick),
            None,
            count.saturating_add(1).min(4096) as usize,
        )?;
        while let Some(batch) = session.next_batch()? {
            let DataBatch::Trade(batch) = batch else {
                anyhow::bail!("RESUME_NATIVE_KIND_CHANGED");
            };
            actual = actual
                .checked_add(batch.len() as u64)
                .context("RESUME_NATIVE_ROWS_CHANGED")?;
            ensure!(actual <= count, "RESUME_NATIVE_ROWS_CHANGED");
        }
        ensure!(actual == count, "RESUME_NATIVE_ROWS_CHANGED");
        Ok(())
    }
}

fn directory_bytes(path: &Path) -> Result<u64> {
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        ensure!(!entry.file_type()?.is_symlink(), "NATIVE_OUTPUT_SYMLINK");
        let size = if metadata.is_dir() {
            directory_bytes(&entry.path())?
        } else {
            metadata.len()
        };
        bytes = bytes
            .checked_add(size)
            .context("NATIVE_OUTPUT_SIZE_RANGE")?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        enums::AggressorSide,
        identifiers::{InstrumentId, TradeId},
        types::{Price, Quantity},
    };
    use std::str::FromStr;

    #[test]
    fn delayed_bars_keep_values_and_clocks_at_one_availability_time() {
        let root = tempfile::tempdir().unwrap();
        let id = InstrumentId::from_str("TOKEN/QUOTE.VENUE").unwrap();
        let mut writer = Writer::new(&root.path().join("output"), &[]).unwrap();
        let mut aggregation = super::super::records::Aggregation::new(Some(60));
        let mut bars = Vec::new();
        for (event, available, price) in [
            (1_u64, 10_u64, "1.2345"),
            (20, 180, "1.23"),
            (61, 180, "1.24"),
        ] {
            let trade = TradeTick::new_checked(
                id,
                Price::from(price),
                Quantity::from("1"),
                AggressorSide::Buy,
                TradeId::new(event.to_string()),
                (event * 1_000_000_000).into(),
                (available * 1_000_000_000).into(),
            )
            .unwrap();
            if let Some(bar) = aggregation.update(&trade).unwrap() {
                bars.push(bar);
            }
        }
        bars.extend(aggregation.finish().unwrap());
        let kind = bars[0].bar_type.to_string();
        for bar in bars {
            writer.push(Data::Bar(bar)).unwrap();
        }
        writer.finish().unwrap();
        let readback = writer
            .catalog
            .query::<Bar>(Some(vec![kind]), None, None, None, None, false)
            .unwrap()
            .into_iter()
            .map(nautilus_model::data::Data::from)
            .collect::<Vec<_>>();
        let bars = readback
            .into_iter()
            .map(|data| {
                let Data::Bar(bar) = data else { unreachable!() };
                bar
            })
            .collect::<Vec<_>>();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].open, Price::from("1.2345"));
        assert_eq!(bars[1].close, Price::from("1.24"));
        assert_eq!(bars[0].volume, Quantity::from("2"));
        assert_eq!(bars[1].volume, Quantity::from("1"));
        assert_eq!(bars[0].ts_event.as_u64(), 60_000_000_000);
        assert_eq!(bars[1].ts_event.as_u64(), 120_000_000_000);
        for bar in bars {
            assert_eq!(bar.ts_init.as_u64(), 180_000_000_000);
            assert_eq!(bar.open.precision, 4);
        }
    }

    #[test]
    fn trade_cohort_overflow_fails_before_native_publication() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("bounded");
        let mut writer = Writer::configured(&output, &[], false, Some(10 * 1024 * 1024)).unwrap();
        let id = InstrumentId::from_str("TOKEN/QUOTE.VENUE").unwrap();
        for index in 0..BATCH_RECORDS {
            writer
                .push(Data::Trade(
                    TradeTick::new_checked(
                        id,
                        Price::from("1"),
                        Quantity::from("1"),
                        AggressorSide::Buy,
                        TradeId::new(index.to_string()),
                        1_u64.into(),
                        1_u64.into(),
                    )
                    .unwrap(),
                ))
                .unwrap();
        }
        let overflow = TradeTick::new_checked(
            id,
            Price::from("1"),
            Quantity::from("1"),
            AggressorSide::Buy,
            TradeId::new("overflow"),
            1_u64.into(),
            1_u64.into(),
        )
        .unwrap();
        assert!(writer
            .push(Data::Trade(overflow))
            .unwrap_err()
            .to_string()
            .contains("COHORT_LIMIT"));
        assert!(writer.counts.is_empty());
        assert!(!output.join("import-report.json").exists());
    }

    #[test]
    fn precision_changes_remain_contiguous_across_native_batches() {
        let root = tempfile::tempdir().unwrap();
        let id = InstrumentId::from_str("TOKEN/QUOTE.VENUE").unwrap();
        let mut writer = Writer::new(&root.path().join("output"), &[]).unwrap();
        for (i, (price, size)) in [
            ("1.23", "10"),
            ("1.234", "1.25"),
            ("1.23", "10"),
            ("1.23", "1.25"),
            ("1.23", "10"),
        ]
        .into_iter()
        .enumerate()
        {
            let at = (i as u64 + 1).into();
            writer
                .push(Data::Trade(
                    TradeTick::new_checked(
                        id,
                        Price::from(price),
                        Quantity::from(size),
                        AggressorSide::Buy,
                        TradeId::new(i.to_string()),
                        at,
                        at,
                    )
                    .unwrap(),
                ))
                .unwrap();
            if i == 2 {
                writer.flush(false).unwrap();
            }
        }
        writer.finish().unwrap();
        assert_eq!(writer.counts["trades"][&id.to_string()], 5);
        let ids = Some(vec![id.to_string()]);
        assert_eq!(
            writer
                .catalog
                .query_files(
                    &nautilus_persistence::catalog::types::CatalogDataType::Data(
                        NautilusDataType::TradeTick
                    ),
                    ids.clone(),
                    None,
                    None
                )
                .unwrap()
                .len(),
            5
        );
        let readback = writer
            .catalog
            .query::<TradeTick>(ids, None, None, None, None, false)
            .unwrap()
            .into_iter()
            .map(nautilus_model::data::Data::from)
            .collect::<Vec<_>>();
        let precision = readback
            .into_iter()
            .map(|data| {
                let Data::Trade(trade) = data else {
                    unreachable!()
                };
                (trade.price.precision, trade.size.precision)
            })
            .collect::<Vec<_>>();
        assert_eq!(precision, [(2, 0), (3, 2), (2, 0), (2, 2), (2, 0)]);
    }
}
