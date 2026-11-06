//! Read selected published book hours directly, without a second catalog.
use super::books::Snapshot;
use crate::source_market::{
    mapped_field, parquet_rows, scalar, timestamp, BookSnapshot, Encoding, FlatSnapshots,
};
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, NaiveDate};
use nautilus_model::{
    identifiers::InstrumentId,
    instruments::{Instrument, InstrumentAny},
};
use parquet::record::reader::RowIter;
use serde::{
    de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::File,
    io::BufReader,
    path::{Component, Path, PathBuf},
    sync::{Arc, Weak},
    time::SystemTime,
};

const HOUR: u64 = 3_600_000_000_000;

#[derive(Clone, Deserialize)]
struct Stream {
    name: String,
    kind: String,
    format: String,
    time_unit: String,
    available_time_unit: Option<String>,
    columns: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct SourceFile {
    path: String,
    size: u64,
    stream: Option<String>,
}
#[derive(Deserialize)]
struct Profile {
    streams: Vec<Stream>,
}
#[derive(Deserialize)]
struct Header {
    source_root: PathBuf,
    start_date: NaiveDate,
    end_date: NaiveDate,
    profile: Profile,
    instrument_source: serde_json::Value,
    repository: Option<String>,
    source_reference: Option<String>,
    coverage: Option<String>,
}
impl Header {
    fn observed(&self) -> bool {
        self.repository.as_deref() == Some("CryptoHFTData")
            && self.source_reference.as_deref()
                == Some("https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data")
            && self.coverage.as_deref() == Some("OBSERVED_PUBLISHED_FILES")
    }
}
fn date_ns(date: NaiveDate) -> Result<u64> {
    u64::try_from(
        date.and_hms_opt(0, 0, 0)
            .context("ECONOMIC_BOOK_SOURCE_DATE_INVALID")?
            .and_utc()
            .timestamp(),
    )?
    .checked_mul(1_000_000_000)
    .context("ECONOMIC_BOOK_SOURCE_DATE_INVALID")
}
fn identity(path: &str) -> Option<(&str, &str, &str, &str)> {
    let mut parts = path.split('/');
    let (market, day, hour, name) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let symbol = name
        .strip_suffix("_orderbook.parquet.zst")
        .or_else(|| name.strip_suffix("_orderbook.parquet"))?;
    Some((market, day, hour, symbol))
}
fn hour_ns(day: &str, hour: &str) -> Result<u64> {
    let hour = hour.parse::<u64>()?;
    ensure!(hour < 24, "ECONOMIC_BOOK_SOURCE_HOUR_INVALID");
    date_ns(NaiveDate::parse_from_str(day, "%Y-%m-%d")?)?
        .checked_add(hour * HOUR)
        .context("ECONOMIC_BOOK_SOURCE_DATE_INVALID")
}

struct Layout {
    market: String,
    symbol: String,
    stream: Stream,
}
impl Layout {
    fn path(&self, at: u64) -> Result<String> {
        let date = DateTime::from_timestamp(i64::try_from(at / 1_000_000_000)?, 0)
            .context("ECONOMIC_BOOK_SOURCE_DATE_INVALID")?;
        Ok(format!(
            "{}/{}/{:02}/{}_orderbook.parquet{}",
            self.market,
            date.format("%Y-%m-%d"),
            (at / HOUR) % 24,
            self.symbol,
            if self.stream.format == "parquet-zstd" {
                ".zst"
            } else {
                ""
            }
        ))
    }
}
#[derive(Clone, Copy)]
struct HourFile {
    at: u64,
    size: u64,
    layout: usize,
}
struct IndexedSource {
    root: PathBuf,
    start: u64,
    end: u64,
    observed: bool,
    instruments: BTreeSet<InstrumentId>,
    layouts: Vec<Layout>,
    files: BTreeMap<InstrumentId, Arc<Vec<HourFile>>>,
    missing: BTreeMap<InstrumentId, Vec<(u64, String)>>,
}
struct IndexBuilder {
    aliases: BTreeMap<String, BTreeMap<String, InstrumentId>>,
    streams: BTreeMap<String, Stream>,
    layouts: Vec<Layout>,
    layout_ids: BTreeMap<(String, String, String), usize>,
    files: BTreeMap<InstrumentId, Vec<HourFile>>,
    missing: BTreeMap<InstrumentId, Vec<(u64, String)>>,
    observed: bool,
}
impl IndexBuilder {
    fn target(&self, market: &str, symbol: &str) -> Option<InstrumentId> {
        self.aliases.get(market)?.get(symbol).copied()
    }
    fn file(&mut self, file: SourceFile) -> Result<()> {
        let Some(stream) = file.stream.as_ref().and_then(|name| self.streams.get(name)) else {
            return Ok(());
        };
        let Some((market, day, hour, symbol)) = identity(&file.path) else {
            return Ok(());
        };
        let Some(id) = self.target(market, symbol) else {
            return Ok(());
        };
        ensure!(
            Path::new(&file.path)
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "ECONOMIC_BOOK_SOURCE_PATH_INVALID"
        );
        let at = hour_ns(day, hour)?;
        let key = (market.to_string(), symbol.to_string(), stream.name.clone());
        let layout = if let Some(index) = self.layout_ids.get(&key) {
            *index
        } else {
            let index = self.layouts.len();
            self.layouts.push(Layout {
                market: market.into(),
                symbol: symbol.into(),
                stream: stream.clone(),
            });
            self.layout_ids.insert(key, index);
            index
        };
        // Store each source's layout once; verify that compact hours reproduce its exact paths.
        ensure!(
            self.layouts[layout].path(at)? == file.path,
            "ECONOMIC_BOOK_SOURCE_PATH_INVALID"
        );
        self.files.entry(id).or_default().push(HourFile {
            at,
            size: file.size,
            layout,
        });
        Ok(())
    }
    fn missing(&mut self, value: serde_json::Value) -> Result<()> {
        let Some(path) = value.get("pattern").and_then(serde_json::Value::as_str) else {
            return Ok(());
        };
        let normalized = path.strip_suffix("[.zst]").unwrap_or(path);
        let Some((market, day, hour, symbol)) = identity(normalized) else {
            return Ok(());
        };
        if let Some(id) = self.target(market, symbol) {
            self.missing
                .entry(id)
                .or_default()
                .push((hour_ns(day, hour)?, path.into()));
        }
        Ok(())
    }
}

// Stream the nested file array rather than materialize millions of path objects per asset.
struct Files<'a>(&'a mut IndexBuilder);
impl<'de> DeserializeSeed<'de> for Files<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Files<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("source manifest")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<(), M::Error> {
        let mut files = false;
        while let Some(key) = map.next_key::<String>()? {
            files |= key == "files";
            if key == "files" || (key == "missing" && !self.0.observed) {
                map.next_value_seed(Entries {
                    builder: &mut *self.0,
                    files: key == "files",
                })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        if !files {
            return Err(serde::de::Error::custom(
                "ECONOMIC_BOOK_SOURCE_FILES_REQUIRED",
            ));
        }
        Ok(())
    }
}
struct Entries<'a> {
    builder: &'a mut IndexBuilder,
    files: bool,
}
impl<'de> DeserializeSeed<'de> for Entries<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for Entries<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("source entries")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> std::result::Result<(), S::Error> {
        if self.files {
            while let Some(file) = sequence.next_element::<SourceFile>()? {
                self.builder.file(file).map_err(serde::de::Error::custom)?;
            }
        } else {
            while let Some(missing) = sequence.next_element::<serde_json::Value>()? {
                self.builder
                    .missing(missing)
                    .map_err(serde::de::Error::custom)?;
            }
        }
        Ok(())
    }
}

