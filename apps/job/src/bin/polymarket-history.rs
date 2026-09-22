//! Operator-only historical data preparation. Not a scientific job or trading client.
//! Native clients own HTTP, pagination, asset parsing and market-data serialization.
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use contracts::SchemaV1;
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, InstrumentClose, OrderBookDelta, QuoteTick, TradeTick},
    enums::{InstrumentCloseType, PriceType},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use nautilus_polymarket::http::{
    data_api::PolymarketDataApiHttpClient,
    gamma::PolymarketGammaRawHttpClient,
    parse::{create_instrument_from_def, parse_gamma_market},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ROWS: usize = 1_000_000;
const NATIVE_VERSION: &str = "0.63.0";

#[derive(Parser)]
#[command(
    version,
    about = "Prepare native Polymarket history; does not qualify or register research data"
)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read public history through the pinned Nautilus Rust clients. Coverage remains unproven.
    Fetch {
        #[arg(long)]
        market_slug: String,
        /// Inclusive original API epoch-second boundary.
        #[arg(long)]
        start_seconds: u64,
        /// Exclusive epoch-second boundary in this command.
        #[arg(long)]
        end_seconds: u64,
        /// Per-outcome bound, not a completeness claim.
        #[arg(long, default_value_t = 5000, value_parser = clap::value_parser!(u32).range(1..=10000))]
        max_trades: u32,
        #[arg(long)]
        output: PathBuf,
    },
    /// Import an explicitly mapped native-record JSON archive, not arbitrary vendor Parquet.
    Import {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

/// Local file interchange only. HTTP/MCP and scientific admission do not accept this as evidence.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeArchive {
    schema_version: SchemaV1,
    source_reference: String,
    source_observed_at: DateTime<Utc>,
    /// Detached source snapshot; never installed in the native instrument metadata.
    source_metadata: serde_json::Value,
    instruments: Vec<InstrumentAny>,
    #[serde(default)]
    trades: Vec<TradeTick>,
    #[serde(default)]
    quotes: Vec<QuoteTick>,
    #[serde(default)]
    deltas: Vec<OrderBookDelta>,
    #[serde(default)]
    bars: Vec<Bar>,
    #[serde(default)]
    closes: Vec<InstrumentClose>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportReport {
    schema_version: SchemaV1,
    native_version: String,
    source_reference: String,
    source_observed_at: DateTime<Utc>,
    imported_at: DateTime<Utc>,
    instruments: usize,
    trades: usize,
    quotes: usize,
    deltas: usize,
    bars: usize,
    closes: usize,
    catalog_relative_path: String,
    coverage: String,
    historical_availability: String,
    registered_in_quazonai: bool,
    limitations: Vec<String>,
}

fn epoch_ns(seconds: u64) -> Result<UnixNanos> {
    Ok(seconds
        .checked_mul(1_000_000_000)
        .context("TIMESTAMP_RANGE")?
        .into())
}

fn valid_price(price: Decimal) -> bool {
    (Decimal::ZERO..=Decimal::ONE).contains(&price)
}

fn time_order(event: UnixNanos, available: UnixNanos) -> Result<()> {
    ensure!(available >= event, "EVENT_AFTER_NATIVE_INIT");
    Ok(())
}

/// Strip terminal/live observations without inventing earlier metadata timestamps.
fn historical_instrument(instrument: &InstrumentAny) -> Result<InstrumentAny> {
    ensure!(
        matches!(instrument, InstrumentAny::BinaryOption(_)),
        "BINARY_OPTION_REQUIRED"
    );
    ensure!(
        instrument.venue().to_string() == "POLYMARKET",
        "POLYMARKET_VENUE_REQUIRED"
    );
    let mut value = serde_json::to_value(instrument)?;
    if let Some(info) = value
        .get_mut("BinaryOption")
        .and_then(|v| v.get_mut("info"))
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in [
            "active",
            "closed",
            "closedTime",
            "closed_time",
            "resolved",
            "winner",
            "tokens",
            "outcomePrices",
            "outcome_prices",
            "umaResolutionStatus",
            "accepting_orders",
            "acceptingOrders",
            "gamma_market",
            "gamma_event",
        ] {
            info.remove(key);
        }
    }
    Ok(serde_json::from_value(value)?)
}

