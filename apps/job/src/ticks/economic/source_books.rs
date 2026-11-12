//! Read selected published book hours directly, without a second catalog.
use super::books::Snapshot;
use crate::source_market::{
    mapped_field, parquet_file_rows, scalar, timestamp, BookSnapshot, Encoding, FlatSnapshots,
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
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{File, Metadata},
    io::BufReader,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
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
    file_identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified: i64,
    changed: i64,
}
impl FileIdentity {
    fn nanoseconds(seconds: i64, nanos: i64) -> Result<i64> {
        seconds
            .checked_mul(1_000_000_000)
            .and_then(|value| value.checked_add(nanos))
            .context("ECONOMIC_BOOK_SOURCE_FILE_TIME_RANGE")
    }
    fn metadata(value: &Metadata) -> Result<Self> {
        Ok(Self {
            device: value.dev(),
            inode: value.ino(),
            size: value.len(),
            modified: Self::nanoseconds(value.mtime(), value.mtime_nsec())?,
            changed: Self::nanoseconds(value.ctime(), value.ctime_nsec())?,
        })
    }
    fn parse(value: &str) -> Result<Self> {
        let mut parts = value.split(':');
        let mut next = || {
            parts
                .next()
                .context("ECONOMIC_BOOK_SOURCE_FILE_IDENTITY_INVALID")
        };
        let identity = Self {
            device: next()?.parse()?,
            inode: next()?.parse()?,
            size: next()?.parse()?,
            modified: next()?.parse()?,
            changed: next()?.parse()?,
        };
        ensure!(
            parts.next().is_none(),
            "ECONOMIC_BOOK_SOURCE_FILE_IDENTITY_INVALID"
        );
        Ok(identity)
    }
    fn check(&self, file: &File, path: &Path) -> Result<()> {
        ensure!(
            *self == Self::metadata(&file.metadata()?)?
                && *self == Self::metadata(&path.metadata()?)?,
            "ECONOMIC_BOOK_SOURCE_FILE_CHANGED: {}",
            path.display()
        );
        Ok(())
    }
}
impl fmt::Display for FileIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}:{}:{}",
            self.device, self.inode, self.size, self.modified, self.changed
        )
    }
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
    instrument_id: InstrumentId,
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
    identity: FileIdentity,
    layout: usize,
}
pub(super) struct IndexedSource {
    root: PathBuf,
    start: u64,
    end: u64,
    observed: bool,
    instruments: BTreeSet<InstrumentId>,
    aliases: BTreeMap<String, BTreeMap<String, InstrumentId>>,
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
                instrument_id: id,
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
        let identity = FileIdentity::parse(&file.file_identity)
            .context("ECONOMIC_BOOK_SOURCE_FILE_IDENTITY_INVALID")?;
        ensure!(
            identity.size == file.size,
            "ECONOMIC_BOOK_SOURCE_FILE_IDENTITY_INVALID"
        );
        self.files.entry(id).or_default().push(HourFile {
            at,
            identity,
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
            aliases: builder.aliases,
            layouts: builder.layouts,
            files,
            missing: builder.missing,
        })
    }
}
struct CachedSource {
    path: PathBuf,
    identity: FileIdentity,
    source: Arc<IndexedSource>,
}
static SOURCE: OnceLock<Mutex<Option<CachedSource>>> = OnceLock::new();
pub(super) fn source(root: &Path) -> Result<Arc<IndexedSource>> {
    let path = root.join("book-snapshots.source.json").canonicalize()?;
    let identity = FileIdentity::metadata(&path.metadata()?)?;
    let mut cached = SOURCE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow::anyhow!("ECONOMIC_BOOK_SOURCE_CACHE_POISONED"))?;
    cached_source(&mut cached, path, identity)
}

fn cached_source(
    cached: &mut Option<CachedSource>,
    path: PathBuf,
    identity: FileIdentity,
) -> Result<Arc<IndexedSource>> {
    if let Some(previous) = cached {
        if previous.path == path && previous.identity == identity {
            return Ok(Arc::clone(&previous.source));
        }
    }
    let source = Arc::new(IndexedSource::open(&path)?);
    *cached = Some(CachedSource {
        path,
        identity,
        source: Arc::clone(&source),
    });
    Ok(source)
}

struct Hour {
    rows: Option<RowIter<'static>>,
    ready: std::vec::IntoIter<BookSnapshot>,
    file: File,
    path: PathBuf,
    identity: FileIdentity,
    layout: usize,
    at: u64,
    checkpoint: Option<CarryCheckpoint>,
    carry_snapshots: u64,
    carry_rows: u64,
}
struct CarryCheckpoint {
    event: u64,
    received: u64,
    rows: u64,
    all_transaction_null: bool,
}
impl Hour {
    fn complete(&mut self, snapshot: BookSnapshot) -> Option<BookSnapshot> {
        let received = snapshot.deltas.ts_init.as_u64();
        if let Some(checkpoint) = self.checkpoint.take() {
            if checkpoint.all_transaction_null
                && checkpoint.event == snapshot.deltas.ts_event.as_u64()
                && checkpoint.received == received
            {
                self.carry_snapshots += 1;
                self.carry_rows += checkpoint.rows;
                return None;
            }
        }
        Some(snapshot)
    }

