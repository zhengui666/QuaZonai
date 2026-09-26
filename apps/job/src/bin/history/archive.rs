//! Public vendor schemas stop at this operator boundary. Scientific jobs stay offline.
use super::{epoch_ns, historical_instrument, NativeArchive, MAX_INPUT_BYTES, MAX_ROWS};
use anyhow::{bail, ensure, Context, Result};
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
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

const EXCHANGES: [&str; 2] = [
    "0x4bfb41d5b3570defd03c39a9a4d8de6bd8b8982e",
    "0xc5d563a36ae78145c45a50134d48a1215220f80a",
];

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Envio v1 OrderFilled; exclude exchange-counterparty taker summaries.
    MooseFills,
    /// Joseph3222 minute-END full-book snapshots, not a continuous event feed.
    JosephBooks,
}

#[derive(clap::Args)]
pub struct Arguments {
    /// snapshot.json emitted by runtimes/data/snapshot.py (fixed revision and hashes).
    #[arg(long)]
    snapshot: PathBuf,
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
    one_sided_books: usize,
    empty_books: usize,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= MAX_INPUT_BYTES,
        "INPUT_FILE_LIMIT"
    );
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_INPUT_BYTES, "INPUT_FILE_LIMIT");
    Ok(serde_json::from_slice(&bytes)?)
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
    ensure!(
        !snapshot.files.is_empty() && snapshot.files.len() <= 100_000,
        "SOURCE_FILE_LIMIT"
    );
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
        ensure!(levels.len() <= 10_000, "BOOK_LEVEL_LIMIT");
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
    ensure!(
        archive.deltas.len() + sides[0].len() + sides[1].len() < MAX_ROWS,
        "NATIVE_ROW_LIMIT"
    );
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
            let bar = builder.build(end.into(), end.into());
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
            args.format == Format::MooseFills
                && interval > 0
                && interval <= 86400
                && args.start_seconds.is_multiple_of(u64::from(interval))
                && args.end_seconds.is_multiple_of(u64::from(interval)),
            "FULL_BAR_INTERVALS_REQUIRED"
        );
    }
    let snapshot: Snapshot = read_json(&args.snapshot)?;
    let root = args.snapshot.parent().context("SNAPSHOT_ROOT")?;
    let files = verified_files(&snapshot, root)?;
    let instruments: Vec<InstrumentAny> = read_json(&args.instruments)?;
    ensure!((1..=256).contains(&instruments.len()), "INSTRUMENT_LIMIT");
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
    let mut archive = NativeArchive {
        schema_version: contracts::SchemaV1,
        source_reference: format!(
            "https://huggingface.co/datasets/{}/tree/{}",
            snapshot.repository, snapshot.revision
        ),
        source_observed_at: snapshot.retrieved_at,
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
    for file in &files {
        let relative = file.strip_prefix(root)?.to_string_lossy();
        let prefix = match args.format {
            Format::MooseFills => "order_filled/",
            Format::JosephBooks => "orderbook_1min/",
        };
        if !relative.starts_with(prefix) {
            continue;
        }
        let reader = SerializedFileReader::new(fs::File::open(file)?)?;
        for row in reader.get_row_iter(None)? {
            let row = row?;
            quality.scanned_rows += 1;
            let label = match args.format {
                Format::MooseFills => "timestamp",
                Format::JosephBooks => "minute_ts",
            };
            let label_at = seconds(&row, label)?;
            let at = match args.format {
                Format::MooseFills => label_at,
                Format::JosephBooks => label_at.checked_add(60).context("TIMESTAMP_RANGE")?,
            };
            if at < args.start_seconds || at >= args.end_seconds {
                continue;
            }
            let (identity, signature) = match args.format {
                Format::MooseFills => {
                    let mut row_quality = Quality::default();
                    let Some(trade) = fill(&row, &by_token, &mut row_quality)? else {
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
                    quality.rounded_trade_prices += row_quality.rounded_trade_prices;
                    quality.self_trades += row_quality.self_trades;
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
                    book(&row, instrument, &mut archive, &mut quality)?;
                    (identity, signature)
                }
            };
            quality.selected_rows += 1;
            ensure!(
                archive.trades.len() + archive.quotes.len() + archive.deltas.len() <= MAX_ROWS,
                "NATIVE_ROW_LIMIT"
            );
            identities.insert(identity, signature);
        }
    }
    ensure!(quality.selected_rows > 0, "EMPTY_ARCHIVE_SELECTION");
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
        "snapshot": snapshot, "format": args.format,
        "selection": {"start_seconds": args.start_seconds, "end_seconds": args.end_seconds, "bar_seconds": args.bar_seconds},
        "quality": quality,
        "availability": "UNVERIFIED: block time or minute end is an event-time proxy, not measured reception/finality latency.",
        "coverage": "Only selected files/assets/window; no provider completeness claim is verified.",
        "settlement": "Not inferred from state tables, redemption times, end dates or last prices.",
        "book_semantics": "Minute-end snapshots, sequence unknown (0); missing or one-sided books never create tradable zero quotes.",
        "trade_semantics": "v1 USDC.e cash/token OrderFilled, excluding exchange-counterparty summaries; self trades retained; aggressor unknown; price rounding counted.",
        "instruments_sha256": digest(&args.instruments)?,
    });
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

    fn instrument() -> InstrumentAny {
        InstrumentAny::BinaryOption(
            BinaryOption::builder()
                .instrument_id(InstrumentId::from_str("condition-123.POLYMARKET").unwrap())
                .raw_symbol(Symbol::new("123"))
                .asset_class(AssetClass::Alternative)
                .currency(Currency::from_str("USDC.e").unwrap())
                .activation_ns(0_u64.into())
                .expiration_ns(1000_000_000_000_u64.into())
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

    fn fixture(root: &Path, rows: &[Row]) -> Arguments {
        fixture_for(root, rows, Format::MooseFills)
    }

    fn fixture_for(root: &Path, rows: &[Row], format: Format) -> Arguments {
        let relative = match format {
            Format::MooseFills => "order_filled/year=2022/month=11.parquet",
            Format::JosephBooks => "orderbook_1min/date=2026-05-01/data_0.parquet",
        };
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let fields = rows[0]
            .get_column_iter()
            .map(|(key, _)| format!("REQUIRED BINARY {key} (UTF8);"))
            .collect::<Vec<_>>()
            .join(" ");
        let schema =
            Arc::new(parse_message_type(&format!("message source {{ {fields} }}")).unwrap());
        let mut writer =
            SerializedFileWriter::new(fs::File::create(&path).unwrap(), schema, Default::default())
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
            snapshot: manifest,
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
        assert!(prepare(&args)
            .unwrap_err()
            .to_string()
            .contains("CONFLICTING_SOURCE_EVENT"));
        args.start_seconds = 1;
        assert!(prepare(&args)
            .unwrap_err()
            .to_string()
            .contains("FULL_BAR_INTERVALS_REQUIRED"));
        args.start_seconds = 0;
        fs::write(
            directory
                .path()
                .join("order_filled/year=2022/month=11.parquet"),
            b"corrupted",
        )
        .unwrap();
        assert!(prepare(&args)
            .unwrap_err()
            .to_string()
            .contains("SOURCE_CHECKSUM_MISMATCH"));
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
        assert!(book(
            &make("60", "[[0.9,2]]", "[[0.75,5.76]]"),
            &instrument(),
            &mut archive,
            &mut Quality::default()
        )
        .is_err());
    }
}