fn validate(archive: &NativeArchive) -> Result<()> {
    ensure!(
        !archive.source_reference.trim().is_empty() && archive.source_reference.len() <= 2000,
        "SOURCE_REFERENCE_REQUIRED"
    );
    ensure!(
        archive.source_observed_at <= Utc::now(),
        "FUTURE_SOURCE_OBSERVATION"
    );
    ensure!(
        (1..=256).contains(&archive.instruments.len()),
        "INSTRUMENT_LIMIT"
    );
    let rows = archive
        .trades
        .len()
        .checked_add(archive.quotes.len())
        .and_then(|n| n.checked_add(archive.deltas.len()))
        .and_then(|n| n.checked_add(archive.bars.len()))
        .and_then(|n| n.checked_add(archive.closes.len()))
        .context("ROW_COUNT_RANGE")?;
    ensure!((1..=MAX_ROWS).contains(&rows), "NATIVE_ROW_LIMIT_OR_EMPTY");
    let mut ids = BTreeSet::new();
    for instrument in &archive.instruments {
        historical_instrument(instrument)?;
        ensure!(ids.insert(instrument.id()), "DUPLICATE_INSTRUMENT");
    }
    let mut trade_ids = BTreeSet::new();
    for trade in &archive.trades {
        ensure!(ids.contains(&trade.instrument_id), "UNMAPPED_TRADE");
        ensure!(
            valid_price(trade.price.as_decimal()) && trade.size.as_decimal() > Decimal::ZERO,
            "INVALID_BINARY_TRADE"
        );
        ensure!(
            trade_ids.insert((trade.instrument_id, trade.trade_id)),
            "DUPLICATE_TRADE_ID"
        );
        time_order(trade.ts_event, trade.ts_init)?;
    }
    for quote in &archive.quotes {
        ensure!(ids.contains(&quote.instrument_id), "UNMAPPED_QUOTE");
        ensure!(
            valid_price(quote.bid_price.as_decimal())
                && valid_price(quote.ask_price.as_decimal())
                && quote.bid_price <= quote.ask_price,
            "INVALID_BINARY_QUOTE"
        );
        time_order(quote.ts_event, quote.ts_init)?;
    }
    for delta in &archive.deltas {
        ensure!(ids.contains(&delta.instrument_id), "UNMAPPED_BOOK_DELTA");
        ensure!(
            valid_price(delta.order.price.as_decimal()),
            "INVALID_BINARY_BOOK_PRICE"
        );
        time_order(delta.ts_event, delta.ts_init)?;
    }
    let mut settled = BTreeSet::new();
    for close in &archive.closes {
        ensure!(
            ids.contains(&close.instrument_id),
            "UNMAPPED_INSTRUMENT_CLOSE"
        );
        ensure!(
            close.close_type == InstrumentCloseType::ContractExpired
                && valid_price(close.close_price.as_decimal()),
            "INVALID_BINARY_CLOSE"
        );
        ensure!(
            settled.insert(close.instrument_id),
            "DUPLICATE_BINARY_CLOSE"
        );
        time_order(close.ts_event, close.ts_init)?;
    }
    // A partial archive stays unqualified, but observed sibling payouts must
    // never contradict each other. This checks source conservation, not cash replay.
    let mut conditions = std::collections::BTreeMap::<
        String,
        Vec<contracts::settlement::NativeSettlementOutcomeV1>,
    >::new();
    for close in &archive.closes {
        let identity = close.instrument_id.to_string();
        let (condition, _) = identity
            .rsplit_once('-')
            .context("POLYMARKET_CLOSE_IDENTITY")?;
        conditions.entry(condition.into()).or_default().push(
            contracts::settlement::NativeSettlementOutcomeV1 {
                instrument_id: identity,
                close_price: close
                    .close_price
                    .to_string()
                    .parse()
                    .map_err(anyhow::Error::msg)?,
                ts_event: contracts::DbCounter::new(close.ts_event.as_u64())
                    .map_err(anyhow::Error::msg)?,
                ts_init: contracts::DbCounter::new(close.ts_init.as_u64())
                    .map_err(anyhow::Error::msg)?,
            },
        );
    }
    for (condition_id, outcomes) in conditions {
        if outcomes.len() > 1 {
            domain::prediction::settlements(&[contracts::settlement::NativeSettlementGroupV1 {
                condition_id,
                source_reference: archive.source_reference.clone(),
                outcomes,
            }])?;
        }
    }
    for bar in &archive.bars {
        ensure!(ids.contains(&bar.bar_type.instrument_id()), "UNMAPPED_BAR");
        ensure!(
            bar.bar_type.is_externally_aggregated()
                && bar.bar_type.spec().price_type == PriceType::Last,
            "ONLY_NATIVE_LAST_BARS"
        );
        ensure!(
            [bar.open, bar.high, bar.low, bar.close]
                .iter()
                .all(|p| valid_price(p.as_decimal()))
                && bar.low <= bar.open
                && bar.low <= bar.close
                && bar.high >= bar.open
                && bar.high >= bar.close
                && bar.high >= bar.low,
            "INVALID_BINARY_BAR"
        );
        time_order(bar.ts_event, bar.ts_init)?;
    }
    Ok(())
}

