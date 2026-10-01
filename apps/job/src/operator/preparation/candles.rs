//! Offline, exact conversion of frozen public OHLCV observations into native BARs.
//! Acquisition and serialization do not certify historical availability or permission.
use anyhow::{ensure, Context, Result};
use bigdecimal::BigDecimal;
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Parser;
use nautilus_model::{
    data::{Bar, BarType, Data},
    instruments::{Instrument, InstrumentAny},
    types::{Price, Quantity},
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, value::RawValue, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    str::FromStr,
};

const MIB: u64 = 1024 * 1024;
const MAX_ROWS: usize = 100_000;
const SCHEMA: &str = "qz.public_acquisition/1";
const PROVIDER: &str = "coinbase-candles";
const TERMS: &str = "https://www.coinbase.com/legal/market_data";

#[derive(Parser)]
#[command(about = "Import frozen crypto OHLCV into native BARs without certifying historical PIT")]
pub struct Arguments {
    /// Original acquisition.json from the bounded public-source runner.
    #[arg(long)]
    pub acquisition: PathBuf,
    /// Original native InstrumentAny JSON array; no product definitions are synthesized.
    #[arg(long)]
    pub instruments: PathBuf,
    /// New directory for native catalog, detached source evidence and final import-report.json.
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    provider: Value,
    selection: Selection,
    selection_bounds: String,
    limits: Limits,
    requests: Vec<Request>,
    admission: Value,
    created_at: String,
    source_terms: Terms,
    responses: Vec<Response>,
    records: File,
    record_count: usize,
    raw_response_bytes: u64,
    observation_status: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    instrument: String,
    start_seconds: u64,
    end_seconds: u64,
    interval_seconds: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    max_response_bytes: u64,
    max_response_total_bytes: u64,
    max_requests: usize,
    max_records: usize,
    max_output_bytes: u64,
}

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Request {
    url: String,
    start_seconds: u64,
    end_seconds: u64,
    method: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Terms {
    file: File,
    reference: String,
    evidence_status: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    request: Request,
    observation: Observation,
    file: File,
    counts: Counts,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    status: u16,
    headers: BTreeMap<String, String>,
    request_started_at: String,
    retrieved_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Counts {
    source_rows: usize,
    selected_rows: usize,
    outside_request_window: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    selection_time_seconds: u64,
    event_time_seconds: u64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    instrument: String,
    kind: String,
    interval_seconds: u64,
    observed_at: String,
    historical_available_at: Value,
    source_response: String,
    source_row_index: usize,
}

struct Original {
    values: [Decimal; 5], // open, high, low, close, volume
    received: u64,
    response: String,
    index: usize,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn no_symlinks(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        if parent.as_os_str().is_empty() {
            continue;
        }
        ensure!(
            !fs::symlink_metadata(parent)?.file_type().is_symlink(),
            "SOURCE_SYMLINK"
        );
    }
    Ok(())
}

fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    no_symlinks(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "SOURCE_FILE_LIMIT"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 == metadata.len(), "SOURCE_FILE_CHANGED");
    Ok(bytes)
}

fn verified_file(root: &Path, file: &File, expected: &str, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        file.path == expected && file.size <= limit,
        "SOURCE_FILE_IDENTITY"
    );
    let bytes = read(&root.join(expected), limit)?;
    ensure!(
        bytes.len() as u64 == file.size && digest(&bytes) == file.sha256,
        "SOURCE_CHECKSUM_MISMATCH"
    );
    Ok(bytes)
}

fn nanos(seconds: u64) -> Result<u64> {
    let value = seconds
        .checked_mul(1_000_000_000)
        .context("TIMESTAMP_RANGE")?;
    ensure!(value <= i64::MAX as u64, "TIMESTAMP_RANGE");
    Ok(value)
}

