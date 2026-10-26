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
    equals: BTreeMap<String, serde_json::Value>,
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

fn import(
    manifest_path: &Path,
    instruments_path: &Path,
    start: Option<u64>,
    end: Option<u64>,
    bar_interval: (Option<u32>, Option<u32>),
    native_precision: bool,
    output: &Path,
) -> Result<Report> {
    let (bar_seconds, bar_milliseconds) = bar_interval;
    let manifest: Manifest = json(manifest_path)?;
    ensure!(
        manifest.schema_version == 1 && !manifest.license.is_empty(),
        "SOURCE_MANIFEST_INVALID"
    );
    let root = manifest_path
        .parent()
        .context("SOURCE_ROOT")?
        .canonicalize()?;
    let start = start
        .map(seconds_ns)
        .transpose()?
        .unwrap_or(date_ns(manifest.start_date)?);
    let end = end
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
    let mut sink = writer::Writer::new(output, &instruments)?;
    let mut aggregation = records::Aggregation::with_milliseconds(bar_seconds, bar_milliseconds)?;
    let mut report = Report {
        schema_version: 1,
        source_reference: format!("https://huggingface.co/datasets/{}/tree/{}", manifest.repository, manifest.revision),
        source_observed_at: Some(manifest.retrieved_at),
        license: manifest.license.clone(),
        attribution: manifest.attribution.clone(),
        event_start_ns: start,
        event_end_ns: end,
        record_precision: if native_precision {
            format!("NATIVE_ENCODING_{}; not a historical contract tick/lot claim", nautilus_model::types::fixed::FIXED_PRECISION)
        } else {
            "INSTRUMENT_DEFINITION; derived BARs use exact maximum observed precision, including equal-availability groups".into()
        },
        missing_partitions: manifest.missing.clone(),
        historical_availability: "UNVERIFIED; event-time proxy unless an original availability column is mapped; native definition times unchanged".into(),
        ..Report::default()
    };
    // shortcut: deduplication retains IDs for this import; split long histories into dated catalogs when ID memory grows.
    let mut trades = BTreeMap::new();
    for file in &manifest.files {
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
            let converted =
                records::Encoding::new(id, instrument, native_precision).and_then(|encoding| {
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
                    let key = (trade.instrument_id, trade.trade_id);
                    if let Some(previous) = trades.get(&key) {
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
    report.native_records = sink.counts;
    report.readback_records = report
        .native_records
        .values()
        .flat_map(|v| v.values())
        .sum();
    report.status = if report.missing_symbols.is_empty()
        && report.rejected_rows.is_empty()
        && report.missing_partitions.is_empty()
        && report.readback_records > 0
    {
        "IMPORTED"
    } else {
        "INCOMPLETE"
    }
    .into();
    publish(&output.join("import-report.json"), &report)?;
    Ok(report)
}

#[tokio::main]
async fn main() {
    let result = match Arguments::parse().command {
        Command::HyperliquidInstruments { output, manifest } => {
            hyperliquid_instruments(&output, manifest.as_deref()).await
        }
        Command::Import {
            manifest,
            instruments,
            start_seconds,
            end_seconds,
            bar_seconds,
            bar_milliseconds,
            native_precision,
            output,
        } => import(
            &manifest,
            &instruments,
            start_seconds,
            end_seconds,
            (bar_seconds, bar_milliseconds),
            native_precision,
            &output,
        )
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
