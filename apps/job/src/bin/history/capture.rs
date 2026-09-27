//! A recorded live feed, with two independent connections and contemporaneous metadata.
//! This verifies the selected capture, not completeness of the exchange or its whole history.
use super::*;
use nautilus_core::UnixNanos;
use nautilus_model::{data::InstrumentClose, enums::InstrumentCloseType};
use nautilus_polymarket::{
    http::{
        models::GammaMarket,
        parse::{create_instrument_from_def, parse_gamma_market},
    },
    websocket::{
        messages::{PolymarketMarketResolved, PolymarketTrade},
        parse::parse_trade_tick,
    },
};

#[derive(clap::Args)]
pub struct Arguments {
    /// Fixed, checksummed lokima capture snapshot from snapshot.py.
    #[arg(long)]
    snapshot: PathBuf,
    #[arg(long)]
    market_slug: String,
    #[arg(long)]
    start_seconds: u64,
    #[arg(long)]
    end_seconds: u64,
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=86400))]
    bar_seconds: u32,
    #[arg(long)]
    pub output: PathBuf,
}

fn number(row: &Row, key: &str) -> Result<u64> {
    match field(row, key)? {
        Field::Int(v) => Ok(u64::try_from(*v)?),
        Field::Long(v) => Ok(u64::try_from(*v)?),
        _ => bail!("CAPTURE_INTEGER_REQUIRED:{key}"),
    }
}

fn event_ns(value: &serde_json::Value) -> Result<u64> {
    value
        .get("timestamp")
        .and_then(|v| v.as_str())
        .context("CAPTURE_EVENT_TIMESTAMP")?
        .parse::<u64>()?
        .checked_mul(1_000_000)
        .context("TIMESTAMP_RANGE")
}

fn for_rows(path: &Path, mut visit: impl FnMut(Row) -> Result<()>) -> Result<()> {
    let reader = SerializedFileReader::new(fs::File::open(path)?)?;
    ensure!(
        reader.metadata().file_metadata().num_rows() <= MAX_ROWS as i64,
        "CAPTURE_ROW_LIMIT"
    );
    for row in reader.get_row_iter(None)? {
        visit(row?)?;
    }
    Ok(())
}

struct Definition {
    instruments: Vec<InstrumentAny>,
    market: GammaMarket,
    at: u64,
    symbol: String,
    window: u64,
    raw: serde_json::Value,
}