fn clock(text: &str) -> Result<u64> {
    let core = text
        .strip_suffix('Z')
        .or_else(|| text.strip_suffix("+00:00"))
        .context("UTC_OBSERVATION_REQUIRED")?;
    let (whole, fraction) = core
        .split_once('.')
        .map_or((core, None), |(a, b)| (a, Some(b)));
    ensure!(
        whole.len() == 19
            && whole.as_bytes().iter().enumerate().all(|(index, byte)| {
                match index {
                    4 | 7 => *byte == b'-',
                    10 => *byte == b'T',
                    13 | 16 => *byte == b':',
                    _ => byte.is_ascii_digit(),
                }
            })
            && fraction.is_none_or(|digits| (1..=6).contains(&digits.len())
                && digits.bytes().all(|b| b.is_ascii_digit())),
        "UTC_OBSERVATION_REQUIRED"
    );
    let value = DateTime::parse_from_rfc3339(text)?;
    ensure!(
        value.offset().local_minus_utc() == 0,
        "UTC_OBSERVATION_REQUIRED"
    );
    Ok(u64::try_from(
        value.timestamp_nanos_opt().context("TIMESTAMP_RANGE")?,
    )?)
}

fn query_clock(seconds: u64) -> Result<String> {
    nanos(seconds)?;
    Ok(DateTime::<Utc>::from_timestamp(i64::try_from(seconds)?, 0)
        .context("TIMESTAMP_RANGE")?
        .to_rfc3339_opts(SecondsFormat::Secs, true)
        .replace(':', "%3A"))
}

fn exact(text: &str) -> Result<Decimal> {
    ensure!(
        !text.is_empty() && text.len() <= 256,
        "SOURCE_DECIMAL_RANGE"
    );
    let value = if text.contains(['e', 'E']) {
        Decimal::from_scientific(text)?
    } else {
        Decimal::from_str_exact(text)?
    };
    // The pinned native decimal parser must not round any source representation.
    ensure!(
        BigDecimal::from_str(text)? == BigDecimal::from_str(&value.to_string())?,
        "SOURCE_DECIMAL_LOSS"
    );
    Ok(value)
}

fn valid_values(values: &[Decimal; 5]) -> Result<()> {
    let [open, high, low, close, volume] = values;
    ensure!(
        *low > Decimal::ZERO
            && low <= open
            && low <= close
            && high >= open
            && high >= close
            && *volume >= Decimal::ZERO,
        "SOURCE_CANDLE_INVALID"
    );
    Ok(())
}

fn selection(manifest: &Manifest) -> Result<(&str, &str)> {
    let selected = &manifest.selection;
    let (base, quote) = selected
        .instrument
        .split_once('-')
        .context("SOURCE_PRODUCT_INVALID")?;
    ensure!(
        [base, quote].iter().all(|s| !s.is_empty()
            && s.len() <= 20
            && s.bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())),
        "SOURCE_PRODUCT_INVALID"
    );
    ensure!(
        [60, 300, 900, 3600, 21600, 86400].contains(&selected.interval_seconds)
            && selected.start_seconds < selected.end_seconds
            && selected
                .start_seconds
                .is_multiple_of(selected.interval_seconds)
            && selected
                .end_seconds
                .is_multiple_of(selected.interval_seconds),
        "SOURCE_SELECTION_INVALID"
    );
    nanos(selected.start_seconds)?;
    nanos(selected.end_seconds)?;
    Ok((base, quote))
}

