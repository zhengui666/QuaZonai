//! Public vendor schemas stop at this operator boundary. Scientific jobs stay offline.
use super::{NativeArchive, epoch_ns, historical_instrument};
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use nautilus_data::aggregation::BarBuilder;
use nautilus_model::{
    data::{Bar, BarType, BookOrder, OrderBookDelta, QuoteTick, TradeTick},
    enums::{AggressorSide, BookAction, OrderSide, RecordFlag},
    identifiers::TradeId,
    instruments::{Instrument, InstrumentAny},
    types::{Price, Quantity},
};
use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::{Field, Row},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Read,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

const EXCHANGES: [&str; 2] = [
    "0x4bfb41d5b3570defd03c39a9a4d8de6bd8b8982e",
    "0xc5d563a36ae78145c45a50134d48a1215220f80a",
];

#[path = "capture.rs"]
pub mod capture;

#[path = "v2.rs"]
mod v2;

#[path = "sii.rs"]
mod sii;

#[path = "chain.rs"]
pub mod chain;

#[path = "hf_selection.rs"]
mod hf_selection;

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Envio v1 OrderFilled; exclude exchange-counterparty taker summaries.
    MooseFills,
    /// SII-WANGZJ raw snake_case OrderFilled, with exact little-endian uint256 amounts.
    SiiOrderFilled,
    /// TimeSeventeen v2 normalized amounts, with exact six-decimal recovery.
    TimeSeventeenV2,
    /// Joseph3222 minute-END full-book snapshots, not a continuous event feed.
    JosephBooks,
}

#[derive(clap::Args)]
pub struct Arguments {
    /// snapshot.json emitted by runtimes/data/snapshot.py (fixed revision and hashes).
    #[arg(
        long,
        conflicts_with = "selection",
        required_unless_present = "selection"
    )]
    snapshot: Option<PathBuf>,
    /// selection.json emitted by hf-dataset; reads only already-cached selected Parquet.
    #[arg(
        long,
        conflicts_with = "snapshot",
        required_unless_present = "snapshot"
    )]
    selection: Option<PathBuf>,
    /// Optional original EVM evidence from runtimes/data/evm.py, for v2 corroboration.
    #[arg(long)]
    chain_evidence: Option<PathBuf>,
    #[arg(long, value_enum)]
    format: Format,
    /// Original native InstrumentAny JSON array; original observation times are preserved.
    #[arg(long)]
    instruments: PathBuf,
    #[arg(long)]
    start_seconds: u64,
    #[arg(long)]
    end_seconds: u64,
    /// Aggregate observed fills into full UTC-aligned intervals; never fill empty intervals.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=86400))]
    bar_seconds: Option<u32>,
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: contracts::SchemaV1,
    repository: String,
    revision: String,
    license: String,
    license_reference: String,
    retrieved_at: DateTime<Utc>,
    files: Vec<SourceFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceFile {
    path: String,
    size: u64,
    sha256: String,
    url: String,
}

#[derive(Default, Serialize)]
struct Quality {
    scanned_rows: u64,
    selected_rows: usize,
    exchange_summaries_excluded: usize,
    duplicate_rows: usize,
    self_trades: usize,
    rounded_trade_prices: usize,
    chain_corroborated_fills: usize,
    chain_recovered_amounts: usize,
    one_sided_books: usize,
    empty_books: usize,
}

// The CLI may display only this typed context, never arbitrary source errors.
#[derive(Debug)]
pub(super) struct SourceRowContext {
    stage: &'static str,
    relative: String,
    file_row: usize,
}

impl SourceRowContext {
    fn new(stage: &'static str, relative: &str, file_row: usize) -> Self {
        Self {
            stage,
            relative: relative.into(),
            file_row,
        }
    }
}

impl fmt::Display for SourceRowContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:file_row={}",
            self.stage,
            self.relative.escape_debug(),
            self.file_row
        )
    }
}

fn read_input_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    ensure!(file.metadata()?.is_file(), "INPUT_FILE_LIMIT");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&read_input_bytes(path)?)?)
}

fn digest(path: &Path) -> Result<String> {
    let mut input = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verified_files(snapshot: &Snapshot, root: &Path) -> Result<Vec<PathBuf>> {
    ensure!(
        snapshot.revision.len() == 40 && snapshot.revision.bytes().all(|c| c.is_ascii_hexdigit()),
        "SOURCE_REVISION_REQUIRED"
    );
    ensure!(
        !snapshot.repository.trim().is_empty()
            && !snapshot.license.trim().is_empty()
            && !snapshot.license_reference.trim().is_empty()
            && snapshot.retrieved_at <= Utc::now(),
        "SOURCE_PROVENANCE_REQUIRED"
    );
    ensure!(!snapshot.files.is_empty(), "SOURCE_FILE_LIMIT");
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();
    for source in &snapshot.files {
        let relative = Path::new(&source.path);
        ensure!(
            !source.path.is_empty()
                && !source.path.contains('\\')
                && relative
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
                && seen.insert(&source.path),
            "SOURCE_PATH_INVALID"
        );
        let mut path = root.to_path_buf();
        for part in relative.components() {
            path.push(part);
            ensure!(
                !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                "SOURCE_SYMLINK"
            );
        }
        let metadata = fs::metadata(&path)?;
        ensure!(
            metadata.is_file()
                && metadata.len() == source.size
                && source.sha256.len() == 64
                && digest(&path)? == source.sha256,
            "SOURCE_CHECKSUM_MISMATCH"
        );
        if path.extension().is_some_and(|x| x == "parquet") {
            files.push(path);
        }
    }
    ensure!(!files.is_empty(), "NO_PARQUET_FILES");
    Ok(files)
}

fn field<'a>(row: &'a Row, name: &str) -> Result<&'a Field> {
    row.get_column_iter()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, value)| value)
        .with_context(|| format!("SOURCE_COLUMN_REQUIRED:{name}"))
}

fn text<'a>(row: &'a Row, name: &str) -> Result<&'a str> {
    match field(row, name)? {
        Field::Str(value) => Ok(value),
        _ => bail!("SOURCE_STRING_REQUIRED:{name}"),
    }
}

fn seconds(row: &Row, name: &str) -> Result<u64> {
    match field(row, name)? {
        Field::Str(value) => Ok(value.parse()?),
        Field::Long(value) => Ok(u64::try_from(*value)?),
        Field::ULong(value) => Ok(*value),
        _ => bail!("SOURCE_SECONDS_REQUIRED:{name}"),
    }
}

fn amount(row: &Row, name: &str) -> Result<Decimal> {
    let value = text(row, name)?;
    ensure!(
        !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
        "SOURCE_RAW_AMOUNT_INVALID"
    );
    let result = Decimal::from_str(value)?;
    ensure!(result > Decimal::ZERO, "SOURCE_RAW_AMOUNT_NONPOSITIVE");
    Ok(result)
}

fn event_key(id: &str) -> Result<(u64, u64)> {
    let parts = id.split('_').collect::<Vec<_>>();
    ensure!(
        parts.len() == 3 && parts[0] == "137",
        "SOURCE_EVENT_ID_INVALID"
    );
    let key = (parts[1].parse::<u64>()?, parts[2].parse::<u64>()?);
    ensure!(
        id == format!("137_{}_{}", key.0, key.1),
        "SOURCE_EVENT_ID_INVALID"
    );
    Ok(key)
}