impl IndexedSource {
    fn open(path: &Path) -> Result<Self> {
        let header: Header = serde_json::from_reader(BufReader::new(File::open(path)?))?;
        let observed = header.observed();
        let instruments =
            crate::catalog::currencies::instruments_from_value(header.instrument_source.clone())?;
        let mut markets = BTreeMap::new();
        let mut aliases = BTreeMap::<String, BTreeMap<String, InstrumentId>>::new();
        for instrument in &instruments {
            let market = match instrument {
                InstrumentAny::CurrencyPair(_) => "hyperliquid_spot",
                InstrumentAny::CryptoPerpetual(_) => "hyperliquid_futures",
                _ => continue,
            };
            markets.insert(instrument.id(), market);
            if let Some(previous) = aliases
                .entry(market.into())
                .or_default()
                .insert(instrument.raw_symbol().to_string(), instrument.id())
            {
                ensure!(previous == instrument.id(), "NATIVE_SYMBOL_ID_CONFLICT");
            }
        }
        if let Some(values) = header.instrument_source.get("symbol_ids") {
            let mapping: BTreeMap<String, InstrumentId> = serde_json::from_value(values.clone())?;
            for (symbol, id) in mapping {
                if let Some(market) = markets.get(&id) {
                    if let Some(previous) = aliases
                        .entry((*market).into())
                        .or_default()
                        .insert(symbol, id)
                    {
                        ensure!(previous == id, "NATIVE_SYMBOL_ID_CONFLICT");
                    }
                }
            }
        }
        let mut builder = IndexBuilder {
            aliases,
            streams: header
                .profile
                .streams
                .into_iter()
                .filter(|stream| stream.kind == "book_snapshots")
                .map(|stream| (stream.name.clone(), stream))
                .collect(),
            layouts: Vec::new(),
            layout_ids: BTreeMap::new(),
            files: BTreeMap::new(),
            missing: BTreeMap::new(),
            observed,
        };
        Files(&mut builder).deserialize(&mut serde_json::Deserializer::from_reader(
            BufReader::new(File::open(path)?),
        ))?;
        let mut files = BTreeMap::new();
        for (id, mut hours) in builder.files {
            hours.sort_by_key(|file| (file.at, file.layout));
            ensure!(
                hours.windows(2).all(|pair| pair[0].at != pair[1].at),
                "ECONOMIC_BOOK_SOURCE_DUPLICATE_HOUR: {id}"
            );
            files.insert(id, Arc::new(hours));
        }
        Ok(Self {
            root: header.source_root.canonicalize()?,
            start: date_ns(header.start_date)?,
            end: date_ns(header.end_date)?,
            observed,
            instruments: markets.into_keys().collect(),
            layouts: builder.layouts,
            files,
            missing: builder.missing,
        })
    }
}
struct CachedSource {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    source: Weak<IndexedSource>,
}
thread_local! { static SOURCE: RefCell<Option<CachedSource>> = const { RefCell::new(None) }; }
fn source(root: &Path) -> Result<Arc<IndexedSource>> {
    let path = root.join("book-snapshots.source.json").canonicalize()?;
    let metadata = path.metadata()?;
    let modified = metadata.modified()?;
    SOURCE.with(|cached| {
        let mut cached = cached.borrow_mut();
        if let Some(previous) = &*cached {
            if previous.path == path
                && previous.size == metadata.len()
                && previous.modified == modified
            {
                if let Some(source) = previous.source.upgrade() {
                    return Ok(source);
                }
            }
        }
        let source = Arc::new(IndexedSource::open(&path)?);
        *cached = Some(CachedSource {
            path,
            size: metadata.len(),
            modified,
            source: Arc::downgrade(&source),
        });
        Ok(source)
    })
}