    fn next(
        &mut self,
        source: &IndexedSource,
        encoding: Encoding,
        start: u64,
        end: u64,
    ) -> Result<Option<BookSnapshot>> {
        if let Some(rows) = self.rows.take() {
            let layout = &source.layouts[self.layout];
            let mut snapshots = FlatSnapshots::default();
            let mut complete = Vec::new();
            for (index, row) in rows.enumerate() {
                let row = row?;
                let symbol = scalar(mapped_field(&row, &layout.stream.columns, "symbol")?)?;
                ensure!(
                    symbol == layout.symbol
                        || source
                            .aliases
                            .get(&layout.market)
                            .and_then(|aliases| aliases.get(&symbol))
                            == Some(&layout.instrument_id),
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
                let transaction_null = |row: &parquet::record::Row| {
                    row.get_column_iter().any(|(name, value)| {
                        name == "transaction_time" && matches!(value, parquet::record::Field::Null)
                    })
                };
                if index == 0 {
                    if source.observed && received == self.at && transaction_null(&row) {
                        self.checkpoint = Some(CarryCheckpoint {
                            event,
                            received,
                            rows: 0,
                            all_transaction_null: true,
                        });
                    }
                }
                if let Some(checkpoint) = &mut self.checkpoint {
                    if (checkpoint.event, checkpoint.received) == (event, received) {
                        checkpoint.rows += 1;
                        checkpoint.all_transaction_null &= transaction_null(&row);
                    }
                }
                if received < start || received >= end {
                    continue;
                }
                if let Some(snapshot) =
                    snapshots.push(&layout.stream.columns, row, encoding, event, received)?
                {
                    // Parse the complete frame first; a carried checkpoint is not new liquidity.
                    if let Some(snapshot) = self.complete(snapshot) {
                        complete.push(snapshot);
                    }
                }
            }
            if let Some(snapshot) = snapshots.finish(&layout.stream.columns)? {
                if let Some(snapshot) = self.complete(snapshot) {
                    complete.push(snapshot);
                }
            }
            // Published rows may follow exchange time instead of provider receipt order.
            complete.sort_by_key(|snapshot| snapshot.deltas.ts_init);
            self.ready = complete.into_iter();
        }
        if self.ready.len() <= 1 {
            self.identity.check(&self.file, &self.path)?;
        }
        Ok(self.ready.next())
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
    carry_snapshots: u64,
    carry_rows: u64,
}
impl RawBooks {
    pub fn open(root: &Path, id: InstrumentId, start: u64, end: u64) -> Result<Self> {
        Self::open_indexed(source(root)?, id, start, end)
    }
    pub(super) fn open_indexed(
        source: Arc<IndexedSource>,
        id: InstrumentId,
        start: u64,
        end: u64,
    ) -> Result<Self> {
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
            carry_snapshots: 0,
            carry_rows: 0,
        })
    }
    pub fn checkpoint_counts(&self) -> (u64, u64) {
        (self.carry_snapshots, self.carry_rows)
    }
    pub fn next(&mut self) -> Result<Option<Snapshot>> {
        loop {
            if let Some(hour) = &mut self.hour {
                let before = (hour.carry_snapshots, hour.carry_rows);
                let next = hour.next(&self.source, self.encoding, self.start, self.end)?;
                self.carry_snapshots += hour.carry_snapshots - before.0;
                self.carry_rows += hour.carry_rows - before.1;
                if let Some(snapshot) = next {
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
                path.starts_with(&self.source.root),
                "ECONOMIC_BOOK_SOURCE_FILE_MISMATCH: {relative}"
            );
            let opened = File::open(&path)?;
            file.identity.check(&opened, &path)?;
            self.hour = Some(Hour {
                rows: Some(parquet_file_rows(
                    opened.try_clone()?,
                    &layout.stream.format,
                )?),
                ready: Vec::new().into_iter(),
                file: opened,
                path,
                identity: file.identity,
                layout: file.layout,
                at: file.at,
                checkpoint: None,
                carry_snapshots: 0,
                carry_rows: 0,
            });
        }
    }
}

/// Convert the existing published-hour source to an ordinary native catalog.
/// Source files are read only; carry checkpoints remain validation-only, not new events.
pub fn export_catalog(
    root: &Path,
    output: &Path,
    start: u64,
    end: u64,
) -> Result<serde_json::Value> {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    let header: Header = serde_json::from_reader(BufReader::new(File::open(
        root.join("book-snapshots.source.json"),
    )?))?;
    let instruments = crate::catalog::currencies::instruments_from_value(header.instrument_source)?;
    let indexed = source(root)?;
    // Publish only after all source reads and native writes succeeded. Existing data is never replaced.
    ensure!(!output.exists(), "OUTPUT_MUST_BE_NEW");
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::tempdir_in(parent)?;
    let catalog_root = staging.path().join("catalog");
    std::fs::create_dir(&catalog_root)?;
    crate::catalog::currencies::write(&catalog_root, &instruments)?;
    let catalog = ParquetDataCatalog::new(&catalog_root, None, None, None, None);
    catalog.write_instruments(instruments.clone())?;
    let mut rows = 0u64;
    let mut frames = 0u64;
    let mut checkpoints = (0u64, 0u64);
    for instrument in instruments {
        if !indexed.instruments.contains(&instrument.id()) {
            continue;
        }
        let mut books = RawBooks::open_indexed(indexed.clone(), instrument.id(), start, end)?;
        // Native engines require the declared instrument precision. Existing parsing checks
        // exact Decimal equality and refuses any value that would be rounded.
        books.encoding = Encoding::new(instrument.id(), Some(&instrument), false)?;
        let mut pending = Vec::new();
        while let Some(snapshot) = books.next()? {
            frames += 1;
            rows += snapshot.deltas.deltas.len() as u64;
            pending.extend(snapshot.deltas.deltas);
            // Flush only complete F_LAST batches, preserving the original pair of clocks.
            if pending.len() >= 100_000 {
                catalog.write_to_parquet(&pending, None, None, None)?;
                pending.clear();
            }
        }
        if !pending.is_empty() {
            catalog.write_to_parquet(&pending, None, None, None)?;
        }
        let counts = books.checkpoint_counts();
        checkpoints.0 += counts.0;
        checkpoints.1 += counts.1;
    }
    ensure!(frames > 0, "PUBLISHED_BOOK_SELECTION_EMPTY");
    let report = serde_json::json!({
        "native_catalog":"catalog", "available_start_ns":start.to_string(),
        "available_end_ns":end.to_string(), "frames":frames,"rows":rows,
        "validated_carry_snapshots":checkpoints.0,"validated_carry_rows":checkpoints.1,
        "clock_semantics":"ORIGINAL_TS_EVENT_AND_PROVIDER_RECEIPT_TS_INIT",
        "scope":"L2_ONLY_NO_SYNTHETIC_TRADES_QUOTES_OR_FUNDING"
    });
    std::fs::write(
        staging.path().join("import.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    ensure!(!output.exists(), "OUTPUT_MUST_BE_NEW");
    std::fs::rename(staging.path(), output)?;
    Ok(report)
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
    use rust_decimal::Decimal;
    use std::sync::Arc;

    const START: u64 = 1_767_225_600_000_000_000;

    fn write_hour(path: &Path, rows: &[(u64, u64, &str, &str, &str)]) {
        write_hour_transactions(path, rows, None);
    }

    fn write_hour_transactions(
        path: &Path,
        rows: &[(u64, u64, &str, &str, &str)],
        transactions: Option<&[Option<i64>]>,
    ) {
        write_hour_transactions_with_symbol(path, rows, "BTC_USDT", transactions);
    }

    fn write_hour_transactions_with_symbol(
        path: &Path,
        rows: &[(u64, u64, &str, &str, &str)],
        symbol: &str,
        transactions: Option<&[Option<i64>]>,
    ) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let schema = Arc::new(parse_message_type(&format!("message source {{
            REQUIRED INT64 event_time; REQUIRED INT64 received_time;
            REQUIRED BINARY symbol (UTF8); REQUIRED BINARY event_type (UTF8);
            REQUIRED BINARY side (UTF8); REQUIRED BINARY price (UTF8); REQUIRED BINARY quantity (UTF8);
            {} }}", if transactions.is_some() { "OPTIONAL INT64 transaction_time;" } else { "" })).unwrap());
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
                            2 => symbol,
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
        if let Some(transactions) = transactions {
            assert_eq!(rows.len(), transactions.len());
            let mut column = group.next_column().unwrap().unwrap();
            let values = transactions.iter().flatten().copied().collect::<Vec<_>>();
            let levels = transactions
                .iter()
                .map(|value| i16::from(value.is_some()))
                .collect::<Vec<_>>();
            column
                .typed::<Int64Type>()
                .write_batch(&values, Some(&levels), None)
                .unwrap();
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
            "files":paths.map(|path| {
                let metadata = archive.join(path).metadata().unwrap();
                serde_json::json!({"path":path,"size":metadata.len(),"stream":"books",
                    "file_identity":FileIdentity::metadata(&metadata).unwrap().to_string()})
            }),
            "instrument_source":{"currencies":crate::catalog::currencies::definitions(std::slice::from_ref(&instrument)),
                "instruments":[instrument],"symbol_ids":{"BTC_USDT":id,"BTC_USDT_ALIAS":id}}
        })).unwrap()).unwrap();
        (temp, id)
    }

