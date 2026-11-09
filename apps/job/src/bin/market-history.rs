//! Operator-only, declaratively mapped history. Scientific jobs remain offline.
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Parser, Subcommand};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Data, TradeTick},
    identifiers::{InstrumentId, Symbol, TradeId},
    instruments::{Instrument, InstrumentAny},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Write},
    path::{Component, Path, PathBuf},
};

#[path = "market/compose.rs"]
mod compose;
#[path = "market/records.rs"]
mod records;
#[path = "market/writer.rs"]
mod writer;

#[derive(Parser)]
#[command(
    version,
    about = "Import mapped public history into native catalogs; does not certify research data"
)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Concatenate complete, disjoint source-day cohort units without changing their marks.
    ComposeCohorts {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        instruments: PathBuf,
        #[arg(long, required = true)]
        units: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        /// Legacy output allowance; accepted and ignored.
        #[arg(long, default_value_t = 0)]
        max_output_bytes: u64,
    },
    /// Obtain current definitions using the official public adapter, without historical backdating.
    HyperliquidInstruments {
        #[arg(long)]
        output: PathBuf,
        /// Resolve archived symbols through official native parsers; does not invent definitions.
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Import all mapped markets in a public acquisition, or an explicit event-time subwindow.
    Import {
        #[arg(long)]
        manifest: PathBuf,
        /// Original native definitions with currency properties, or a legacy InstrumentAny array.
        #[arg(long)]
        instruments: PathBuf,
        #[arg(long)]
        start_seconds: Option<u64>,
        #[arg(long)]
        end_seconds: Option<u64>,
        /// Optionally build observed bars from trades, instead of importing supplied candles.
        #[arg(long)]
        bar_seconds: Option<u32>,
        /// Optionally build observed subsecond bars from trades.
        #[arg(long, conflicts_with = "bar_seconds")]
        bar_milliseconds: Option<u32>,
        /// Preserve archive values at native encoding precision, not current contract precision.
        #[arg(long)]
        native_precision: bool,
        /// Explicit native trade-only preparation; never aggregates or admits other record kinds.
        #[arg(long, conflicts_with_all = ["bar_seconds", "bar_milliseconds"])]
        trade_ticks: bool,
        /// Derived event-time cohort marks for bounded-memory full-history factor research.
        #[arg(long, conflicts_with_all = ["trade_ticks", "bar_seconds", "bar_milliseconds", "resume", "cohort_flow_cache"])]
        cohort_cache: bool,
        /// Derived event-time cohorts with explicit signed aggressive quantity.
        #[arg(long, conflicts_with_all = ["trade_ticks", "cohort_cache", "bar_seconds", "bar_milliseconds", "resume"])]
        cohort_flow_cache: bool,
        /// Select and group original collector receive times instead of exchange event times.
        #[arg(long, requires = "maximum_receive_delay_ns")]
        received_time: bool,
        #[arg(long, requires = "received_time")]
        maximum_receive_delay_ns: Option<u64>,
        /// Select declared source directories without requiring unrelated streams or paths.
        #[arg(long = "path-prefix")]
        path_prefixes: Vec<String>,
        /// Verify and reuse an already completed immutable unit; interrupted outputs need a new directory.
        #[arg(long)]
        resume: bool,
        /// Legacy trade-row allowance; accepted and ignored.
        #[arg(long, default_value_t = 0)]
        max_trade_rows: u64,
        /// Legacy output allowance; accepted and ignored.
        #[arg(long, default_value_t = 0)]
        max_output_bytes: u64,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Clone, Deserialize)]
struct Stream {
    name: String,
    kind: String,
    format: String,
    time_unit: String,
    available_time_unit: Option<String>,
    columns: BTreeMap<String, String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    equals: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    not_equals: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    not_in: BTreeMap<String, Vec<serde_json::Value>>,
    #[serde(default)]
    sides: BTreeMap<String, String>,
    #[serde(default)]
    symbol_from_file_stem: bool,
    bar_seconds: Option<u32>,
    /// Supplied subsecond BAR interval; mutually exclusive with bar_seconds.
    bar_milliseconds: Option<u32>,
    bar_timestamp: Option<String>,
}

#[derive(Deserialize)]
struct Profile {
    streams: Vec<Stream>,
}

