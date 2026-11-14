//! Operator-only, declaratively mapped history. Scientific jobs remain offline.
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Parser, Subcommand};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Data, FundingRateUpdate, TradeTick},
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

#[path = "market/context.rs"]
mod context;
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
    /// Convert an existing published-hour source into a new native L2 catalog, preserving both clocks.
    PublishedBooks {
        /// Exact official instrument ID from the source definitions; repeat to select several. Omit for all.
        #[arg(long = "instrument-id")]
        instrument_ids: Vec<InstrumentId>,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Inclusive original provider-receipt timestamp, nanoseconds.
        #[arg(long)]
        available_start_ns: u64,
        /// Exclusive original provider-receipt timestamp, nanoseconds.
        #[arg(long)]
        available_end_ns: u64,
    },
    /// Encode official historical settlement events as native InstrumentClose data.
    InstrumentCloses {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        instruments: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Derive received-time oracle/mark CSV sidecars from published context files.
    ContextPrices {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        instruments: PathBuf,
        #[arg(long)]
        research_source_revision: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Obtain settled funding history for perpetuals selected by native definitions.
    HyperliquidFunding {
        #[arg(long)]
        instruments: PathBuf,
        #[arg(long)]
        start_seconds: u64,
        #[arg(long)]
        end_seconds: u64,
        #[arg(long)]
        output: PathBuf,
        /// Explicit existing native market catalog to append funding to; omitted creates output/catalog.
        #[arg(long)]
        native_catalog: Option<PathBuf>,
        /// Research source revision to which the funding sidecar belongs.
        #[arg(long)]
        source_revision: String,
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

fn publish_funding_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("FUNDING_OUTPUT_PARENT")?)?;
    serde_json::to_writer(file.as_file_mut(), value)?;
    writeln!(file)?;
    file.persist(path)?;
    Ok(())
}

fn funding_parts(output: &Path, request: &serde_json::Value) -> Result<PathBuf> {
    let parts = output.join(".funding-parts");
    let metadata = parts.join("request.json");
    ensure!(
        !output.join("funding-rates.manifest.json").exists(),
        "FUNDING_OUTPUT_ALREADY_COMPLETE"
    );
    if output.exists() {
        ensure!(metadata.is_file(), "FUNDING_OUTPUT_ALREADY_EXISTS");
        ensure!(
            json::<serde_json::Value>(&metadata)? == *request,
            "FUNDING_RESUME_REQUEST_CHANGED"
        );
    } else {
        fs::create_dir(output)?;
        fs::create_dir(&parts)?;
        publish_funding_json(&metadata, request)?;
    }
    Ok(parts)
}

fn publish_funding(
    output: &Path,
    source_revision: &str,
    start_ns: u64,
    end_ns: u64,
    instruments: Vec<String>,
    rates: BTreeMap<(u64, String), rust_decimal::Decimal>,
) -> Result<()> {
    let observed: BTreeSet<_> = rates.keys().map(|(_, id)| id.as_str()).collect();
    let missing: Vec<_> = instruments
        .iter()
        .filter(|id| !observed.contains(id.as_str()))
        .collect();
    let mut file = tempfile::NamedTempFile::new_in(output)?;
    for ((event_ns, instrument_id), rate) in &rates {
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({
                "instrument_id": instrument_id,
                "event_ns": event_ns.to_string(),
                "rate": rate.to_string(),
            }),
        )?;
        writeln!(file)?;
    }
    file.flush()?;
    file.persist(output.join("funding-rates.jsonl"))?;
    publish_funding_json(
        &output.join("funding-rates.manifest.json"),
        &serde_json::json!({
            "source_revision": source_revision,
            "start_ns": start_ns.to_string(),
            "end_ns": end_ns.to_string(),
            "instruments": instruments,
            "rows": rates.len(),
            "missing_instruments": missing,
            "native_catalog": "catalog",
            "native_funding_fields": "Original official FundingRateUpdate: rate, interval, next_funding_ns, ts_event and ts_init preserved",
            "historical_clock_semantics": "Official historical adapter maps venue interval-end time to both ts_event and ts_init; not measured collector receive time",
            "simulation_scope": "Nautilus standard funding uses mark price or book midpoint, not Hyperliquid oracle-price payment equivalence",
            "source": "Hyperliquid public fundingHistory at https://api.hyperliquid.xyz/info via official NautilusTrader adapter; settled hourly interval-end timestamps; half-open event-time window; no collector receive timestamps",
        }),
    )?;
    eprintln!(
        "{} funding rows, {} missing perpetuals",
        rates.len(),
        missing.len()
    );
    Ok(())
}