struct Hour {
    rows: RowIter<'static>,
    layout: usize,
    snapshots: FlatSnapshots,
}
impl Hour {
    fn next(
        &mut self,
        source: &IndexedSource,
        encoding: Encoding,
        start: u64,
        end: u64,
    ) -> Result<Option<BookSnapshot>> {
        let layout = &source.layouts[self.layout];
        for row in self.rows.by_ref() {
            let row = row?;
            ensure!(
                scalar(mapped_field(&row, &layout.stream.columns, "symbol")?)? == layout.symbol,
                "ECONOMIC_BOOK_SOURCE_SYMBOL_MISMATCH"
            );
            let event = timestamp(
                mapped_field(&row, &layout.stream.columns, "time")?,
                &layout.stream.time_unit,
            )?;
            let received = timestamp(
                mapped_field(&row, &layout.stream.columns, "available_at")?,
                layout
                    .stream
                    .available_time_unit
                    .as_deref()
                    .unwrap_or(&layout.stream.time_unit),
            )?;
            ensure!(received >= event, "SOURCE_EVENT_AFTER_AVAILABILITY");
            if received < start || received >= end {
                continue;
            }
            if let Some(snapshot) =
                self.snapshots
                    .push(&layout.stream.columns, row, encoding, event, received)?
            {
                return Ok(Some(snapshot));
            }
        }
        self.snapshots.finish(&layout.stream.columns)
    }
}

