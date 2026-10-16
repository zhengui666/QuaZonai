use anyhow::{ensure, Context, Result};
use nautilus_model::{
    data::{
        Bar, Data, FundingRateUpdate, HasTsInit, IndexPriceUpdate, MarkPriceUpdate, OrderBookDelta,
        QuoteTick, TradeTick,
    },
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

const BATCH_RECORDS: usize = 100_000;

pub(super) struct Writer {
    catalog: ParquetDataCatalog,
    pending: BTreeMap<(String, String), Vec<Data>>,
    last_written: BTreeMap<(String, String), u64>,
    pending_records: usize,
    pub(super) counts: BTreeMap<String, BTreeMap<String, u64>>,
}

fn identity(data: &Data) -> Result<(String, String)> {
    let kind = match data {
        Data::Trade(_) => "trades",
        Data::Bar(_) => "bars",
        Data::Delta(_) => "book_deltas",
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
        Data::Delta(d) => OrderBookDelta::get_metadata(
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
    pub(super) fn new(output: &Path, instruments: &[InstrumentAny]) -> Result<Self> {
        fs::create_dir(output).context("OUTPUT_MUST_BE_NEW")?;
        let root = output.join("catalog");
        fs::create_dir(&root)?;
        let catalog = ParquetDataCatalog::from_uri(
            root.to_str().context("CATALOG_PATH_ENCODING")?,
            None,
            Some(4096),
            None,
            None,
        )?;
        job::catalog::currencies::write(&root, instruments)?;
        let mut instruments = instruments.to_vec();
        instruments.sort_by_key(|i| (i.id(), Instrument::ts_init(i)));
        catalog.write_instruments(instruments.clone())?;
        let mut readback = catalog.instruments(None, None, None)?;
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
            // Split contiguous native encodings, not all equal precisions across intervening versions.
            for items in items.chunk_by(|a, b| metadata(a) == metadata(b)) {
                self.catalog.write_data_enum(items, None, None, None)?;
                self.catalog.reset_session();
                let start = Some(items[0].ts_init());
                let end = Some(items.last().unwrap().ts_init());
                let ids = Some(vec![key.1.clone()]);
                macro_rules! read {
                    ($type:ty) => {{
                        self.catalog
                            .query::<$type>(ids, start, end, None, None, false)?
                            .take(items.len() + 1)
                            .collect::<std::result::Result<Vec<Data>, _>>()?
                    }};
                }
                let readback = match &items[0] {
                    Data::Trade(_) => read!(TradeTick),
                    Data::Bar(_) => read!(Bar),
                    Data::Delta(_) => read!(OrderBookDelta),
                    Data::Quote(_) => read!(QuoteTick),
                    Data::FundingRate(_) => read!(FundingRateUpdate),
                    Data::MarkPrice(_) => read!(MarkPriceUpdate),
                    Data::IndexPrice(_) => read!(IndexPriceUpdate),
                    _ => anyhow::bail!("NATIVE_RECORD_UNSUPPORTED"),
                };
                ensure!(
                    serde_json::to_value(&readback)? == serde_json::to_value(items)?,
                    "NATIVE_RECORD_READBACK_MISMATCH"
                );
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
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        self.flush(true)
    }
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
                .query_files("trades", ids.clone(), None, None)
                .unwrap()
                .len(),
            5
        );
        writer.catalog.reset_session();
        let readback = writer
            .catalog
            .query::<TradeTick>(ids, None, None, None, None, false)
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
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