fn normalized_trade(
    instrument: &InstrumentAny,
    id: &str,
    at_seconds: u64,
    shares: Decimal,
    cash: Decimal,
    self_trade: bool,
    quality: &mut Quality,
) -> Result<TradeTick> {
    ensure!(
        shares > Decimal::ZERO && cash > Decimal::ZERO,
        "INVALID_BINARY_TRADE"
    );
    let ratio = cash.checked_div(shares).context("TRADE_PRICE_RANGE")?;
    ensure!(
        ratio > Decimal::ZERO && ratio <= Decimal::ONE,
        "INVALID_BINARY_TRADE"
    );
    let price = Price::from_decimal_dp(ratio, instrument.price_precision())?;
    let size = Quantity::from_decimal_dp(shares, instrument.size_precision())?;
    ensure!(
        price.as_decimal() > Decimal::ZERO && size.as_decimal() == shares,
        "SOURCE_PRECISION_LOSS"
    );
    quality.rounded_trade_prices += usize::from(price.as_decimal() != ratio);
    quality.self_trades += usize::from(self_trade);
    event_key(id)?;
    let at = epoch_ns(at_seconds)?;
    Ok(TradeTick::new(
        instrument.id(),
        price,
        size,
        AggressorSide::NoAggressor,
        TradeId::new(id),
        at,
        at,
    ))
}

fn instruments_from_bytes(
    bytes: &[u8],
) -> Result<(Vec<InstrumentAny>, BTreeMap<String, InstrumentAny>)> {
    // Preserve the original typed parser, including duplicate-field rejection.
    let instruments: Vec<InstrumentAny> = serde_json::from_slice(bytes)?;
    ensure!(!instruments.is_empty(), "INSTRUMENT_LIMIT");
    let mut by_token = BTreeMap::new();
    for instrument in &instruments {
        historical_instrument(instrument)?;
        let token = instrument.raw_symbol().to_string();
        ensure!(
            !token.is_empty()
                && token.bytes().all(|b| b.is_ascii_digit())
                && instrument
                    .id()
                    .symbol
                    .as_str()
                    .rsplit_once('-')
                    .is_some_and(|(condition, suffix)| !condition.is_empty() && suffix == token)
                && by_token.insert(token, instrument.clone()).is_none(),
            "INSTRUMENT_TOKEN_INVALID"
        );
    }
    Ok((instruments, by_token))
}

// Shared by the existing chain module through use super::*; keep its contract.
fn load_instruments(path: &Path) -> Result<(Vec<InstrumentAny>, BTreeMap<String, InstrumentAny>)> {
    instruments_from_bytes(&read_input_bytes(path)?)
}

fn load_instruments_with_definitions(
    path: &Path,
) -> Result<(
    Vec<InstrumentAny>,
    BTreeMap<String, InstrumentAny>,
    serde_json::Value,
)> {
    let bytes = read_input_bytes(path)?;
    let (instruments, by_token) = instruments_from_bytes(&bytes)?;
    let definitions: serde_json::Value = serde_json::from_slice(&bytes)?;
    Ok((instruments, by_token, definitions))
}

fn fill(
    row: &Row,
    instruments: &BTreeMap<String, InstrumentAny>,
    quality: &mut Quality,
) -> Result<Option<TradeTick>> {
    let maker_asset = text(row, "makerAssetId")?;
    let taker_asset = text(row, "takerAssetId")?;
    if !instruments.contains_key(maker_asset) && !instruments.contains_key(taker_asset) {
        return Ok(None);
    }
    ensure!(
        (maker_asset == "0") != (taker_asset == "0"),
        "SOURCE_CASH_LEG_REQUIRED"
    );
    let taker = text(row, "taker")?;
    if EXCHANGES
        .iter()
        .any(|address| taker.eq_ignore_ascii_case(address))
    {
        quality.exchange_summaries_excluded += 1;
        return Ok(None);
    }
    let (asset, cash, tokens) = if maker_asset == "0" {
        (
            taker_asset,
            amount(row, "makerAmountFilled")?,
            amount(row, "takerAmountFilled")?,
        )
    } else {
        (
            maker_asset,
            amount(row, "takerAmountFilled")?,
            amount(row, "makerAmountFilled")?,
        )
    };
    let instrument = &instruments[asset];
    // v1 uses Polygon bridged USDC.e (0x2791...), not native USDC or v2 pUSD.
    ensure!(
        instrument.quote_currency().code.as_str() == "USDC.e",
        "V1_BRIDGED_USDC_INSTRUMENT_REQUIRED"
    );
    let ratio = cash.checked_div(tokens).context("TRADE_PRICE_RANGE")?;
    ensure!(
        ratio > Decimal::ZERO && ratio <= Decimal::ONE,
        "INVALID_BINARY_TRADE"
    );
    let price = Price::from_decimal_dp(ratio, instrument.price_precision())?;
    let shares = tokens
        .checked_div(Decimal::from(1_000_000))
        .context("TRADE_SIZE_RANGE")?;
    let size = Quantity::from_decimal_dp(shares, instrument.size_precision())?;
    ensure!(
        price.as_decimal() > Decimal::ZERO && size.as_decimal() == shares,
        "SOURCE_PRECISION_LOSS"
    );
    quality.rounded_trade_prices += usize::from(price.as_decimal() != ratio);
    quality.self_trades += usize::from(text(row, "maker")?.eq_ignore_ascii_case(taker));
    let id = text(row, "id")?;
    event_key(id)?;
    let at = epoch_ns(seconds(row, "timestamp")?)?;
    Ok(Some(TradeTick::new(
        instrument.id(),
        price,
        size,
        AggressorSide::NoAggressor,
        TradeId::new(id),
        at,
        at,
    )))
}

fn book(
    row: &Row,
    instrument: &InstrumentAny,
    archive: &mut NativeArchive,
    quality: &mut Quality,
) -> Result<()> {
    let condition = text(row, "condition_id")?;
    ensure!(
        instrument.id().to_string()
            == format!("{condition}-{}.POLYMARKET", instrument.raw_symbol()),
        "SOURCE_CONDITION_MISMATCH"
    );
    let minute = seconds(row, "minute_ts")?;
    ensure!(minute.is_multiple_of(60), "SOURCE_MINUTE_ALIGNMENT");
    // Source minute_ts labels the BEGINNING; the state contains events through minute END.
    let at = epoch_ns(minute.checked_add(60).context("TIMESTAMP_RANGE")?)?;
    let mut sides = Vec::new();
    for (name, side) in [
        ("bids_json", OrderSide::Buy),
        ("asks_json", OrderSide::Sell),
    ] {
        let levels: Vec<[Decimal; 2]> = serde_json::from_str(text(row, name)?)?;
        let mut native = Vec::new();
        for [price, size] in levels {
            ensure!(
                (Decimal::ZERO..=Decimal::ONE).contains(&price) && size > Decimal::ZERO,
                "SOURCE_BOOK_LEVEL_INVALID"
            );
            let p = Price::from_decimal_dp(price, instrument.price_precision())?;
            let q = Quantity::from_decimal_dp(size, instrument.size_precision())?;
            ensure!(
                p.as_decimal() == price && q.as_decimal() == size,
                "SOURCE_PRECISION_LOSS"
            );
            if let Some((previous, _)) = native.last() {
                ensure!(
                    if side == OrderSide::Buy {
                        previous > &p
                    } else {
                        previous < &p
                    },
                    "SOURCE_BOOK_ORDER_INVALID"
                );
            }
            native.push((p, q));
        }
        sides.push(native);
    }
    archive
        .deltas
        .push(OrderBookDelta::clear(instrument.id(), 0, at, at));
    for (levels, side) in sides.iter().zip([OrderSide::Buy, OrderSide::Sell]) {
        for (p, q) in levels {
            archive.deltas.push(OrderBookDelta::new(
                instrument.id(),
                BookAction::Add,
                BookOrder::new(side, *p, *q, 0),
                RecordFlag::F_SNAPSHOT as u8,
                0,
                at,
                at,
            ));
        }
    }
    archive.deltas.last_mut().context("BOOK_EMPTY")?.flags |= RecordFlag::F_LAST as u8;
    match (sides[0].first(), sides[1].first()) {
        (Some((bid, bid_size)), Some((ask, ask_size))) => {
            ensure!(bid <= ask, "SOURCE_BOOK_CROSSED");
            archive.quotes.push(QuoteTick::new(
                instrument.id(),
                *bid,
                *ask,
                *bid_size,
                *ask_size,
                at,
                at,
            ));
        }
        (None, None) => quality.empty_books += 1,
        _ => quality.one_sided_books += 1,
    }
    Ok(())
}