    fn replace_first_hour(
        root: &Path,
        rows: &[(u64, u64, &str, &str, &str)],
        row_symbol: &str,
        transactions: Option<&[Option<i64>]>,
        observed: bool,
    ) {
        let file = root.join("archive/hyperliquid_spot/2026-01-01/00/BTC_USDT_orderbook.parquet");
        write_hour_transactions_with_symbol(&file, rows, row_symbol, transactions);
        let path = root.join("book-snapshots.source.json");
        let mut manifest: serde_json::Value =
            serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        manifest["files"].as_array_mut().unwrap().truncate(1);
        let metadata = file.metadata().unwrap();
        manifest["files"][0]["size"] = serde_json::json!(metadata.len());
        manifest["files"][0]["file_identity"] =
            serde_json::json!(FileIdentity::metadata(&metadata).unwrap().to_string());
        if observed {
            manifest["repository"] = serde_json::json!("CryptoHFTData");
            manifest["source_reference"] = serde_json::json!(
                "https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data"
            );
            manifest["coverage"] = serde_json::json!("OBSERVED_PUBLISHED_FILES");
        }
        std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }

    #[test]
    fn published_index_survives_last_reader_and_reloads_changed_manifest() {
        let (root, id) = source(false);
        let path = root
            .path()
            .join("book-snapshots.source.json")
            .canonicalize()
            .unwrap();
        let identity = FileIdentity::metadata(&path.metadata().unwrap()).unwrap();
        let mut cached = None;
        let first = cached_source(&mut cached, path.clone(), identity).unwrap();
        let previous = Arc::downgrade(&first);
        let reader = RawBooks::open_indexed(first, id, START, START + 2 * HOUR).unwrap();
        drop(reader);
        let reloaded = cached_source(&mut cached, path.clone(), identity).unwrap();
        let previous = previous.upgrade().unwrap();
        assert!(Arc::ptr_eq(&previous, &reloaded));
        drop(reloaded);

        let mut manifest = std::fs::read_to_string(&path).unwrap();
        manifest.push('\n');
        std::fs::write(&path, manifest).unwrap();
        let changed = FileIdentity::metadata(&path.metadata().unwrap()).unwrap();
        let reloaded = cached_source(&mut cached, path, changed).unwrap();
        assert!(!Arc::ptr_eq(&previous, &reloaded));
    }