fn original_rows(manifest: &Manifest, root: &Path) -> Result<BTreeMap<u64, Original>> {
    selection(manifest)?;
    ensure!(
        manifest.schema == SCHEMA
            && manifest.selection_bounds == "[start_seconds,end_seconds)"
            && manifest.provider["id"] == PROVIDER
            && manifest.provider["version"] == 1
            && manifest.provider["record_kind"] == "OHLCV_CANDLE"
            && manifest.provider["venue"] == "COINBASE"
            && manifest.provider["terms_reference"] == TERMS
            && manifest.provider["authentication"] == "NONE"
            && manifest.provider["hosts"] == json!(["api.exchange.coinbase.com"])
            && manifest.provider["license"].is_null()
            && manifest.provider["permission_status"] == "REQUIRES_INDEPENDENT_REVIEW"
            && manifest.admission
                == json!({"coverage":"UNPROVEN", "historical_availability":"UNVERIFIED",
            "research_qualified":false, "registered_in_quazonai":false,
            "permission_status":"REQUIRES_INDEPENDENT_REVIEW"}),
        "SOURCE_ACQUISITION_CONTRACT"
    );
    let limits = &manifest.limits;
    ensure!(
        limits.max_response_bytes == 4 * MIB
            && (1..=128 * MIB).contains(&limits.max_response_total_bytes)
            && limits.max_requests == 128
            && limits.max_records == MAX_ROWS
            && limits.max_output_bytes == 128 * MIB,
        "SOURCE_ACQUISITION_LIMITS"
    );
    ensure!(
        manifest.source_terms.reference == TERMS
            && manifest.source_terms.evidence_status
                == "OPERATOR_SUPPLIED_NOT_INDEPENDENTLY_VERIFIED",
        "SOURCE_TERMS_REQUIRED"
    );
    let terms = verified_file(root, &manifest.source_terms.file, "source-terms.bin", MIB)?;
    ensure!(
        !terms.is_empty() && terms.iter().any(|byte| !byte.is_ascii_whitespace()),
        "SOURCE_TERMS_REQUIRED"
    );
    ensure!(
        (1..=MAX_ROWS).contains(&manifest.record_count)
            && manifest.observation_status == "OBSERVED"
            && (1..=128).contains(&manifest.requests.len())
            && manifest.requests.len() == manifest.responses.len(),
        "SOURCE_EMPTY_OR_INCOMPLETE"
    );
    let published = clock(&manifest.created_at)?;
    ensure!(
        published
            <= u64::try_from(
                Utc::now()
                    .timestamp_nanos_opt()
                    .context("TIMESTAMP_RANGE")?
            )?,
        "SOURCE_PUBLICATION_FROM_FUTURE"
    );
    let selected = &manifest.selection;
    let step = selected.interval_seconds * 299;
    let mut cursor = selected.start_seconds;
    let mut previous_received = 0;
    let mut bytes_read = 0;
    let mut originals = BTreeMap::new();
    for (index, (request, response)) in manifest
        .requests
        .iter()
        .zip(&manifest.responses)
        .enumerate()
    {
        let end = cursor.saturating_add(step).min(selected.end_seconds);
        let expected = Request {
            url: format!("https://api.exchange.coinbase.com/products/{}/candles?start={}&end={}&granularity={}",
                selected.instrument, query_clock(cursor)?, query_clock(end)?, selected.interval_seconds),
            start_seconds: cursor, end_seconds: end, method: "GET".into(),
        };
        ensure!(
            cursor < end && *request == expected && response.request == expected,
            "SOURCE_REQUEST_CHANGED"
        );
        let started = clock(&response.observation.request_started_at)?;
        let received = clock(&response.observation.retrieved_at)?;
        ensure!(
            response.observation.status == 200
                && started >= nanos(selected.end_seconds)?
                && previous_received <= started
                && started <= received
                && received <= published,
            "SOURCE_OBSERVATION_ORDER"
        );
        ensure!(
            response
                .observation
                .headers
                .get("Content-Type")
                .is_some_and(|v| v
                    .split(';')
                    .next()
                    .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"))),
            "SOURCE_RESPONSE_TYPE"
        );
        let path = format!("raw/{index:04}.json");
        let bytes = verified_file(root, &response.file, &path, 4 * MIB)?;
        bytes_read += bytes.len() as u64;
        ensure!(
            bytes_read <= limits.max_response_total_bytes,
            "SOURCE_BYTE_LIMIT"
        );
        let source: Vec<[Box<RawValue>; 6]> = serde_json::from_slice(&bytes)?;
        ensure!(source.len() <= 300, "SOURCE_RESPONSE_ROW_LIMIT");
        let mut seen = BTreeSet::new();
        let mut selected_count = 0;
        for (row_index, row) in source.iter().enumerate() {
            let at: u64 = serde_json::from_str(row[0].get())?;
            ensure!(
                at.is_multiple_of(selected.interval_seconds) && seen.insert(at),
                "SOURCE_DUPLICATE_OR_MISALIGNED_CANDLE"
            );
            nanos(
                at.checked_add(selected.interval_seconds)
                    .context("TIMESTAMP_RANGE")?,
            )?;
            let values = [
                exact(row[3].get())?,
                exact(row[2].get())?,
                exact(row[1].get())?,
                exact(row[4].get())?,
                exact(row[5].get())?,
            ];
            valid_values(&values)?;
            if cursor <= at && at < end {
                ensure!(
                    originals
                        .insert(
                            at,
                            Original {
                                values,
                                received,
                                response: path.clone(),
                                index: row_index
                            }
                        )
                        .is_none(),
                    "SOURCE_OVERLAPPING_CANDLE"
                );
                selected_count += 1;
            }
        }
        ensure!(
            response.counts.source_rows == source.len()
                && response.counts.selected_rows == selected_count
                && response.counts.outside_request_window == source.len() - selected_count,
            "SOURCE_COUNT_MISMATCH"
        );
        previous_received = received;
        cursor = end;
    }
    ensure!(
        cursor == selected.end_seconds
            && originals.len() == manifest.record_count
            && bytes_read == manifest.raw_response_bytes,
        "SOURCE_ACQUISITION_INCOMPLETE"
    );
    Ok(originals)
}

fn definitions(bytes: &[u8], manifest: &Manifest) -> Result<Vec<InstrumentAny>> {
    let values: Vec<Value> = serde_json::from_slice(bytes)?;
    let originals: Vec<InstrumentAny> = serde_json::from_slice(bytes)?;
    ensure!(
        serde_json::to_value(&originals)? == Value::Array(values.clone()),
        "ORIGINAL_NATIVE_DEFINITIONS_REQUIRED"
    );
    let chains = domain::catalogs::instrument_versions(&values)?;
    ensure!(chains.len() == 1, "ONE_SOURCE_PRODUCT_REQUIRED");
    let (base, quote) = selection(manifest)?;
    for instrument in &originals {
        let InstrumentAny::CurrencyPair(pair) = instrument else {
            anyhow::bail!("SPOT_CURRENCY_PAIR_REQUIRED")
        };
        ensure!(
            instrument.venue().as_str() == "COINBASE"
                && instrument.raw_symbol().as_str() == manifest.selection.instrument
                && pair.base_currency.code.as_str() == base
                && pair.quote_currency.code.as_str() == quote
                && pair.multiplier == Quantity::from("1")
                && pair.tick_scheme.is_none(),
            "SOURCE_INSTRUMENT_MISMATCH"
        );
        ensure!(
            pair.price_increment.precision == pair.price_precision
                && pair.size_increment.precision == pair.size_precision
                && pair.price_increment.as_decimal() > Decimal::ZERO
                && pair.size_increment.as_decimal() > Decimal::ZERO,
            "SOURCE_INSTRUMENT_PRECISION"
        );
    }
    Ok(originals)
}

fn native_bars(
    bytes: &[u8],
    manifest: &Manifest,
    originals: &BTreeMap<u64, Original>,
    instruments: &[InstrumentAny],
) -> Result<Vec<Bar>> {
    let interval = manifest.selection.interval_seconds;
    let (step, unit) = if interval.is_multiple_of(86400) {
        (interval / 86400, "DAY")
    } else if interval.is_multiple_of(3600) {
        (interval / 3600, "HOUR")
    } else {
        (interval / 60, "MINUTE")
    };
    let kind = BarType::from_str(&format!(
        "{}-{step}-{unit}-LAST-EXTERNAL",
        instruments[0].id()
    ))?;
    let mut bars = Vec::new();
    let mut previous = None;
    for line in bytes.split(|c| *c == b'\n').filter(|line| !line.is_empty()) {
        ensure!(bars.len() < MAX_ROWS, "SOURCE_ROW_LIMIT");
        let row: Row = serde_json::from_slice(line)?;
        let original = originals
            .get(&row.selection_time_seconds)
            .context("UNSOURCED_CANDLE")?;
        ensure!(
            previous.is_none_or(|value| value < row.selection_time_seconds),
            "SOURCE_RECORD_ORDER"
        );
        previous = Some(row.selection_time_seconds);
        let event = row
            .selection_time_seconds
            .checked_add(interval)
            .context("TIMESTAMP_RANGE")?;
        ensure!(
            row.event_time_seconds == event
                && row.instrument == manifest.selection.instrument
                && row.kind == "OHLCV_CANDLE"
                && row.interval_seconds == interval
                && row.historical_available_at.is_null()
                && row.source_response == original.response
                && row.source_row_index == original.index
                && clock(&row.observed_at)? == original.received,
            "SOURCE_ROW_LINEAGE"
        );
        let values = [
            exact(&row.open)?,
            exact(&row.high)?,
            exact(&row.low)?,
            exact(&row.close)?,
            exact(&row.volume)?,
        ];
        ensure!(values == original.values, "SOURCE_DERIVED_VALUE_MISMATCH");
        let instrument = instruments
            .iter()
            .rev()
            .find(|definition| definition.ts_init().as_u64() <= original.received)
            .context("INSTRUMENT_DEFINITION_FROM_FUTURE")?;
        ensure!(
            !instruments
                .iter()
                .skip(1)
                .any(|definition| definition.ts_init().as_u64() == original.received),
            "AMBIGUOUS_INSTRUMENT_UPDATE"
        );
        let price = |value| -> Result<Price> {
            let price = Price::from_decimal_dp(value, instrument.price_precision())?;
            ensure!(price.as_decimal() == value, "NATIVE_PRICE_PRECISION_LOSS");
            ensure!(
                instrument.try_normalize_price(price)? == price
                    && instrument.min_price().is_none_or(|bound| price >= bound)
                    && instrument.max_price().is_none_or(|bound| price <= bound),
                "NATIVE_PRICE_GRID_OR_BOUNDS"
            );
            Ok(price)
        };
        let volume = Quantity::from_decimal_dp(values[4], instrument.size_precision())?;
        ensure!(
            volume.as_decimal() == values[4] && instrument.try_normalize_qty(volume)? == volume,
            "NATIVE_VOLUME_PRECISION_LOSS"
        );
        let ts_event = nanos(event)?;
        ensure!(
            ts_event <= original.received,
            "CANDLE_EVENT_AFTER_OBSERVATION"
        );
        bars.push(Bar::new_checked(
            kind,
            price(values[0])?,
            price(values[1])?,
            price(values[2])?,
            price(values[3])?,
            volume,
            ts_event.into(),
            original.received.into(),
        )?);
    }
    ensure!(
        bars.len() == originals.len(),
        "SOURCE_RECORD_COUNT_MISMATCH"
    );
    Ok(bars)
}

fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    let partial = path.with_extension("json.partial");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    fs::hard_link(&partial, path)?;
    let _ = fs::remove_file(partial);
    Ok(())
}