fn aggregate(archive: &mut NativeArchive, interval: u32) -> Result<()> {
    let (divisor, unit) = [
        (86400, "DAY"),
        (3600, "HOUR"),
        (60, "MINUTE"),
        (1, "SECOND"),
    ]
    .into_iter()
    .find(|(divisor, _)| interval.is_multiple_of(*divisor))
    .context("BAR_INTERVAL")?;
    let step = interval / divisor;
    for trades in archive
        .trades
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
    {
        let instrument = archive
            .instruments
            .iter()
            .find(|i| i.id() == trades[0].instrument_id)
            .context("UNMAPPED_TRADE")?;
        let internal =
            BarType::from_str(&format!("{}-{step}-{unit}-LAST-INTERNAL", instrument.id()))?;
        let external =
            BarType::from_str(&format!("{}-{step}-{unit}-LAST-EXTERNAL", instrument.id()))?;
        let mut builder = BarBuilder::new(
            internal,
            instrument.price_precision(),
            instrument.size_precision(),
        );
        let interval_ns = u64::from(interval) * 1_000_000_000;
        // Iterate only nonempty buckets. No native carry-forward/zero-volume bars.
        for bucket in trades
            .chunk_by(|a, b| a.ts_event.as_u64() / interval_ns == b.ts_event.as_u64() / interval_ns)
        {
            let mut volume = Quantity::zero(instrument.size_precision());
            for trade in bucket {
                volume = volume.checked_add(trade.size).context("BAR_VOLUME_RANGE")?;
                builder.update(trade.price, trade.size, trade.ts_event);
            }
            let end = (bucket[0].ts_event.as_u64() / interval_ns + 1)
                .checked_mul(interval_ns)
                .context("BAR_TIMESTAMP_RANGE")?;
            let available = bucket
                .iter()
                .map(|t| t.ts_init.as_u64())
                .max()
                .unwrap()
                .max(end);
            let bar = builder.build(end.into(), available.into());
            archive.bars.push(Bar {
                bar_type: external,
                ..bar
            });
        }
    }
    Ok(())
}