    #[test]
    fn published_carry_checkpoint_is_validated_without_refreshing_books_or_clocks() {
        for (only_checkpoint, valid) in [(false, true), (true, true), (false, false)] {
            let (root, id) = source(false);
            // Venue time can be ahead; only the source's exact checkpoint marker decides.
            let event = START / 1_000_000 + 1;
            let mut rows = vec![
                (
                    event,
                    START,
                    "bid",
                    if valid { "10.12" } else { "10.14" },
                    "1",
                ),
                (event, START, "ask", "10.13", "2"),
            ];
            let mut transactions = vec![None, None];
            if !only_checkpoint {
                rows.extend([
                    (event + 2, START + 2_000_000, "bid", "10.11", "3"),
                    (event + 2, START + 2_000_000, "ask", "10.15", "4"),
                ]);
                transactions.extend([Some(1), Some(1)]);
            }
            replace_first_hour(root.path(), &rows, "BTC_USDT", Some(&transactions), true);
            let mut books = RawBooks::open(root.path(), id, START, START + HOUR).unwrap();
            if !valid {
                assert_eq!(books.next().err().unwrap().to_string(), "BOOK_CROSSED");
                assert_eq!(books.checkpoint_counts(), (0, 0));
                continue;
            }
            let next = books.next().unwrap();
            if only_checkpoint {
                assert!(next.is_none());
            } else {
                let next = next.unwrap();
                assert_eq!(next.received(), START + 2_000_000);
                assert_eq!(next.deltas.ts_event.as_u64(), START + 3_000_000);
                assert_eq!(next.bid_depth, rust_decimal::Decimal::from(3));
                assert!(books.next().unwrap().is_none());
            }
            assert_eq!(books.checkpoint_counts(), (1, 2));
            let output = root.path().join("exported");
            let export = export_catalog(root.path(), &output, START, START + HOUR);
            if only_checkpoint {
                assert!(export.is_err());
                assert!(!output.exists());
            } else {
                let report = export.unwrap();
                assert_eq!(report["frames"], 1);
                assert_eq!(report["validated_carry_snapshots"], 1);
                assert_eq!(report["validated_carry_rows"], 2);
            }
        }
    }

    #[test]
    fn checkpoint_marker_never_discards_missing_nonnull_mixed_or_nonfirst_real_frames() {
        for (observed, offset, transactions) in [
            (true, 0, None),
            (true, 0, Some(vec![Some(1), Some(1)])),
            (true, 0, Some(vec![None, Some(1)])),
            (true, 123, Some(vec![None, None])),
            (false, 0, Some(vec![None, None])),
        ] {
            let (root, id) = source(false);
            let event = START / 1_000_000 + 1;
            replace_first_hour(
                root.path(),
                &[
                    (event, START + offset, "bid", "10.12", "1"),
                    (event, START + offset, "ask", "10.13", "2"),
                ],
                "BTC_USDT",
                transactions.as_deref(),
                observed,
            );
            let mut books = RawBooks::open(root.path(), id, START, START + HOUR).unwrap();
            let first = books.next().unwrap().unwrap();
            assert_eq!(first.received(), START + offset);
            assert_eq!(first.deltas.ts_event.as_u64(), START + 1_000_000);
            assert!(books.next().unwrap().is_none());
            assert_eq!(books.checkpoint_counts(), (0, 0));
        }
        let (root, id) = source(false);
        let event = START / 1_000_000;
        replace_first_hour(
            root.path(),
            &[
                (event, START - 1, "bid", "10.12", "1"),
                (event, START - 1, "ask", "10.13", "2"),
                (event + 1, START, "bid", "10.11", "3"),
                (event + 1, START, "ask", "10.15", "4"),
            ],
            "BTC_USDT",
            Some(&[Some(1), Some(1), None, None]),
            true,
        );
        let mut books = RawBooks::open(root.path(), id, START, START + HOUR).unwrap();
        assert_eq!(books.next().unwrap().unwrap().received(), START);
        assert_eq!(books.checkpoint_counts(), (0, 0));
    }