/// Publish the adapter's original typed funding records, without reconstructing clocks
/// or inventing a settlement boundary. Existing published output is verified, never erased.
fn publish_native_funding(
    output: &Path,
    native_catalog: Option<&Path>,
    instruments: &[InstrumentAny],
    updates: &BTreeMap<String, Vec<FundingRateUpdate>>,
) -> Result<()> {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    let destination = native_catalog.map_or_else(|| output.join("catalog"), Path::to_path_buf);
    if native_catalog.is_some() {
        ensure!(destination.is_dir(), "FUNDING_TARGET_CATALOG_REQUIRED");
    }
    let staged = (!destination.exists())
        .then(|| {
            tempfile::Builder::new()
                .prefix(".native-funding-")
                .tempdir_in(output)
        })
        .transpose()?;
    let root = staged
        .as_ref()
        .map_or(destination.as_path(), |dir| dir.path());
    if staged.is_some() {
        job::catalog::currencies::write(root, instruments)?;
    } else {
        job::catalog::currencies::load(root)?;
    }
    let mut catalog = ParquetDataCatalog::from_uri(
        root.to_str().context("FUNDING_CATALOG_PATH")?,
        None,
        Some(4096),
        None,
        None,
    )?;
    if staged.is_some() {
        catalog.write_instruments(instruments.to_vec())?;
    }
    let definitions = catalog.query_instruments(None)?;
    for expected in instruments {
        let expected = serde_json::to_value(expected)?;
        ensure!(
            definitions
                .iter()
                .any(|actual| serde_json::to_value(actual).ok().as_ref() == Some(&expected)),
            "FUNDING_CATALOG_INSTRUMENT_MISMATCH"
        );
    }
    let ids = instruments
        .iter()
        .map(|value| value.id().to_string())
        .collect::<Vec<_>>();
    let actual =
        catalog.query::<FundingRateUpdate>(Some(ids.clone()), None, None, None, None, false)?;
    let mut existing = BTreeMap::new();
    for row in actual {
        existing
            .entry((row.instrument_id, row.ts_event))
            .or_insert_with(Vec::new)
            .push(row);
    }
    // Validate every collision before writing any new funding data. Never replace a
    // pre-existing file or change a conflicting settlement/clock to make it fit.
    let mut additions = BTreeMap::<String, Vec<FundingRateUpdate>>::new();
    for (id, rows) in updates {
        let mut requested = BTreeSet::new();
        for row in rows {
            ensure!(
                requested.insert(row.ts_event),
                "FUNDING_NATIVE_DUPLICATE_EVENT"
            );
            if let Some(saved) = existing.get(&(row.instrument_id, row.ts_event)) {
                ensure!(
                    saved.len() == 1
                        && serde_json::to_value(saved[0])? == serde_json::to_value(row)?,
                    "FUNDING_CATALOG_RECORD_CONFLICT"
                );
            } else {
                additions.entry(id.clone()).or_default().push(*row);
            }
        }
    }
    for rows in additions.values_mut() {
        rows.sort_by_key(|row| row.ts_init);
        if !rows.is_empty() {
            catalog.write_to_parquet(rows, None, None, None)?;
        }
    }
    let saved = catalog.query::<FundingRateUpdate>(Some(ids), None, None, None, None, false)?;
    for row in updates.values().flatten() {
        let matches = saved
            .iter()
            .filter(|value| {
                value.instrument_id == row.instrument_id && value.ts_event == row.ts_event
            })
            .collect::<Vec<_>>();
        // FundingRateUpdate::PartialEq ignores timestamps; official serialization doesn't.
        ensure!(
            matches.len() == 1 && serde_json::to_value(matches[0])? == serde_json::to_value(row)?,
            "FUNDING_CATALOG_RECORD_MISMATCH"
        );
    }
    if staged.is_some() {
        fs::rename(root, &destination)?;
    }
    Ok(())
}