fn import(mut archive: NativeArchive, output: &Path) -> Result<ImportReport> {
    validate(&archive)?;
    // Exclusively create a new destination. Never overwrite a registered catalog or user file.
    fs::create_dir(output).context("OUTPUT_MUST_BE_NEW")?;
    fs::write(
        output.join("source-evidence.json"),
        serde_json::to_vec_pretty(&archive)?,
    )?;
    let root = output.join("catalog");
    fs::create_dir(&root)?;
    let catalog = ParquetDataCatalog::from_uri(
        root.to_str().context("CATALOG_PATH_ENCODING")?,
        None,
        Some(4096),
        None,
        None,
    )?;
    let instruments = archive
        .instruments
        .iter()
        .map(historical_instrument)
        .collect::<Result<Vec<_>>>()?;
    catalog.write_instruments(instruments)?;
    // Native Parquet metadata belongs to one instrument (one BarType for bars).
    // Stable ordering preserves the source order of simultaneous book updates.
    archive.trades.sort_by_key(|r| (r.instrument_id, r.ts_init));
    archive.quotes.sort_by_key(|r| (r.instrument_id, r.ts_init));
    archive.deltas.sort_by_key(|r| (r.instrument_id, r.ts_init));
    archive.bars.sort_by_key(|r| (r.bar_type, r.ts_init));
    archive.closes.sort_by_key(|r| (r.instrument_id, r.ts_init));
    for rows in archive
        .trades
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
    {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive
        .quotes
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
    {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive
        .deltas
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
    {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive.bars.chunk_by(|a, b| a.bar_type == b.bar_type) {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive
        .closes
        .chunk_by(|a, b| a.instrument_id == b.instrument_id)
    {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    let report = ImportReport {
        schema_version: SchemaV1,
        native_version: NATIVE_VERSION.into(),
        source_reference: archive.source_reference,
        source_observed_at: archive.source_observed_at,
        imported_at: Utc::now(),
        instruments: archive.instruments.len(),
        trades: archive.trades.len(), quotes: archive.quotes.len(),
        deltas: archive.deltas.len(), bars: archive.bars.len(), closes: archive.closes.len(),
        catalog_relative_path: "catalog".into(),
        coverage: "UNPROVEN".into(),
        historical_availability: "UNVERIFIED".into(),
        registered_in_quazonai: false,
        limitations: vec![
            "Native serialization is not a coverage, historical fee, settlement or PIT verification.".into(),
            "Current metadata retains its observation time; it is not backdated for historical research.".into(),
            "Pinned HTTP history may be truncated; same-second ordering is synthesized by upstream, not observed latency.".into(),
            "Book records are not claimed gap-free or replayable without separate snapshot/sequence validation.".into(),
            "No synthetic OHLCV or depth is created. Source evidence is outside the native catalog mount.".into(),
        ],
    };
    // Written last: a directory without this report is an interrupted, unpublished import.
    fs::write(
        output.join("import-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

fn read_archive(path: &Path) -> Result<NativeArchive> {
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

async fn fetch(slug: &str, start: u64, end: u64, max_trades: u32) -> Result<NativeArchive> {
    ensure!(
        !slug.is_empty()
            && slug.len() <= 240
            && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "INVALID_MARKET_SLUG"
    );
    ensure!(
        start < end && end <= u64::try_from(Utc::now().timestamp())?,
        "INVALID_HISTORY_WINDOW"
    );
    ensure!((1..=10000).contains(&max_trades), "TRADE_LIMIT");
    let start_ns = epoch_ns(start)?;
    let end_ns = epoch_ns(end)?;
    let gamma = PolymarketGammaRawHttpClient::new(None, 30)?;
    let market = gamma.get_gamma_market_by_slug(slug).await?;
    let observed = Utc::now();
    let observed_ns = UnixNanos::from(u64::try_from(
        observed.timestamp_nanos_opt().context("CLOCK_RANGE")?,
    )?);
    let defs = parse_gamma_market(&market)?;
    let client = PolymarketDataApiHttpClient::new(None, 30)?;
    let mut instruments = Vec::new();
    let mut trades = Vec::new();
    for def in defs {
        let instrument = create_instrument_from_def(&def, observed_ns)?;
        let rows = client
            .request_trade_ticks(
                instrument.id(),
                def.condition_id.as_str(),
                def.token_id.as_str(),
                instrument.price_precision(),
                instrument.size_precision(),
                Some(start_ns),
                Some(epoch_ns(end - 1)?),
                Some(max_trades),
            )
            .await?;
        // Enforce this command's half-open interval even if the upstream API returns a wider page.
        trades.extend(
            rows.into_iter()
                .filter(|r| r.ts_event >= start_ns && r.ts_event < end_ns),
        );
        instruments.push(instrument);
    }
    Ok(NativeArchive {
        schema_version: SchemaV1,
        source_reference: format!("nautilus-polymarket/{NATIVE_VERSION}:market/{slug};seconds=[{start},{end});per_outcome_limit={max_trades}"),
        source_observed_at: observed,
        source_metadata: serde_json::to_value(market)?,
        instruments, trades, quotes: Vec::new(), deltas: Vec::new(), bars: Vec::new(), closes: Vec::new(),
    })
}

#[tokio::main]
async fn main() {
    let args = Arguments::parse();
    let result = match args.command {
        Command::Import { input, output } => read_archive(&input).and_then(|a| import(a, &output)),
        Command::Fetch {
            market_slug,
            start_seconds,
            end_seconds,
            max_trades,
            output,
        } => match fetch(&market_slug, start_seconds, end_seconds, max_trades).await {
            Ok(archive) => import(archive, &output),
            Err(error) => Err(error),
        },
    };
    match result {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string(&report).expect("report serialization")
            );
        }
        Err(_) => {
            eprintln!("QZ_POLYMARKET_HISTORY_FAILED");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        enums::{AggressorSide, AssetClass},
        identifiers::{InstrumentId, Symbol, TradeId},
        instruments::BinaryOption,
        types::{Currency, Price, Quantity},
    };
    use std::str::FromStr;

    fn archive() -> NativeArchive {
        let id = InstrumentId::from_str("test-condition-123456789012345678901234567890.POLYMARKET")
            .unwrap();
        let instrument = BinaryOption::builder()
            .instrument_id(id)
            .raw_symbol(Symbol::new("123456789012345678901234567890"))
            .asset_class(AssetClass::Alternative)
            .currency(Currency::from_str("USD").unwrap())
            .activation_ns(0_u64.into())
            .expiration_ns(1000_u64.into())
            .price_precision(4)
            .size_precision(6)
            .price_increment(Price::from("0.0001"))
            .size_increment(Quantity::from("0.000001"))
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap();
        let trade = |time: u64, name: &str| {
            TradeTick::new(
                id,
                Price::from("0.4200"),
                Quantity::from("2.000000"),
                AggressorSide::Buy,
                TradeId::new(name),
                time.into(),
                (time + 1).into(),
            )
        };
        NativeArchive {
            schema_version: SchemaV1,
            source_reference: "FIXTURE: native catalog round-trip".into(),
            source_observed_at: DateTime::from_timestamp(0, 0).unwrap(),
            source_metadata: serde_json::json!({"winner": "Yes", "closed": true}),
            instruments: vec![InstrumentAny::BinaryOption(instrument)],
            trades: vec![trade(20, "second"), trade(10, "first")],
            quotes: Vec::new(),
            deltas: Vec::new(),
            bars: Vec::new(),
            closes: Vec::new(),
        }
    }

    #[test]
    fn native_records_round_trip_without_manufacturing_bars_or_qualification() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("new-import");
        let expected = archive();
        let expected_id = expected.instruments[0].id();
        let report = import(expected, &target).unwrap();
        assert_eq!(report.trades, 2);
        assert_eq!(report.bars, 0);
        assert_eq!(report.coverage, "UNPROVEN");
        assert!(!report.registered_in_quazonai);
        let root = target.join("catalog");
        let mut catalog =
            ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, None, None, None).unwrap();
        let instruments = catalog.instruments(None, None, None).unwrap();
        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].id(), expected_id);
        let rows = catalog
            .query::<TradeTick>(None, None, None, None, None, true)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(target.join("source-evidence.json").exists());
        assert!(!root.join("source-evidence.json").exists());
        assert!(target.join("import-report.json").exists());
    }

    #[test]
    fn source_settlement_events_round_trip_separately_from_trades() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("settlement");
        let mut input = archive();
        input.closes = vec![InstrumentClose::new(
            input.instruments[0].id(),
            Price::from("0.5000"),
            InstrumentCloseType::ContractExpired,
            1000_u64.into(),
            1200_u64.into(),
        )];
        let expected = input.closes[0];
        let report = import(input, &target).unwrap();
        assert_eq!(report.closes, 1);
        let path = target.join("catalog");
        let mut catalog =
            ParquetDataCatalog::from_uri(path.to_str().unwrap(), None, None, None, None).unwrap();
        let rows = catalog
            .query::<InstrumentClose>(None, None, None, None, None, true)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![nautilus_model::data::Data::InstrumentClose(expected)]
        );
        assert_eq!(report.bars, 0);
        assert_eq!(report.historical_availability, "UNVERIFIED");
    }

    #[test]
    fn multiple_outcomes_and_bar_types_round_trip_in_distinct_native_partitions() {
        use nautilus_model::{
            data::{BarType, BookOrder},
            enums::{BookAction, OrderSide, RecordFlag},
        };
        let mut input = archive();
        let second_id =
            InstrumentId::from_str("test-condition-987654321098765432109876543210.POLYMARKET")
                .unwrap();
        let mut definition = serde_json::to_value(&input.instruments[0]).unwrap();
        definition["BinaryOption"]["id"] = second_id.to_string().into();
        definition["BinaryOption"]["raw_symbol"] = "987654321098765432109876543210".into();
        input
            .instruments
            .push(serde_json::from_value(definition).unwrap());
        let original_trades = input.trades.clone();
        for mut trade in original_trades {
            trade.instrument_id = second_id;
            input.trades.push(trade);
        }
        for id in input.instruments.iter().map(Instrument::id) {
            input.quotes.push(QuoteTick::new(
                id,
                Price::from("0.4000"),
                Price::from("0.4500"),
                Quantity::from("1.000000"),
                Quantity::from("2.000000"),
                30_u64.into(),
                31_u64.into(),
            ));
            input.deltas.push(OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(
                    OrderSide::Buy,
                    Price::from("0.4000"),
                    Quantity::from("1.000000"),
                    1,
                ),
                RecordFlag::F_LAST as u8,
                1,
                40_u64.into(),
                41_u64.into(),
            ));
            for minutes in [1, 5] {
                input.bars.push(
                    Bar::new_checked(
                        BarType::from_str(&format!("{id}-{minutes}-MINUTE-LAST-EXTERNAL")).unwrap(),
                        Price::from("0.4000"),
                        Price::from("0.4500"),
                        Price::from("0.3500"),
                        Price::from("0.4200"),
                        Quantity::from("10.000000"),
                        50_u64.into(),
                        51_u64.into(),
                    )
                    .unwrap(),
                );
            }
            input.closes.push(InstrumentClose::new(
                id,
                Price::from("0.5000"),
                InstrumentCloseType::ContractExpired,
                1000_u64.into(),
                1200_u64.into(),
            ));
        }
        let ids = input
            .instruments
            .iter()
            .map(Instrument::id)
            .collect::<Vec<_>>();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("multiple-outcomes");
        let report = import(input, &output).unwrap();
        assert_eq!(
            (
                report.instruments,
                report.trades,
                report.quotes,
                report.deltas,
                report.bars,
                report.closes
            ),
            (2, 4, 2, 2, 4, 2)
        );
        let root = output.join("catalog");
        let mut catalog =
            ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, None, None, None).unwrap();
        for id in ids {
            macro_rules! rows {
                ($kind:ty, $key:expr) => {
                    catalog
                        .query::<$kind>(Some(vec![$key]), None, None, None, None, true)
                        .unwrap()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap()
                };
            }
            assert_eq!(rows!(TradeTick, id.to_string()).len(), 2);
            assert_eq!(rows!(QuoteTick, id.to_string()).len(), 1);
            assert_eq!(rows!(OrderBookDelta, id.to_string()).len(), 1);
            assert_eq!(rows!(InstrumentClose, id.to_string()).len(), 1);
            for minutes in [1, 5] {
                assert_eq!(
                    rows!(Bar, format!("{id}-{minutes}-MINUTE-LAST-EXTERNAL")).len(),
                    1
                );
            }
        }
    }

    #[test]
    fn invalid_or_duplicate_records_do_not_publish() {
        for mutation in 0..5 {
            let mut data = archive();
            match mutation {
                0 => data.trades.push(data.trades[0]),
                1 => data.source_reference.clear(),
                2 => {
                    data.trades[0].instrument_id =
                        InstrumentId::from_str("other.POLYMARKET").unwrap()
                }
                3 => data.trades[0].ts_init = 0_u64.into(),
                _ => data.trades.clear(),
            }
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("invalid");
            assert!(import(data, &target).is_err(), "mutation {mutation}");
            assert!(!target.exists());
        }
    }

    #[test]
    fn existing_output_is_never_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("owner-data"), b"preserve").unwrap();
        assert!(import(archive(), directory.path()).is_err());
        assert_eq!(
            fs::read(directory.path().join("owner-data")).unwrap(),
            b"preserve"
        );
        assert!(!directory.path().join("import-report.json").exists());
    }

    #[test]
    fn terminal_metadata_is_detached_and_observation_times_are_preserved() {
        let mut value = serde_json::to_value(&archive().instruments[0]).unwrap();
        value["BinaryOption"]["info"] = serde_json::json!({"closed": true, "winner": true,
            "gamma_market": "terminal JSON", "fee_schedule": {"rate": "0.01"}, "condition_id": "original"});
        let original: InstrumentAny = serde_json::from_value(value).unwrap();
        let cleaned = historical_instrument(&original).unwrap();
        let result = serde_json::to_value(&cleaned).unwrap();
        let info = &result["BinaryOption"]["info"];
        assert!(info.get("closed").is_none());
        assert!(info.get("winner").is_none());
        assert!(info.get("gamma_market").is_none());
        assert_eq!(info["condition_id"], "original");
        assert_eq!(info["fee_schedule"]["rate"], "0.01");
        assert_eq!(cleaned.ts_init(), original.ts_init());
    }

    #[test]
    fn equal_reception_times_preserve_original_arrival_order_in_native_partitions() {
        use nautilus_model::{
            data::{BookOrder, Data},
            enums::{BookAction, OrderSide, RecordFlag},
        };
        let mut input = archive();
        let id = input.instruments[0].id();
        for row in &mut input.trades {
            row.ts_init = 50_u64.into();
        }
        for event in [20_u64, 10_u64] {
            input.quotes.push(QuoteTick::new(
                id,
                Price::from("0.4000"),
                Price::from("0.4500"),
                Quantity::from("1.000000"),
                Quantity::from("2.000000"),
                event.into(),
                50_u64.into(),
            ));
        }
        input.deltas = vec![
            OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(
                    OrderSide::Buy,
                    Price::from("0.4000"),
                    Quantity::from("1.000000"),
                    1,
                ),
                0,
                10,
                20_u64.into(),
                50_u64.into(),
            ),
            OrderBookDelta::new(
                id,
                BookAction::Update,
                BookOrder::new(
                    OrderSide::Buy,
                    Price::from("0.4000"),
                    Quantity::from("2.000000"),
                    1,
                ),
                RecordFlag::F_LAST as u8,
                11,
                10_u64.into(),
                50_u64.into(),
            ),
        ];
        let trades = input
            .trades
            .iter()
            .copied()
            .map(Data::Trade)
            .collect::<Vec<_>>();
        let quotes = input
            .quotes
            .iter()
            .copied()
            .map(Data::Quote)
            .collect::<Vec<_>>();
        let deltas = input
            .deltas
            .iter()
            .copied()
            .map(Data::Delta)
            .collect::<Vec<_>>();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("arrival-order");
        import(input, &output).unwrap();
        let root = output.join("catalog");
        let mut catalog =
            ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, None, None, None).unwrap();
        macro_rules! rows {
            ($kind:ty) => {
                catalog
                    .query::<$kind>(Some(vec![id.to_string()]), None, None, None, None, true)
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
            };
        }
        assert_eq!(rows!(TradeTick), trades);
        assert_eq!(rows!(QuoteTick), quotes);
        assert_eq!(rows!(OrderBookDelta), deltas);
    }

    #[test]
    fn contradictory_sibling_payouts_are_rejected_before_catalog_publication() {
        for prices in [
            ["1.0000", "1.0000"],
            ["0.8000", "0.8000"],
            ["0.0000", "0.0000"],
        ] {
            let mut input = archive();
            let other =
                InstrumentId::from_str("test-condition-987654321098765432109876543210.POLYMARKET")
                    .unwrap();
            let mut definition = serde_json::to_value(&input.instruments[0]).unwrap();
            definition["BinaryOption"]["id"] = other.to_string().into();
            definition["BinaryOption"]["raw_symbol"] = "987654321098765432109876543210".into();
            input
                .instruments
                .push(serde_json::from_value(definition).unwrap());
            input.closes = input
                .instruments
                .iter()
                .zip(prices)
                .map(|(instrument, price)| {
                    InstrumentClose::new(
                        instrument.id(),
                        Price::from(price),
                        InstrumentCloseType::ContractExpired,
                        1000_u64.into(),
                        1200_u64.into(),
                    )
                })
                .collect();
            let directory = tempfile::tempdir().unwrap();
            let output = directory.path().join("incoherent");
            assert!(import(input, &output).is_err());
            assert!(!output.exists());
        }
    }

    #[test]
    fn schema_and_timestamp_overflow_are_rejected() {
        assert!(epoch_ns(u64::MAX).is_err());
        assert_eq!(epoch_ns(1).unwrap().as_u64(), 1_000_000_000);
        let mut value = serde_json::to_value(archive()).unwrap();
        value["schema_version"] = 2.into();
        assert!(serde_json::from_value::<NativeArchive>(value).is_err());
    }
}