#[derive(Deserialize)]
struct SourceFile {
    path: String,
    size: u64,
    stream: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    repository: String,
    revision: String,
    license: String,
    attribution: String,
    retrieved_at: DateTime<Utc>,
    source_root: Option<PathBuf>,
    source_reference: Option<String>,
    coverage: Option<String>,
    start_date: NaiveDate,
    end_date: NaiveDate,
    profile: Profile,
    files: Vec<SourceFile>,
    missing: Vec<serde_json::Value>,
    #[serde(default)]
    selected_path_prefixes: Vec<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct SourceCoverage {
    source_rows: u64,
    selected_rows: u64,
    first_event_ns: Option<u64>,
    last_event_ns: Option<u64>,
}

#[derive(Default, Deserialize, Serialize)]
struct SourceAgeExclusion {
    rows: u64,
    selected_rows: u64,
    /// Half-open physical row ranges in the original source file.
    row_ranges_zero_based: Vec<[u64; 2]>,
}

#[derive(Default, Deserialize, Serialize)]
struct Report {
    schema_version: u32,
    status: String,
    source_reference: String,
    revision: String,
    source_revision: String,
    selected_streams: Vec<String>,
    selected_path_prefixes: Vec<String>,
    modality: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    generated_bar_semantics: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    representation: Option<String>,
    availability: String,
    source_manifest: serde_json::Value,
    instrument_source: serde_json::Value,
    native_files: Vec<NativeFile>,
    source_observed_at: Option<DateTime<Utc>>,
    license: String,
    attribution: String,
    event_start_ns: u64,
    event_end_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_event_start_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_event_end_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maximum_receive_delay_ns: Option<u64>,
    source_rows: u64,
    outside_window_rows: u64,
    filtered_rows: u64,
    selected_rows: u64,
    duplicate_trades: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_age_population: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_age_excluded_rows: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    source_age_excluded_files: BTreeMap<String, SourceAgeExclusion>,
    missing_symbols: BTreeMap<String, u64>,
    missing_definitions: BTreeMap<String, u64>,
    rejected_rows: BTreeMap<String, u64>,
    source_coverage: BTreeMap<String, SourceCoverage>,
    record_precision: String,
    definitions_observed_after_event_rows: u64,
    empty_books: u64,
    one_sided_books: u64,
    native_records: BTreeMap<String, BTreeMap<String, u64>>,
    readback_records: u64,
    missing_partitions: Vec<serde_json::Value>,
    historical_availability: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deduplication_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deduplicated_trades: Option<u64>,
    registered_in_quazonai: bool,
}

impl Report {
    fn exclude_source_age(&mut self, path: &str, stream: &str, row: u64, selected: bool) {
        *self.source_age_excluded_rows.get_or_insert(0) += 1;
        self.filtered_rows += 1;
        if selected {
            self.selected_rows -= 1;
            self.source_coverage.get_mut(stream).unwrap().selected_rows -= 1;
        }
        let excluded = self
            .source_age_excluded_files
            .entry(path.into())
            .or_default();
        excluded.rows += 1;
        excluded.selected_rows += u64::from(selected);
        if let Some(range) = excluded
            .row_ranges_zero_based
            .last_mut()
            .filter(|range| range[1] == row)
        {
            range[1] = row + 1;
        } else {
            excluded.row_ranges_zero_based.push([row, row + 1]);
        }
    }
}

#[derive(Deserialize, Serialize)]
struct NativeFile {
    path: String,
    size: u64,
}

fn native_files(root: &Path) -> Result<Vec<NativeFile>> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<NativeFile>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(!kind.is_symlink(), "NATIVE_OUTPUT_SYMLINK");
            if kind.is_dir() {
                visit(root, &entry.path(), files)?;
            } else {
                ensure!(kind.is_file(), "NATIVE_OUTPUT_FILE_REQUIRED");
                let path = entry
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .context("NATIVE_OUTPUT_PATH_ENCODING")?
                    .replace('\\', "/");
                if path != "import-provenance.json" {
                    files.push(NativeFile {
                        path,
                        size: entry.metadata()?.len(),
                    });
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = fs::File::open(path)?;
    Ok(serde_json::from_reader(BufReader::new(file))?)
}

fn publish<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    writeln!(file)?;
    Ok(())
}

fn hyperliquid_symbol_id(raw_symbol: &str) -> Result<InstrumentId> {
    use nautilus_hyperliquid::common::{
        consts::HYPERLIQUID_VENUE,
        parse::{format_outcome_nautilus_symbol, parse_outcome_symbol},
    };
    let asset = parse_outcome_symbol(raw_symbol)?;
    Ok(InstrumentId::new(
        Symbol::new(format_outcome_nautilus_symbol(
            asset
                .outcome_index()
                .context("NATIVE_OUTCOME_INDEX_MISSING")?,
            asset
                .outcome_side()
                .context("NATIVE_OUTCOME_SIDE_MISSING")?,
        )),
        *HYPERLIQUID_VENUE,
    ))
}

async fn hyperliquid_instruments(output: &Path, manifest: Option<&Path>) -> Result<()> {
    use nautilus_hyperliquid::{
        common::enums::HyperliquidEnvironment,
        http::{client::HyperliquidHttpClient, parse::create_instrument_from_def},
    };
    let client = HyperliquidHttpClient::new(HyperliquidEnvironment::Mainnet, 60, None)?;
    let definitions = client.request_instrument_defs().await?;
    ensure!(!definitions.is_empty(), "NATIVE_INSTRUMENTS_EMPTY");
    let observed = UnixNanos::from(u64::try_from(
        Utc::now().timestamp_nanos_opt().context("CLOCK_RANGE")?,
    )?);
    let instruments = definitions
        .iter()
        .map(|definition| {
            create_instrument_from_def(definition, observed).with_context(|| {
                format!("NATIVE_INSTRUMENT_UNSUPPORTED: {}", definition.raw_symbol)
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut symbol_ids: BTreeMap<_, _> = instruments
        .iter()
        .map(|i| (i.raw_symbol().to_string(), i.id()))
        .collect();
    if let Some(manifest) = manifest {
        for symbol in source_symbols(manifest)? {
            if let Entry::Vacant(entry) = symbol_ids.entry(symbol) {
                let id = hyperliquid_symbol_id(entry.key())
                    .with_context(|| format!("NATIVE_SYMBOL_UNSUPPORTED: {}", entry.key()))?;
                entry.insert(id);
            }
        }
    }
    publish(
        output,
        &serde_json::json!({"currencies": job::catalog::currencies::definitions(&instruments), "instruments": instruments, "symbol_ids": symbol_ids}),
    )?;
    eprintln!(
        "{} current native definitions; not historical parameter evidence",
        instruments.len()
    );
    Ok(())
}

fn source_file(root: &Path, file: &SourceFile) -> Result<PathBuf> {
    let relative = Path::new(&file.path);
    ensure!(
        relative
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "INVALID_SOURCE_PATH"
    );
    let path = root.join(relative).canonicalize()?;
    ensure!(
        path.starts_with(root) && fs::metadata(&path)?.len() == file.size,
        "SOURCE_FILE_MISMATCH"
    );
    Ok(path)
}

fn file_symbol(path: &Path) -> Result<String> {
    Ok(path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("SOURCE_SYMBOL_ENCODING")?
        .to_owned())
}

impl Manifest {
    fn cryptohftdata(&self) -> bool {
        self.repository == "CryptoHFTData"
            && self.source_reference.as_deref()
                == Some("https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data")
    }

    fn trade_revision(&self) -> bool {
        if self.revision.len() == 40 && self.revision.bytes().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
        self.cryptohftdata()
            && self
                .revision
                .strip_prefix("published-files-")
                .is_some_and(|dates| {
                    dates
                        .get(..10)
                        .zip(dates.get(11..))
                        .is_some_and(|(start, end)| {
                            dates.as_bytes().get(10) == Some(&b'-')
                                && start
                                    .parse::<NaiveDate>()
                                    .ok()
                                    .zip(end.parse::<NaiveDate>().ok())
                                    .is_some_and(|(start, end)| start <= end)
                        })
                })
    }

    fn root(&self, manifest_path: &Path) -> Result<PathBuf> {
        self.source_root
            .as_deref()
            .unwrap_or(manifest_path.parent().context("SOURCE_ROOT")?)
            .canonicalize()
            .context("SOURCE_ROOT")
    }

    fn stream(&self, name: &str) -> Result<&Stream> {
        self.profile
            .streams
            .iter()
            .find(|s| s.name == name)
            .context("UNKNOWN_SOURCE_STREAM")
    }
}

fn source_symbols(manifest_path: &Path) -> Result<BTreeSet<String>> {
    let manifest: Manifest = json(manifest_path)?;
    let root = manifest.root(manifest_path)?;
    let mut symbols = BTreeSet::new();
    for file in &manifest.files {
        let Some(name) = &file.stream else { continue };
        let stream = manifest.stream(name)?;
        let path = source_file(&root, file)?;
        if stream.symbol_from_file_stem {
            symbols.insert(file_symbol(&path)?);
        } else {
            records::read_rows(&path, &stream.format, |row| {
                symbols.insert(records::scalar(stream.field(&row, "symbol")?)?);
                Ok(())
            })?;
        }
    }
    Ok(symbols)
}

fn seconds_ns(value: u64) -> Result<u64> {
    value.checked_mul(1_000_000_000).context("TIMESTAMP_RANGE")
}

fn date_ns(date: NaiveDate) -> Result<u64> {
    seconds_ns(u64::try_from(
        date.and_hms_opt(0, 0, 0)
            .context("DATE_RANGE")?
            .and_utc()
            .timestamp(),
    )?)
}

struct ImportOptions<'a> {
    start: Option<u64>,
    end: Option<u64>,
    bar_interval: (Option<u32>, Option<u32>),
    native_precision: bool,
    trade_ticks: bool,
    cohort_cache: bool,
    cohort_flow_cache: bool,
    received_time: bool,
    maximum_receive_delay_ns: Option<u64>,
    path_prefixes: &'a [String],
    resume: bool,
    _max_trade_rows: u64,
    max_output_bytes: u64,
}

impl ImportOptions<'_> {
    fn is_cache(&self) -> bool {
        self.cohort_cache || self.cohort_flow_cache
    }
}

fn same_execution(a: &TradeTick, b: &TradeTick) -> bool {
    a.price == b.price && a.size == b.size && a.aggressor_side == b.aggressor_side
}

struct PublishedFileTrade {
    execution: TradeTick,
    first_eligible: Option<TradeTick>,
    selected_deliveries: u64,
    selected: bool,
}

#[derive(Default)]
struct PublishedTrades {
    seen: BTreeMap<InstrumentId, BTreeMap<(UnixNanos, TradeId), TradeTick>>,
    latest: BTreeMap<InstrumentId, UnixNanos>,
}

impl PublishedTrades {
    fn check_execution(&self, trade: &TradeTick) -> Result<()> {
        if let Some(previous) = self
            .seen
            .get(&trade.instrument_id)
            .and_then(|seen| seen.get(&(trade.ts_event, trade.trade_id)))
        {
            ensure!(
                same_execution(previous, trade),
                "CONFLICTING_SOURCE_TRADE_ID"
            );
        }
        Ok(())
    }

    fn admit(&mut self, trade: TradeTick, maximum_delay: Option<u64>) -> Result<bool> {
        let seen = self.seen.entry(trade.instrument_id).or_default();
        let key = (trade.ts_event, trade.trade_id);
        if let Some(previous) = seen.get(&key) {
            ensure!(
                same_execution(previous, &trade),
                "CONFLICTING_SOURCE_TRADE_ID"
            );
            ensure!(
                previous.ts_init <= trade.ts_init,
                "SOURCE_RECEIVE_ORDER_INVALID"
            );
            return Ok(false);
        }
        ensure!(
            self.latest
                .get(&trade.instrument_id)
                .is_none_or(|last| *last <= trade.ts_init),
            "SOURCE_RECEIVE_ORDER_INVALID"
        );
        if let Some(delay) = maximum_delay {
            ensure!(
                job::ticks::source_age_within(
                    trade.ts_event.as_u64(),
                    trade.ts_init.as_u64(),
                    delay
                ),
                "SOURCE_RECEIVE_DELAY_EXCEEDED"
            );
            let minimum_event = trade.ts_init.as_u64().saturating_sub(delay);
            while seen
                .first_key_value()
                .is_some_and(|((event, _), _)| event.as_u64() < minimum_event)
            {
                seen.pop_first();
            }
        }
        self.latest.insert(trade.instrument_id, trade.ts_init);
        seen.insert(key, trade);
        Ok(true)
    }
}

fn selected_path(path: &str, prefixes: &[String]) -> bool {
    prefixes.is_empty() || prefixes.iter().any(|prefix| path.starts_with(prefix))
}

fn selection_prefixes(manifest: &Manifest, requested: &[String]) -> Result<Vec<String>> {
    let mut prefixes = if requested.is_empty() {
        manifest.selected_path_prefixes.clone()
    } else {
        requested.to_vec()
    };
    prefixes.sort();
    prefixes.dedup();
    for prefix in &prefixes {
        ensure!(
            prefix.ends_with('/')
                && Path::new(prefix)
                    .components()
                    .all(|p| matches!(p, Component::Normal(_)))
                && !prefix.contains('\\')
                && manifest.profile.streams.iter().any(|s| {
                    s.paths.iter().any(|path| {
                        path.starts_with(prefix)
                            || (prefix.starts_with(path.split(['{', '*']).next().unwrap())
                                && manifest.files.iter().any(|file| {
                                    file.stream.as_deref() == Some(s.name.as_str())
                                        && file.path.starts_with(prefix)
                                }))
                    })
                }),
            "SOURCE_PATH_PREFIX_INVALID"
        );
        ensure!(
            manifest.selected_path_prefixes.is_empty()
                || manifest
                    .selected_path_prefixes
                    .iter()
                    .any(|p| prefix.starts_with(p)),
            "SOURCE_PATH_PREFIX_OUTSIDE_ACQUISITION"
        );
    }
    Ok(prefixes)
}

fn selected_missing(manifest: &Manifest, prefixes: &[String]) -> Vec<serde_json::Value> {
    manifest
        .missing
        .iter()
        .filter_map(|missing| {
            let Some(path) = missing.get("pattern").and_then(serde_json::Value::as_str) else {
                return Some(missing.clone());
            };
            if let Some(hours) = missing
                .get("hours")
                .and_then(serde_json::Value::as_array)
                .filter(|_| path.contains("{hour}"))
            {
                let hours = hours
                    .iter()
                    .filter(|hour| {
                        hour.as_u64().is_some_and(|hour| {
                            selected_path(&path.replace("{hour}", &format!("{hour:02}")), prefixes)
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if hours.is_empty() {
                    return None;
                }
                let mut selected = missing.clone();
                if hours.len() == 1 {
                    let hour = hours[0].as_u64().unwrap();
                    selected.as_object_mut().unwrap().remove("hours");
                    selected["hour"] = serde_json::json!(hour);
                    selected["pattern"] =
                        serde_json::json!(path.replace("{hour}", &format!("{hour:02}")));
                } else {
                    selected["hours"] = serde_json::Value::Array(hours);
                }
                return Some(selected);
            }
            selected_path(path, prefixes).then(|| missing.clone())
        })
        .collect()
}

fn import(
    manifest_path: &Path,
    instruments_path: &Path,
    output: &Path,
    options: ImportOptions<'_>,
) -> Result<Report> {
    let (bar_seconds, bar_milliseconds) = options.bar_interval;
    let manifest: Manifest = json(manifest_path)?;
    let published_source = manifest.cryptohftdata();
    let observed_source =
        published_source && manifest.coverage.as_deref() == Some("OBSERVED_PUBLISHED_FILES");
    let freshness_source = observed_source && options.received_time;
    ensure!(
        manifest.schema_version == 1 && !manifest.license.is_empty(),
        "SOURCE_MANIFEST_INVALID"
    );
    let prefixes = selection_prefixes(&manifest, options.path_prefixes)?;
    ensure!(
        !(options.trade_ticks || options.is_cache()) || manifest.trade_revision(),
        "SOURCE_REVISION_REQUIRED"
    );
    ensure!(
        !options.received_time || (options.trade_ticks || options.is_cache()),
        "RECEIVED_TIME_REQUIRES_TRADE_INPUT"
    );
    ensure!(
        options.received_time == options.maximum_receive_delay_ns.is_some(),
        "RECEIVED_TIME_REQUIRES_MAXIMUM_DELAY"
    );
    ensure!(
        !options.resume || options.trade_ticks,
        "RESUME_REQUIRES_EXPLICIT_TRADE_TICKS"
    );
    ensure!(
        !options.trade_ticks || (bar_seconds.is_none() && bar_milliseconds.is_none()),
        "TRADE_INPUT_RESOURCE_OR_MODALITY_INVALID"
    );
    let mut selected_files = manifest
        .files
        .iter()
        .filter(|f| f.stream.is_some() && selected_path(&f.path, &prefixes))
        .collect::<Vec<_>>();
    ensure!(!selected_files.is_empty(), "SOURCE_SELECTION_EMPTY");
    if published_source {
        selected_files.sort_by(|a, b| a.path.cmp(&b.path));
    }
    if options.trade_ticks || options.is_cache() {
        for file in &selected_files {
            ensure!(
                manifest
                    .stream(file.stream.as_deref().context("SOURCE_STREAM_MISSING")?)?
                    .kind
                    == "trades",
                "TRADE_TICKS_REQUIRE_TRADE_ONLY_SELECTION"
            );
            ensure!(
                !options.received_time
                    || manifest
                        .stream(file.stream.as_deref().unwrap())?
                        .columns
                        .contains_key("available_at"),
                "RECEIVED_TIME_REQUIRES_SOURCE_AVAILABILITY"
            );
        }
    }
    let previous_report: Option<serde_json::Value> = if options.resume && output.try_exists()? {
        let previous: serde_json::Value = json(&output.join("import-report.json"))
            .context("RESUME_REQUIRES_COMPLETED_UNIT_USE_NEW_OUTPUT_FOR_INTERRUPTED_IMPORT")?;
        ensure!(
            previous["status"] == "IMPORTED",
            "RESUME_REQUIRES_COMPLETE_IMPORT"
        );
        Some(previous)
    } else {
        None
    };
    let root = manifest.root(manifest_path)?;
    let start = options
        .start
        .map(seconds_ns)
        .transpose()?
        .unwrap_or(date_ns(manifest.start_date)?);
    let end = options
        .end
        .map(seconds_ns)
        .transpose()?
        .unwrap_or(date_ns(manifest.end_date)?);
    ensure!(
        start < end && start >= date_ns(manifest.start_date)? && end <= date_ns(manifest.end_date)?,
        "HISTORY_WINDOW_INVALID"
    );
    let interval_ns = match (bar_seconds, bar_milliseconds) {
        (Some(seconds), None) if seconds > 0 && seconds <= 86400 => {
            Some(u64::from(seconds) * 1_000_000_000)
        }
        (None, Some(milliseconds)) if milliseconds > 0 && milliseconds <= 86_400_000 => {
            Some(u64::from(milliseconds) * 1_000_000)
        }
        (None, None) => None,
        _ => anyhow::bail!("BAR_INTERVAL_UNSUPPORTED_OR_CONFLICTING"),
    };
    if let Some(interval) = interval_ns {
        ensure!(
            start.is_multiple_of(interval) && end.is_multiple_of(interval),
            "FULL_BAR_INTERVALS_REQUIRED"
        );
        ensure!(
            !manifest
                .files
                .iter()
                .filter(|file| selected_path(&file.path, &prefixes))
                .any(|file| file.stream.as_ref().is_some_and(|name| manifest
                    .profile
                    .streams
                    .iter()
                    .any(|s| s.name == *name && s.kind == "bars"))),
            "SELECT_TRADES_WITHOUT_SUPPLIED_BARS_FOR_AGGREGATION"
        );
    }
    let instruments = job::catalog::currencies::read_instruments(instruments_path)?;
    let mut symbol_ids: BTreeMap<String, InstrumentId> =
        json::<serde_json::Value>(instruments_path)?
            .get("symbol_ids")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?
            .unwrap_or_default();
    let mut by_symbol: BTreeMap<String, Vec<&InstrumentAny>> = BTreeMap::new();
    for instrument in &instruments {
        if let Some(id) = symbol_ids.insert(instrument.raw_symbol().to_string(), instrument.id()) {
            ensure!(id == instrument.id(), "NATIVE_SYMBOL_ID_CONFLICT");
        }
        let versions = by_symbol
            .entry(instrument.raw_symbol().to_string())
            .or_default();
        ensure!(
            versions
                .iter()
                .all(|v| v.id() == instrument.id() && v.ts_init() != instrument.ts_init()),
            "AMBIGUOUS_NATIVE_SYMBOL_OR_VERSION"
        );
        versions.push(instrument);
    }
    ensure!(!symbol_ids.is_empty(), "NATIVE_SYMBOLS_EMPTY");
    for versions in by_symbol.values_mut() {
        versions.sort_by_key(|v| v.ts_init());
    }
    // Publisher aliases select original definitions by their resolved native ID.
    for (symbol, id) in &symbol_ids {
        if !by_symbol.contains_key(symbol) {
            if let Some(versions) = by_symbol.values().find(|versions| versions[0].id() == *id) {
                let versions = versions.clone();
                by_symbol.insert(symbol.clone(), versions);
            }
        }
    }
    ensure!(
        !options.is_cache()
            || (!options.resume && bar_seconds.is_none() && bar_milliseconds.is_none()),
        "COHORT_CACHE_OPTIONS_INVALID"
    );
    let mut cache = options
        .is_cache()
        .then(|| {
            if options.received_time {
                job::ticks::cohort_cache::Builder::new_received(
                    output,
                    domain::ticks::MAXIMUM_COHORT_ROWS,
                    domain::ticks::MAXIMUM_TICK_ROWS as usize,
                    options.max_output_bytes,
                    options.cohort_flow_cache,
                    options.maximum_receive_delay_ns.unwrap(),
                )
            } else {
                let builder = if options.cohort_flow_cache {
                    job::ticks::cohort_cache::Builder::new_flow
                } else {
                    job::ticks::cohort_cache::Builder::new
                };
                builder(
                    output,
                    domain::ticks::MAXIMUM_COHORT_ROWS,
                    domain::ticks::MAXIMUM_TICK_ROWS as usize,
                    options.max_output_bytes,
                )
            }
        })
        .transpose()?;
    let mut sink = if options.is_cache() {
        None
    } else {
        Some(writer::Writer::configured(
            output,
            &instruments,
            previous_report.is_some(),
            None,
        )?)
    };
    let mut aggregation = records::Aggregation::with_milliseconds(bar_seconds, bar_milliseconds)?;
    let mut report = Report {
        schema_version: 1,
        source_reference: manifest.source_reference.clone().unwrap_or_else(|| format!("https://huggingface.co/datasets/{}/tree/{}", manifest.repository, manifest.revision)),
        revision: manifest.revision.clone(), source_revision: manifest.revision.clone(),
        selected_streams: selected_files.iter().filter_map(|f| f.stream.clone()).collect::<BTreeSet<_>>().into_iter().collect(),
        selected_path_prefixes: prefixes.clone(),
        modality: if options.trade_ticks || options.is_cache() { "TRADE_TICK" } else { "NATIVE_SOURCE" }.into(),
        generated_bar_semantics: (bar_seconds.is_some() || bar_milliseconds.is_some())
            .then(|| job::catalog::aggregation::CONVENTION.into()),
        representation: options.is_cache().then(|| match (options.received_time, options.cohort_flow_cache) {
            (true, true) => "RECEIVED_TIME_COHORT_FLOW_CACHE_V1",
            (true, false) => "RECEIVED_TIME_COHORT_CACHE_V1",
            (false, true) => job::ticks::cohort_cache::FLOW_REPRESENTATION,
            (false, false) => job::ticks::cohort_cache::REPRESENTATION,
        }.into()),
        availability: if options.received_time { "RECEIVED_TIME_OBSERVED" } else if selected_files.iter().all(|f| manifest.stream(f.stream.as_deref().unwrap()).is_ok_and(|s| !s.columns.contains_key("available_at"))) { "EVENT_TIME_PROXY_UNVERIFIED" } else { "SOURCE_AVAILABILITY_UNVERIFIED" }.into(),
        source_manifest: json(manifest_path)?, instrument_source: json(instruments_path)?,
        source_observed_at: Some(manifest.retrieved_at),
        license: manifest.license.clone(),
        attribution: manifest.attribution.clone(),
        event_start_ns: start,
        event_end_ns: end,
        maximum_receive_delay_ns: options.maximum_receive_delay_ns,
        source_age_population: freshness_source.then(|| "Original collector-receipt window with positive receiver-minus-venue source age at most maximum_receive_delay_ns; venue-ahead clock skew is retained. EARLIEST_RECEIPT_IDENTICAL_TRADE applies among eligible deliveries. Every processed payload is validated before selection; identity conflicts are checked within each file and the retained eligible ID window, not across all historical excluded deliveries. Excluded original rows and files remain unchanged; this is a freshness selection, not complete exchange history or synchronized latency evidence.".into()),
        source_age_excluded_rows: freshness_source.then_some(0),
        record_precision: if options.native_precision {
            format!("NATIVE_ENCODING_{}; not a historical contract tick/lot claim", nautilus_model::types::fixed::FIXED_PRECISION)
        } else {
            "INSTRUMENT_DEFINITION; derived BARs use exact maximum observed precision, including equal-availability groups".into()
        },
        missing_partitions: selected_missing(&manifest, &prefixes),
        historical_availability: "UNVERIFIED; event-time proxy unless an original availability column is mapped; native definition times unchanged".into(),
        coverage: observed_source.then(|| "OBSERVED_PUBLISHED_FILES".into()),
        deduplication_policy: published_source.then(|| "EARLIEST_RECEIPT_IDENTICAL_TRADE".into()),
        ..Report::default()
    };
    // shortcut: deduplication retains IDs for this import; split long histories into dated catalogs when ID memory grows.
    let mut trades = BTreeMap::new();
    // shortcut: excluded identities use each file and the eligible D-window; all-history identity claims need a separate index.
    let mut published_trades = PublishedTrades::default();
    for file in selected_files {
        let Some(name) = &file.stream else { continue };
        let stream = manifest.stream(name)?;
        let path = source_file(&root, file)?;
        report.source_coverage.entry(name.clone()).or_default();
        let mut snapshots = records::FlatSnapshots::default();
        let mut file_trades: BTreeMap<(InstrumentId, UnixNanos, TradeId), PublishedFileTrade> =
            BTreeMap::new();
        let mut file_row = 0;
        records::read_rows(&path, &stream.format, |row| {
            let ordinal = file_row;
            file_row += 1;
            report.source_rows += 1;
            let event = records::timestamp(stream.field(&row, "time")?, &stream.time_unit)?;
            let coverage = report.source_coverage.get_mut(name).unwrap();
            coverage.source_rows += 1;
            coverage.first_event_ns = Some(coverage.first_event_ns.map_or(event, |v| v.min(event)));
            coverage.last_event_ns = Some(coverage.last_event_ns.map_or(event, |v| v.max(event)));
            let received = options
                .received_time
                .then(|| {
                    records::timestamp(
                        stream.field(&row, "available_at")?,
                        stream
                            .available_time_unit
                            .as_deref()
                            .unwrap_or(&stream.time_unit),
                    )
                })
                .transpose()?;
            let selected_at = received.unwrap_or(event);
            let selected = start <= selected_at && selected_at < end;
            if !selected {
                report.outside_window_rows += 1;
                let lookbehind = published_source
                    && stream.kind == "trades"
                    && options.received_time
                    && selected_at < start
                    && selected_at
                        >= start.saturating_sub(options.maximum_receive_delay_ns.unwrap());
                if !lookbehind {
                    return Ok(());
                }
            }
            if !stream.matches(&row)? {
                report.filtered_rows += 1;
                return Ok(());
            }
            if selected {
                report.selected_rows += 1;
                coverage.selected_rows += 1;
            }
            if options.received_time && selected && !published_source {
                report.source_event_start_ns = Some(
                    report
                        .source_event_start_ns
                        .map_or(event, |first| first.min(event)),
                );
                let event_end = event.checked_add(1).context("TIMESTAMP_RANGE")?;
                report.source_event_end_ns = Some(
                    report
                        .source_event_end_ns
                        .map_or(event_end, |last| last.max(event_end)),
                );
            }
            let symbol = if stream.symbol_from_file_stem {
                file_symbol(&path)?
            } else {
                records::scalar(stream.field(&row, "symbol")?)?
            };
            let Some(id) = symbol_ids.get(&symbol).copied() else {
                if selected {
                    *report.missing_symbols.entry(symbol).or_default() += 1;
                }
                return Ok(());
            };
            let available = if let Some(received) = received {
                received
            } else {
                stream
                    .columns
                    .get("available_at")
                    .map(|_| {
                        records::timestamp(
                            stream.field(&row, "available_at")?,
                            stream
                                .available_time_unit
                                .as_deref()
                                .unwrap_or(&stream.time_unit),
                        )
                    })
                    .transpose()?
                    .unwrap_or(event)
            };
            // A late definition can decode raw records, but is never backdated into research evidence.
            let instrument = by_symbol.get(&symbol).map(|versions| {
                versions
                    .iter()
                    .rev()
                    .find(|v| v.ts_init().as_u64() <= available)
                    .copied()
                    .unwrap_or(versions[0])
            });
            if let Some(instrument) = instrument {
                if selected && instrument.ts_init().as_u64() > available {
                    report.definitions_observed_after_event_rows += 1;
                }
            } else if selected {
                *report.missing_definitions.entry(symbol).or_default() += 1;
            }
            let converted = records::Encoding::new(id, instrument, options.native_precision)
                .and_then(|encoding| {
                    if stream.kind == "book_snapshots" {
                        snapshots.push(stream, row, encoding, event, available, &mut report)
                    } else {
                        records::convert(stream, &row, &encoding, event, available, &mut report)
                    }
                });
            let converted = match converted {
                Ok(data) => data,
                Err(error) => {
                    if selected {
                        *report
                            .rejected_rows
                            .entry(format!("{}: {error}", stream.name))
                            .or_default() += 1;
                    }
                    return Ok(());
                }
            };
            for data in converted {
                if published_source {
                    if let Data::Trade(trade) = data {
                        // Validate every payload before selecting freshness; old deliveries are not duplicates.
                        let eligible = !freshness_source
                            || job::ticks::source_age_within(
                                trade.ts_event.as_u64(),
                                trade.ts_init.as_u64(),
                                options.maximum_receive_delay_ns.unwrap(),
                            );
                        if !eligible {
                            report.exclude_source_age(&file.path, name, ordinal, selected);
                        }
                        let key = (trade.instrument_id, trade.ts_event, trade.trade_id);
                        match file_trades.entry(key) {
                            Entry::Vacant(entry) => {
                                entry.insert(PublishedFileTrade {
                                    execution: trade,
                                    first_eligible: eligible.then_some(trade),
                                    selected_deliveries: u64::from(selected && eligible),
                                    selected,
                                });
                            }
                            Entry::Occupied(mut entry) => {
                                if same_execution(&entry.get().execution, &trade) {
                                    let previous = entry.get_mut();
                                    previous.selected |= selected;
                                    previous.selected_deliveries += u64::from(selected && eligible);
                                    if eligible
                                        && previous
                                            .first_eligible
                                            .is_none_or(|first| trade.ts_init < first.ts_init)
                                    {
                                        previous.first_eligible = Some(trade);
                                    }
                                } else if selected || entry.get().selected {
                                    *report
                                        .rejected_rows
                                        .entry("CONFLICTING_SOURCE_TRADE_ID".into())
                                        .or_default() += 1;
                                }
                            }
                        }
                        continue;
                    }
                }
                if let Some(cache) = &mut cache {
                    let Data::Trade(trade) = data else {
                        anyhow::bail!("COHORT_CACHE_REQUIRES_TRADE_SOURCE");
                    };
                    cache.push(trade)?;
                    continue;
                }
                if let Data::Trade(trade) = &data {
                    let key = (trade.instrument_id, trade.ts_event, trade.trade_id);
                    if let Some(previous) = trades.get(&key) {
                        if previous != trade && options.trade_ticks {
                            *report
                                .rejected_rows
                                .entry("CONFLICTING_SOURCE_TRADE_ID".into())
                                .or_default() += 1;
                            continue;
                        }
                        ensure!(previous == trade, "CONFLICTING_SOURCE_TRADE_ID");
                        report.duplicate_trades += 1;
                        continue;
                    }
                    trades.insert(key, *trade);
                    if let Some(bar) = aggregation.update(trade)? {
                        sink.as_mut().unwrap().push(Data::Bar(bar))?;
                    }
                }
                sink.as_mut().unwrap().push(data)?;
            }
            Ok(())
        })
        .with_context(|| format!("source file {}", file.path))?;
        // Published files are arrival-hour partitions; sort one hour before choosing its earliest deliveries.
        let mut file_trades = file_trades.into_values().collect::<Vec<_>>();
        file_trades.sort_by_key(|delivery| {
            let trade = delivery.first_eligible.unwrap_or(delivery.execution);
            (
                trade.ts_init,
                trade.instrument_id,
                trade.ts_event,
                trade.trade_id,
            )
        });
        for delivery in file_trades {
            let Some(trade) = delivery.first_eligible else {
                if let Err(error) = published_trades.check_execution(&delivery.execution) {
                    if delivery.selected {
                        *report.rejected_rows.entry(error.to_string()).or_default() += 1;
                    }
                }
                continue;
            };
            let selected_deliveries = delivery.selected_deliveries;
            match published_trades.admit(trade, options.maximum_receive_delay_ns) {
                Ok(true) => {}
                Ok(false) => {
                    report.duplicate_trades += selected_deliveries;
                    continue;
                }
                Err(error) => {
                    if selected_deliveries > 0 {
                        *report.rejected_rows.entry(error.to_string()).or_default() += 1;
                    }
                    continue;
                }
            }
            let at = if options.received_time {
                trade.ts_init
            } else {
                trade.ts_event
            }
            .as_u64();
            if at < start || at >= end {
                report.duplicate_trades += selected_deliveries;
                continue;
            }
            report.duplicate_trades += selected_deliveries.saturating_sub(1);
            if options.received_time {
                let event = trade.ts_event.as_u64();
                report.source_event_start_ns = Some(
                    report
                        .source_event_start_ns
                        .map_or(event, |first| first.min(event)),
                );
                let end = event.checked_add(1).context("TIMESTAMP_RANGE")?;
                report.source_event_end_ns =
                    Some(report.source_event_end_ns.map_or(end, |last| last.max(end)));
            }
            if let Some(cache) = &mut cache {
                cache.push(trade)?;
            } else {
                if let Some(bar) = aggregation.update(&trade)? {
                    sink.as_mut().unwrap().push(Data::Bar(bar))?;
                }
                sink.as_mut().unwrap().push(Data::Trade(trade))?;
            }
        }
        match snapshots.finish(stream, &mut report) {
            Ok(data) => {
                for data in data {
                    sink.as_mut()
                        .context("BOOK_SNAPSHOT_SINK_REQUIRED")?
                        .push(data)?;
                }
            }
            Err(error) => {
                *report
                    .rejected_rows
                    .entry(format!("{}: {error}", stream.name))
                    .or_default() += 1;
            }
        }
        eprintln!("scanned {}", file.path);
    }
    if let Some(mut cache) = cache {
        report.deduplicated_trades = published_source.then_some(report.duplicate_trades);
        let manifest = cache.seal(&manifest.revision, start, end)?;
        report.native_files = manifest
            .files
            .iter()
            .map(|file| NativeFile {
                path: file.path.clone(),
                size: file.bytes.get(),
            })
            .collect();
        report.native_records.insert(
            "cohorts".into(),
            manifest
                .files
                .iter()
                .map(|file| (file.instrument_id.clone(), file.cohort_count.get()))
                .collect(),
        );
        report.readback_records = manifest.cohort_count.get();
        report.status = if report.missing_symbols.is_empty()
            && report.rejected_rows.is_empty()
            && (observed_source || report.missing_partitions.is_empty())
            && (published_source || report.duplicate_trades == 0)
            && report.readback_records > 0
            && manifest.trade_count.get() + report.duplicate_trades == report.selected_rows
        {
            "IMPORTED"
        } else {
            "INCOMPLETE"
        }
        .into();
        publish(&output.join("import-report.json"), &report)?;
        ensure!(
            report.status == "IMPORTED",
            "INCOMPLETE_COHORT_CACHE_IMPORT"
        );
        cache.publish(manifest, serde_json::to_value(&report)?)?;
        return Ok(report);
    }
    let mut sink = sink.unwrap();
    for bar in aggregation.finish()? {
        sink.push(Data::Bar(bar))?;
    }
    sink.finish()?;
    report.native_files = native_files(&output.join("catalog"))?;
    report.native_records = std::mem::take(&mut sink.counts);
    report.readback_records = report
        .native_records
        .values()
        .flat_map(|v| v.values())
        .sum();
    report.deduplicated_trades = published_source.then_some(report.duplicate_trades);
    report.status = if report.missing_symbols.is_empty()
        && report.rejected_rows.is_empty()
        && (observed_source || report.missing_partitions.is_empty())
        && (!options.trade_ticks
            || ((published_source || report.duplicate_trades == 0)
                && report.missing_definitions.is_empty()))
        && report.readback_records > 0
    {
        "IMPORTED"
    } else {
        "INCOMPLETE"
    }
    .into();
    if let Some(previous) = previous_report {
        ensure!(
            previous == serde_json::to_value(&report)?,
            "RESUME_SOURCE_SELECTION_OR_RESULT_CHANGED"
        );
        sink.verify_trade_count(report.readback_records)?;
        let embedded: serde_json::Value = json(&output.join("catalog/import-provenance.json"))?;
        ensure!(embedded == previous, "RESUME_PROVENANCE_CHANGED");
    } else {
        if options.trade_ticks {
            sink.check_output_limit_for(&report)?;
        }
        publish(&output.join("catalog/import-provenance.json"), &report)?;
        publish(&output.join("import-report.json"), &report)?;
    }
    Ok(report)
}

fn trade_memory_limit() -> Result<()> {
    // Legacy entrypoint retained; no application address-space quota is installed.
    Ok(())
}

fn main() {
    let result = match Arguments::parse().command {
        Command::ComposeCohorts {
            manifest,
            instruments,
            units,
            output,
            max_output_bytes,
        } => compose::run(&manifest, &instruments, &units, &output, max_output_bytes).and_then(
            |report| {
                println!("{}", serde_json::to_string_pretty(&report)?);
                Ok(())
            },
        ),
        Command::HyperliquidInstruments { output, manifest } => tokio::runtime::Runtime::new()
            .context("METADATA_RUNTIME_UNAVAILABLE")
            .and_then(|runtime| {
                runtime.block_on(hyperliquid_instruments(&output, manifest.as_deref()))
            }),
        Command::Import {
            manifest,
            instruments,
            start_seconds,
            end_seconds,
            bar_seconds,
            bar_milliseconds,
            native_precision,
            trade_ticks,
            cohort_cache,
            cohort_flow_cache,
            received_time,
            maximum_receive_delay_ns,
            path_prefixes,
            resume,
            max_trade_rows,
            max_output_bytes,
            output,
        } => (if trade_ticks || cohort_cache || cohort_flow_cache {
            trade_memory_limit()
        } else {
            Ok(())
        })
        .and_then(|_| {
            import(
                &manifest,
                &instruments,
                &output,
                ImportOptions {
                    start: start_seconds,
                    end: end_seconds,
                    bar_interval: (bar_seconds, bar_milliseconds),
                    native_precision,
                    trade_ticks,
                    cohort_cache,
                    cohort_flow_cache,
                    received_time,
                    maximum_receive_delay_ns,
                    path_prefixes: &path_prefixes,
                    resume,
                    _max_trade_rows: max_trade_rows,
                    max_output_bytes,
                },
            )
        })
        .and_then(|report| {
            println!("{}", serde_json::to_string_pretty(&report)?);
            ensure!(
                report.status == "IMPORTED",
                "INCOMPLETE_IMPORT: inspect import-report.json; original sources are unchanged"
            );
            Ok(())
        }),
    };
    if let Err(error) = result {
        eprintln!("market-history: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_options(prefixes: &[String], resume: bool, rows: u64, bytes: u64) -> ImportOptions<'_> {
        ImportOptions {
            start: None,
            end: None,
            bar_interval: (None, None),
            native_precision: true,
            trade_ticks: true,
            cohort_cache: false,
            cohort_flow_cache: false,
            received_time: false,
            maximum_receive_delay_ns: None,
            path_prefixes: prefixes,
            resume,
            _max_trade_rows: rows,
            max_output_bytes: bytes,
        }
    }

    fn tick_source(root: &Path, rows: &str) -> (PathBuf, PathBuf, PathBuf) {
        let source = root.join("source");
        fs::create_dir_all(source.join("perps")).unwrap();
        let csv = source.join("perps/fills.csv");
        fs::write(
            &csv,
            format!("coin,timestamp,price,size,trade_id,side\n{rows}"),
        )
        .unwrap();
        let manifest = source.join("market-data.json");
        fs::write(&manifest, serde_json::to_vec(&serde_json::json!({
            "schema_version":1,"repository":"fixture/declared-source","revision":"0123456789012345678901234567890123456789",
            "license":"mit","attribution":"controlled exact-native test","retrieved_at":"2026-01-03T00:00:00Z",
            "start_date":"2026-01-01","end_date":"2026-01-02",
            "profile":{"streams":[{"name":"trades","kind":"trades","format":"csv","time_unit":"ms",
                "paths":["perps/{date}/fills.csv","spot/{date}/fills.csv","outcomes/{date}/fills.csv"],
                "columns":{"symbol":"coin","time":"timestamp","price":"price","size":"size","trade_id":"trade_id","side":"side"},
                "sides":{"B":"BUYER","S":"SELLER"}}]},
            "files":[{"path":"perps/fills.csv","size":fs::metadata(&csv).unwrap().len(),"stream":"trades"},
                     {"path":"outcomes/not-downloaded.csv","size":999,"stream":"trades"}],
            "missing":[{"stream":"trades","pattern":"outcomes/2026-01-01/fills.csv"}]
        })).unwrap()).unwrap();
        let instruments = root.join("instruments.json");
        fs::write(
            &instruments,
            serde_json::to_vec(&vec![InstrumentAny::CurrencyPair(
                nautilus_model::instruments::stubs::currency_pair_btcusdt(),
            )])
            .unwrap(),
        )
        .unwrap();
        (manifest, instruments, csv)
    }

    #[test]
    fn published_revision_does_not_relax_hub_revision_rules() {
        let temp = tempfile::tempdir().unwrap();
        let (path, _, _) = tick_source(temp.path(), "");
        let mut manifest: Manifest = json(&path).unwrap();
        assert!(manifest.trade_revision());
        manifest.revision = "published-files-2026-01-01-2026-01-02".into();
        assert!(!manifest.trade_revision());
        manifest.repository = "CryptoHFTData".into();
        manifest.source_reference =
            Some("https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data".into());
        assert!(manifest.trade_revision());
        manifest.revision = "published-files-not-a-date".into();
        assert!(!manifest.trade_revision());
    }

    #[test]
    fn grouped_missing_hours_follow_selected_hour_prefixes_without_expanding_partitions() {
        let temp = tempfile::tempdir().unwrap();
        let (path, _, _) = tick_source(temp.path(), "");
        let mut manifest: Manifest = json(&path).unwrap();
        manifest.missing = vec![
            serde_json::json!({"stream":"trades","date":"2026-01-01","symbol":"ETH",
                "pattern":"hyperliquid_futures/2026-01-01/{hour}/ETH_trades.parquet[.zst]","hours":[21,22,23]}),
            serde_json::json!({"stream":"legacy","pattern":"hyperliquid_futures/2026-01-01/23/legacy.parquet"}),
            serde_json::json!({"stream":"legacy","pattern":"hyperliquid_futures/2026-01-01/22/legacy.parquet"}),
        ];
        let selected = selected_missing(&manifest, &["hyperliquid_futures/2026-01-01/23/".into()]);
        assert_eq!(
            selected,
            vec![
                serde_json::json!({"stream":"trades","date":"2026-01-01","symbol":"ETH","hour":23,
                "pattern":"hyperliquid_futures/2026-01-01/23/ETH_trades.parquet[.zst]"}),
                manifest.missing[1].clone(),
            ]
        );
        let selected = selected_missing(
            &manifest,
            &[
                "hyperliquid_futures/2026-01-01/21/".into(),
                "hyperliquid_futures/2026-01-01/23/".into(),
            ],
        );
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0]["hours"], serde_json::json!([21, 23]));
        assert_eq!(selected[0]["pattern"], manifest.missing[0]["pattern"]);
        assert_eq!(selected_missing(&manifest, &[]), manifest.missing);
    }

    #[test]
    fn published_receiver_cache_selects_receive_time_and_keeps_earliest_duplicate() {
        for (earliest, selected, count, first) in [
            (1767225600001000000_u64, 3, 2, 1767225600001000000_u64),
            (1767225599999500000_u64, 2, 1, 1767225600003000000_u64),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let (path, instruments, csv) = tick_source(temp.path(), "");
            fs::write(
                &csv,
                format!(
                    concat!(
                        "coin,timestamp,price,size,trade_id,side,received\n",
                        "BTCUSDT,1767225599999,10.12,0.1,T-1,B,1767225600002000000\n",
                        "BTCUSDT,1767225599999,10.12,0.1,T-1,B,{}\n",
                        "BTCUSDT,1767225600002,10.13,0.1,T-2,B,1767225600003000000\n"
                    ),
                    earliest
                ),
            )
            .unwrap();
            let mut metadata: serde_json::Value = json(&path).unwrap();
            metadata["repository"] = serde_json::json!("CryptoHFTData");
            metadata["source_reference"] = serde_json::json!(
                "https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data"
            );
            metadata["revision"] = serde_json::json!("published-files-2026-01-01-2026-01-01");
            metadata["coverage"] = serde_json::json!("OBSERVED_PUBLISHED_FILES");
            metadata["missing"] =
                serde_json::json!([{"stream":"trades", "pattern":"perps/unprovided.csv"}]);
            metadata["profile"]["streams"][0]["available_time_unit"] = serde_json::json!("ns");
            metadata["profile"]["streams"][0]["columns"]["available_at"] =
                serde_json::json!("received");
            metadata["files"][0]["size"] = serde_json::json!(fs::metadata(&csv).unwrap().len());
            fs::write(&path, serde_json::to_vec(&metadata).unwrap()).unwrap();
            let prefixes = ["perps/".into()];
            let mut options = tick_options(&prefixes, false, 10, 10_000_000);
            options.trade_ticks = false;
            options.cohort_cache = true;
            options.received_time = true;
            options.maximum_receive_delay_ns = Some(3_000_000);
            let output = temp.path().join("received");
            let report = import(&path, &instruments, &output, options).unwrap();
            assert_eq!(report.status, "IMPORTED");
            assert_eq!(report.availability, "RECEIVED_TIME_OBSERVED");
            assert_eq!(report.selected_rows, selected);
            assert_eq!(report.duplicate_trades, 1);
            assert_eq!(report.deduplicated_trades, Some(1));
            assert_eq!(
                report.deduplication_policy.as_deref(),
                Some("EARLIEST_RECEIPT_IDENTICAL_TRADE")
            );
            assert_eq!(report.coverage.as_deref(), Some("OBSERVED_PUBLISHED_FILES"));
            assert_eq!(report.missing_partitions.len(), 1);
            let cached = job::ticks::cohort_cache::manifest(&output.join("catalog")).unwrap();
            assert_eq!(cached.trade_count.get(), count);
            assert_eq!(cached.files[0].first_event_ns.unwrap().get(), first);
        }
    }

    fn received_source(root: &Path, rows: &str, observed: bool) -> (PathBuf, PathBuf) {
        let (path, instruments, csv) = tick_source(root, "");
        fs::write(
            &csv,
            format!("coin,timestamp,price,size,trade_id,side,received\n{rows}"),
        )
        .unwrap();
        let mut metadata: serde_json::Value = json(&path).unwrap();
        metadata["repository"] = serde_json::json!("CryptoHFTData");
        metadata["source_reference"] =
            serde_json::json!("https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data");
        metadata["revision"] = serde_json::json!("published-files-2026-01-01-2026-01-01");
        if observed {
            metadata["coverage"] = serde_json::json!("OBSERVED_PUBLISHED_FILES");
        }
        metadata["missing"] = serde_json::json!([]);
        metadata["profile"]["streams"][0]["available_time_unit"] = serde_json::json!("ns");
        metadata["profile"]["streams"][0]["columns"]["available_at"] =
            serde_json::json!("received");
        metadata["files"][0]["size"] = serde_json::json!(fs::metadata(&csv).unwrap().len());
        fs::write(&path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        (path, instruments)
    }

    #[test]
    fn observed_received_freshness_keeps_venue_ahead_and_earliest_eligible_deliveries() {
        let temp = tempfile::tempdir().unwrap();
        let (path, instruments) = received_source(
            temp.path(),
            concat!(
                "BTCUSDT,1767225599998,10.12,0.1,T-1,B,1767225600005000000\n",
                "BTCUSDT,1767225599998,10.12,0.1,T-1,B,1767225600001000000\n",
                "BTCUSDT,1767225599990,10.11,0.1,T-old,B,1767225600002000000\n",
                "BTCUSDT,1767225601000,10.13,0.1,T-ahead,B,1767225600003000000\n",
                "BTCUSDT,1767225599998,10.12,0.1,T-1,B,1767225600001500000\n",
                "BTCUSDT,1767225599990,10.11,0.1,T-older,B,1767225600004500000\n",
            ),
            true,
        );
        let prefixes = ["perps/".into()];
        let mut options = tick_options(&prefixes, false, 0, 10_000_000);
        options.trade_ticks = false;
        options.cohort_cache = true;
        options.received_time = true;
        options.maximum_receive_delay_ns = Some(3_000_000);
        options.end = Some(1767225601);
        let output = temp.path().join("received");
        let report = import(&path, &instruments, &output, options).unwrap();
        assert_eq!(report.status, "IMPORTED");
        assert_eq!(report.source_rows, 6);
        assert_eq!(report.selected_rows, 2);
        assert_eq!(report.filtered_rows, 4);
        assert_eq!(report.duplicate_trades, 0);
        assert_eq!(report.source_age_excluded_rows, Some(4));
        let excluded = &report.source_age_excluded_files["perps/fills.csv"];
        assert_eq!((excluded.rows, excluded.selected_rows), (4, 4));
        assert_eq!(excluded.row_ranges_zero_based, [[0, 1], [2, 3], [4, 6]]);
        assert!(report
            .source_age_population
            .unwrap()
            .contains("among eligible deliveries"));
        assert_eq!(report.source_event_start_ns, Some(1767225599998000000));
        assert_eq!(report.source_event_end_ns, Some(1767225601000000001));
        assert!(report.source_event_end_ns.unwrap() > report.event_end_ns);
        let cached = job::ticks::cohort_cache::manifest(&output.join("catalog")).unwrap();
        assert_eq!(cached.trade_count.get(), 2);
        assert_eq!(
            cached.files[0].first_event_ns.unwrap().get(),
            1767225600001000000
        );
        assert_eq!(
            cached.files[0].last_event_ns.unwrap().get(),
            1767225600003000000
        );
        assert_eq!(
            cached.source_event_end_ns.unwrap().get(),
            1767225601000000001
        );
    }

    #[test]
    fn freshness_selection_still_rejects_bad_payload_conflicts_and_other_coverage() {
        for (row, observed, reason) in [
            (
                "BTCUSDT,1767225599990,10.13,0.1,T-1,B,1767225600005000000\n",
                true,
                "CONFLICTING_SOURCE_TRADE_ID",
            ),
            (
                "BTCUSDT,1767225599990,invalid,0.1,T-bad,B,1767225600005000000\n",
                true,
                "trades:",
            ),
            (
                "BTCUSDT,1767225599990,10.12,0.1,T-old,B,1767225600005000000\n",
                false,
                "SOURCE_RECEIVE_DELAY_EXCEEDED",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let rows = format!("BTCUSDT,1767225599990,10.12,0.1,T-1,B,1767225600001000000\n{row}");
            let (path, instruments) = received_source(temp.path(), &rows, observed);
            let prefixes = ["perps/".into()];
            let mut options = tick_options(&prefixes, false, 0, 10_000_000);
            options.trade_ticks = false;
            options.cohort_cache = true;
            options.received_time = true;
            options.maximum_receive_delay_ns = Some(12_000_000);
            let output = temp.path().join("received");
            assert_eq!(
                import(&path, &instruments, &output, options)
                    .err()
                    .unwrap()
                    .to_string(),
                "INCOMPLETE_COHORT_CACHE_IMPORT"
            );
            let report: Report = json(&output.join("import-report.json")).unwrap();
            assert!(report
                .rejected_rows
                .keys()
                .any(|key| key.starts_with(reason)));
            assert_eq!(report.source_age_excluded_rows.is_some(), observed);
        }
    }

    #[test]
    fn published_trade_identity_rejects_different_execution_and_bounds_seen_rows() {
        let mut seen = PublishedTrades::default();
        let trade = TradeTick::new_checked(
            "BTCUSDT.BINANCE".parse().unwrap(),
            "10.12".parse().unwrap(),
            "0.1".parse().unwrap(),
            nautilus_model::enums::AggressorSide::Buy,
            TradeId::new_checked("T-1").unwrap(),
            1_000_000_000.into(),
            1_001_000_000.into(),
        )
        .unwrap();
        assert!(seen.admit(trade, Some(2_000_000)).unwrap());
        let mut replay = trade;
        replay.ts_init = 1_002_000_000.into();
        assert!(!seen.admit(replay, Some(2_000_000)).unwrap());
        replay.price = "10.13".parse().unwrap();
        assert_eq!(
            seen.admit(replay, Some(2_000_000)).unwrap_err().to_string(),
            "CONFLICTING_SOURCE_TRADE_ID"
        );
        let mut later = trade;
        later.ts_event = 2_000_000_000.into();
        later.ts_init = 2_001_000_000.into();
        assert!(seen.admit(later, Some(2_000_000)).unwrap());
        assert_eq!(seen.seen[&trade.instrument_id].len(), 1);
    }

    #[test]
    fn flat_book_import_uses_source_root_alias_definitions_and_original_receipt_units() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("archive");
        let selection = temp.path().join("selection");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&selection).unwrap();
        let csv = source.join("book.csv");
        fs::write(&csv, "symbol,time,received,event_type,side,price,quantity\nALIAS,1767225600001,1767225600001000123,snapshot,bid,10.12,2\nALIAS,1767225600001,1767225600001000123,snapshot,ask,10.13,3\nALIAS,1767225600002,1767225600002000456,snapshot,noop,0,0\n").unwrap();
        let manifest = selection.join("market-data.json");
        fs::write(&manifest, serde_json::to_vec(&serde_json::json!({
            "schema_version":1,"repository":"CryptoHFTData","revision":"published selection",
            "source_root":source,"source_reference":"https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data",
            "license":"UNSPECIFIED_BY_LOCAL_ARCHIVE","attribution":"CryptoHFTData","retrieved_at":"2026-01-03T00:00:00Z",
            "start_date":"2026-01-01","end_date":"2026-01-02",
            "profile":{"streams":[{"name":"books","kind":"book_snapshots","format":"csv","time_unit":"ms","available_time_unit":"ns",
                "columns":{"symbol":"symbol","time":"time","available_at":"received","event_type":"event_type","side":"side","price":"price","size":"quantity"}}]},
            "files":[{"path":"book.csv","size":fs::metadata(&csv).unwrap().len(),"stream":"books"}],"missing":[]
        })).unwrap()).unwrap();
        let instrument = InstrumentAny::CurrencyPair(
            nautilus_model::instruments::stubs::currency_pair_btcusdt(),
        );
        let instruments = temp.path().join("instruments.json");
        fs::write(&instruments, serde_json::to_vec(&serde_json::json!({
            "currencies":job::catalog::currencies::definitions(std::slice::from_ref(&instrument)),
            "instruments":[instrument],"symbol_ids":{"ALIAS":instrument.id()}
        })).unwrap()).unwrap();
        assert_eq!(
            source_symbols(&manifest).unwrap(),
            BTreeSet::from(["ALIAS".into()])
        );
        let mut options = tick_options(&[], false, 0, 0);
        options.trade_ticks = false;
        options.native_precision = false;
        let report = import(
            &manifest,
            &instruments,
            &temp.path().join("native"),
            options,
        )
        .unwrap();
        assert_eq!(report.status, "IMPORTED");
        assert_eq!(
            report.source_reference,
            "https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data"
        );
        assert_eq!(report.source_rows, 3);
        assert_eq!(report.readback_records, 5);
        assert!(report.missing_definitions.is_empty());
        assert_eq!(
            report.native_records["book_deltas"][&instrument.id().to_string()],
            4
        );
        assert_eq!(
            report.native_records["quotes"][&instrument.id().to_string()],
            1
        );
    }

    #[test]
    fn tick_import_preserves_exact_source_and_reuses_only_verified_completed_units() {
        let temp = tempfile::tempdir().unwrap();
        let rows = "BTCUSDT,1767225600001,10.1234567890123456,0.1234567890,T-1,B\nBTCUSDT,1767225600001,11.1234567890123456,0.1234567890,T-2,S\n";
        let (manifest, instruments, csv) = tick_source(temp.path(), rows);
        let original = fs::read(&csv).unwrap();
        let prefixes = vec!["perps/".into(), "spot/".into()];
        let output = temp.path().join("native");
        let report = import(
            &manifest,
            &instruments,
            &output,
            tick_options(&prefixes, false, 0, 0),
        )
        .unwrap();
        assert_eq!(report.status, "IMPORTED");
        assert_eq!(report.readback_records, 2);
        assert_eq!(report.native_records.len(), 1);
        assert!(!report.native_records.contains_key("bars"));
        assert!(report.missing_partitions.is_empty());
        let receipt = fs::read(output.join("import-report.json")).unwrap();
        assert_eq!(
            fs::read(output.join("catalog/import-provenance.json")).unwrap(),
            receipt
        );
        import(
            &manifest,
            &instruments,
            &output,
            tick_options(&prefixes, true, 10, 10_000_000),
        )
        .unwrap();
        assert_eq!(
            fs::read(output.join("import-report.json")).unwrap(),
            receipt
        );
        assert_eq!(fs::read(&csv).unwrap(), original);
        fs::write(
            &csv,
            String::from_utf8(original.clone())
                .unwrap()
                .replace("10.1234567890123456", "20.1234567890123456"),
        )
        .unwrap();
        assert!(import(
            &manifest,
            &instruments,
            &output,
            tick_options(&prefixes, true, 10, 10_000_000)
        )
        .is_err());
        fs::write(&csv, &original).unwrap();
        let native_file = report
            .native_files
            .iter()
            .find(|file| {
                file.path.split('/').any(|part| part == "trades") && file.path.ends_with(".parquet")
            })
            .unwrap();
        fs::remove_file(output.join("catalog").join(&native_file.path)).unwrap();
        assert!(
            import(
                &manifest,
                &instruments,
                &output,
                tick_options(&prefixes, true, 10, 10_000_000)
            )
            .is_err(),
            "deleted native data cannot be accepted as a completed unit"
        );
        assert_eq!(
            fs::read(output.join("import-report.json")).unwrap(),
            receipt
        );
    }

    #[test]
    fn source_cohort_cache_preserves_complete_events_and_original_archive() {
        for flow in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (manifest, instruments, csv) = tick_source(
                temp.path(),
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,B\nBTCUSDT,1767225600001,11.12,0.2234567890,T-2,S\nBTCUSDT,1767225600002,12.12,0.3234567890,T-3,B\n",
            );
            let original = fs::read(&csv).unwrap();
            let prefixes = vec!["perps/".into(), "spot/".into()];
            let output = temp.path().join("cohorts");
            let mut options = tick_options(&prefixes, false, 10, 10_000_000);
            options.trade_ticks = false;
            options.cohort_cache = !flow;
            options.cohort_flow_cache = flow;
            let report = import(&manifest, &instruments, &output, options).unwrap();
            assert_eq!(report.status, "IMPORTED");
            assert_eq!(report.selected_rows, 3);
            assert_eq!(report.readback_records, 2);
            assert_eq!(
                report.representation.as_deref(),
                Some(if flow {
                    job::ticks::cohort_cache::FLOW_REPRESENTATION
                } else {
                    job::ticks::cohort_cache::REPRESENTATION
                })
            );
            let cached = job::ticks::cohort_cache::manifest(&output.join("catalog")).unwrap();
            assert_eq!(cached.trade_count.get(), 3);
            assert_eq!(cached.cohort_count.get(), 2);
            assert_eq!(cached.files[0].bytes.get(), if flow { 80 } else { 64 });
            if flow {
                let bytes = fs::read(output.join("catalog").join(&cached.files[0].path)).unwrap();
                assert_eq!(
                    f64::from_bits(u64::from_le_bytes(bytes[32..40].try_into().unwrap())),
                    -0.1
                );
            }
            assert_eq!(
                cached.source_report["event_start_ns"],
                "1767225600000000000"
            );
            assert!(!output.join("catalog/data/trades").exists());
            assert_eq!(fs::read(csv).unwrap(), original);
        }
    }

    #[test]
    fn source_cohort_cache_keeps_file_boundary_cohorts_and_identity_checks() {
        for (id, price, rejected) in [
            ("T-4", "13.12", None),
            ("T-3", "12.12", Some("TICK_CACHE_SOURCE_ID_DUPLICATE")),
            ("T-3", "13.12", Some("TICK_CACHE_SOURCE_ID_CONFLICT")),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let (manifest, instruments, csv) = tick_source(
                temp.path(),
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,B\nBTCUSDT,1767225600002,12.12,0.3234567890,T-3,B\n",
            );
            let source = manifest.parent().unwrap();
            fs::create_dir(source.join("spot")).unwrap();
            let next = source.join("spot/fills.csv");
            fs::write(&next, format!("coin,timestamp,price,size,trade_id,side\nBTCUSDT,1767225600002,{price},0.3234567890,{id},B\nBTCUSDT,1767225600003,14.12,0.4234567890,T-5,B\n")).unwrap();
            let original = fs::read(&csv).unwrap();
            let mut declared: serde_json::Value = json(&manifest).unwrap();
            declared["files"].as_array_mut().unwrap().push(serde_json::json!({
                "path":"spot/fills.csv","size":fs::metadata(next).unwrap().len(),"stream":"trades"
            }));
            fs::write(&manifest, serde_json::to_vec(&declared).unwrap()).unwrap();
            let prefixes = vec!["perps/".into(), "spot/".into()];
            let mut options = tick_options(&prefixes, false, 10, 10_000_000);
            options.trade_ticks = false;
            options.cohort_cache = true;
            let output = temp.path().join("cohorts");
            let result = import(&manifest, &instruments, &output, options);
            if let Some(reason) = rejected {
                assert!(format!("{:#}", result.err().unwrap()).contains(reason));
                assert!(!output.join("catalog/cohort-cache.json").exists());
            } else {
                let report = result.unwrap();
                assert_eq!(report.selected_rows, 4);
                assert_eq!(report.readback_records, 3);
                assert_eq!(
                    job::ticks::cohort_cache::manifest(&output.join("catalog"))
                        .unwrap()
                        .trade_count
                        .get(),
                    4
                );
            }
            assert_eq!(fs::read(csv).unwrap(), original);
        }
    }

    #[test]
    fn compose_day_cohorts_matches_monolithic_marks_and_rejects_overlap_or_partial_days() {
        for flow in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (manifest, instruments, _) = tick_source(temp.path(), "");
            let source = manifest.parent().unwrap();
            let paths = [
                "perps/date=2026-01-01/fills.csv",
                "perps/date=2026-01-02/fills.csv",
            ];
            let rows = [
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,B\nBTCUSDT,1767225600001,11.12,0.2234567890,T-2,S\nBTCUSDT,1767225602000,12.12,0.3234567890,T-3,B\n",
                "BTCUSDT,1767312000001,13.12,0.4234567890,T-4,S\nBTCUSDT,1767312000001,14.12,0.4234567890,T-5,B\nBTCUSDT,1767312002000,15.12,0.5234567890,T-6,S\n",
            ];
            let mut files = Vec::new();
            for (path, rows) in paths.iter().zip(rows) {
                let file = source.join(path);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(
                    &file,
                    format!("coin,timestamp,price,size,trade_id,side\n{rows}"),
                )
                .unwrap();
                files.push(serde_json::json!({"path":path,"size":fs::metadata(file).unwrap().len(),"stream":"trades"}));
            }
            let mut metadata: serde_json::Value = json(&manifest).unwrap();
            metadata["end_date"] = serde_json::json!("2026-01-03");
            metadata["profile"]["streams"][0]["paths"] =
                serde_json::json!(["perps/date={date}/fills.csv"]);
            metadata["files"] = serde_json::json!(files);
            metadata["missing"] = serde_json::json!([]);
            metadata["selected_path_prefixes"] = serde_json::json!(["perps/"]);
            fs::write(&manifest, serde_json::to_vec(&metadata).unwrap()).unwrap();
            let build = |output: &Path, prefixes: &[String], start, end| {
                let mut options = tick_options(prefixes, false, 100, 10_000_000);
                options.trade_ticks = false;
                options.cohort_cache = !flow;
                options.cohort_flow_cache = flow;
                options.start = start;
                options.end = end;
                import(&manifest, &instruments, output, options).unwrap();
            };
            let mono = temp.path().join("monolithic");
            build(&mono, &["perps/".into()], None, None);
            let units = vec![temp.path().join("day1"), temp.path().join("day2")];
            build(
                &units[0],
                &["perps/date=2026-01-01/".into()],
                Some(1767225600),
                Some(1767312000),
            );
            build(
                &units[1],
                &["perps/date=2026-01-02/".into()],
                Some(1767312000),
                Some(1767398400),
            );
            let output = temp.path().join("composed");
            compose::run(&manifest, &instruments, &units, &output, 10_000_000).unwrap();
            let expected = job::ticks::cohort_cache::manifest(&mono.join("catalog")).unwrap();
            let actual = job::ticks::cohort_cache::manifest(&output.join("catalog")).unwrap();
            assert_eq!(actual.representation, expected.representation);
            assert_eq!(actual.trade_count, expected.trade_count);
            assert_eq!(actual.cohort_count, expected.cohort_count);
            assert_eq!(actual.event_start_ns, expected.event_start_ns);
            assert_eq!(actual.event_end_ns, expected.event_end_ns);
            assert_eq!(actual.files.len(), expected.files.len());
            for (a, e) in actual.files.iter().zip(&expected.files) {
                assert_eq!(a.instrument_id, e.instrument_id);
                assert_eq!(
                    fs::read(output.join("catalog").join(&a.path)).unwrap(),
                    fs::read(mono.join("catalog").join(&e.path)).unwrap()
                );
            }
            let start = actual.event_start_ns.get();
            let request = serde_json::from_value(serde_json::json!({"schema_version":1,
                "input":{"modality":"TRADE_TICK","instrument_ids":actual.files.iter().map(|file| &file.instrument_id).collect::<Vec<_>>(),
                    "event_start_ns":actual.event_start_ns,"event_end_ns":actual.event_end_ns,
                    "decision_cutoff_ns":actual.event_end_ns,"source_revision":actual.source_revision,
                    "availability":"EVENT_TIME_PROXY_UNVERIFIED","representation":actual.representation},
                "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
                "target":{"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1},
                "parameters":{"fast_period_cohorts":1,"slow_period_cohorts":2},
                "train_end_ns":(start+1_000_000_000).to_string(),
                "validation_end_ns":(start+86_401_000_000_000).to_string()})).unwrap();
            let prepared = temp.path().join("prepared-composed");
            job::ticks::cohort_cache::prepare_views(
                &output.join("catalog"),
                &request,
                &prepared,
                10_000_000,
            )
            .unwrap();
            let model = wat::parse_str(r#"(module
                (global $value (mut f64) (f64.const 0))
                (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
                    local.get 0 i32.eqz if local.get 1 global.set $value end)
                (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) global.get $value))"#).unwrap();
            assert_eq!(
                serde_json::to_value(
                    job::ticks::evaluate(&mono.join("catalog"), &request, &model, true).unwrap()
                )
                .unwrap(),
                serde_json::to_value(
                    job::ticks::evaluate(
                        &prepared.join("independent/catalog"),
                        &request,
                        &model,
                        true
                    )
                    .unwrap()
                )
                .unwrap()
            );
            let relative = tempfile::Builder::new()
                .prefix(".qz-relative-compose-")
                .tempdir_in(std::env::current_dir().unwrap())
                .unwrap();
            fs::remove_dir(relative.path()).unwrap();
            compose::run(
                &manifest,
                &instruments,
                &units,
                Path::new(relative.path().file_name().unwrap()),
                10_000_000,
            )
            .unwrap();
            assert!(relative.path().join("catalog/cohort-cache.json").is_file());
            assert!(compose::run(
                &manifest,
                &instruments,
                &[units[0].clone(), units[0].clone(), units[1].clone()],
                &temp.path().join("overlap"),
                10_000_000
            )
            .is_err());
            let partial = temp.path().join("partial");
            build(
                &partial,
                &["perps/date=2026-01-01/".into()],
                Some(1767225601),
                Some(1767312000),
            );
            assert!(compose::run(
                &manifest,
                &instruments,
                &[partial, units[1].clone()],
                &temp.path().join("partial-compose"),
                10_000_000
            )
            .is_err());
            metadata["profile"]["streams"][0]["paths"] =
                serde_json::json!(["perps/date={date}/fills.csv", "spot/date={date}/fills.csv"]);
            metadata["selected_path_prefixes"] = serde_json::json!(["perps/", "spot/"]);
            metadata["missing"] = serde_json::json!([{"pattern":"spot/date=2026-01-01/fills.csv"}]);
            fs::write(&manifest, serde_json::to_vec(&metadata).unwrap()).unwrap();
            let missing_units = vec![
                temp.path().join("missing-day1"),
                temp.path().join("missing-day2"),
            ];
            build(
                &missing_units[0],
                &["perps/date=2026-01-01/".into()],
                Some(1767225600),
                Some(1767312000),
            );
            build(
                &missing_units[1],
                &["perps/date=2026-01-02/".into()],
                Some(1767312000),
                Some(1767398400),
            );
            for unit in &missing_units {
                assert!(json::<Report>(&unit.join("import-report.json"))
                    .unwrap()
                    .missing_partitions
                    .is_empty());
            }
            assert!(compose::run(
                &manifest,
                &instruments,
                &missing_units,
                &temp.path().join("missing-compose"),
                10_000_000
            )
            .err()
            .unwrap()
            .to_string()
            .contains("COHORT_COMPOSE_MISSING_SOURCE_PARTITIONS"));
        }
    }

    #[test]
    fn tick_import_selects_existing_dated_files_before_scanning_rows() {
        let temp = tempfile::tempdir().unwrap();
        let (manifest_path, instruments, csv) = tick_source(
            temp.path(),
            "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,B\n",
        );
        let source = manifest_path.parent().unwrap();
        let first = "perps/date=2026-01-01/fills.csv";
        let second = "perps/date=2026-01-02/fills.csv";
        for path in [first, second] {
            fs::create_dir_all(source.join(path).parent().unwrap()).unwrap();
            fs::copy(&csv, source.join(path)).unwrap();
        }
        fs::write(
            source.join(second),
            fs::read_to_string(&csv)
                .unwrap()
                .replace("1767225600001", "1767312000001"),
        )
        .unwrap();
        let mut metadata: serde_json::Value = json(&manifest_path).unwrap();
        metadata["end_date"] = serde_json::json!("2026-01-03");
        metadata["profile"]["streams"][0]["paths"] =
            serde_json::json!(["perps/date={date}/fills.csv"]);
        metadata["files"] = serde_json::json!([
            {"path":first,"size":fs::metadata(&csv).unwrap().len(),"stream":"trades"},
            {"path":second,"size":fs::metadata(&csv).unwrap().len(),"stream":"trades"}
        ]);
        metadata["missing"] = serde_json::json!([]);
        metadata["selected_path_prefixes"] = serde_json::json!(["perps/"]);
        fs::write(&manifest_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        let parsed: Manifest = json(&manifest_path).unwrap();
        let prefixes = vec!["perps/date=2026-01-01/".into()];
        let mut options = tick_options(&prefixes, false, 10, 10_000_000);
        options.end = Some(1767312000);
        let report = import(
            &manifest_path,
            &instruments,
            &temp.path().join("native"),
            options,
        )
        .unwrap();
        assert_eq!(report.status, "IMPORTED");
        assert_eq!(report.source_rows, 1);
        assert_eq!(report.readback_records, 1);
        assert_eq!(report.selected_path_prefixes, prefixes);
        assert!(selection_prefixes(&parsed, &["perps/".into()]).is_ok());
        for prefix in [
            "perps/date=2026-01-03/",
            "other/date=2026-01-01/",
            "../perps/",
            "perps/date=2026-01-01",
        ] {
            assert!(selection_prefixes(&parsed, &[prefix.into()]).is_err());
        }
        let mut restricted = parsed;
        restricted.selected_path_prefixes = vec!["perps/date=2026-01-02/".into()];
        assert_eq!(
            selection_prefixes(&restricted, &prefixes)
                .unwrap_err()
                .to_string(),
            "SOURCE_PATH_PREFIX_OUTSIDE_ACQUISITION"
        );
    }

    #[test]
    fn tick_import_applies_declared_exclusions_and_preserves_their_provenance() {
        let temp = tempfile::tempdir().unwrap();
        let (manifest, instruments, csv) = tick_source(temp.path(), "");
        fs::write(
            &csv,
            concat!(
                "coin,timestamp,price,size,trade_id,side,crossed,direction,liquidation_method\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,S,true,Close Long,market\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-2,B,true,Close Short,market\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-3,B,true,Buy,\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-4,S,true,Auto-Deleveraging,backstop\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-5,S,true,Liquidated Cross Long,backstop\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-6,B,true,Net Child Vaults,\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-7,B,true,Settlement,\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-8,S,true,Backstop Borrow Liquidation,\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-9,S,true,Partial Borrow Liquidation,\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,0,S,true,Spot Dust Conversion,\n",
                "BTCUSDT,1767225600001,11.12,0.1234567890,0,S,true,Spot Dust Conversion,\n",
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-10,B,false,Buy,\n"
            ),
        )
        .unwrap();
        let original = fs::read(&csv).unwrap();
        let mut metadata: serde_json::Value = json(&manifest).unwrap();
        metadata["profile"]["streams"][0]["equals"] = serde_json::json!({"crossed":true});
        metadata["profile"]["streams"][0]["not_equals"] =
            serde_json::json!({"direction":"Spot Dust Conversion","liquidation_method":"backstop"});
        metadata["profile"]["streams"][0]["not_in"] = serde_json::json!({"direction":[
            "Net Child Vaults", "Settlement", "Backstop Borrow Liquidation", "Partial Borrow Liquidation"
        ]});
        metadata["files"][0]["size"] = serde_json::json!(original.len());
        fs::write(&manifest, serde_json::to_vec(&metadata).unwrap()).unwrap();
        let prefixes = vec!["perps/".into()];
        let report = import(
            &manifest,
            &instruments,
            &temp.path().join("native"),
            tick_options(&prefixes, false, 10, 10_000_000),
        )
        .unwrap();
        assert_eq!(report.status, "IMPORTED");
        assert_eq!(report.source_rows, 12);
        assert_eq!(report.filtered_rows, 9);
        assert_eq!(report.selected_rows, 3);
        assert_eq!(report.readback_records, 3);
        assert_eq!(report.duplicate_trades, 0);
        assert_eq!(report.source_manifest, metadata);
        assert_eq!(fs::read(csv).unwrap(), original);
    }

    #[test]
    fn tick_import_scopes_trade_identity_to_its_event_time() {
        for (second_time, status, conflicts) in [
            (1767225600002_u64, "IMPORTED", 0),
            (1767225600001_u64, "INCOMPLETE", 1),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let rows = format!(
                "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,B\nBTCUSDT,{second_time},11.12,0.1234567890,T-1,B\n"
            );
            let (manifest, instruments, _) = tick_source(temp.path(), &rows);
            let prefixes = vec!["perps/".into(), "spot/".into()];
            let report = import(
                &manifest,
                &instruments,
                &temp.path().join("native"),
                tick_options(&prefixes, false, 10, 10_000_000),
            )
            .unwrap();
            assert_eq!(report.status, status);
            assert_eq!(report.readback_records, 2 - conflicts);
            assert_eq!(report.duplicate_trades, 0);
            assert_eq!(
                report
                    .rejected_rows
                    .get("CONFLICTING_SOURCE_TRADE_ID")
                    .copied()
                    .unwrap_or(0),
                conflicts
            );
        }
    }

    #[test]
    fn tick_import_preserves_duplicate_identity_failure_and_ignores_legacy_quotas() {
        let temp = tempfile::tempdir().unwrap();
        let row = "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,B\n";
        let (manifest, instruments, csv) = tick_source(temp.path(), &row.repeat(2));
        let original = fs::read(&csv).unwrap();
        let prefixes = vec!["perps/".into(), "spot/".into()];
        let output = temp.path().join("duplicates");
        let report = import(
            &manifest,
            &instruments,
            &output,
            tick_options(&prefixes, false, 10, 10_000_000),
        )
        .unwrap();
        assert_eq!(report.status, "INCOMPLETE");
        assert_eq!(report.duplicate_trades, 1);
        assert!(import(
            &manifest,
            &instruments,
            &output,
            tick_options(&prefixes, true, 10, 10_000_000)
        )
        .is_err());
        for (name, max_rows, max_bytes) in [("row-limit", 1, 10_000_000), ("byte-limit", 10, 1)] {
            let output = temp.path().join(name);
            let report = import(
                &manifest,
                &instruments,
                &output,
                tick_options(&prefixes, false, max_rows, max_bytes),
            )
            .unwrap();
            assert_eq!(
                report.status, "INCOMPLETE",
                "the duplicate source remains explicit; legacy quotas are ignored"
            );
            assert!(output.join("import-report.json").exists());
            assert!(
                import(
                    &manifest,
                    &instruments,
                    &output,
                    tick_options(&prefixes, true, 10, 10_000_000)
                )
                .is_err(),
                "incomplete native source evidence must not be reused"
            );
        }
        assert_eq!(fs::read(csv).unwrap(), original);
    }

    #[test]
    fn missing_native_definitions_reject_tick_preparation_without_changing_raw_import() {
        let temp = tempfile::tempdir().unwrap();
        let (manifest, instruments, _) = tick_source(
            temp.path(),
            "BTCUSDT,1767225600001,10.12,0.1234567890,T-1,B\n",
        );
        fs::write(&instruments, serde_json::to_vec(&serde_json::json!({"currencies":[],"instruments":[],"symbol_ids":{"BTCUSDT":"BTCUSDT.BINANCE"}})).unwrap()).unwrap();
        let prefixes = vec!["perps/".into(), "spot/".into()];
        let report = import(
            &manifest,
            &instruments,
            &temp.path().join("tick"),
            tick_options(&prefixes, false, 10, 10_000_000),
        )
        .unwrap();
        assert_eq!(report.status, "INCOMPLETE");
        assert_eq!(report.missing_definitions["BTCUSDT"], 1);
        let mut raw = tick_options(&prefixes, false, 10, 10_000_000);
        raw.trade_ticks = false;
        let legacy = import(&manifest, &instruments, &temp.path().join("raw"), raw).unwrap();
        assert_eq!(legacy.status, "IMPORTED");
        assert_eq!(legacy.missing_definitions["BTCUSDT"], 1);
    }

    #[test]
    fn archive_bar_cli_accepts_milliseconds_and_rejects_mixed_units() {
        let args = [
            "market-history",
            "import",
            "--manifest",
            "source.json",
            "--instruments",
            "instruments.json",
            "--output",
            "native",
            "--bar-milliseconds",
            "100",
        ];
        let parsed = Arguments::try_parse_from(args).unwrap();
        assert!(matches!(
            parsed.command,
            Command::Import {
                bar_milliseconds: Some(100),
                bar_seconds: None,
                ..
            }
        ));
        assert!(Arguments::try_parse_from(args.into_iter().chain(["--bar-seconds", "1"])).is_err());
        assert!(Arguments::try_parse_from(args.into_iter().chain(["--trade-ticks"])).is_err());
    }

    #[test]
    fn historical_identity_uses_the_official_parser_without_contract_parameters() {
        assert_eq!(
            hyperliquid_symbol_id("#108040").unwrap().to_string(),
            "10804-YES-OUTCOME.HYPERLIQUID"
        );
        assert!(hyperliquid_symbol_id("unknown").is_err());
    }
}
