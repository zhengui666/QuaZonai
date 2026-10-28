//! Operator-only, declaratively mapped history. Scientific jobs remain offline.
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Parser, Subcommand};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::Data,
    identifiers::{InstrumentId, Symbol},
    instruments::{Instrument, InstrumentAny},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

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
        /// Select declared source directories without requiring unrelated streams or paths.
        #[arg(long = "path-prefix")]
        path_prefixes: Vec<String>,
        /// Verify and reuse an already completed immutable unit; interrupted outputs need a new directory.
        #[arg(long)]
        resume: bool,
        #[arg(long, default_value_t = 1_000_000)]
        max_trade_rows: u64,
        #[arg(long, default_value_t = 10_u64 * 1024 * 1024 * 1024)]
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
    start_date: NaiveDate,
    end_date: NaiveDate,
    profile: Profile,
    files: Vec<SourceFile>,
    missing: Vec<serde_json::Value>,
    #[serde(default)]
    selected_path_prefixes: Vec<String>,
}

#[derive(Default, Serialize)]
struct SourceCoverage {
    source_rows: u64,
    selected_rows: u64,
    first_event_ns: Option<u64>,
    last_event_ns: Option<u64>,
}

#[derive(Default, Serialize)]
struct Report {
    schema_version: u32,
    status: String,
    source_reference: String,
    revision: String,
    source_revision: String,
    selected_streams: Vec<String>,
    selected_path_prefixes: Vec<String>,
    modality: String,
    availability: String,
    source_manifest: serde_json::Value,
    instrument_source: serde_json::Value,
    native_files: Vec<NativeFile>,
    source_observed_at: Option<DateTime<Utc>>,
    license: String,
    attribution: String,
    event_start_ns: u64,
    event_end_ns: u64,
    source_rows: u64,
    outside_window_rows: u64,
    filtered_rows: u64,
    selected_rows: u64,
    duplicate_trades: u64,
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
    registered_in_quazonai: bool,
}

#[derive(Serialize)]
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
    Ok(serde_json::from_reader(fs::File::open(path)?)?)
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
    let root = manifest_path
        .parent()
        .context("SOURCE_ROOT")?
        .canonicalize()?;
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
    path_prefixes: &'a [String],
    resume: bool,
    max_trade_rows: u64,
    max_output_bytes: u64,
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