fn definition(files: &[PathBuf], args: &Arguments) -> Result<Definition> {
    let mut found: Option<Definition> = None;
    for path in files
        .iter()
        .filter(|p| p.ends_with("data/reference/discovery.parquet"))
    {
        for_rows(path, |row| {
            if text(&row, "record_type")? != "rest_response"
                || text(&row, "channel")? != "gamma/markets"
                || text(&row, "venue")? != "polymarket_gamma"
                || text(&row, "slug")? != args.market_slug
            {
                return Ok(());
            }
            ensure!(
                number(&row, "schema_version")? == 1 && number(&row, "http_status")? == 200,
                "CAPTURE_DISCOVERY_RESPONSE"
            );
            let at = number(&row, "t_arrival_ns")?;
            let markets: Vec<serde_json::Value> = serde_json::from_str(text(&row, "payload_raw")?)?;
            for raw in markets {
                let market: GammaMarket = serde_json::from_value(raw.clone())?;
                if market.market_slug.as_deref() != Some(args.market_slug.as_str()) {
                    continue;
                }
                ensure!(
                    at <= epoch_ns(args.start_seconds)?.as_u64(),
                    "LATE_HISTORICAL_DEFINITION"
                );
                // The native parser has live defaults. A historical capture must supply these.
                ensure!(
                    market.order_price_min_tick_size.is_some()
                        && market.order_min_size.is_some_and(|v| v > Decimal::ZERO)
                        && market.start_date.is_some()
                        && market.end_date.is_some()
                        && market.fee_schedule.is_some()
                        && market.active == Some(true)
                        && market.closed == Some(false)
                        && market.accepting_orders == Some(true),
                    "CAPTURE_HISTORICAL_PARAMETERS_REQUIRED"
                );
                let instruments = parse_gamma_market(&market)?
                    .iter()
                    .map(|d| create_instrument_from_def(d, at.into()))
                    .collect::<Result<Vec<_>>>()?;
                // Native live parsing defaults an invalid start date to epoch zero.
                // Historical definitions must retain two actual, parseable dates.
                let original_ns = |value: Option<&str>| -> Result<u64> {
                    Ok(u64::try_from(
                        DateTime::parse_from_rfc3339(value.context("CAPTURE_DATE_REQUIRED")?)?
                            .timestamp_nanos_opt()
                            .context("CAPTURE_DATE_RANGE")?,
                    )?)
                };
                let activation = original_ns(market.start_date.as_deref())?;
                let expiration = original_ns(market.end_date.as_deref())?;
                for instrument in &instruments {
                    let value = serde_json::to_value(instrument)?;
                    let payload = &value["BinaryOption"];
                    let (start, end) = domain::prediction::instrument(payload)?;
                    domain::prediction::planning_fee(payload)?;
                    ensure!(
                        start == activation
                            && end == expiration
                            && epoch_ns(args.start_seconds)?.as_u64() >= start
                            && epoch_ns(args.end_seconds)?.as_u64() <= end,
                        "CAPTURE_LIFETIME"
                    );
                }
                if let Some(previous) = &found {
                    // Repeated observations must not silently change the frozen parameters.
                    let comparable = |items: &[InstrumentAny]| -> Result<Vec<serde_json::Value>> {
                        items
                            .iter()
                            .map(|i| {
                                let mut v = serde_json::to_value(historical_instrument(i)?)?;
                                v["BinaryOption"]
                                    .as_object_mut()
                                    .context("BINARY_OPTION")?
                                    .remove("ts_event");
                                v["BinaryOption"]
                                    .as_object_mut()
                                    .context("BINARY_OPTION")?
                                    .remove("ts_init");
                                Ok(v)
                            })
                            .collect()
                    };
                    ensure!(
                        comparable(&previous.instruments)? == comparable(&instruments)?,
                        "CAPTURE_PARAMETER_CHANGE"
                    );
                    if previous.at <= at {
                        continue;
                    }
                }
                found = Some(Definition {
                    instruments,
                    market,
                    at,
                    symbol: text(&row, "sym")?.to_owned(),
                    window: number(&row, "window_ts")?,
                    raw,
                });
            }
            Ok(())
        })?;
    }
    found.context("CAPTURE_DISCOVERY_MISSING")
}

struct Capture {
    trades: Vec<TradeTick>,
    print_keys: BTreeSet<String>,
    resolved: serde_json::Value,
    resolved_at: u64,
    rows: u64,
    first_at: u64,
    last_at: u64,
}