pub fn run(args: &Arguments) -> Result<Value> {
    let source_bytes = read(&args.acquisition, MIB)?;
    let manifest: Manifest = serde_json::from_slice(&source_bytes)?;
    let source = args
        .acquisition
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let originals = original_rows(&manifest, source)?;
    ensure!(
        source_bytes.len() as u64
            + manifest.source_terms.file.size
            + manifest.raw_response_bytes
            + manifest.records.size
            <= 128 * MIB,
        "SOURCE_OUTPUT_LIMIT"
    );
    let definition_bytes = read(&args.instruments, MIB)?;
    let instruments = definitions(&definition_bytes, &manifest)?;
    let records = verified_file(source, &manifest.records, "records.jsonl", 128 * MIB)?;
    let bars = native_bars(&records, &manifest, &originals, &instruments)?;
    let cutoff = bars.last().context("SOURCE_EMPTY")?.ts_init;
    ensure!(
        instruments
            .iter()
            .all(|definition| definition.ts_init() <= cutoff),
        "UNOBSERVED_FUTURE_DEFINITION"
    );
    let source = source.canonicalize()?;
    let parent = args
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    no_symlinks(parent)?;
    let output = parent.canonicalize()?.join(
        args.output
            .file_name()
            .context("OUTPUT_DIRECTORY_REQUIRED")?,
    );
    ensure!(
        !output.starts_with(&source)
            && !args
                .output
                .components()
                .any(|part| matches!(part, Component::ParentDir)),
        "OUTPUT_OVERLAPS_SOURCE"
    );
    fs::create_dir(&output).context("OUTPUT_MUST_BE_NEW")?;
    let evidence = json!({"schema_version":1, "source_acquisition_path":args.acquisition.canonicalize()?,
        "acquisition_sha256":digest(&source_bytes), "acquisition":serde_json::from_slice::<Value>(&source_bytes)?,
        "instrument_definitions_sha256":digest(&definition_bytes),
        "instrument_definitions":serde_json::from_slice::<Value>(&definition_bytes)?});
    publish(
        &output.join("source-evidence.json"),
        &serde_json::to_vec_pretty(&evidence)?,
    )?;
    let root = output.join("catalog");
    fs::create_dir(&root)?;
    let mut native = super::native(&root)?;
    native.write_instruments(instruments.clone())?;
    native.write_to_parquet(&bars, None, None, None)?;
    let readback = native
        .query::<Bar>(
            Some(vec![bars[0].bar_type.to_string()]),
            None,
            None,
            None,
            None,
            true,
        )?
        .map(|row| match row? {
            Data::Bar(bar) => Ok(bar),
            _ => anyhow::bail!("NATIVE_RECORD_TYPE"),
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(readback == bars, "NATIVE_BAR_READBACK_MISMATCH");
    let readback_definitions =
        native.instruments(Some(&[instruments[0].id().to_string()]), None, Some(cutoff))?;
    ensure!(
        serde_json::to_value(readback_definitions)? == serde_json::to_value(&instruments)?,
        "NATIVE_DEFINITION_READBACK_MISMATCH"
    );
    // A direct native invocation has the same frozen-input obligation as the
    // operator wrapper. Recheck originals after native writes/readback.
    ensure!(
        read(&args.acquisition, MIB)? == source_bytes
            && read(&args.instruments, MIB)? == definition_bytes,
        "SOURCE_INPUT_CHANGED_DURING_IMPORT"
    );
    verified_file(
        &source,
        &manifest.source_terms.file,
        "source-terms.bin",
        MIB,
    )?;
    verified_file(&source, &manifest.records, "records.jsonl", 128 * MIB)?;
    for (index, response) in manifest.responses.iter().enumerate() {
        verified_file(
            &source,
            &response.file,
            &format!("raw/{index:04}.json"),
            4 * MIB,
        )?;
    }
    let counter = |value| contracts::DbCounter::new(value).map_err(anyhow::Error::msg);
    let native_selection = contracts::science::NativeBarSelectionV1 {
        schema_version: contracts::SchemaV1,
        bar_types: vec![bars[0].bar_type.to_string()],
        event_start_ns: counter(bars[0].ts_event.as_u64())?,
        event_end_ns: counter(
            bars.last()
                .context("SOURCE_EMPTY")?
                .ts_event
                .as_u64()
                .checked_add(nanos(manifest.selection.interval_seconds)?)
                .context("TIMESTAMP_RANGE")?,
        )?,
        decision_cutoff_ns: counter(cutoff.as_u64())?,
        maximum_rows: u32::try_from(bars.len())?,
    };
    let report = json!({"schema_version":1, "native_version":"0.63.0", "source_provider":PROVIDER,
    "source_record_kind":"OHLCV_CANDLE", "native_selection":native_selection, "acquisition_sha256":digest(&source_bytes),
    "instrument_definitions_sha256":digest(&definition_bytes), "instruments":1,
    "instrument_versions":instruments.len(), "bars":bars.len(), "catalog_relative_path":"catalog",
    "source_evidence_relative_path":"source-evidence.json", "native_readback_verified":true,
    "coverage":"UNPROVEN", "historical_availability":"UNVERIFIED", "registered_in_quazonai":false,
    "research_qualified":false, "limitations":[
        "Original REST retrieval times are retained, including equal timestamps within a response.",
        "Current or late instrument definitions are not evidence of historical parameters or availability.",
            "Batched REST history does not satisfy historical forecast/label availability contracts by native conversion alone.",
        "No data-use grant, PIT verification, catalog registration or scientific qualification is created.",
        "Retain original acquisition/raw/terms files; detached source evidence stays outside research mounts."
    ]});
    publish(
        &output.join("import-report.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