pub fn prepare(args: &Arguments) -> Result<NativeArchive> {
    ensure!(
        args.start_seconds < args.end_seconds
            && args.end_seconds <= u64::try_from(Utc::now().timestamp())?,
        "INVALID_HISTORY_WINDOW"
    );
    if let Some(interval) = args.bar_seconds {
        ensure!(
            matches!(
                args.format,
                Format::MooseFills | Format::TimeSeventeenV2 | Format::SiiOrderFilled
            ) && interval > 0
                && interval <= 86400
                && args.start_seconds.is_multiple_of(u64::from(interval))
                && args.end_seconds.is_multiple_of(u64::from(interval)),
            "FULL_BAR_INTERVALS_REQUIRED"
        );
    }
    // Keep the legacy checksum/provenance path intact. The explicit selection
    // path checks ordinary cached bytes and format, never fabricates a snapshot.
    let (snapshot, selection, root, files, file_states, repository, revision, retrieved_at) =
        match (&args.snapshot, &args.selection) {
            (Some(path), None) => {
                let snapshot: Snapshot = read_json(path)?;
                let root = path.parent().context("SNAPSHOT_ROOT")?.to_path_buf();
                let files = verified_files(&snapshot, &root)?;
                let repository = snapshot.repository.clone();
                let revision = snapshot.revision.clone();
                let retrieved_at = snapshot.retrieved_at;
                (
                    Some(snapshot),
                    None,
                    root,
                    files,
                    None,
                    repository,
                    revision,
                    retrieved_at,
                )
            }
            (None, Some(path)) => {
                let selection = hf_selection::load(path, args.start_seconds, args.end_seconds)?;
                (
                    None,
                    Some(selection.manifest),
                    selection.files_root,
                    selection.files,
                    Some(selection.file_states),
                    selection.repository,
                    selection.revision,
                    selection.retrieved_at,
                )
            }
            _ => bail!("EXACTLY_ONE_ARCHIVE_SOURCE_REQUIRED"),
        };
    let (instruments, by_token, instrument_definitions) = if snapshot.is_some() {
        let (instruments, by_token) = load_instruments(&args.instruments)?;
        (instruments, by_token, serde_json::Value::Null)
    } else {
        load_instruments_with_definitions(&args.instruments)?
    };
    ensure!(
        args.chain_evidence.is_none() || args.format == Format::TimeSeventeenV2,
        "CHAIN_EVIDENCE_REQUIRES_V2"
    );
    let chain = args
        .chain_evidence
        .as_deref()
        .map(chain::Evidence::load)
        .transpose()?;
    if let Some(evidence) = &chain {
        evidence.require_v2_exchanges()?;
    }
    let mut archive = NativeArchive {
        schema_version: contracts::SchemaV1,
        source_reference: format!(
            "https://huggingface.co/datasets/{}/tree/{}",
            repository, revision
        ),
        source_observed_at: chain
            .as_ref()
            .map_or(retrieved_at, |c| retrieved_at.max(c.observed_at())),
        source_metadata: serde_json::Value::Null,
        instruments,
        trades: Vec::new(),
        quotes: Vec::new(),
        deltas: Vec::new(),
        bars: Vec::new(),
        closes: Vec::new(),
    };
    let mut quality = Quality::default();
    let mut identities = BTreeMap::new();
    let mut sii_events = BTreeMap::new();
    let mut sii_transaction_logs = BTreeMap::new();
    for file in &files {
        let relative = file.strip_prefix(&root)?.to_string_lossy();
        let prefix = match args.format {
            Format::MooseFills => "order_filled/",
            Format::SiiOrderFilled => "orderfilled",
            Format::TimeSeventeenV2 => "OrderFilled/",
            Format::JosephBooks => "orderbook_1min/",
        };
        if snapshot.is_some() && !relative.starts_with(prefix) {
            continue;
        }
        if let Some(states) = &file_states {
            ensure!(
                states.get(file) == Some(&hf_selection::file_state(file)?),
                "SELECTION_FILE_CHANGED"
            );
        }
        let reader = SerializedFileReader::new(fs::File::open(file)?)?;
        for (row_index, row) in reader.get_row_iter(None)?.enumerate() {
            let file_row = row_index + 1;
            let row = row
                .with_context(|| SourceRowContext::new("SOURCE_ROW_DECODE", &relative, file_row))?;
            quality.scanned_rows += 1;
            let label = match args.format {
                Format::MooseFills | Format::TimeSeventeenV2 | Format::SiiOrderFilled => {
                    "timestamp"
                }
                Format::JosephBooks => "minute_ts",
            };
            let label_at = seconds(&row, label)
                .with_context(|| SourceRowContext::new("SOURCE_TIMESTAMP", &relative, file_row))?;
            let at = match args.format {
                Format::MooseFills | Format::TimeSeventeenV2 | Format::SiiOrderFilled => label_at,
                Format::JosephBooks => label_at
                    .checked_add(60)
                    .context("TIMESTAMP_RANGE")
                    .with_context(|| {
                        SourceRowContext::new("SOURCE_TIMESTAMP", &relative, file_row)
                    })?,
            };
            if at < args.start_seconds || at >= args.end_seconds {
                continue;
            }
            let (identity, signature) = match args.format {
                Format::MooseFills | Format::TimeSeventeenV2 | Format::SiiOrderFilled => {
                    let mut row_quality = Quality::default();
                    let mut sii_event = None;
                    let parsed = match args.format {
                        Format::TimeSeventeenV2 => {
                            v2::fill(&row, &by_token, &mut row_quality, chain.as_ref())
                        }
                        Format::SiiOrderFilled => {
                            sii::fill(&row, &by_token, &mut row_quality).map(|parsed| {
                                parsed.map(|(trade, event)| {
                                    sii_event = Some(event);
                                    trade
                                })
                            })
                        }
                        _ => fill(&row, &by_token, &mut row_quality),
                    }
                    .with_context(|| SourceRowContext::new("SOURCE_FILL", &relative, file_row))?;
                    let Some(trade) = parsed else {
                        quality.exchange_summaries_excluded +=
                            row_quality.exchange_summaries_excluded;
                        continue;
                    };
                    let identity = trade.trade_id.to_string();
                    let signature = format!("{row:?}");
                    if let Some(previous) = identities.get(&identity) {
                        ensure!(previous == &signature, "CONFLICTING_SOURCE_EVENT");
                        quality.duplicate_rows += 1;
                        continue;
                    }
                    if let Some(event) = sii_event {
                        let key = (event.transaction_hash.clone(), event.log_index);
                        ensure!(
                            sii_transaction_logs.insert(key, identity.clone()).is_none(),
                            "CONFLICTING_SII_TRANSACTION_LOG"
                        );
                        sii_events.insert(identity.clone(), event);
                    }
                    quality.rounded_trade_prices += row_quality.rounded_trade_prices;
                    quality.self_trades += row_quality.self_trades;
                    quality.chain_corroborated_fills += row_quality.chain_corroborated_fills;
                    quality.chain_recovered_amounts += row_quality.chain_recovered_amounts;
                    archive.trades.push(trade);
                    (identity, signature)
                }
                Format::JosephBooks => {
                    let asset = text(&row, "asset_id")?;
                    let Some(instrument) = by_token.get(asset) else {
                        continue;
                    };
                    let identity = format!("{asset}_{at}");
                    let signature = format!("{row:?}");
                    if let Some(previous) = identities.get(&identity) {
                        ensure!(previous == &signature, "CONFLICTING_SOURCE_EVENT");
                        quality.duplicate_rows += 1;
                        continue;
                    }
                    book(&row, instrument, &mut archive, &mut quality).with_context(|| {
                        SourceRowContext::new("SOURCE_BOOK", &relative, file_row)
                    })?;
                    (identity, signature)
                }
            };
            quality.selected_rows += 1;
            identities.insert(identity, signature);
        }
        if let Some(states) = &file_states {
            ensure!(
                states.get(file) == Some(&hf_selection::file_state(file)?),
                "SELECTION_FILE_CHANGED"
            );
        }
    }
    // A later file's scan can overlap a change to an earlier cached source.
    // Recheck the complete selected set before constructing a publication.
    if let Some(states) = &file_states {
        for (file, state) in states {
            ensure!(
                state == &hf_selection::file_state(file)?,
                "SELECTION_FILE_CHANGED"
            );
        }
    }
    ensure!(quality.selected_rows > 0, "EMPTY_ARCHIVE_SELECTION");
    if let Some(evidence) = &chain {
        evidence.check_archive_selection(
            &by_token,
            args.start_seconds,
            args.end_seconds,
            &identities,
        )?;
    }
    // Canonical chain order within a block, never lexicographic log-index order.
    archive.trades.sort_by_key(|t| {
        (
            t.instrument_id,
            t.ts_event,
            event_key(t.trade_id.as_str()).expect("validated ID"),
        )
    });
    if let Some(interval) = args.bar_seconds {
        aggregate(&mut archive, interval)?;
    }
    archive.source_metadata = serde_json::json!({
        "format": args.format,
        "chain_evidence": chain.as_ref().map(chain::Evidence::metadata),
        "selection": {"start_seconds": args.start_seconds, "end_seconds": args.end_seconds, "bar_seconds": args.bar_seconds},
        "quality": quality,
        "availability": "UNVERIFIED: block time or minute end is an event-time proxy, not measured reception/finality latency.",
        "coverage": "Only selected files/assets/window; no provider completeness claim is verified.",
        "settlement": "Not inferred from state tables, redemption times, end dates or last prices.",
        "book_semantics": "Minute-end snapshots, sequence unknown (0); missing or one-sided books never create tradable zero quotes.",
        "trade_semantics": match args.format {
            Format::SiiOrderFilled => "SII raw v1/v2 OrderFilled. Original 32-byte little-endian uint256 amounts; collateral follows the explicit contract family. Original transaction/log identity retained and checked independently of native chain/block/log IDs. Exchange-counterparty summaries excluded, self trades retained, aggressor unknown. Raw per-fill fees do not establish a historical fee schedule. Vendor timestamps remain unverified event-time proxies.",
            Format::TimeSeventeenV2 => "TimeSeventeen v2 normalized float64 amounts, pUSD collateral. Optional matching raw Polygon logs supply original integers, contract and transaction identity; otherwise only unique exact six-decimal recovery is accepted. Exchange-counterparty summaries excluded; self trades retained; aggressor unknown; price rounding counted.",
            _ => "v1 USDC.e cash/token OrderFilled, excluding exchange-counterparty summaries; self trades retained; aggressor unknown; price rounding counted.",
        },
    });
    if args.format == Format::SiiOrderFilled {
        archive.source_metadata["sii_event_identities"] =
            serde_json::to_value(sii_events.into_values().collect::<Vec<_>>())?;
    }
    if let Some(snapshot) = snapshot {
        archive.source_metadata["snapshot"] = serde_json::to_value(snapshot)?;
        archive.source_metadata["instruments_sha256"] = digest(&args.instruments)?.into();
    } else {
        archive.source_metadata["selection_manifest"] = selection.context("SELECTION_REQUIRED")?;
        archive.source_metadata["clock_basis"] =
            "REQUEST_SELECTION_AT_NOT_HISTORICAL_AVAILABILITY".into();
        archive.source_metadata["instruments_bytes"] =
            fs::metadata(&args.instruments)?.len().into();
        archive.source_metadata["instruments_definitions"] = instrument_definitions;
    }
    super::validate(&archive)?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        enums::AssetClass,
        identifiers::{InstrumentId, Symbol},
        instruments::BinaryOption,
        types::Currency,
    };
    use parquet::{
        data_type::ByteArrayType, file::writer::SerializedFileWriter,
        schema::parser::parse_message_type,
    };
    use std::sync::Arc;

    pub(super) fn instrument() -> InstrumentAny {
        InstrumentAny::BinaryOption(
            BinaryOption::builder()
                .instrument_id(InstrumentId::from_str("condition-123.POLYMARKET").unwrap())
                .raw_symbol(Symbol::new("123"))
                .asset_class(AssetClass::Alternative)
                .currency(Currency::from_str("USDC.e").unwrap())
                .activation_ns(0_u64.into())
                .expiration_ns(1_000_000_000_000_u64.into())
                .price_precision(4)
                .size_precision(6)
                .price_increment(Price::from("0.0001"))
                .size_increment(Quantity::from("0.000001"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        )
    }

    fn row(id: &str, at: u64, cash: &str) -> Row {
        Row::new(
            [
                ("id", id.to_owned()),
                ("timestamp", at.to_string()),
                ("makerAssetId", "0".into()),
                ("takerAssetId", "123".into()),
                ("makerAmountFilled", cash.into()),
                ("takerAmountFilled", "1000000".into()),
                ("maker", "alice".into()),
                ("taker", "bob".into()),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), Field::Str(value)))
            .collect(),
        )
    }

    fn write_parquet(path: &Path, rows: &[Row]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let fields = rows[0]
            .get_column_iter()
            .map(|(key, _)| format!("REQUIRED BINARY {key} (UTF8);"))
            .collect::<Vec<_>>()
            .join(" ");
        let schema =
            Arc::new(parse_message_type(&format!("message source {{ {fields} }}")).unwrap());
        let mut writer =
            SerializedFileWriter::new(fs::File::create(path).unwrap(), schema, Default::default())
                .unwrap();
        let mut group = writer.next_row_group().unwrap();
        for (key, _) in rows[0].get_column_iter() {
            let values = rows
                .iter()
                .map(|r| text(r, key).unwrap().into())
                .collect::<Vec<_>>();
            let mut column = group.next_column().unwrap().unwrap();
            column
                .typed::<ByteArrayType>()
                .write_batch(&values, None, None)
                .unwrap();
            column.close().unwrap();
        }
        group.close().unwrap();
        writer.close().unwrap();
    }

    fn fixture(root: &Path, rows: &[Row]) -> Arguments {
        fixture_for(root, rows, Format::MooseFills)
    }

    fn fixture_for(root: &Path, rows: &[Row], format: Format) -> Arguments {
        let relative = match format {
            Format::MooseFills => "order_filled/year=2022/month=11.parquet",
            Format::SiiOrderFilled => "orderfilled.parquet",
            Format::TimeSeventeenV2 => "OrderFilled/2026-08-09.parquet",
            Format::JosephBooks => "orderbook_1min/date=2026-05-01/data_0.parquet",
        };
        let path = root.join(relative);
        write_parquet(&path, rows);
        let snapshot = Snapshot {
            schema_version: contracts::SchemaV1,
            repository: "fixture/public-history".into(),
            revision: "a".repeat(40),
            license: "FIXTURE".into(),
            license_reference: "FIXTURE".into(),
            retrieved_at: Utc::now(),
            files: vec![SourceFile {
                path: relative.into(),
                size: fs::metadata(&path).unwrap().len(),
                sha256: digest(&path).unwrap(),
                url: "FIXTURE".into(),
            }],
        };
        let manifest = root.join("snapshot.json");
        fs::write(&manifest, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let definitions = root.join("instruments.json");
        fs::write(
            &definitions,
            serde_json::to_vec(&vec![instrument()]).unwrap(),
        )
        .unwrap();
        Arguments {
            snapshot: Some(manifest),
            selection: None,
            chain_evidence: None,
            instruments: definitions,
            format,
            start_seconds: 0,
            end_seconds: 240,
            bar_seconds: if format == Format::MooseFills {
                Some(60)
            } else {
                None
            },
            output: root.join("native"),
        }
    }

    fn selection_fixture(root: &Path, rows: &[Row]) -> Arguments {
        // Some platforms place their temporary directory beneath a symlink.
        // The handoff's absolute root itself must have no symlink ancestors.
        let root = root.canonicalize().unwrap();
        let revision = "a".repeat(40);
        let retrieved_at = Utc::now();
        let relative = "data/selected fill +\u{e9}.parquet";
        let cache_root = root.join("cache");
        let cached = cache_root.join("files").join(relative);
        write_parquet(&cached, rows);
        let size = fs::metadata(&cached).unwrap().len();
        let url = format!(
            "https://huggingface.co/datasets/fixture/public-history/resolve/{}/data/selected%20fill%20%2B%C3%A9.parquet",
            revision
        );
        let plan_file = serde_json::json!({
            "path": relative, "size": size, "url": url, "format": "parquet"
        });
        let mut record = plan_file.clone();
        record["local_path"] = cached.to_str().unwrap().into();
        record["cached"] = true.into();
        record["resumed_bytes"] = 0.into();
        record["validation"] = "PARQUET_ENVELOPE".into();
        let selection = serde_json::json!({
            "schema": "qz.hf_selection/1",
            "plan": {
                "schema": "qz.hf_dataset_plan/1", "repository": "fixture/public-history",
                "requested_revision": "main", "revision": revision, "license": null,
                "partition_index": null,
                "request": {"includes": [relative], "markets": [], "start_date": null, "end_date": null},
                "selection_bounds": "[start_date,end_date)", "download_granularity": "FILE_PARTITION",
                "coverage": "NOT_ASSERTED", "max_bytes": 1_000_000,
                "total_bytes": size, "files": [plan_file]
            },
            "cache_root": cache_root.to_str().unwrap(), "files": [record],
            "retrieved_at": retrieved_at, "downloaded_bytes": 0, "cached_files": 1
        });
        let manifest = root.join("selection.json");
        fs::write(&manifest, serde_json::to_vec(&selection).unwrap()).unwrap();
        let definitions = root.join("instruments.json");
        fs::write(
            &definitions,
            serde_json::to_vec(&vec![instrument()]).unwrap(),
        )
        .unwrap();
        Arguments {
            snapshot: None,
            selection: Some(manifest),
            chain_evidence: None,
            instruments: definitions,
            format: Format::MooseFills,
            start_seconds: 0,
            end_seconds: 240,
            bar_seconds: Some(60),
            output: root.join("native"),
        }
    }

    fn alter_selection(args: &Arguments, alter: impl FnOnce(&mut serde_json::Value)) {
        let path = args.selection.as_ref().unwrap();
        let mut value: serde_json::Value = read_json(path).unwrap();
        alter(&mut value);
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn selection_fixture_with_second_file(root: &Path, rows: &[Row]) -> Arguments {
        let args = selection_fixture(
            root,
            &[
                row("137_1_1", 10, "420000"),
                row("137_1_2", 20, "420000"),
                row("137_1_3", 30, "420000"),
            ],
        );
        alter_selection(&args, |value| {
            let relative = "data/second.parquet";
            let cached = Path::new(value["cache_root"].as_str().unwrap())
                .join("files")
                .join(relative);
            write_parquet(&cached, rows);
            let size = fs::metadata(&cached).unwrap().len();
            let mut item = value["plan"]["files"][0].clone();
            item["path"] = relative.into();
            item["size"] = size.into();
            item["url"] = format!(
                "https://huggingface.co/datasets/fixture/public-history/resolve/{}/{relative}",
                "a".repeat(40)
            )
            .into();
            let mut record = value["files"][0].clone();
            record["path"] = item["path"].clone();
            record["size"] = item["size"].clone();
            record["url"] = item["url"].clone();
            record["local_path"] = cached.to_str().unwrap().into();
            value["plan"]["files"].as_array_mut().unwrap().push(item);
            value["files"].as_array_mut().unwrap().push(record);
            value["plan"]["request"]["includes"]
                .as_array_mut()
                .unwrap()
                .push(relative.into());
            value["plan"]["total_bytes"] =
                (value["plan"]["total_bytes"].as_u64().unwrap() + size).into();
            value["cached_files"] = 2.into();
        });
        args
    }

    fn indexed_selection(args: &Arguments) {
        // The market ID is declared by the fixture's indexed partition. Its
        // mapping to instrument tokens is absent and must never be invented.
        alter_selection(args, |value| {
            value["plan"]["partition_index"] = serde_json::json!({
                "path": "partitions.json", "size": 200,
                "url": format!("https://huggingface.co/datasets/fixture/public-history/resolve/{}/partitions.json", "a".repeat(40))
            });
            value["plan"]["request"] = serde_json::json!({
                "includes": [], "markets": ["indexed-market-without-token-map"],
                "start_date": "1970-01-01", "end_date": "1970-01-02"
            });
            let partition = serde_json::json!({
                "markets": ["indexed-market-without-token-map", "co-partitioned-market"],
                "start_date": "1970-01-01", "end_date": "1970-01-03", "format": "parquet"
            });
            value["plan"]["files"][0]["partition"] = partition.clone();
            value["files"][0]["partition"] = partition;
        });
    }

    #[test]
    fn archive_cli_requires_exactly_one_snapshot_or_selection() {
        use clap::Parser;
        let base = [
            "polymarket-history",
            "archive",
            "--format",
            "moose-fills",
            "--instruments",
            "instruments.json",
            "--start-seconds",
            "0",
            "--end-seconds",
            "60",
            "--output",
            "native",
        ];
        assert!(super::super::Arguments::try_parse_from(base).is_err());
        for option in ["--snapshot", "--selection"] {
            let mut argv = base.to_vec();
            argv.extend([option, "source.json"]);
            assert!(super::super::Arguments::try_parse_from(argv).is_ok());
        }
        let mut argv = base.to_vec();
        argv.extend([
            "--snapshot",
            "snapshot.json",
            "--selection",
            "selection.json",
        ]);
        assert!(super::super::Arguments::try_parse_from(argv).is_err());
    }

    #[test]
    fn hf_selection_invalid_amount_reports_second_file_row_and_preserves_cause() {
        let directory = tempfile::tempdir().unwrap();
        let args = selection_fixture_with_second_file(
            directory.path(),
            &[
                row("137_2_1", 70, "420000"),
                row("137_2_2", 80, "invalid-amount"),
            ],
        );
        let error = prepare(&args)
            .and_then(|archive| super::super::import(archive, &args.output))
            .unwrap_err();
        let message = format!("{error:#}");
        assert_eq!(
            super::super::failure_message(&error),
            "QZ_POLYMARKET_HISTORY_FAILED: SOURCE_FILL:data/second.parquet:file_row=2"
        );
        assert!(message.contains("data/second.parquet"));
        assert!(message.contains("file_row=2"));
        assert!(message.contains("SOURCE_RAW_AMOUNT_INVALID"));
        assert_eq!(error.root_cause().to_string(), "SOURCE_RAW_AMOUNT_INVALID");
        assert!(!message.contains(directory.path().to_str().unwrap()));
        assert!(!message.contains("makerAmountFilled"));
        assert!(!message.contains("420000"));
        assert!(!args.output.join("import-report.json").exists());
        assert!(!args.output.exists());
    }

    #[test]
    fn hf_selection_invalid_timestamp_reports_second_file_row_and_preserves_cause() {
        let directory = tempfile::tempdir().unwrap();
        let mut invalid = row("137_2_2", 80, "420000").into_columns();
        invalid
            .iter_mut()
            .find(|(k, _)| k == "timestamp")
            .unwrap()
            .1 = Field::Str("invalid-timestamp".into());
        let args = selection_fixture_with_second_file(
            directory.path(),
            &[row("137_2_1", 70, "420000"), Row::new(invalid)],
        );
        let error = prepare(&args)
            .and_then(|archive| super::super::import(archive, &args.output))
            .unwrap_err();
        let original = "invalid-timestamp".parse::<u64>().unwrap_err();
        let message = format!("{error:#}");
        assert_eq!(
            super::super::failure_message(&error),
            "QZ_POLYMARKET_HISTORY_FAILED: SOURCE_TIMESTAMP:data/second.parquet:file_row=2"
        );
        assert!(message.contains("data/second.parquet"));
        assert!(message.contains("file_row=2"));
        assert!(message.contains(&original.to_string()));
        assert_eq!(error.root_cause().to_string(), original.to_string());
        assert!(error.downcast_ref::<std::num::ParseIntError>().is_some());
        assert!(!message.contains(directory.path().to_str().unwrap()));
        assert!(!message.contains("makerAmountFilled"));
        assert!(!message.contains("420000"));
        assert!(!args.output.join("import-report.json").exists());
        assert!(!args.output.exists());
    }

    #[test]
    fn cli_row_context_escapes_controls_and_excludes_source_error_data() {
        let error = anyhow::anyhow!(
            "/private/cache/source.parquet maker=0x1111111111111111111111111111111111111111 amount=420000"
        )
        .context(SourceRowContext::new("SOURCE_FILL", "data/a\n\r\t.parquet", 2));
        assert_eq!(
            super::super::failure_message(&error),
            "QZ_POLYMARKET_HISTORY_FAILED: SOURCE_FILL:data/a\\n\\r\\t.parquet:file_row=2"
        );
        assert!(error.root_cause().to_string().contains("maker=0x1111"));
    }

    #[test]
    fn hf_selection_reads_real_cached_parquet_without_hashes_and_preserves_request_clock() {
        let directory = tempfile::tempdir().unwrap();
        let args = selection_fixture(
            directory.path(),
            &[
                row("137_1_10", 10, "500000"),
                row("137_1_2", 10, "420000"),
                row("137_2_1", 130, "600000"),
                row("137_3_1", 240, "900000"),
            ],
        );
        let manifest: serde_json::Value = read_json(args.selection.as_ref().unwrap()).unwrap();
        // Existing chain consumers destructure this shared loader as two values.
        let (instruments, by_token) = load_instruments(&args.instruments).unwrap();
        assert_eq!(instruments.len(), 1);
        assert!(by_token.contains_key("123"));
        // Only the selected cached source is consumed; an unrelated legacy-path
        // file neither supplies nor invalidates this handoff.
        assert!(!directory.path().join("snapshot.json").exists());
        let legacy = directory
            .path()
            .join("order_filled/year=2022/month=11.parquet");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"unused").unwrap();
        let archive = prepare(&args).unwrap();
        assert_eq!(archive.trades.len(), 3);
        assert_eq!(archive.bars.len(), 2);
        assert_eq!(archive.trades[0].trade_id.as_str(), "137_1_2");
        assert_eq!(archive.source_metadata["selection_manifest"], manifest);
        assert_eq!(archive.source_metadata["selection"]["end_seconds"], 240);
        assert_eq!(
            archive.source_metadata["clock_basis"],
            "REQUEST_SELECTION_AT_NOT_HISTORICAL_AVAILABILITY"
        );
        assert!(archive.source_metadata.get("snapshot").is_none());
        assert!(archive.source_metadata.get("instruments_sha256").is_none());
        assert_eq!(
            archive.source_metadata["instruments_bytes"],
            fs::metadata(&args.instruments).unwrap().len()
        );
        assert_eq!(
            archive.source_metadata["instruments_definitions"],
            read_json::<serde_json::Value>(&args.instruments).unwrap()
        );
        assert_eq!(
            archive.source_observed_at,
            serde_json::from_value::<DateTime<Utc>>(manifest["retrieved_at"].clone()).unwrap()
        );
        let report = super::super::import(archive, &args.output).unwrap();
        assert_eq!(report.trades, 3);
        assert_eq!(report.bars, 2);
        assert_eq!(report.coverage, "UNPROVEN");
        assert_eq!(report.historical_availability, "UNVERIFIED");
        assert!(!report.registered_in_quazonai);
        let receipt = serde_json::json!({
            "schema_version": 1,
            "native_version": super::super::NATIVE_VERSION,
            "source_reference": format!(
                "https://huggingface.co/datasets/fixture/public-history/tree/{}",
                "a".repeat(40)
            ),
            "source_observed_at": manifest["retrieved_at"],
            "imported_at": report.imported_at,
            "instruments": 1,
            "instrument_versions": 1,
            "trades": 3,
            "quotes": 0,
            "deltas": 0,
            "bars": 2,
            "closes": 0,
            "catalog_relative_path": "catalog",
            "coverage": "UNPROVEN",
            "historical_availability": "UNVERIFIED",
            "registered_in_quazonai": false,
            "limitations": [
                "Native serialization is not a coverage, historical fee, settlement or PIT verification.",
                "Current metadata retains its observation time; it is not backdated for historical research.",
                "Availability, ordering and truncation depend on the original source; see source-evidence.json.",
                "Book records are not claimed gap-free or replayable without separate snapshot/sequence validation.",
                "Missing intervals, prices and depth are not imputed. Source evidence is outside the native catalog mount."
            ]
        });
        assert_eq!(serde_json::to_value(&report).unwrap(), receipt);
        assert_eq!(
            read_json::<serde_json::Value>(&args.output.join("import-report.json")).unwrap(),
            receipt
        );
        let mut catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
            args.output.join("catalog").to_str().unwrap(),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let trades = catalog
            .query::<TradeTick>(None, None, None, None, None, true)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(trades.len(), 3);
        assert!(trades.iter().any(|data| matches!(
            data, nautilus_model::data::Data::Trade(trade)
                if trade.trade_id.as_str() == "137_1_2"
        )));
        assert_eq!(
            catalog
                .query::<Bar>(None, None, None, None, None, true)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn hf_index_dates_bound_native_half_open_rows_and_do_not_infer_market_token_mapping() {
        let directory = tempfile::tempdir().unwrap();
        let mut other = row("137_1_3", 30, "420000").into_columns();
        other
            .iter_mut()
            .find(|(k, _)| k == "takerAssetId")
            .unwrap()
            .1 = Field::Str("456".into());
        let mut args = selection_fixture(
            directory.path(),
            &[
                row("137_1_1", 0, "420000"),
                row("137_1_2", 60, "500000"),
                Row::new(other),
                row("137_2_1", 86_400, "900000"),
            ],
        );
        indexed_selection(&args);
        args.end_seconds = 86_400;
        let archive = prepare(&args).unwrap();
        assert_eq!(archive.trades.len(), 2);
        assert_eq!(archive.trades[0].ts_event.as_u64(), 0);
        assert_eq!(archive.trades[1].ts_event.as_u64(), 60_000_000_000);
        assert_eq!(
            archive.source_metadata["selection_manifest"]["plan"]["request"]["markets"][0],
            "indexed-market-without-token-map"
        );
        args.start_seconds = 60;
        args.end_seconds = 120;
        assert_eq!(prepare(&args).unwrap().trades.len(), 1);
        args.end_seconds = 86_460;
        assert!(
            prepare(&args)
                .unwrap_err()
                .to_string()
                .contains("SELECTION_WINDOW_OUTSIDE_REQUEST")
        );
        args.start_seconds = 0;
        args.end_seconds = 120;
        alter_selection(&args, |value| {
            value["plan"]["request"]["start_date"] = "1970-01-02".into();
            value["plan"]["request"]["end_date"] = "1970-01-03".into();
        });
        assert!(
            prepare(&args)
                .unwrap_err()
                .to_string()
                .contains("SELECTION_WINDOW_OUTSIDE_REQUEST")
        );
    }

    #[test]
    fn hf_selection_rejects_identity_size_format_and_record_changes_before_publication() {
        let cases: [(&str, &str); 9] = [
            ("schema", "SELECTION_SCHEMA_INVALID"),
            ("revision", "SELECTION_REVISION_REQUIRED"),
            ("url", "SELECTION_SOURCE_URL_INVALID"),
            ("traversal", "SELECTION_PATH_INVALID"),
            ("size", "SELECTION_BYTE_SIZE_MISMATCH"),
            ("format", "SELECTION_NATIVE_PARQUET_REQUIRED"),
            ("local_path", "SELECTION_LOCAL_PATH_INVALID"),
            ("record", "SELECTION_RECORD_DIFFERS_FROM_PLAN"),
            ("coverage", "SELECTION_SCOPE_INVALID"),
        ];
        for (case, expected) in cases {
            let directory = tempfile::tempdir().unwrap();
            let args = selection_fixture(directory.path(), &[row("137_1_1", 10, "420000")]);
            alter_selection(&args, |value| match case {
                "schema" => value["schema"] = "qz.hf_selection/2".into(),
                "revision" => value["plan"]["revision"] = "main".into(),
                "url" => {
                    value["plan"]["files"][0]["url"] = "https://huggingface.co/datasets/fixture/public-history/resolve/main/data.parquet".into();
                    value["files"][0]["url"] = value["plan"]["files"][0]["url"].clone();
                }
                "traversal" => value["plan"]["files"][0]["path"] = "../outside.parquet".into(),
                "size" => {
                    let size = value["plan"]["files"][0]["size"].as_u64().unwrap() + 1;
                    value["plan"]["files"][0]["size"] = size.into();
                    value["files"][0]["size"] = size.into();
                    value["plan"]["total_bytes"] = size.into();
                }
                "format" => value["plan"]["files"][0]["format"] = "opaque".into(),
                "local_path" => {
                    value["files"][0]["local_path"] = directory
                        .path()
                        .join("outside.parquet")
                        .to_str()
                        .unwrap()
                        .into()
                }
                "record" => value["files"][0]["url"] = "different".into(),
                "coverage" => value["plan"]["coverage"] = "COMPLETE".into(),
                _ => unreachable!(),
            });
            assert!(
                prepare(&args).unwrap_err().to_string().contains(expected),
                "{case}"
            );
            assert!(!args.output.exists());
        }
    }

    #[test]
    fn hf_selection_rejects_missing_corrupt_and_reordered_cached_files() {
        for case in ["missing", "header", "footer", "decode", "reordered"] {
            let directory = tempfile::tempdir().unwrap();
            let args = selection_fixture(directory.path(), &[row("137_1_1", 10, "420000")]);
            let value: serde_json::Value = read_json(args.selection.as_ref().unwrap()).unwrap();
            let cached = PathBuf::from(value["files"][0]["local_path"].as_str().unwrap());
            match case {
                "missing" => fs::remove_file(&cached).unwrap(),
                "header" | "footer" => {
                    let mut bytes = fs::read(&cached).unwrap();
                    let offset = if case == "header" { 0 } else { bytes.len() - 4 };
                    bytes[offset..offset + 4].copy_from_slice(b"FAIL");
                    fs::write(&cached, bytes).unwrap();
                }
                "decode" => {
                    // Matching size and a valid envelope alone cannot qualify
                    // bytes as native Parquet; the real reader must decode them.
                    let mut bytes = fs::read(&cached).unwrap();
                    let end = bytes.len() - 4;
                    bytes[4..end].fill(0);
                    fs::write(&cached, bytes).unwrap();
                    assert!(
                        hf_selection::load(
                            args.selection.as_ref().unwrap(),
                            args.start_seconds,
                            args.end_seconds
                        )
                        .is_ok()
                    );
                    assert!(SerializedFileReader::new(fs::File::open(&cached).unwrap()).is_err());
                }
                "reordered" => {
                    let second = cached.parent().unwrap().join("other.parquet");
                    fs::copy(&cached, &second).unwrap();
                    alter_selection(&args, |value| {
                        let mut item = value["plan"]["files"][0].clone();
                        item["path"] = "data/other.parquet".into();
                        item["url"] = format!("https://huggingface.co/datasets/fixture/public-history/resolve/{}/data/other.parquet", "a".repeat(40)).into();
                        let mut record = value["files"][0].clone();
                        record["path"] = item["path"].clone();
                        record["url"] = item["url"].clone();
                        record["local_path"] = second.to_str().unwrap().into();
                        value["plan"]["files"].as_array_mut().unwrap().push(item);
                        value["files"].as_array_mut().unwrap().insert(0, record);
                        value["plan"]["total_bytes"] =
                            (value["plan"]["total_bytes"].as_u64().unwrap() * 2).into();
                        value["cached_files"] = 2.into();
                    });
                }
                _ => unreachable!(),
            }
            let error = prepare(&args).unwrap_err();
            if case != "decode" {
                let expected = match case {
                    "missing" => "SELECTION_FILE_MISSING",
                    "reordered" => "SELECTION_RECORD_DIFFERS_FROM_PLAN",
                    _ => "SELECTION_PARQUET_ENVELOPE",
                };
                assert!(error.to_string().contains(expected), "{case}");
            }
            assert!(!args.output.exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn hf_selection_stat_detects_same_size_rewrite_with_restored_mtime() {
        use std::{
            io::{Seek, SeekFrom, Write},
            time::Duration,
        };
        let directory = tempfile::tempdir().unwrap();
        let args = selection_fixture(directory.path(), &[row("137_1_1", 10, "420000")]);
        let value: serde_json::Value = read_json(args.selection.as_ref().unwrap()).unwrap();
        let cached = PathBuf::from(value["files"][0]["local_path"].as_str().unwrap());
        let metadata = fs::metadata(&cached).unwrap();
        let before = hf_selection::file_state(&cached).unwrap();
        // Ensure even second-resolution Unix timestamps can observe the write.
        std::thread::sleep(Duration::from_secs(1));
        let mut file = fs::OpenOptions::new().write(true).open(&cached).unwrap();
        file.seek(SeekFrom::Start(4)).unwrap();
        file.write_all(b"X").unwrap();
        file.set_times(fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
            .unwrap();
        file.sync_all().unwrap();
        let after = fs::metadata(&cached).unwrap();
        assert_eq!(metadata.len(), after.len());
        assert_eq!(metadata.modified().unwrap(), after.modified().unwrap());
        assert_ne!(before, hf_selection::file_state(&cached).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn hf_selection_rejects_file_and_cache_ancestor_symlinks() {
        use std::os::unix::fs::symlink;
        for ancestor in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let args = selection_fixture(directory.path(), &[row("137_1_1", 10, "420000")]);
            let value: serde_json::Value = read_json(args.selection.as_ref().unwrap()).unwrap();
            let target = if ancestor {
                directory.path().join("cache")
            } else {
                PathBuf::from(value["files"][0]["local_path"].as_str().unwrap())
            };
            let moved = directory.path().join("original");
            fs::rename(&target, &moved).unwrap();
            symlink(&moved, &target).unwrap();
            assert!(
                prepare(&args)
                    .unwrap_err()
                    .to_string()
                    .contains("SELECTION_SYMLINK")
            );
            assert!(!args.output.exists());
        }
    }

    #[test]
    fn parquet_roundtrip_preserves_chain_order_deduplicates_and_does_not_fill_gaps() {
        let directory = tempfile::tempdir().unwrap();
        let mut summary = row("137_1_4", 10, "420000").into_columns();
        *summary.iter_mut().find(|(k, _)| k == "taker").unwrap() =
            ("taker".into(), Field::Str(EXCHANGES[0].to_uppercase()));
        let mut other = row("137_1_5", 10, "420000").into_columns();
        other
            .iter_mut()
            .find(|(k, _)| k == "takerAssetId")
            .unwrap()
            .1 = Field::Str("456".into());
        let args = fixture(
            directory.path(),
            &[
                row("137_1_10", 10, "500000"),
                row("137_1_2", 10, "420000"),
                row("137_2_1", 130, "600000"),
                row("137_1_2", 10, "420000"),
                Row::new(summary),
                Row::new(other),
                row("137_3_1", 240, "900000"),
            ],
        );
        let archive = prepare(&args).unwrap();
        assert_eq!(archive.trades.len(), 3);
        assert_eq!(archive.trades[0].trade_id.as_str(), "137_1_2");
        assert_eq!(archive.bars.len(), 2);
        assert_eq!(archive.bars[0].open, Price::from("0.4200"));
        assert_eq!(archive.bars[0].close, Price::from("0.5000"));
        assert_eq!(archive.bars[0].volume, Quantity::from("2.000000"));
        assert_eq!(archive.bars[0].ts_event.as_u64(), 60_000_000_000);
        assert_eq!(archive.bars[1].ts_event.as_u64(), 180_000_000_000);
        assert_eq!(archive.source_metadata["quality"]["duplicate_rows"], 1);
        assert!(archive.source_metadata.get("selection_manifest").is_none());
        assert!(archive.source_metadata.get("clock_basis").is_none());
        assert_eq!(
            archive.source_metadata["instruments_sha256"],
            digest(&args.instruments).unwrap()
        );
        assert_eq!(
            archive.source_metadata["quality"]["exchange_summaries_excluded"],
            1
        );
        let report = super::super::import(archive, &args.output).unwrap();
        assert_eq!(report.coverage, "UNPROVEN");
        assert_eq!(report.historical_availability, "UNVERIFIED");
        let mut catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
            args.output.join("catalog").to_str().unwrap(),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            catalog
                .query::<Bar>(None, None, None, None, None, true)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn corrupt_files_conflicting_events_and_partial_bars_fail_before_publication() {
        let directory = tempfile::tempdir().unwrap();
        let mut args = fixture(
            directory.path(),
            &[row("137_1_1", 10, "420000"), row("137_1_1", 10, "500000")],
        );
        assert!(
            prepare(&args)
                .unwrap_err()
                .to_string()
                .contains("CONFLICTING_SOURCE_EVENT")
        );
        args.start_seconds = 1;
        assert!(
            prepare(&args)
                .unwrap_err()
                .to_string()
                .contains("FULL_BAR_INTERVALS_REQUIRED")
        );
        args.start_seconds = 0;
        fs::write(
            directory
                .path()
                .join("order_filled/year=2022/month=11.parquet"),
            b"corrupted",
        )
        .unwrap();
        assert!(
            prepare(&args)
                .unwrap_err()
                .to_string()
                .contains("SOURCE_CHECKSUM_MISMATCH")
        );
        assert!(!args.output.exists());
    }

    #[test]
    fn cash_ratio_and_unknown_aggressor_preserve_economic_semantics() {
        let mut instruments = BTreeMap::from([("123".into(), instrument())]);
        let mut quality = Quality::default();
        let result = fill(&row("137_1_1", 10, "420000"), &instruments, &mut quality)
            .unwrap()
            .unwrap();
        assert_eq!(result.price, Price::from("0.4200")); // not cash / (cash + shares)
        assert_eq!(result.size, Quantity::from("1.000000"));
        assert_eq!(result.aggressor_side, AggressorSide::NoAggressor);
        assert!(fill(&row("137_1_1", 10, "0"), &instruments, &mut quality).is_err());
        assert!(fill(&row("137_1_1", 10, "1000001"), &instruments, &mut quality).is_err());
        assert!(event_key("137_1_-1").is_err());
        assert!(event_key("1_1_1").is_err());
        for currency in ["USDC", "pUSD"] {
            let mut value = serde_json::to_value(instrument()).unwrap();
            value["BinaryOption"]["currency"] = currency.into();
            instruments.insert("123".into(), serde_json::from_value(value).unwrap());
            assert!(
                fill(&row("137_1_1", 10, "420000"), &instruments, &mut quality)
                    .unwrap_err()
                    .to_string()
                    .contains("V1_BRIDGED_USDC_INSTRUMENT_REQUIRED")
            );
        }
    }

    #[test]
    fn minute_end_books_roundtrip_clear_precision_and_half_open_selection() {
        let directory = tempfile::tempdir().unwrap();
        let make = |minute: &str, bids: &str, asks: &str| {
            Row::new(
                [
                    ("asset_id", "123"),
                    ("condition_id", "condition"),
                    ("minute_ts", minute),
                    ("bids_json", bids),
                    ("asks_json", asks),
                ]
                .into_iter()
                .map(|(k, v)| (k.into(), Field::Str(v.into())))
                .collect(),
            )
        };
        let mut args = fixture_for(
            directory.path(),
            &[
                make("0", "[]", "[[0.75,5.76],[0.98,55]]"),
                make("60", "[[0.50,2]]", "[[0.75,5.76]]"),
                make("120", "[]", "[]"),
                make("180", "[[0.90,1]]", "[[0.95,1]]"),
            ],
            Format::JosephBooks,
        );
        args.start_seconds = 60;
        args.end_seconds = 240;
        let mut archive = prepare(&args).unwrap();
        assert_eq!(archive.source_metadata["quality"]["selected_rows"], 3);
        assert_eq!(archive.source_metadata["quality"]["one_sided_books"], 1);
        assert_eq!(archive.source_metadata["quality"]["empty_books"], 1);
        assert_eq!(archive.quotes.len(), 1);
        assert_eq!(archive.quotes[0].bid_size, Quantity::from("2.000000"));
        assert_eq!(archive.deltas.len(), 7);
        assert_eq!(archive.deltas[0].ts_event.as_u64(), 60_000_000_000);
        assert_eq!(
            archive.deltas.last().unwrap().ts_event.as_u64(),
            180_000_000_000
        );
        assert_eq!(
            archive.deltas.last().unwrap().flags,
            RecordFlag::F_SNAPSHOT as u8 | RecordFlag::F_LAST as u8
        );
        let source = serde_json::from_value(serde_json::to_value(&archive).unwrap()).unwrap();
        let report = super::super::import(source, &args.output).unwrap();
        assert_eq!((report.deltas, report.quotes), (7, 1));
        let mut catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
            args.output.join("catalog").to_str().unwrap(),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            catalog
                .query::<OrderBookDelta>(None, None, None, None, None, true)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .len(),
            7
        );
        assert_eq!(
            catalog
                .query::<QuoteTick>(None, None, None, None, None, true)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .len(),
            1
        );
        assert!(
            book(
                &make("60", "[[0.9,2]]", "[[0.75,5.76]]"),
                &instrument(),
                &mut archive,
                &mut Quality::default()
            )
            .is_err()
        );
    }
}