fn capture(path: &Path, def: &Definition, args: &Arguments, leg: &str) -> Result<Capture> {
    let start = epoch_ns(args.start_seconds)?.as_u64();
    let end = epoch_ns(args.end_seconds)?.as_u64();
    let instruments = def
        .instruments
        .iter()
        .map(|i| (i.raw_symbol().to_string(), i))
        .collect::<BTreeMap<_, _>>();
    let tokens = instruments.keys().cloned().collect::<BTreeSet<_>>();
    let mut sequence = 0;
    let mut last_at = 0;
    let mut last_mono = 0;
    let mut first_at = None;
    let mut subscribed = false;
    let mut disruptions = Vec::new();
    let mut trades = Vec::new();
    let mut print_keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut resolution: Option<(serde_json::Value, u64)> = None;
    for_rows(path, |row| {
        let at = number(&row, "t_arrival_ns")?;
        let mono = number(&row, "t_arrival_mono_ns")?;
        ensure!(
            number(&row, "schema_version")? == 1
                && number(&row, "window_ts")? == def.window
                && text(&row, "stream")? == format!("{}/{leg}", def.symbol)
                && text(&row, "venue")? == "polymarket_clob",
            "CAPTURE_STREAM_IDENTITY"
        );
        ensure!(
            number(&row, "stream_seq")? == sequence + 1
                && at >= last_at
                && mono >= last_mono
                && at > 0
                && mono > 0,
            "CAPTURE_SEQUENCE_OR_CLOCK_GAP"
        );
        sequence += 1;
        last_at = at;
        last_mono = mono;
        first_at.get_or_insert(at);
        match text(&row, "record_type")? {
            "feed_event" => {
                let channel = text(&row, "channel")?;
                if channel == "subscribe" {
                    let frame: serde_json::Value = serde_json::from_str(text(&row, "frame")?)?;
                    let assets: BTreeSet<String> =
                        serde_json::from_value(frame["assets_ids"].clone())?;
                    ensure!(
                        assets == tokens && frame["custom_feature_enabled"] == true,
                        "CAPTURE_SUBSCRIPTION"
                    );
                    if at <= start {
                        subscribed = true;
                    }
                } else if channel != "connected" || sequence != 1 {
                    disruptions.push(at);
                }
                return Ok(());
            }
            "ws_frame" => (),
            _ => bail!("CAPTURE_RECORD_TYPE"),
        }
        let raw = text(&row, "payload_raw")?;
        ensure!(raw.len() <= 1024 * 1024, "CAPTURE_FRAME_LIMIT");
        if matches!(raw, "PONG" | "PING" | "pong" | "ping") {
            return Ok(());
        }
        let payload: serde_json::Value = serde_json::from_str(raw)?;
        let items = if let Some(items) = payload.as_array() {
            items.clone()
        } else {
            vec![payload]
        };
        for item in items {
            if item.get("market").and_then(|v| v.as_str()) != Some(def.market.condition_id.as_str())
            {
                continue;
            }
            match item.get("event_type").and_then(|v| v.as_str()) {
                Some("last_trade_price") => {
                    let event = event_ns(&item)?;
                    if !(start..end).contains(&event) {
                        continue;
                    }
                    ensure!(event <= at && at >= def.at, "CAPTURE_FUTURE_EVENT");
                    let trade: PolymarketTrade = serde_json::from_value(item.clone())?;
                    let instrument = instruments
                        .get(trade.asset_id.as_str())
                        .context("CAPTURE_UNKNOWN_TOKEN")?;
                    let tick = parse_trade_tick(
                        &trade,
                        instrument.id(),
                        instrument.price_precision(),
                        instrument.size_precision(),
                        at.into(),
                    )?;
                    ensure!(
                        tick.price.as_decimal() == Decimal::from_str(&trade.price)?
                            && tick.size.as_decimal() == Decimal::from_str(&trade.size)?
                            && tick.price.as_decimal() % instrument.price_increment().as_decimal()
                                == Decimal::ZERO,
                        "CAPTURE_LOSSY_TRADE"
                    );
                    // A transaction can contain several fills. Neither transaction hash nor a
                    // repeated price/size/timestamp tuple is sufficient to invent a distinct fill.
                    ensure!(
                        ids.insert((tick.instrument_id, tick.trade_id)),
                        "CAPTURE_AMBIGUOUS_TRADE_ID"
                    );
                    ensure!(
                        print_keys.insert(serde_json::to_string(&item)?),
                        "CAPTURE_DUPLICATE_PRINT"
                    );
                    trades.push(tick);
                }
                Some("tick_size_change") => {
                    ensure!(
                        event_ns(&item)? >= end && at >= end,
                        "CAPTURE_PARAMETER_CHANGE"
                    );
                }
                Some("market_resolved") => {
                    let event = event_ns(&item)?;
                    ensure!(event >= end && event <= at, "CAPTURE_RESOLUTION_TIME");
                    let message: PolymarketMarketResolved = serde_json::from_value(item.clone())?;
                    ensure!(
                        message.assets_ids.into_iter().collect::<BTreeSet<_>>() == tokens
                            && tokens.contains(&message.winning_asset_id),
                        "CAPTURE_PAYOUT_IDENTITY"
                    );
                    let winner = serde_json::to_value(instruments[&message.winning_asset_id])?;
                    ensure!(
                        winner["BinaryOption"]["outcome"].as_str()
                            == Some(message.winning_outcome.as_str()),
                        "CAPTURE_PAYOUT_LABEL"
                    );
                    if let Some((previous, _)) = &resolution {
                        ensure!(previous == &item, "CAPTURE_CONFLICTING_RESOLUTION");
                    } else {
                        resolution = Some((item, at));
                    }
                }
                _ => (),
            }
        }
        Ok(())
    })?;
    let (resolved, resolved_at) = resolution.context("CAPTURE_RESOLUTION_MISSING")?;
    ensure!(
        subscribed
            && first_at.is_some_and(|v| v <= start)
            && last_at >= resolved_at
            && disruptions.iter().all(|&at| at > resolved_at),
        "CAPTURE_INTERRUPTED"
    );
    ensure!(
        !trades.is_empty() && trades.len() <= MAX_ROWS,
        "CAPTURE_EMPTY_OR_TOO_LARGE"
    );
    Ok(Capture {
        trades,
        print_keys,
        resolved,
        resolved_at,
        rows: sequence,
        first_at: first_at.unwrap(),
        last_at,
    })
}