async fn hyperliquid_funding(
    instruments_path: &Path,
    start_seconds: u64,
    end_seconds: u64,
    output: &Path,
    source_revision: &str,
    native_catalog: Option<&Path>,
) -> Result<()> {
    use nautilus_hyperliquid::{
        common::{consts::HYPERLIQUID_VENUE, enums::HyperliquidEnvironment},
        http::client::HyperliquidHttpClient,
    };
    ensure!(start_seconds < end_seconds, "FUNDING_TIME_RANGE_INVALID");
    if let Some(path) = native_catalog {
        ensure!(path.is_dir(), "FUNDING_TARGET_CATALOG_REQUIRED");
    }
    ensure!(
        !source_revision.trim().is_empty(),
        "FUNDING_SOURCE_REVISION_EMPTY"
    );
    let start_ns = start_seconds
        .checked_mul(1_000_000_000)
        .context("FUNDING_TIME_RANGE_INVALID")?;
    let end_ns = end_seconds
        .checked_mul(1_000_000_000)
        .context("FUNDING_TIME_RANGE_INVALID")?;
    let start = DateTime::from_timestamp(start_seconds as i64, 0)
        .context("FUNDING_TIME_RANGE_INVALID")?
        .to_rfc3339()
        .parse()?;
    let end = DateTime::from_timestamp(end_seconds as i64, 0)
        .context("FUNDING_TIME_RANGE_INVALID")?
        .to_rfc3339()
        .parse()?;
    let instruments: BTreeMap<_, _> = job::catalog::currencies::read_instruments(instruments_path)?
        .into_iter()
        .filter(|instrument| matches!(instrument, InstrumentAny::CryptoPerpetual(_)))
        .map(|instrument| (instrument.id().to_string(), instrument))
        .collect();
    ensure!(!instruments.is_empty(), "FUNDING_PERPETUALS_EMPTY");
    let selected: Vec<_> = instruments.keys().cloned().collect();
    let mut acquisition = serde_json::json!({
        "source_revision": source_revision,
        "start_ns": start_ns.to_string(),
        "end_ns": end_ns.to_string(),
        "instruments": instruments.iter().map(|(id, instrument)| (id, instrument.raw_symbol().to_string())).collect::<BTreeMap<_, _>>(),
    });
    if let Some(path) = native_catalog {
        acquisition["native_catalog"] = serde_json::to_value(path.canonicalize()?)?;
    }
    let parts = funding_parts(output, &acquisition)?;
    let client = HyperliquidHttpClient::new(HyperliquidEnvironment::Mainnet, 60, None)?;
    let mut rates = BTreeMap::new();
    let mut native_updates = BTreeMap::<String, Vec<FundingRateUpdate>>::new();
    for (index, (id, instrument)) in instruments.iter().enumerate() {
        ensure!(
            instrument.id().venue == *HYPERLIQUID_VENUE,
            "FUNDING_HYPERLIQUID_INSTRUMENT_REQUIRED: {id}"
        );
        let part = parts.join(format!("{index}.json"));
        let updates: Vec<FundingRateUpdate> = if part.exists() {
            json(&part)?
        } else {
            let updates = client
                .request_funding_rates(instrument, Some(start), Some(end), None)
                .await
                .with_context(|| format!("FUNDING_REQUEST_FAILED: {id}"))?;
            publish_funding_json(&part, &updates)?;
            updates
        };
        for update in updates {
            let time = update.ts_event.as_u64();
            if (start_ns..end_ns).contains(&time) {
                ensure!(
                    update.instrument_id == instrument.id(),
                    "FUNDING_NATIVE_IDENTITY_MISMATCH"
                );
                rates.insert((time, id.clone()), update.rate);
                native_updates.entry(id.clone()).or_default().push(update);
            }
        }
    }
    publish_native_funding(
        output,
        native_catalog,
        &instruments.values().cloned().collect::<Vec<_>>(),
        &native_updates,
    )?;
    publish_funding(output, source_revision, start_ns, end_ns, selected, rates)?;
    if let Some(path) = native_catalog {
        let manifest_path = output.join("funding-rates.manifest.json");
        let mut manifest: serde_json::Value = json(&manifest_path)?;
        manifest["native_catalog"] = serde_json::to_value(path.canonicalize()?)?;
        publish_funding_json(&manifest_path, &manifest)?;
    }
    fs::remove_dir_all(parts)?;
    Ok(())
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
        !self.revision.trim().is_empty()
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
    received_time: bool,
    maximum_receive_delay_ns: Option<u64>,
    path_prefixes: &'a [String],
    resume: bool,
    _max_trade_rows: u64,
    max_output_bytes: u64,
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
        !(options.trade_ticks) || manifest.trade_revision(),
        "SOURCE_REVISION_REQUIRED"
    );
    ensure!(
        !options.received_time || (options.trade_ticks),
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
    if options.trade_ticks {
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
    let mut sink = Some(writer::Writer::configured(
        output,
        &instruments,
        previous_report.is_some(),
        None,
    )?);
    let mut aggregation = records::Aggregation::with_milliseconds(bar_seconds, bar_milliseconds)?;
    let mut report = Report {
        schema_version: 1,
        source_reference: manifest.source_reference.clone().unwrap_or_else(|| format!("https://huggingface.co/datasets/{}/tree/{}", manifest.repository, manifest.revision)),
        revision: manifest.revision.clone(), source_revision: manifest.revision.clone(),
        selected_streams: selected_files.iter().filter_map(|f| f.stream.clone()).collect::<BTreeSet<_>>().into_iter().collect(),
        selected_path_prefixes: prefixes.clone(),
        modality: if options.trade_ticks { "TRADE_TICK" } else { "NATIVE_SOURCE" }.into(),
        generated_bar_semantics: (bar_seconds.is_some() || bar_milliseconds.is_some())
            .then(|| job::catalog::aggregation::CONVENTION.into()),
        representation: None,
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
    let mut trades: BTreeMap<_, (TradeTick, usize)> = BTreeMap::new();
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
                if let Data::Trade(trade) = &data {
                    let key = (trade.instrument_id, trade.ts_event, trade.trade_id);
                    if let Some(previous) = trades.get(&key) {
                        if &previous.0 != trade && options.trade_ticks {
                            *report
                                .rejected_rows
                                .entry("CONFLICTING_SOURCE_TRADE_ID".into())
                                .or_default() += 1;
                            continue;
                        }
                        ensure!(&previous.0 == trade, "CONFLICTING_SOURCE_TRADE_ID");
                        report.duplicate_trades += 1;
                        continue;
                    }
                    trades.insert(key, (*trade, trades.len()));
                    if options.trade_ticks {
                        continue;
                    }
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
            if let Some(bar) = aggregation.update(&trade)? {
                sink.as_mut().unwrap().push(Data::Bar(bar))?;
            }
            sink.as_mut().unwrap().push(Data::Trade(trade))?;
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
    let mut sink = sink.unwrap();
    if options.trade_ticks && !published_source {
        let mut ordered = trades.values().collect::<Vec<_>>();
        ordered.sort_by_key(|(trade, ordinal)| (trade.instrument_id, *ordinal));
        for group in
            ordered.chunk_by(|(left, _), (right, _)| left.instrument_id == right.instrument_id)
        {
            for (trade, _) in group {
                sink.push(Data::Trade(*trade))?;
            }
            sink.finish()?;
        }
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

fn retired_cohort_mode(value: &std::ffi::OsStr) -> bool {
    ["compose-cohorts", "--cohort-cache", "--cohort-flow-cache"]
        .iter()
        .any(|name| value == std::ffi::OsStr::new(name))
}

fn main() {
    if std::env::args_os()
        .skip(1)
        .any(|arg| retired_cohort_mode(&arg))
    {
        eprintln!("QZ_DERIVED_TICK_COHORT_RETIRED: preserve original data; use import --trade-ticks for original Nautilus TradeTick records. Existing cohort files are not migrated or recomputed.");
        std::process::exit(2);
    }
    let result = match Arguments::parse().command {
        Command::PublishedBooks {
            instrument_ids,
            source,
            output,
            available_start_ns,
            available_end_ns,
        } => job::ticks::economic::export_published_books_selected(
            &source,
            &output,
            available_start_ns,
            available_end_ns,
            &instrument_ids,
        )
        .and_then(|report| {
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }),
        Command::InstrumentCloses {
            source,
            instruments,
            output,
        } => job::ticks::economic::closes::import(&source, &instruments, &output).and_then(
            |result| {
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            },
        ),
        Command::HyperliquidFunding {
            instruments,
            start_seconds,
            end_seconds,
            output,
            source_revision,
            native_catalog,
        } => tokio::runtime::Runtime::new()
            .context("FUNDING_RUNTIME_UNAVAILABLE")
            .and_then(|runtime| {
                runtime.block_on(hyperliquid_funding(
                    &instruments,
                    start_seconds,
                    end_seconds,
                    &output,
                    &source_revision,
                    native_catalog.as_deref(),
                ))
            }),
        Command::ContextPrices {
            manifest,
            instruments,
            research_source_revision,
            output,
        } => context::run(&manifest, &instruments, &research_source_revision, &output).and_then(
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
            received_time,
            maximum_receive_delay_ns,
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

    #[test]
    fn derived_cohort_commands_are_absent_while_native_trade_import_remains() {
        assert!(Arguments::try_parse_from(["market-history", "compose-cohorts"]).is_err());
        for retired in ["--cohort-cache", "--cohort-flow-cache"] {
            assert!(retired_cohort_mode(std::ffi::OsStr::new(retired)));
            assert!(Arguments::try_parse_from([
                "market-history",
                "import",
                "--manifest",
                "source.json",
                "--instruments",
                "instruments.json",
                "--output",
                "out",
                retired
            ])
            .is_err());
        }
        assert!(Arguments::try_parse_from([
            "market-history",
            "import",
            "--manifest",
            "source.json",
            "--instruments",
            "instruments.json",
            "--output",
            "out",
            "--trade-ticks"
        ])
        .is_ok());
    }

    #[test]
    fn received_context_sidecar_keeps_clock_decimal_missing_and_aliases() {
        let temp = tempfile::tempdir().unwrap();
        let raw = temp.path().join("context.csv");
        let text = concat!(
            "symbol,received_time,index,mark,rate\n",
            "ALIAS,1767225600000000002,,123.123456789012345678901,999999\n",
            "ALIAS,1767225600000000001,1e-8,0.0000000200,-999999\n",
            "ALIAS,1767225600000000001,1e-8,0.0000000200,-999999\n"
        );
        fs::write(&raw, text).unwrap();
        let first = nautilus_model::instruments::stubs::crypto_perpetual_ethusdt();
        let mut second = first.clone();
        second.id = "Z-ETHUSDT-PERP.BINANCE".parse().unwrap();
        second.raw_symbol = Symbol::from("ZETHUSDT");
        let first_id = first.id.to_string();
        let second_id = second.id.to_string();
        let definitions = temp.path().join("definitions.json");
        publish(&definitions, &serde_json::json!({"currencies":[],
            "instruments":[InstrumentAny::CryptoPerpetual(first),InstrumentAny::CryptoPerpetual(second)],
            "symbol_ids":{"ALIAS":first_id,"EMPTY":second_id}})).unwrap();
        let manifest = temp.path().join("source.json");
        let source = serde_json::json!({"schema_version":1,"repository":"CryptoHFTData","revision":"original-context",
            "license":"original","attribution":"original","retrieved_at":"2026-01-02T00:00:00Z",
            "source_root":temp.path(),"source_reference":"https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data",
            "coverage":"OBSERVED_PUBLISHED_FILES","start_date":"2026-01-01","end_date":"2026-01-02",
            "selection":{"symbols":["ALIAS","EMPTY"]},"missing":[{"stream":"context","symbol":"EMPTY"}],
            "files":[{"path":"context.csv","size":text.len(),"stream":"context"}],
            "profile":{"streams":[{"name":"context","kind":"context","format":"csv","time_unit":"ns",
                "available_time_unit":"ns","columns":{"symbol":"symbol","available_at":"received_time","index_price":"index","mark_price":"mark"}}]}});
        publish(&manifest, &source).unwrap();
        let output = temp.path().join("derived");
        let report = context::run(&manifest, &definitions, "original-research", &output).unwrap();
        assert_eq!(report["readback_records"], 2);
        let catalog = output.join("catalog");
        let result: serde_json::Value =
            json(&catalog.join("context-prices.manifest.json")).unwrap();
        assert_eq!(result["clock"], "RECEIVED_TIME_OBSERVED");
        assert_eq!(result["source_revision"], "original-context");
        assert_eq!(result["research_source_revision"], "original-research");
        assert_eq!(result["missing"], source["missing"]);
        let rows = fs::read_to_string(catalog.join("context-prices-000.csv")).unwrap();
        assert_eq!(rows,"event_ns,oracle_px,mark_px\n1767225600000000001,1e-8,0.0000000200\n1767225600000000002,,123.123456789012345678901\n");
        assert_eq!(result["files"][1]["rows"], "0");
        assert_eq!(
            result["files"][1]["first_event_ns"],
            serde_json::Value::Null
        );
        assert!(!catalog.join("funding-rates.jsonl").exists());
        assert_eq!(fs::read_to_string(&raw).unwrap(), text);
        assert!(context::run(&manifest, &definitions, "", &temp.path().join("unbound")).is_err());
        fs::write(&raw, text.replace("1e-8", "-1e8")).unwrap();
        assert!(context::run(
            &manifest,
            &definitions,
            "research",
            &temp.path().join("invalid")
        )
        .is_err());
    }

    #[test]
    fn funding_sidecar_preserves_decimal_time_order_and_missing_markets() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("funding");
        let request = serde_json::json!({"source_revision":"research-v1", "start_ns":"18000000000000000000", "end_ns":"18000000000000000002"});
        let parts = funding_parts(&output, &request).unwrap();
        publish_funding_json(&parts.join("0.json"), &Vec::<FundingRateUpdate>::new()).unwrap();
        assert_eq!(funding_parts(&output, &request).unwrap(), parts);
        assert!(json::<Vec<FundingRateUpdate>>(&parts.join("0.json"))
            .unwrap()
            .is_empty());
        assert!(
            funding_parts(&output, &serde_json::json!({"source_revision":"different"})).is_err()
        );
        let rates = BTreeMap::from([
            (
                (18_000_000_000_000_000_001, "Z.HYPERLIQUID".into()),
                "-0.000012345678901234".parse().unwrap(),
            ),
            (
                (18_000_000_000_000_000_001, "A.HYPERLIQUID".into()),
                "0.000000000000000001".parse().unwrap(),
            ),
        ]);
        publish_funding(
            &output,
            "research-v1",
            18_000_000_000_000_000_000,
            18_000_000_000_000_000_002,
            vec![
                "A.HYPERLIQUID".into(),
                "M.HYPERLIQUID".into(),
                "Z.HYPERLIQUID".into(),
            ],
            rates,
        )
        .unwrap();
        let rows: Vec<serde_json::Value> = fs::read_to_string(output.join("funding-rates.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[0]["instrument_id"], "A.HYPERLIQUID");
        assert_eq!(rows[0]["event_ns"], "18000000000000000001");
        assert_eq!(rows[0]["rate"], "0.000000000000000001");
        assert_eq!(rows[1]["rate"], "-0.000012345678901234");
        let manifest: serde_json::Value =
            json(&output.join("funding-rates.manifest.json")).unwrap();
        assert_eq!(manifest["rows"], 2);
        assert_eq!(manifest["source_revision"], "research-v1");
        assert_eq!(
            manifest["missing_instruments"],
            serde_json::json!(["M.HYPERLIQUID"])
        );
    }

    #[tokio::test]
    async fn native_funding_append_preserves_market_data_and_rejects_conflicts() {
        use nautilus_model::{
            data::QuoteTick,
            types::{Price, Quantity},
        };
        use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
        let root = tempfile::tempdir().unwrap();
        let market = root.path().join("market");
        fs::create_dir(&market).unwrap();
        let mut instrument = nautilus_model::instruments::stubs::crypto_perpetual_ethusdt();
        instrument.id = "ETHUSDT-PERP.HYPERLIQUID".parse().unwrap();
        let instrument = InstrumentAny::CryptoPerpetual(instrument);
        let id = instrument.id();
        job::catalog::currencies::write(&market, std::slice::from_ref(&instrument)).unwrap();
        let mut catalog = ParquetDataCatalog::new(&market, None, None, None, None);
        catalog.write_instruments(vec![instrument.clone()]).unwrap();
        let quote = QuoteTick::new(
            id,
            Price::from("100.00"),
            Price::from("101.00"),
            Quantity::from("1.000"),
            Quantity::from("1.000"),
            1.into(),
            2.into(),
        );
        catalog
            .write_to_parquet(&[quote], None, None, None)
            .unwrap();
        let rate = FundingRateUpdate::new(
            id,
            "0.000123456789".parse().unwrap(),
            Some(60),
            None,
            3_600_000_000_000u64.into(),
            3_600_000_000_000u64.into(),
        );
        let updates = BTreeMap::from([(id.to_string(), vec![rate])]);
        let definitions = root.path().join("instruments.json");
        publish_funding_json(&definitions, &vec![instrument.clone()]).unwrap();
        let output = root.path().join("funding-sidecar");
        let parts = funding_parts(&output, &serde_json::json!({
            "source_revision":"native-append-test","start_ns":"1000000000","end_ns":"3601000000000",
            "instruments":{id.to_string():instrument.raw_symbol().to_string()},
            "native_catalog":market.canonicalize().unwrap()
        })).unwrap();
        publish_funding_json(&parts.join("0.json"), &vec![rate]).unwrap();
        hyperliquid_funding(
            &definitions,
            1,
            3601,
            &output,
            "native-append-test",
            Some(&market),
        )
        .await
        .unwrap();
        let manifest: serde_json::Value =
            json(&output.join("funding-rates.manifest.json")).unwrap();
        assert_eq!(
            manifest["native_catalog"],
            serde_json::to_value(market.canonicalize().unwrap()).unwrap()
        );
        assert!(!output.join("catalog").exists());
        publish_native_funding(
            root.path(),
            Some(&market),
            std::slice::from_ref(&instrument),
            &updates,
        )
        .unwrap();
        let saved = catalog
            .query::<FundingRateUpdate>(None, None, None, None, None, false)
            .unwrap();
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(vec![rate]).unwrap()
        );
        let quotes = catalog
            .query::<QuoteTick>(None, None, None, None, None, false)
            .unwrap();
        assert_eq!(
            serde_json::to_value(quotes).unwrap(),
            serde_json::to_value(vec![quote]).unwrap()
        );
        let replay: contracts::native_strategy::NativeStrategyBacktestRequestV1 = serde_json::from_value(serde_json::json!({
            "schema_version":1,"instrument_id":id.to_string(),"selectors":[{"data_type":"QuoteTick","identifier":id.to_string()},{"data_type":"FundingRateUpdate","identifier":id.to_string()}],
            "event_start_ns":"1","event_end_ns":"3601000000000","available_end_ns":"3601000000000",
            "venue":{"account_type":"MARGIN","oms_type":"NETTING","book_type":"L1_MBP","starting_balances":["100000 USDT"],"base_currency":null,"leverage":"1","maker_fee":"0","taker_fee":"0"},
            "strategy":{"kind":"EMA_CROSS","trade_size":"1.000","fast_period":2,"slow_period":3}
        })).unwrap();
        let replay = job::native_strategy::run(&market, replay).unwrap();
        assert_eq!(replay.data_rows_by_type["QuoteTick"].get(), 1);
        assert_eq!(replay.data_rows_by_type["FundingRateUpdate"].get(), 1);
        let mut changed = rate;
        changed.rate = "0.02".parse().unwrap();
        let conflict = BTreeMap::from([(id.to_string(), vec![changed])]);
        assert!(
            publish_native_funding(root.path(), Some(&market), &[instrument], &conflict)
                .unwrap_err()
                .to_string()
                .contains("FUNDING_CATALOG_RECORD_CONFLICT")
        );
        assert_eq!(
            serde_json::to_value(
                catalog
                    .query::<FundingRateUpdate>(None, None, None, None, None, false)
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(vec![rate]).unwrap()
        );
    }

    #[tokio::test]
    async fn funding_acquisition_reuses_completed_markets_and_excludes_end_boundary() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("funding");
        let definitions = root.path().join("instruments.json");
        let mut instrument = nautilus_model::instruments::stubs::crypto_perpetual_ethusdt();
        instrument.id = "ETHUSDT-PERP.HYPERLIQUID".parse().unwrap();
        let id = instrument.id;
        let raw_symbol = instrument.raw_symbol.to_string();
        publish(
            &definitions,
            &vec![InstrumentAny::CryptoPerpetual(instrument)],
        )
        .unwrap();
        let parts = funding_parts(
            &output,
            &serde_json::json!({
                "source_revision":"research-v1", "start_ns":"1000000000", "end_ns":"2000000000",
                "instruments":{id.to_string():raw_symbol},
            }),
        )
        .unwrap();
        let updates: Vec<_> = [1_000_000_000, 2_000_000_000]
            .into_iter()
            .map(|time| {
                FundingRateUpdate::new(
                    id,
                    "0.00001".parse().unwrap(),
                    Some(60),
                    None,
                    UnixNanos::from(time),
                    UnixNanos::from(time + 17),
                )
            })
            .collect();
        publish_funding_json(&parts.join("0.json"), &updates).unwrap();
        hyperliquid_funding(&definitions, 1, 2, &output, "research-v1", None)
            .await
            .unwrap();
        assert!(!parts.exists());
        let rows = fs::read_to_string(output.join("funding-rates.jsonl")).unwrap();
        assert_eq!(rows.lines().count(), 1);
        let row: serde_json::Value = serde_json::from_str(rows.trim()).unwrap();
        assert_eq!(row["event_ns"], "1000000000");
        let manifest: serde_json::Value =
            json(&output.join("funding-rates.manifest.json")).unwrap();
        assert_eq!(manifest["missing_instruments"], serde_json::json!([]));
        assert_eq!(manifest["native_catalog"], "catalog");
        let mut native = nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog::new(
            &output.join("catalog"),
            None,
            None,
            None,
            None,
        );
        let saved = native
            .query::<FundingRateUpdate>(None, None, None, None, None, false)
            .unwrap();
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(&updates[..1]).unwrap()
        );
        let saved_definitions = native.query_instruments(None).unwrap();
        assert_eq!(saved_definitions.len(), 1);
        assert_eq!(saved_definitions[0].id(), id);
    }

    fn tick_options(prefixes: &[String], resume: bool, rows: u64, bytes: u64) -> ImportOptions<'_> {
        ImportOptions {
            start: None,
            end: None,
            bar_interval: (None, None),
            native_precision: true,
            trade_ticks: true,
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
    fn native_trade_ticks_group_interleaved_instruments_before_publication() {
        let temp = tempfile::tempdir().unwrap();
        let mut rows = String::new();
        for index in 0..100_001_u64 {
            for symbol in ["BTCUSDT", "ETHUSDT"] {
                rows.push_str(&format!(
                    "{symbol},{},10.1234567890123456,0.1234567890,T-{index},B\n",
                    1_767_225_600_001 + index
                ));
            }
        }
        let (manifest, instruments, _) = tick_source(temp.path(), &rows);
        fs::write(
            &instruments,
            serde_json::to_vec(&vec![
                InstrumentAny::CurrencyPair(
                    nautilus_model::instruments::stubs::currency_pair_btcusdt(),
                ),
                InstrumentAny::CurrencyPair(
                    nautilus_model::instruments::stubs::currency_pair_ethusdt(),
                ),
            ])
            .unwrap(),
        )
        .unwrap();
        let prefixes = vec!["perps/".into()];
        let report = import(
            &manifest,
            &instruments,
            &temp.path().join("native"),
            tick_options(&prefixes, false, 0, 0),
        )
        .unwrap();
        assert_eq!(report.status, "IMPORTED");
        assert_eq!(report.readback_records, 200_002);
        assert_eq!(report.duplicate_trades, 0);
        assert!(report.rejected_rows.is_empty());
        assert_eq!(
            report
                .native_files
                .iter()
                .filter(|file| file.path.starts_with("data/trades/"))
                .count(),
            4
        );
    }

    #[test]
    fn native_trade_grouping_resumes_legacy_and_new_equal_time_source_order() {
        let temp = tempfile::tempdir().unwrap();
        let (manifest, instruments, _) = tick_source(
            temp.path(),
            "BTCUSDT,1767225600001,10.1234567890123456,0.1234567890,Z,B\nBTCUSDT,1767225600001,10.1234567890123456,0.1234567890,A,B\n",
        );
        let definitions = job::catalog::currencies::read_instruments(&instruments).unwrap();
        let legacy = temp.path().join("legacy");
        let mut writer = writer::Writer::configured(&legacy, &definitions, false, None).unwrap();
        for id in ["Z", "A"] {
            writer
                .push(Data::Trade(
                    nautilus_model::data::TradeTick::new_checked(
                        definitions[0].id(),
                        "10.1234567890123456".parse().unwrap(),
                        "0.1234567890000000".parse().unwrap(),
                        nautilus_model::enums::AggressorSide::Buy,
                        TradeId::new(id),
                        1_767_225_600_001_000_000.into(),
                        1_767_225_600_001_000_000.into(),
                    )
                    .unwrap(),
                ))
                .unwrap();
        }
        writer.finish().unwrap();
        let prefixes = vec!["perps/".into()];
        let output = temp.path().join("new");
        let mut report = import(
            &manifest,
            &instruments,
            &output,
            tick_options(&prefixes, false, 0, 0),
        )
        .unwrap();
        report.native_files = native_files(&legacy.join("catalog")).unwrap();
        report.native_records = writer.counts;
        publish(&legacy.join("import-report.json"), &report).unwrap();
        publish(&legacy.join("catalog/import-provenance.json"), &report).unwrap();
        for path in [&legacy, &output] {
            assert_eq!(
                import(
                    &manifest,
                    &instruments,
                    path,
                    tick_options(&prefixes, true, 0, 0),
                )
                .unwrap()
                .status,
                "IMPORTED"
            );
        }
    }

    #[test]
    fn public_source_revisions_are_nonempty_opaque_versions() {
        let temp = tempfile::tempdir().unwrap();
        let (path, _, _) = tick_source(temp.path(), "");
        let mut manifest: Manifest = json(&path).unwrap();
        assert!(manifest.trade_revision());
        for revision in [
            "global-hl-20250728-20261109-v1",
            "public-release-2",
            "published-files-2026-01-01-2026-01-02",
        ] {
            manifest.revision = revision.into();
            assert!(manifest.trade_revision());
        }
        for revision in ["", " ", "\t\n"] {
            manifest.revision = revision.into();
            assert!(!manifest.trade_revision());
        }
        manifest.repository = "CryptoHFTData".into();
        manifest.source_reference =
            Some("https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data".into());
        manifest.revision = "published-files-2026-01-01-2026-01-02".into();
        assert!(manifest.trade_revision());
        manifest.revision = "published-files-not-a-date".into();
        assert!(manifest.trade_revision());
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
    fn published_trade_ticks_select_receive_time_and_keep_earliest_duplicate() {
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
            options.trade_ticks = true;
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
            assert_eq!(report.readback_records, count);
            let mut native =
                nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog::from_uri(
                    output.join("catalog").to_str().unwrap(),
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
            let ticks = native.query::<TradeTick>(
                None,
                None,
                None,
                None,
                None,
                false,
            )
            .unwrap();
            assert_eq!(
                ticks.iter().map(|tick| tick.ts_init.as_u64()).min(),
                Some(first)
            );
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
        options.trade_ticks = true;
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
        assert_eq!(report.readback_records, 2);
        let mut native =
            nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog::from_uri(
                output.join("catalog").to_str().unwrap(),
                None,
                None,
                None,
                None,
            )
            .unwrap();
        let ticks = native.query::<TradeTick>(
            None,
            None,
            None,
            None,
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            ticks.iter().map(|tick| tick.ts_init.as_u64()).min(),
            Some(1767225600001000000)
        );
        assert_eq!(
            ticks.iter().map(|tick| tick.ts_init.as_u64()).max(),
            Some(1767225600003000000)
        );
        assert_eq!(
            ticks.iter().map(|tick| tick.ts_event.as_u64()).max(),
            Some(1767225601000000000)
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
            options.trade_ticks = true;
            options.received_time = true;
            options.maximum_receive_delay_ns = Some(12_000_000);
            let output = temp.path().join("received");
            let report = import(&path, &instruments, &output, options).unwrap();
            assert_eq!(report.status, "INCOMPLETE");
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
            "BTCUSDT.OTHER".parse().unwrap(),
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
        assert_eq!(report.readback_records, 4);
        assert!(report.missing_definitions.is_empty());
        assert_eq!(
            report.native_records["book_deltas"][&instrument.id().to_string()],
            4
        );
        assert!(!report.native_records.contains_key("quotes"));
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
        fs::write(&instruments, serde_json::to_vec(&serde_json::json!({"currencies":[],"instruments":[],"symbol_ids":{"BTCUSDT":"BTCUSDT.OTHER"}})).unwrap()).unwrap();
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