fn import(
    manifest_path: &Path,
    instruments_path: &Path,
    output: &Path,
    options: ImportOptions<'_>,
) -> Result<Report> {
    let (bar_seconds, bar_milliseconds) = options.bar_interval;
    let manifest: Manifest = json(manifest_path)?;
    ensure!(
        manifest.schema_version == 1 && !manifest.license.is_empty(),
        "SOURCE_MANIFEST_INVALID"
    );
    let prefixes = selection_prefixes(&manifest, options.path_prefixes)?;
    ensure!(
        !options.trade_ticks
            || (manifest.revision.len() == 40
                && manifest.revision.bytes().all(|c| c.is_ascii_hexdigit())),
        "SOURCE_REVISION_REQUIRED"
    );
    ensure!(
        !options.resume || options.trade_ticks,
        "RESUME_REQUIRES_EXPLICIT_TRADE_TICKS"
    );
    ensure!(
        !options.trade_ticks
            || (bar_seconds.is_none()
                && bar_milliseconds.is_none()
                && options.max_trade_rows > 0
                && options.max_trade_rows <= 1_000_000
                && options.max_output_bytes > 0
                && options.max_output_bytes <= 10 * 1024 * 1024 * 1024),
        "TRADE_INPUT_RESOURCE_OR_MODALITY_INVALID"
    );
    let selected_files = manifest
        .files
        .iter()
        .filter(|f| f.stream.is_some() && selected_path(&f.path, &prefixes))
        .collect::<Vec<_>>();
    ensure!(!selected_files.is_empty(), "SOURCE_SELECTION_EMPTY");
    if options.trade_ticks {
        for file in &selected_files {
            ensure!(
                manifest
                    .stream(file.stream.as_deref().context("SOURCE_STREAM_MISSING")?)?
                    .kind
                    == "trades",
                "TRADE_TICKS_REQUIRE_TRADE_ONLY_SELECTION"
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
    let root = manifest_path
        .parent()
        .context("SOURCE_ROOT")?
        .canonicalize()?;
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
    let mut sink = writer::Writer::configured(
        output,
        &instruments,
        previous_report.is_some(),
        options.trade_ticks.then_some(options.max_output_bytes),
    )?;
    let mut aggregation = records::Aggregation::with_milliseconds(bar_seconds, bar_milliseconds)?;
    let mut report = Report {
        schema_version: 1,
        source_reference: format!("https://huggingface.co/datasets/{}/tree/{}", manifest.repository, manifest.revision),
        revision: manifest.revision.clone(), source_revision: manifest.revision.clone(),
        selected_streams: selected_files.iter().filter_map(|f| f.stream.clone()).collect::<BTreeSet<_>>().into_iter().collect(),
        selected_path_prefixes: prefixes.clone(),
        modality: if options.trade_ticks { "TRADE_TICK" } else { "NATIVE_SOURCE" }.into(),
        availability: if selected_files.iter().all(|f| manifest.stream(f.stream.as_deref().unwrap()).is_ok_and(|s| !s.columns.contains_key("available_at"))) { "EVENT_TIME_PROXY_UNVERIFIED" } else { "SOURCE_AVAILABILITY_UNVERIFIED" }.into(),
        source_manifest: json(manifest_path)?, instrument_source: json(instruments_path)?,
        source_observed_at: Some(manifest.retrieved_at),
        license: manifest.license.clone(),
        attribution: manifest.attribution.clone(),
        event_start_ns: start,
        event_end_ns: end,
        record_precision: if options.native_precision {
            format!("NATIVE_ENCODING_{}; not a historical contract tick/lot claim", nautilus_model::types::fixed::FIXED_PRECISION)
        } else {
            "INSTRUMENT_DEFINITION; derived BARs use exact maximum observed precision, including equal-availability groups".into()
        },
        missing_partitions: manifest.missing.iter().filter(|m| m.get("pattern").and_then(serde_json::Value::as_str).is_none_or(|p| selected_path(p, &prefixes))).cloned().collect(),
        historical_availability: "UNVERIFIED; event-time proxy unless an original availability column is mapped; native definition times unchanged".into(),
        ..Report::default()
    };
    // shortcut: deduplication retains IDs for this import; split long histories into dated catalogs when ID memory grows.
    let mut trades = BTreeMap::new();
    for file in selected_files {
        let Some(name) = &file.stream else { continue };
        let stream = manifest.stream(name)?;
        let path = source_file(&root, file)?;
        report.source_coverage.entry(name.clone()).or_default();
        records::read_rows(&path, &stream.format, |row| {
            report.source_rows += 1;
            let event = records::timestamp(stream.field(&row, "time")?, &stream.time_unit)?;
            let coverage = report.source_coverage.get_mut(name).unwrap();
            coverage.source_rows += 1;
            coverage.first_event_ns = Some(coverage.first_event_ns.map_or(event, |v| v.min(event)));
            coverage.last_event_ns = Some(coverage.last_event_ns.map_or(event, |v| v.max(event)));
            if event < start || event >= end {
                report.outside_window_rows += 1;
                return Ok(());
            }
            if !stream.matches(&row)? {
                report.filtered_rows += 1;
                return Ok(());
            }
            report.selected_rows += 1;
            ensure!(
                !options.trade_ticks || report.selected_rows <= options.max_trade_rows,
                "TRADE_INPUT_ROW_LIMIT; use smaller immutable import units"
            );
            coverage.selected_rows += 1;
            let symbol = if stream.symbol_from_file_stem {
                file_symbol(&path)?
            } else {
                records::scalar(stream.field(&row, "symbol")?)?
            };
            let Some(id) = symbol_ids.get(&symbol).copied() else {
                *report.missing_symbols.entry(symbol).or_default() += 1;
                return Ok(());
            };
            let available = stream
                .columns
                .get("available_at")
                .map(|_| records::timestamp(stream.field(&row, "available_at")?, &stream.time_unit))
                .transpose()?
                .unwrap_or(event);
            ensure!(available >= event, "SOURCE_EVENT_AFTER_AVAILABILITY");
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
                if instrument.ts_init().as_u64() > available {
                    report.definitions_observed_after_event_rows += 1;
                }
            } else {
                *report.missing_definitions.entry(symbol).or_default() += 1;
            }
            let converted = records::Encoding::new(id, instrument, options.native_precision)
                .and_then(|encoding| {
                    records::convert(stream, &row, &encoding, event, available, &mut report)
                });
            let converted = match converted {
                Ok(data) => data,
                Err(error) => {
                    *report
                        .rejected_rows
                        .entry(format!("{}: {error}", stream.name))
                        .or_default() += 1;
                    return Ok(());
                }
            };
            for data in converted {
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
                        sink.push(Data::Bar(bar))?;
                    }
                }
                sink.push(data)?;
            }
            Ok(())
        })
        .with_context(|| format!("source file {}", file.path))?;
        eprintln!("scanned {}", file.path);
    }
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
    report.status = if report.missing_symbols.is_empty()
        && report.rejected_rows.is_empty()
        && report.missing_partitions.is_empty()
        && (!options.trade_ticks
            || (report.duplicate_trades == 0 && report.missing_definitions.is_empty()))
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

#[cfg(target_os = "linux")]
fn trade_memory_limit() -> Result<()> {
    use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
    let cap = 2 * 1024 * 1024 * 1024_u64;
    let original = getrlimit(Resource::As);
    let maximum = original.maximum.unwrap_or(cap).min(cap);
    let current = original.current.unwrap_or(cap).min(maximum);
    setrlimit(
        Resource::As,
        Rlimit {
            current: Some(current),
            maximum: Some(maximum),
        },
    )
    .context("TRADE_INPUT_MEMORY_LIMIT_UNAVAILABLE")?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn trade_memory_limit() -> Result<()> {
    anyhow::bail!("TRADE_INPUT_MEMORY_LIMIT_UNSUPPORTED_PLATFORM")
}

fn main() {
    let result = match Arguments::parse().command {
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
            path_prefixes,
            resume,
            max_trade_rows,
            max_output_bytes,
            output,
        } => (if trade_ticks {
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
                    path_prefixes: &path_prefixes,
                    resume,
                    max_trade_rows,
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
            path_prefixes: prefixes,
            resume,
            max_trade_rows: rows,
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
            tick_options(&prefixes, false, 10, 10_000_000),
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
    fn tick_import_rejects_duplicate_identity_and_resource_overflow_without_publishing_success() {
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
            assert!(import(
                &manifest,
                &instruments,
                &output,
                tick_options(&prefixes, false, max_rows, max_bytes)
            )
            .is_err());
            assert!(!output.join("import-report.json").exists());
            assert!(
                import(
                    &manifest,
                    &instruments,
                    &output,
                    tick_options(&prefixes, true, 10, 10_000_000)
                )
                .is_err(),
                "interrupted native output must not be reused"
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