pub(super) struct RawBooks {
    source: Arc<IndexedSource>,
    files: Arc<Vec<HourFile>>,
    position: usize,
    hour: Option<Hour>,
    encoding: Encoding,
    start: u64,
    end: u64,
    previous: Option<u64>,
}
impl RawBooks {
    pub fn open(root: &Path, id: InstrumentId, start: u64, end: u64) -> Result<Self> {
        let source = source(root)?;
        ensure!(
            start < end && start >= source.start && end <= source.end,
            "ECONOMIC_BOOK_SOURCE_WINDOW_INVALID"
        );
        ensure!(
            source.instruments.contains(&id),
            "ECONOMIC_BOOK_SOURCE_INSTRUMENT_MISSING"
        );
        if let Some(missing) = source.missing.get(&id) {
            for (at, path) in missing {
                ensure!(
                    !(*at < end && at.saturating_add(HOUR) > start),
                    "ECONOMIC_BOOK_SOURCE_HOUR_MISSING: {path}"
                );
            }
        }
        let files = source
            .files
            .get(&id)
            .cloned()
            .unwrap_or_else(|| Arc::new(Vec::new()));
        let position = files.partition_point(|file| file.at.saturating_add(HOUR) <= start);
        ensure!(
            source.observed || files.get(position).is_some_and(|file| file.at < end),
            "ECONOMIC_BOOK_SOURCE_SELECTION_EMPTY"
        );
        Ok(Self {
            source,
            files,
            position,
            hour: None,
            encoding: Encoding::new(id, None, true)?,
            start,
            end,
            previous: None,
        })
    }
    pub fn next(&mut self) -> Result<Option<Snapshot>> {
        loop {
            if let Some(hour) = &mut self.hour {
                if let Some(snapshot) =
                    hour.next(&self.source, self.encoding, self.start, self.end)?
                {
                    let received = snapshot.deltas.ts_init.as_u64();
                    ensure!(
                        self.previous.is_none_or(|last| last < received),
                        "ECONOMIC_BOOK_RECEIPT_ORDER_INVALID"
                    );
                    self.previous = Some(received);
                    return Snapshot::new(snapshot.deltas).map(Some);
                }
                self.hour = None;
            }
            let Some(file) = self.files.get(self.position) else {
                return Ok(None);
            };
            if file.at >= self.end {
                return Ok(None);
            }
            self.position += 1;
            let layout = &self.source.layouts[file.layout];
            let relative = layout.path(file.at)?;
            let path = self.source.root.join(&relative).canonicalize()?;
            ensure!(
                path.starts_with(&self.source.root) && path.metadata()?.len() == file.size,
                "ECONOMIC_BOOK_SOURCE_FILE_MISMATCH: {relative}"
            );
            self.hour = Some(Hour {
                rows: parquet_rows(&path, &layout.stream.format)?,
                layout: file.layout,
                snapshots: FlatSnapshots::default(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::enums::BookAction;
    use parquet::{
        data_type::{ByteArray, ByteArrayType, Int64Type},
        file::writer::SerializedFileWriter,
        schema::parser::parse_message_type,
    };
    use std::sync::Arc;

    const START: u64 = 1_767_225_600_000_000_000;

    fn write_hour(path: &Path, rows: &[(u64, u64, &str, &str, &str)]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let schema = Arc::new(parse_message_type("message source {
            REQUIRED INT64 event_time; REQUIRED INT64 received_time;
            REQUIRED BINARY symbol (UTF8); REQUIRED BINARY event_type (UTF8);
            REQUIRED BINARY side (UTF8); REQUIRED BINARY price (UTF8); REQUIRED BINARY quantity (UTF8);
        }").unwrap());
        let mut writer =
            SerializedFileWriter::new(File::create(path).unwrap(), schema, Default::default())
                .unwrap();
        let mut group = writer.next_row_group().unwrap();
        for index in 0..7 {
            let mut column = group.next_column().unwrap().unwrap();
            if index < 2 {
                let values = rows
                    .iter()
                    .map(|row| if index == 0 { row.0 } else { row.1 } as i64)
                    .collect::<Vec<_>>();
                column
                    .typed::<Int64Type>()
                    .write_batch(&values, None, None)
                    .unwrap();
            } else {
                let values = rows
                    .iter()
                    .map(|row| {
                        ByteArray::from(match index {
                            2 => "BTC_USDT",
                            3 => "snapshot",
                            4 => row.2,
                            5 => row.3,
                            6 => row.4,
                            _ => unreachable!(),
                        })
                    })
                    .collect::<Vec<_>>();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            column.close().unwrap();
        }
        group.close().unwrap();
        writer.close().unwrap();
    }

    fn source(duplicate: bool) -> (tempfile::TempDir, InstrumentId) {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("archive");
        let paths = [
            "hyperliquid_spot/2026-01-01/00/BTC_USDT_orderbook.parquet",
            "hyperliquid_spot/2026-01-01/01/BTC_USDT_orderbook.parquet",
        ];
        let event = START / 1_000_000 - 1;
        write_hour(
            &archive.join(paths[0]),
            &[
                (event, START + 123, "bid", "10.12", "1.234567"),
                (event, START + 123, "ask", "10.13", "2"),
                (event + 2, START + 1_000_123, "noop", "0", "0"),
            ],
        );
        let at = if duplicate {
            START + 123
        } else {
            START + HOUR + 123
        };
        write_hour(
            &archive.join(paths[1]),
            &[
                (at / 1_000_000, at, "bid", "10.14", "3"),
                (at / 1_000_000, at, "ask", "10.15", "4"),
            ],
        );
        let instrument = InstrumentAny::CurrencyPair(
            nautilus_model::instruments::stubs::currency_pair_btcusdt(),
        );
        let id = instrument.id();
        std::fs::write(temp.path().join("book-snapshots.source.json"), serde_json::to_vec(&serde_json::json!({
            "source_root":archive,"start_date":"2026-01-01","end_date":"2026-01-02",
            "profile":{"streams":[{"name":"books","kind":"book_snapshots","format":"parquet",
                "time_unit":"ms","available_time_unit":"ns","columns":{"symbol":"symbol","time":"event_time",
                    "available_at":"received_time","event_type":"event_type","side":"side","price":"price","size":"quantity"}}]},
            "files":paths.map(|path| serde_json::json!({"path":path,"size":archive.join(path).metadata().unwrap().len(),"stream":"books"})),
            "instrument_source":{"currencies":crate::catalog::currencies::definitions(std::slice::from_ref(&instrument)),
                "instruments":[instrument],"symbol_ids":{"BTC_USDT":id}}
        })).unwrap()).unwrap();
        (temp, id)
    }

    #[test]
    fn raw_hour_replay_uses_receipt_window_keeps_exchange_time_and_clears_empty_books() {
        let (root, id) = source(false);
        let mut books = RawBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
        let first = books.next().unwrap().unwrap();
        assert_eq!(first.received(), START + 123);
        assert_eq!(first.deltas.ts_event.as_u64(), START - 1_000_000);
        assert_eq!(first.deltas.deltas.len(), 3);
        let empty = books.next().unwrap().unwrap();
        assert!(empty.midpoint.is_none());
        assert_eq!(empty.deltas.deltas.len(), 1);
        assert_eq!(empty.deltas.deltas[0].action, BookAction::Clear);
        assert_eq!(
            books.next().unwrap().unwrap().received(),
            START + HOUR + 123
        );
        assert!(books.next().unwrap().is_none());
        let mut later =
            RawBooks::open(root.path(), id, START + 1_000_000, START + 2 * HOUR).unwrap();
        assert!(Arc::ptr_eq(&books.source, &later.source));
        assert_eq!(later.next().unwrap().unwrap().received(), START + 1_000_123);
    }

    #[test]
    fn raw_hour_replay_rejects_cross_file_receipt_reversal() {
        let (root, id) = source(true);
        let mut books = RawBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
        assert!(books.next().unwrap().is_some());
        assert!(books.next().unwrap().is_some());
        assert_eq!(
            books.next().err().unwrap().to_string(),
            "ECONOMIC_BOOK_RECEIPT_ORDER_INVALID"
        );
    }

    #[test]
    fn observed_published_selection_retains_unobserved_assets_without_inventing_books() {
        let (root, id) = source(false);
        let path = root.path().join("book-snapshots.source.json");
        let mut manifest: serde_json::Value =
            serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        manifest["repository"] = serde_json::json!("CryptoHFTData");
        manifest["source_reference"] =
            serde_json::json!("https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data");
        manifest["coverage"] = serde_json::json!("OBSERVED_PUBLISHED_FILES");
        manifest["files"] = serde_json::json!([]);
        manifest["missing"] = serde_json::json!([{"pattern":"hyperliquid_spot/2026-01-01/00/BTC_USDT_orderbook.parquet[.zst]"}]);
        std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let mut books = RawBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
        assert!(books.next().unwrap().is_none());
    }
}