pub fn prepare(args: &Arguments) -> Result<NativeArchive> {
    // The pinned live parser uses pUSD; this source format is a post-migration capture.
    ensure!(
        args.start_seconds >= 1777420800
            && args.start_seconds < args.end_seconds
            && args.end_seconds <= u64::try_from(Utc::now().timestamp())?
            && args
                .start_seconds
                .is_multiple_of(u64::from(args.bar_seconds))
            && args.end_seconds.is_multiple_of(u64::from(args.bar_seconds)),
        "FULL_BAR_INTERVALS_REQUIRED"
    );
    let snapshot: Snapshot = read_json(&args.snapshot)?;
    let root = args.snapshot.parent().context("SNAPSHOT_ROOT")?;
    let files = verified_files(&snapshot, root)?;
    let def = definition(&files, args)?;
    let file = |leg| -> Result<&PathBuf> {
        let suffix = format!("data/clob_ws/{}/{}/{leg}.parquet", def.symbol, def.window);
        let paths = files
            .iter()
            .filter(|p| p.ends_with(&suffix))
            .collect::<Vec<_>>();
        ensure!(paths.len() == 1, "CAPTURE_BOTH_CONNECTIONS_REQUIRED");
        Ok(paths[0])
    };
    let primary = capture(file("poly_a")?, &def, args, "poly_a")?;
    let secondary = capture(file("poly_b")?, &def, args, "poly_b")?;
    ensure!(
        primary.print_keys == secondary.print_keys && primary.resolved == secondary.resolved,
        "CAPTURE_CONNECTIONS_DISAGREE"
    );
    let retrieved_ns = u64::try_from(
        snapshot
            .retrieved_at
            .timestamp_nanos_opt()
            .context("CLOCK_RANGE")?,
    )?;
    ensure!(
        primary.last_at <= retrieved_ns && secondary.last_at <= retrieved_ns,
        "FUTURE_SOURCE_OBSERVATION"
    );
    let reference = format!(
        "https://huggingface.co/datasets/{}/tree/{}",
        snapshot.repository, snapshot.revision
    );
    let event = UnixNanos::from(event_ns(&primary.resolved)?);
    let mut archive = NativeArchive {
        schema_version: contracts::SchemaV1,
        source_reference: reference,
        source_observed_at: snapshot.retrieved_at,
        source_metadata: serde_json::json!({
            "snapshot": snapshot, "format": "lokima-dual-capture", "market": def.raw,
            "definition_available_ns": def.at,
            "selection": {"start_seconds":args.start_seconds,"end_seconds":args.end_seconds,"bar_seconds":args.bar_seconds},
            "capture_check": {"primary_rows":primary.rows,"secondary_rows":secondary.rows,
                "primary_first_ns":primary.first_at,"primary_last_ns":primary.last_at,
                "matched_trade_prints":primary.trades.len(),"matching_resolution":primary.resolved,
                "resolution_available_ns":primary.resolved_at,"secondary_resolution_available_ns":secondary.resolved_at},
            "availability": "Original CLOCK_REALTIME receive times from poly_a; dual-connection audit preserves those times. Bars use max(interval end, contributing receive times). No retrospective REST trade/label backfill is consumed.",
            "coverage": "Matching trade-print multisets and uninterrupted recorded connections for this selection only. Local stream_seq is not an exchange sequence; neither chain completeness nor all-market history is certified.",
            "settlement": "Original market_resolved payout and event/receive times; not a last-price or strike comparison.",
        }),
        instruments: def.instruments,
        trades: primary.trades,
        quotes: Vec::new(),
        deltas: Vec::new(),
        bars: Vec::new(),
        closes: Vec::new(),
    };
    for instrument in &archive.instruments {
        let payout = if instrument.raw_symbol().as_str()
            == primary.resolved["winning_asset_id"]
                .as_str()
                .context("CAPTURE_WINNER")?
        {
            Decimal::ONE
        } else {
            Decimal::ZERO
        };
        archive.closes.push(InstrumentClose::new(
            instrument.id(),
            Price::from_decimal_dp(payout, instrument.price_precision())?,
            InstrumentCloseType::ContractExpired,
            event,
            primary.resolved_at.into(),
        ));
    }
    // Stable sorting retains original frame/array order for equal native timestamps.
    archive
        .trades
        .sort_by_key(|t| (t.instrument_id, t.ts_event, t.ts_init));
    aggregate(&mut archive, args.bar_seconds)?;
    // Restatement can make availability nonmonotonic. Do not shift its timestamp to pass admission.
    for bars in archive.bars.chunk_by(|a, b| a.bar_type == b.bar_type) {
        ensure!(
            bars.windows(2).all(|p| p[0].ts_init < p[1].ts_init),
            "CAPTURE_NONMONOTONIC_BAR_AVAILABILITY"
        );
    }
    super::super::validate(&archive)?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::{
        data_type::{ByteArrayType, Int64Type},
        file::writer::SerializedFileWriter,
        schema::parser::parse_message_type,
    };
    use std::sync::Arc;

    const START: u64 = 1785357600;
    fn row(
        seq: u64,
        at: u64,
        kind: &str,
        channel: &str,
        payload: serde_json::Value,
        leg: &str,
    ) -> Row {
        let mut columns = BTreeMap::new();
        for (key, value) in [
            ("schema_version", 1),
            ("window_ts", START),
            ("stream_seq", seq),
            ("t_arrival_ns", at),
            ("t_arrival_mono_ns", at),
            ("http_status", 200),
        ] {
            columns.insert(key.to_string(), Field::Long(value as i64));
        }
        for (key, value) in [
            ("record_type", kind.to_owned()),
            ("channel", channel.to_owned()),
            (
                "venue",
                if kind == "rest_response" {
                    "polymarket_gamma"
                } else {
                    "polymarket_clob"
                }
                .to_owned(),
            ),
            ("slug", "fixture-window".into()),
            ("sym", "btc".into()),
            ("stream", format!("btc/{leg}")),
            ("payload_raw", payload.to_string()),
            (
                "frame",
                serde_json::json!({"assets_ids":["123","456"],"custom_feature_enabled":true})
                    .to_string(),
            ),
        ] {
            columns.insert(key.into(), Field::Str(value));
        }
        Row::new(columns.into_iter().collect())
    }

    fn write(root: &Path, relative: &str, rows: &[Row]) -> SourceFile {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let schema = rows[0]
            .get_column_iter()
            .map(|(k, v)| {
                format!(
                    "REQUIRED {} {k}{};",
                    if matches!(v, Field::Long(_)) {
                        "INT64"
                    } else {
                        "BINARY"
                    },
                    if matches!(v, Field::Str(_)) {
                        " (UTF8)"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let mut writer = SerializedFileWriter::new(
            fs::File::create(&path).unwrap(),
            Arc::new(parse_message_type(&format!("message fixture {{ {schema} }}")).unwrap()),
            Default::default(),
        )
        .unwrap();
        let mut group = writer.next_row_group().unwrap();
        for (key, value) in rows[0].get_column_iter() {
            let mut column = group.next_column().unwrap().unwrap();
            if matches!(value, Field::Long(_)) {
                column
                    .typed::<Int64Type>()
                    .write_batch(
                        &rows
                            .iter()
                            .map(|r| number(r, key).unwrap() as i64)
                            .collect::<Vec<_>>(),
                        None,
                        None,
                    )
                    .unwrap();
            } else {
                column
                    .typed::<ByteArrayType>()
                    .write_batch(
                        &rows
                            .iter()
                            .map(|r| text(r, key).unwrap().into())
                            .collect::<Vec<_>>(),
                        None,
                        None,
                    )
                    .unwrap();
            }
            column.close().unwrap();
        }
        group.close().unwrap();
        writer.close().unwrap();
        SourceFile {
            path: relative.into(),
            size: fs::metadata(&path).unwrap().len(),
            sha256: digest(&path).unwrap(),
            url: "FIXTURE".into(),
        }
    }

    fn fixture(root: &Path, failure: &str) -> Arguments {
        let start = START * 1_000_000_000;
        let mut market = serde_json::json!({"id":"fixture", "conditionId":"condition", "slug":"fixture-window",
            "question":"Synthetic test only", "outcomes":"[\"Up\",\"Down\"]", "clobTokenIds":"[\"123\",\"456\"]",
            "startDate":"2026-07-29T20:30:00Z", "endDate":"2026-07-29T20:41:00Z", "active":true,
            "closed":false,"acceptingOrders":true,"orderPriceMinTickSize":0.01,"orderMinSize":5,
            "feeSchedule":{"rate":0.07,"exponent":1,"takerOnly":true,"rebateRate":0.2}});
        if failure == "fees" {
            market.as_object_mut().unwrap().remove("feeSchedule");
        }
        if failure == "date" {
            market["startDate"] = serde_json::json!("not-a-date");
        }
        let available = if failure == "metadata" {
            start + 1
        } else {
            start - 10_000_000_000
        };
        let mut files = vec![write(
            root,
            "data/reference/discovery.parquet",
            &[
                row(
                    1,
                    available,
                    "rest_response",
                    "gamma/markets",
                    serde_json::json!([market]),
                    "discovery",
                ),
                row(
                    2,
                    available + 1,
                    "rest_response",
                    "clob/markets",
                    serde_json::json!({"non_gamma":"object"}),
                    "discovery",
                ),
            ],
        )];
        for leg in ["poly_a", "poly_b"] {
            let shift = u64::from(leg == "poly_b");
            let mut trade = serde_json::json!({"market":"condition","event_type":"last_trade_price","asset_id":"123",
                "price":if failure=="different" && leg=="poly_b" {"0.51"} else {"0.50"}, "size":"2.000001",
                "fee_rate_bps":"0", "side":"BUY", "timestamp":((START+1)*1000).to_string()});
            if failure == "same_time" {
                let mut first = trade.clone();
                first["price"] = serde_json::json!("0.60");
                trade["price"] = serde_json::json!("0.40");
                trade = serde_json::json!([first, trade]);
            }
            let resolved = serde_json::json!({"id":"fixture", "market":"condition","assets_ids":["123","456"],
                "winning_asset_id":"456", "winning_outcome":"Down", "timestamp":((START+65)*1000).to_string(),
                "tags":[],"event_type":"market_resolved"});
            let mut rows = vec![
                row(
                    1,
                    start - 9_000_000_000,
                    "feed_event",
                    "connected",
                    serde_json::Value::Null,
                    leg,
                ),
                row(
                    2,
                    start - 8_000_000_000,
                    "feed_event",
                    "subscribe",
                    serde_json::Value::Null,
                    leg,
                ),
                row(
                    if failure == "gap" { 4 } else { 3 },
                    start + 2_005_000_000 + shift,
                    "ws_frame",
                    "market",
                    trade.clone(),
                    leg,
                ),
            ];
            if failure == "duplicate" {
                rows.push(row(
                    4,
                    start + 3_000_000_000,
                    "ws_frame",
                    "market",
                    trade,
                    leg,
                ));
            }
            if failure == "tick" {
                rows.push(row(4,start+3_000_000_000,"ws_frame","market",serde_json::json!({
                "market":"condition","event_type":"tick_size_change","timestamp":((START+3)*1000).to_string()}),leg));
            }
            let seq = rows.len() as u64 + 1;
            rows.push(row(
                seq,
                start + 65_005_000_000 + shift,
                "ws_frame",
                "market",
                resolved,
                leg,
            ));
            rows.push(row(
                seq + 1,
                start + 66_000_000_000,
                "feed_event",
                "read_error",
                serde_json::Value::Null,
                leg,
            ));
            files.push(write(
                root,
                &format!("data/clob_ws/btc/{START}/{leg}.parquet"),
                &rows,
            ));
        }
        let snapshot = Snapshot {
            schema_version: contracts::SchemaV1,
            repository: "fixture/synthetic-capture".into(),
            revision: "a".repeat(40),
            license: "FIXTURE".into(),
            license_reference: "FIXTURE".into(),
            retrieved_at: Utc::now(),
            files,
        };
        let manifest = root.join("snapshot.json");
        fs::write(&manifest, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        Arguments {
            snapshot: manifest,
            market_slug: "fixture-window".into(),
            start_seconds: START,
            end_seconds: START + 60,
            bar_seconds: 1,
            output: root.join("native"),
        }
    }

    #[test]
    fn captured_parquet_preserves_late_arrival_original_metadata_and_complete_payout() {
        let root = tempfile::tempdir().unwrap();
        let args = fixture(root.path(), "");
        let archive = prepare(&args).unwrap();
        assert_eq!(archive.trades.len(), 1);
        assert_eq!(archive.bars.len(), 1); // empty seconds and the untraded sibling stay empty
        assert_eq!(
            archive.bars[0].ts_init.as_u64(),
            START * 1_000_000_000 + 2_005_000_000
        );
        assert_eq!(archive.closes.len(), 2);
        assert!(archive
            .closes
            .iter()
            .all(|c| c.ts_init.as_u64() == START * 1_000_000_000 + 65_005_000_000));
        assert_eq!(
            archive.instruments[0].ts_init().as_u64(),
            (START - 10) * 1_000_000_000
        );
        let report = crate::import(archive, &args.output).unwrap();
        assert_eq!(report.bars, 1);
        assert_eq!(report.closes, 2);
        assert!(!report.registered_in_quazonai);
        let mut catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
            args.output.join("catalog").to_str().unwrap(),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let bars = catalog
            .query::<Bar>(None, None, None, None, None, true)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(bars.len(), 1);
    }

    #[test]
    fn equal_timestamp_prints_preserve_original_frame_array_order() {
        let root = tempfile::tempdir().unwrap();
        let args = fixture(root.path(), "same_time");
        let archive = prepare(&args).unwrap();
        assert_eq!(archive.trades.len(), 2);
        assert_eq!(archive.bars.len(), 1);
        assert_eq!(archive.bars[0].open, Price::from("0.60"));
        assert_eq!(archive.bars[0].close, Price::from("0.40"));
    }

    #[test]
    fn rejects_missing_or_contradictory_capture_evidence() {
        for failure in [
            "fees",
            "date",
            "metadata",
            "gap",
            "different",
            "duplicate",
            "tick",
        ] {
            let root = tempfile::tempdir().unwrap();
            let args = fixture(root.path(), failure);
            assert!(prepare(&args).is_err(), "accepted {failure}");
            assert!(!args.output.exists());
        }
    }
}
