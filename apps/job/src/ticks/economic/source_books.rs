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
                if index == 0 && source.observed && received == self.at && transaction_null(&row) {
                    self.checkpoint = Some(CarryCheckpoint {
                        event,
                        received,
                        rows: 0,
                        all_transaction_null: true,
                    });
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
    #[cfg(test)]
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
    export_catalog_selected(root, output, start, end, &[])
}

pub fn export_catalog_selected(
    root: &Path,
    output: &Path,
    start: u64,
    end: u64,
    selected: &[InstrumentId],
) -> Result<serde_json::Value> {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    let header: Header = serde_json::from_reader(BufReader::new(File::open(
        root.join("book-snapshots.source.json"),
    )?))?;
    let instruments = crate::catalog::currencies::instruments_from_value(header.instrument_source)?;
    let indexed = source(root)?;
    let requested = selected.iter().copied().collect::<BTreeSet<_>>();
    for id in &requested {
        ensure!(
            instruments.iter().any(|instrument| instrument.id() == *id),
            "PUBLISHED_BOOK_INSTRUMENT_UNKNOWN: {id}"
        );
        ensure!(
            indexed.instruments.contains(id),
            "PUBLISHED_BOOK_INSTRUMENT_UNSUPPORTED: {id}"
        );
    }
    let instruments = instruments
        .into_iter()
        .filter(|instrument| requested.is_empty() || requested.contains(&instrument.id()))
        .collect::<Vec<_>>();
    let exported_ids = instruments
        .iter()
        .map(|instrument| instrument.id().to_string())
        .collect::<Vec<_>>();
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
        let before_frames = frames;
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
        if !requested.is_empty() {
            ensure!(
                frames > before_frames,
                "PUBLISHED_BOOK_SELECTED_INSTRUMENT_EMPTY: {}",
                instrument.id()
            );
        }
        let counts = books.checkpoint_counts();
        checkpoints.0 += counts.0;
        checkpoints.1 += counts.1;
    }
    ensure!(frames > 0, "PUBLISHED_BOOK_SELECTION_EMPTY");
    let report = serde_json::json!({
        "native_catalog":"catalog", "instrument_ids":exported_ids, "available_start_ns":start.to_string(),
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
    fn published_books_select_exact_native_instruments_without_touching_other_data() {
        use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
        let (root, btc) = source(false);
        let eth = InstrumentAny::CurrencyPair(
            nautilus_model::instruments::stubs::currency_pair_ethusdt(),
        );
        let eth_id = eth.id();
        let relative = "hyperliquid_spot/2026-01-01/00/ETH_USDT_orderbook.parquet";
        let file = root.path().join("archive").join(relative);
        write_hour_transactions_with_symbol(
            &file,
            &[
                (START / 1_000_000, START + 456, "bid", "20.12", "1"),
                (START / 1_000_000, START + 456, "ask", "20.13", "2"),
            ],
            "ETH_USDT",
            None,
        );
        let path = root.path().join("book-snapshots.source.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest["instrument_source"]["instruments"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&eth).unwrap());
        manifest["instrument_source"]["symbol_ids"]["ETH_USDT"] = eth_id.to_string().into();
        manifest["instrument_source"]["currencies"] =
            serde_json::to_value(crate::catalog::currencies::definitions(&[
                InstrumentAny::CurrencyPair(
                    nautilus_model::instruments::stubs::currency_pair_btcusdt(),
                ),
                eth,
            ]))
            .unwrap();
        let metadata = file.metadata().unwrap();
        manifest["files"].as_array_mut().unwrap().push(serde_json::json!({"path":relative,"size":metadata.len(),"stream":"books","file_identity":FileIdentity::metadata(&metadata).unwrap().to_string()}));
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let original_manifest = std::fs::read(&path).unwrap();
        let original_eth = std::fs::read(&file).unwrap();
        for id in [btc, eth_id] {
            let output = root.path().join(id.symbol.to_string());
            let report =
                export_catalog_selected(root.path(), &output, START, START + 2 * HOUR, &[id])
                    .unwrap();
            assert_eq!(
                report["instrument_ids"],
                serde_json::json!([id.to_string()])
            );
            let mut native =
                ParquetDataCatalog::new(&output.join("catalog"), None, None, None, None);
            let definitions = native.query_instruments(None).unwrap();
            assert_eq!(definitions.len(), 1);
            assert_eq!(definitions[0].id(), id);
            let actual = native
                .query::<nautilus_model::data::OrderBookDelta>(None, None, None, None, None, false)
                .unwrap();
            let mut original = RawBooks::open(root.path(), id, START, START + 2 * HOUR).unwrap();
            let mut expected = Vec::new();
            while let Some(frame) = original.next().unwrap() {
                expected.extend(frame.deltas.deltas);
            }
            assert_eq!(actual, expected);
        }
        let unknown = root.path().join("unknown");
        assert!(export_catalog_selected(
            root.path(),
            &unknown,
            START,
            START + HOUR,
            &["UNKNOWN.SIM".parse().unwrap()]
        )
        .unwrap_err()
        .to_string()
        .contains("INSTRUMENT_UNKNOWN"));
        assert!(!unknown.exists());
        let empty = root.path().join("empty-selected");
        assert!(export_catalog_selected(
            root.path(),
            &empty,
            START + HOUR,
            START + 2 * HOUR,
            &[btc, eth_id]
        )
        .is_err());
        assert!(!empty.exists());
        assert_eq!(std::fs::read(&path).unwrap(), original_manifest);
        assert_eq!(std::fs::read(&file).unwrap(), original_eth);
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