    #[test]
    fn economic_book_predictions_use_research_root_with_independent_execution_books() {
        use crate::ticks::cohort_cache;
        use contracts::ticks::{
            NativeTickAvailabilityV1, NativeTickFeatureProfileV1, NativeTickRepresentationV1,
            NativeTickSamplingKindV1,
        };
        use nautilus_model::{
            data::TradeTick,
            enums::{AggressorSide, CurrencyType},
            identifiers::{Symbol, TradeId},
            instruments::CurrencyPair,
            types::{Currency, Price, Quantity},
        };
        let (research_books, id) = source(false);
        let (execution, _) = source(false);
        replace_first_hour(
            execution.path(),
            &[
                (START / 1_000_000 - 1, START + 123, "bid", "10.12", "1"),
                (START / 1_000_000 - 1, START + 123, "ask", "10.13", "10"),
            ],
            "BTC_USDT",
            None,
            true,
        );
        let usdt0 = Currency::try_from_str("USDT0").unwrap_or_else(|| {
            let currency = Currency::new("USDT0", 8, 0, "USDT0", CurrencyType::Crypto);
            Currency::register(currency, false).unwrap();
            currency
        });
        let bridge = InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from("USDT0-USDC-SPOT.HYPERLIQUID"))
                .raw_symbol(Symbol::new("BOOK-ROOT-TEST"))
                .base_currency(usdt0)
                .quote_currency(Currency::USDC())
                .price_precision(3)
                .size_precision(4)
                .price_increment(Price::from("0.001"))
                .size_increment(Quantity::from("0.0001"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        );
        let path = research_books.path().join("book-snapshots.source.json");
        let mut research_manifest: serde_json::Value =
            serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        research_manifest["end_date"] = serde_json::json!("2026-01-04");
        let mut instruments: Vec<InstrumentAny> =
            crate::catalog::currencies::instruments_from_value(
                research_manifest["instrument_source"].clone(),
            )
            .unwrap();
        instruments.push(bridge.clone());
        let execution_path = execution.path().join("book-snapshots.source.json");
        let mut execution_manifest: serde_json::Value =
            serde_json::from_reader(File::open(&execution_path).unwrap()).unwrap();
        execution_manifest["end_date"] = serde_json::json!("2026-01-04");
        execution_manifest["instrument_source"]["instruments"] =
            serde_json::to_value(&instruments).unwrap();
        execution_manifest["instrument_source"]["currencies"] =
            serde_json::to_value(crate::catalog::currencies::definitions(&instruments)).unwrap();
        std::fs::write(
            &execution_path,
            serde_json::to_vec(&execution_manifest).unwrap(),
        )
        .unwrap();
        let output = research_books.path().join("ticks");
        let mut builder = cohort_cache::Builder::new_received(&output, 100, 0, 0, true, 0).unwrap();
        for at in [START + 120, START + 123, START + 400] {
            for (index, instrument) in [id, bridge.id()].into_iter().enumerate() {
                builder
                    .push(TradeTick::new(
                        instrument,
                        Price::from("10.12"),
                        Quantity::from("1.0000"),
                        AggressorSide::Buy,
                        TradeId::from(format!("{index}-{at}").as_str()),
                        at.into(),
                        at.into(),
                    ))
                    .unwrap();
            }
        }
        let end = START + 3 * super::super::DAY;
        let empty_id = InstrumentId::from("EMPTY.SIM");
        builder
            .push(TradeTick::new(
                empty_id,
                Price::from("10.12"),
                Quantity::from("1.0000"),
                AggressorSide::Buy,
                TradeId::from("after-primary"),
                (end - 1).into(),
                (end - 1).into(),
            ))
            .unwrap();
        let manifest = builder.seal("fixed-source", START, end).unwrap();
        builder.publish(manifest, serde_json::json!({
            "status":"IMPORTED","availability":"RECEIVED_TIME_OBSERVED","source_revision":"fixed-source",
            "event_start_ns":START.to_string(),"event_end_ns":end.to_string(),
            "missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0,
            "instrument_source":{"currencies":crate::catalog::currencies::definitions(&instruments),"instruments":instruments}
        })).unwrap();
        let root = output.join("catalog");
        std::fs::write(
            root.join("book-snapshots.source.json"),
            serde_json::to_vec(&research_manifest).unwrap(),
        )
        .unwrap();
        let mut request = super::super::tests::request();
        request.research.input.instrument_ids = vec![id.to_string()];
        request.research.input.event_start_ns = super::super::counter(START).unwrap();
        request.research.input.event_end_ns = super::super::counter(end).unwrap();
        request.research.input.decision_cutoff_ns = request.research.input.event_end_ns;
        request.research.input.availability = NativeTickAvailabilityV1::ReceivedTimeObserved;
        request.research.input.representation =
            Some(NativeTickRepresentationV1::ReceivedTimeCohortFlowCacheV1);
        request.research.input.maximum_receive_delay_ns = Some(super::super::counter(0).unwrap());
        request.research.sampling.kind = NativeTickSamplingKindV1::ReceivedTimeCohortV1;
        request.research.parameters.feature_profile =
            Some(NativeTickFeatureProfileV1::ReceivedFlowBookV1);
        request.research.train_end_ns = super::super::counter(START + super::super::DAY).unwrap();
        request.research.validation_end_ns =
            super::super::counter(START + 2 * super::super::DAY).unwrap();
        request.fx_instrument_ids = vec![bridge.id().to_string()];
        let wasm = wat::parse_str(format!(
            r#"(module
            (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
                local.get 0 i32.const 8 i32.eq if
                    local.get 1 f64.const 0.6172835 f64.ne if unreachable end
                    local.get 2 i32.eqz i32.eqz if unreachable end
                    local.get 3 i64.const {} i64.ne if unreachable end
                    local.get 4 i64.const {} i64.ne if unreachable end end)
            (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
                local.get 4 i32.const 12 i32.ne if unreachable end f64.const 0))"#,
            START - 1_000_000,
            START + 123
        ))
        .unwrap();
        for model in [
            None,
            Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::ObservedL2IocV1),
        ] {
            request.scenario.execution_model = model;
            let result =
                super::super::replay_with_funding(&root, execution.path(), &request, &wasm)
                    .unwrap();
            assert_eq!(
                result
                    .assets
                    .iter()
                    .find(|a| a.instrument_id == id.to_string())
                    .unwrap()
                    .cohorts
                    .get(),
                3
            );
        }
        // Identical source through a separate root retains the original two-reader path.
        research_manifest["repository"] = serde_json::json!("CryptoHFTData");
        research_manifest["source_reference"] =
            serde_json::json!("https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data");
        research_manifest["coverage"] = serde_json::json!("OBSERVED_PUBLISHED_FILES");
        research_manifest["instrument_source"]["instruments"] =
            serde_json::to_value(&instruments).unwrap();
        research_manifest["instrument_source"]["currencies"] =
            serde_json::to_value(crate::catalog::currencies::definitions(&instruments)).unwrap();
        let same_source = tempfile::tempdir().unwrap();
        for directory in [&root, &same_source.path().to_path_buf()] {
            std::fs::write(
                directory.join("book-snapshots.source.json"),
                serde_json::to_vec(&research_manifest).unwrap(),
            )
            .unwrap();
        }
        for model in [
            contracts::economic_tick::NativeTickEconomicExecutionModelV1::ObservedL2IocV1,
            contracts::economic_tick::NativeTickEconomicExecutionModelV1::ObservedL2IocV2,
        ] {
            request.scenario.execution_model = Some(model);
            let original =
                super::super::replay_with_funding(&root, same_source.path(), &request, &wasm)
                    .unwrap();
            let shared = super::super::replay_models_with_funding_and_cores(
                &root,
                &root,
                &request,
                &[wasm.clone()],
                3,
            )
            .unwrap();
            let mut shared = serde_json::to_value(shared).unwrap();
            let mut original = serde_json::to_value(original).unwrap();
            for result in [&mut shared, &mut original] {
                let evidence = &mut result["methods"]["initial_conversion_native_evidence"];
                let mut native: serde_json::Value =
                    serde_json::from_str(evidence.as_str().unwrap()).unwrap();
                native.as_object_mut().unwrap().remove("native_session_id");
                *evidence = serde_json::to_value(native.to_string()).unwrap();
            }
            for (key, value) in shared.as_object().unwrap() {
                assert_eq!(value, &original[key], "{key}");
            }
        }
        let mut clipped = request.research.clone();
        clipped.input.instrument_ids.push(empty_id.to_string());
        let prepared = research_books.path().join("clipped");
        let metadata = cohort_cache::prepare_views(&root, &clipped, &prepared, 1_000_000).unwrap();
        clipped = serde_json::from_value(metadata["research_request"].clone()).unwrap();
        let result =
            cohort_cache::load(&prepared.join("research/catalog"), &clipped)
                .unwrap();
        let empty = result
            .coverage
            .instrument_coverage
            .iter()
            .find(|asset| asset.instrument_id == empty_id.to_string())
            .unwrap();
        assert_eq!(empty.trade_count.get(), 0);
        assert_eq!(empty.cohort_count.get(), 0);
        assert_eq!(empty.first_event_ns, None);
        assert_eq!(empty.last_event_ns, None);

    }

    #[test]
    fn skipped_feature_snapshot_still_validates_source_event_counter() {
        use super::super::books::FeatureBooks;
        let (root, id) = source(false);
        replace_first_hour(
            root.path(),
            &[
                (
                    i64::MAX as u64 / 1_000_000 + 1,
                    START + 123,
                    "bid",
                    "10.12",
                    "1",
                ),
                (
                    i64::MAX as u64 / 1_000_000 + 1,
                    START + 123,
                    "ask",
                    "10.13",
                    "1",
                ),
                (START / 1_000_000 + 1, START + 1_000_123, "noop", "0", "0"),
            ],
            "BTC_USDT",
            None,
            true,
        );
        let mut reader = FeatureBooks::open(root.path(), id, START, START + HOUR).unwrap();
        let expected = reader.advance(START + 1_000_123).err().unwrap().to_string();
        let mut raw =
            super::super::books::Books::open(root.path(), id, START, START + HOUR).unwrap();
        let mut shared = FeatureBooks::shared();
        while let Some(frame) = raw.next().unwrap() {
            shared.observe(&frame);
        }
        assert_eq!(
            shared.advance(START + 1_000_123).err().unwrap().to_string(),
            expected
        );
    }

    #[test]
    fn received_book_features_match_research_and_economic_prediction() {
        use super::super::{
            books::{BookFeatures, Books, FeatureBooks},
            models::EconomicModels,
        };
        use crate::{feature_model::FeatureModel, signals::SignalModule};
        use contracts::science::{FeatureMissingReasonV1, FeatureValueV1};
        let (root, id) = source(false);
        let mut research = FeatureBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
        let mut economic = Books::open(root.path(), id, START, START + 2 * HOUR).unwrap();
        let mut head = economic.next().unwrap();
        let mut shared = FeatureBooks::shared();
        let mut latest = None;
        let prefix = vec![
            FeatureValueV1 {
                value: Some(17.0),
                missing_reason: None,
                event_ns: None,
                observed_available_ns: None,
                effective_available_ns: None,
                sequence: None,
            };
            8
        ];
        let times = [
            START + 122,
            START + 123,
            START + 124,
            START + 1_000_123,
            START + HOUR + 123,
        ];
        for at in times {
            while head.as_ref().is_some_and(|frame| frame.received() <= at) {
                let frame = head.take().unwrap();
                shared.observe(&frame);
                latest = Some(BookFeatures::from_snapshot(&frame).unwrap());
                // Execution normalization must never replace the observed feature frame.
                frame
                    .floor_sizes(nautilus_model::types::Quantity::from("1"))
                    .unwrap();
                head = economic.next().unwrap();
            }
            let mut research_features = prefix.clone();
            BookFeatures::append(&mut research_features, research.advance(at).unwrap(), at);
            let mut economic_features = prefix.clone();
            BookFeatures::append(&mut economic_features, latest.as_ref(), at);
            assert_eq!(research_features, economic_features);
            let mut shared_features = prefix.clone();
            BookFeatures::append(&mut shared_features, shared.advance(at).unwrap(), at);
            assert_eq!(shared_features, research_features);
            assert_eq!(&research_features[..8], &prefix);
            if at == START + 122 {
                assert_eq!(
                    research_features[8].missing_reason,
                    Some(FeatureMissingReasonV1::NotYetAvailable)
                );
            } else if at == START + 1_000_123 {
                assert!(research_features[8..].iter().all(|f| f.value.is_none()
                    && f.missing_reason == Some(FeatureMissingReasonV1::SourceMissing)));
            } else {
                assert!(research_features[8..].iter().all(|f| f.value.is_some()));
                if at <= START + 124 {
                    assert_eq!(
                        research_features[8].event_ns.unwrap().get(),
                        START - 1_000_000
                    );
                    assert_eq!(
                        research_features[8].effective_available_ns.unwrap().get(),
                        START + 123
                    );
                }
            }
            for slot in 8..12 {
                let wasm = crate::ticks::tests::model(slot);
                let module = SignalModule::new(&wasm, false).unwrap();
                let mut model = FeatureModel::new(&module, Some(1), None).unwrap();
                let (mut models, _) = EconomicModels::new(&[(1.0, module)], 1, None).unwrap();
                assert_eq!(
                    model.predict(at, at, 0, &research_features).unwrap(),
                    models
                        .predict(at, at, 0, &economic_features, None)
                        .unwrap()
                        .0
                );
            }
        }
        // A single cohort can pass complete and empty intermediate snapshots.
        for at in [START + 1_000_123, START + HOUR + 123] {
            let mut jumped = FeatureBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
            let mut expected = Books::open(root.path(), id, START, START + 2 * HOUR).unwrap();
            let mut selected = None;
            while let Some(frame) = expected.next().unwrap() {
                if frame.received() > at {
                    break;
                }
                selected = Some(BookFeatures::from_snapshot(&frame).unwrap());
            }
            let mut actual_features = prefix.clone();
            BookFeatures::append(&mut actual_features, jumped.advance(at).unwrap(), at);
            let mut expected_features = prefix.clone();
            BookFeatures::append(&mut expected_features, selected.as_ref(), at);
            assert_eq!(actual_features, expected_features);
            let mut unchanged = prefix.clone();
            BookFeatures::append(&mut unchanged, jumped.advance(at).unwrap(), at);
            assert_eq!(unchanged, actual_features);
        }
        let missing = tempfile::tempdir().unwrap();
        assert!(FeatureBooks::open(missing.path(), id, START, START + HOUR).is_err());
    }

    #[test]
    fn published_export_rejects_precision_loss_without_publishing_or_changing_source() {
        let (root, _) = source(false);
        replace_first_hour(
            root.path(),
            &[
                (START / 1_000_000, START + 123, "bid", "10.123", "1"),
                (START / 1_000_000, START + 123, "ask", "10.13", "2"),
            ],
            "BTC_USDT",
            None,
            true,
        );
        let before = std::fs::read(root.path().join("book-snapshots.source.json")).unwrap();
        let output = root.path().join("unrepresentable");
        assert!(export_catalog(root.path(), &output, START, START + HOUR)
            .unwrap_err()
            .to_string()
            .contains("PRICE_PRECISION_LOSS"));
        assert!(!output.exists());
        assert_eq!(
            before,
            std::fs::read(root.path().join("book-snapshots.source.json")).unwrap()
        );
    }

    #[test]
    fn exported_published_books_preserve_clocks_and_run_native_strategy() {
        use nautilus_model::data::OrderBookDelta;
        use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
        let (root, id) = source(false);
        let output = root.path().join("native-output");
        let report = export_catalog(root.path(), &output, START, START + 2 * HOUR).unwrap();
        assert_eq!(report["frames"], 3);
        let mut native = ParquetDataCatalog::new(&output.join("catalog"), None, None, None, None);
        let rows = native
            .query::<OrderBookDelta>(Some(vec![id.to_string()]), None, None, None, None, false)
            .unwrap();
        let mut original = RawBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
        let mut expected = Vec::new();
        while let Some(frame) = original.next().unwrap() {
            expected.extend(frame.deltas.deltas);
        }
        assert_eq!(rows.len(), expected.len());
        for (actual, original) in rows.iter().zip(&expected) {
            assert_eq!(actual.ts_event, original.ts_event);
            assert_eq!(actual.ts_init, original.ts_init);
            assert_eq!(actual.action, original.action);
            assert_eq!(actual.flags, original.flags);
            assert_eq!(
                actual.order.price.as_decimal(),
                original.order.price.as_decimal()
            );
            assert_eq!(
                actual.order.size.as_decimal(),
                original.order.size.as_decimal()
            );
        }
        assert_eq!(rows[0].ts_event.as_u64(), START - 1_000_000);
        assert_eq!(rows[0].ts_init.as_u64(), START + 123);
        assert_eq!(rows[3].action, BookAction::Clear);
        let request = serde_json::from_value(serde_json::json!({
          "schema_version":1,"instrument_id":id.to_string(),
          "selectors":[{"data_type":"OrderBookDelta","identifier":id.to_string()}],
          "event_start_ns":(START-1_000_000).to_string(),"event_end_ns":(START+2*HOUR).to_string(),
          "available_start_ns":START.to_string(),"available_end_ns":(START+2*HOUR).to_string(),
          "venue":{"account_type":"CASH","oms_type":"NETTING","book_type":"L2_MBP","starting_balances":["100000 USDT","1 BTC"],"base_currency":null,"leverage":"1","maker_fee":"0","taker_fee":"0"},
          "strategy":{"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"0.010000","trigger_min_size":"0.1","trigger_imbalance_ratio":"0.9","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":false}
        })).unwrap();
        let result = crate::native_strategy::run(&output.join("catalog"), request).unwrap();
        assert_eq!(
            result.data_rows_by_type["OrderBookDelta"].get(),
            rows.len() as u64
        );
        assert!(!result.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(export_catalog(root.path(), &output, START, START + 2 * HOUR).is_err());
        assert!(root.path().join("book-snapshots.source.json").exists());
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
        assert_eq!(later.next().unwrap().unwrap().received(), START + 1_000_123);
    }

    #[test]
    fn raw_hour_replay_matches_row_alias_to_file_instrument() {
        let rows = [
            (START / 1_000_000 - 1, START + 123, "bid", "10.12", "1"),
            (START / 1_000_000 - 1, START + 123, "ask", "10.13", "2"),
        ];
        let (root, id) = source(false);
        replace_first_hour(root.path(), &rows, "BTC_USDT_ALIAS", None, false);
        let mut books = RawBooks::open(root.path(), id, START, START + HOUR).unwrap();
        assert_eq!(books.next().unwrap().unwrap().received(), START + 123);

        let (root, id) = source(false);
        replace_first_hour(root.path(), &rows, "UNKNOWN", None, false);
        let mut books = RawBooks::open(root.path(), id, START, START + HOUR).unwrap();
        assert_eq!(
            books.next().err().unwrap().to_string(),
            "ECONOMIC_BOOK_SOURCE_SYMBOL_MISMATCH"
        );
    }

    #[test]
    fn raw_hour_replay_orders_complete_frames_by_original_provider_receipt() {
        let (root, id) = source(false);
        let early = START + 31_375_438_289;
        let late = START + 31_389_330_520;
        let event = START / 1_000_000 + 9_016;
        replace_first_hour(
            root.path(),
            &[
                (event, late, "bid", "10.12", "1.234567"),
                (event, late, "ask", "10.13", "2"),
                (event + 545, early, "bid", "10.14", "3"),
                (event + 545, early, "ask", "10.15", "4"),
            ],
            "BTC_USDT",
            None,
            true,
        );
        let mut books = RawBooks::open(root.path(), id, START, START + HOUR).unwrap();
        let first = books.next().unwrap().unwrap();
        let second = books.next().unwrap().unwrap();
        assert_eq!(first.received(), early);
        assert_eq!(second.received(), late);
        assert_eq!(first.deltas.ts_event.as_u64(), (event + 545) * 1_000_000);
        assert_eq!(second.deltas.ts_event.as_u64(), event * 1_000_000);
        assert_eq!(first.deltas.deltas.len(), 3);
        assert_eq!(second.deltas.deltas.len(), 3);
        assert_eq!(first.bid_depth, Decimal::from(3));
        assert_eq!(first.ask_depth, Decimal::from(4));
        assert_eq!(second.bid_depth, Decimal::new(1_234_567, 6));
        assert_eq!(second.ask_depth, Decimal::from(2));
        assert_eq!(first.price(true), Some(Decimal::new(1015, 2)));
        assert_eq!(second.price(false), Some(Decimal::new(1012, 2)));
        assert!(books.next().unwrap().is_none());
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

    fn rewrite_same_size_preserving_mtime(path: &Path) {
        let before = path.metadata().unwrap();
        let mut bytes = std::fs::read(path).unwrap();
        let price = bytes
            .windows(5)
            .position(|value| value == b"10.12")
            .unwrap();
        bytes[price + 4] = b'1';
        std::thread::sleep(std::time::Duration::from_millis(2));
        std::fs::write(path, bytes).unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(before.modified().unwrap()))
            .unwrap();
        let after = path.metadata().unwrap();
        assert_eq!(
            (
                before.dev(),
                before.ino(),
                before.len(),
                before.modified().unwrap()
            ),
            (
                after.dev(),
                after.ino(),
                after.len(),
                after.modified().unwrap()
            )
        );
        assert_ne!(
            (before.ctime(), before.ctime_nsec()),
            (after.ctime(), after.ctime_nsec())
        );
    }

    #[test]
    fn raw_hour_identity_rejects_same_size_rewrites_before_open_and_at_read_end() {
        assert_eq!(FileIdentity::nanoseconds(-1, 999_999_999).unwrap(), -1);
        for reading in [false, true] {
            let (root, id) = source(false);
            let mut books = RawBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
            if reading {
                assert!(books.next().unwrap().is_some());
            }
            rewrite_same_size_preserving_mtime(
                &root
                    .path()
                    .join("archive/hyperliquid_spot/2026-01-01/00/BTC_USDT_orderbook.parquet"),
            );
            assert!(books
                .next()
                .err()
                .unwrap()
                .to_string()
                .starts_with("ECONOMIC_BOOK_SOURCE_FILE_CHANGED:"));
        }
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
