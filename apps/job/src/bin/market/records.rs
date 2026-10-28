use super::{Report, Stream};
use anyhow::{ensure, Context, Result};
use nautilus_core::UnixNanos;
use nautilus_data::aggregation::BarBuilder;
use nautilus_model::{
    data::{
        Bar, BarType, BookOrder, Data, FundingRateUpdate, IndexPriceUpdate, MarkPriceUpdate,
        OrderBookDelta, QuoteTick, TradeTick,
    },
    enums::{AggregationSource, AggressorSide, BookAction, OrderSide, RecordFlag},
    identifiers::{InstrumentId, TradeId},
    instruments::{Instrument, InstrumentAny},
    types::{fixed::FIXED_PRECISION, Price, Quantity},
};
use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::{Field, Row},
};
use rust_decimal::Decimal;
use std::{collections::BTreeMap, fs::File, io::Read, path::Path, str::FromStr};

pub(super) struct Encoding {
    id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
}

impl Encoding {
    pub(super) fn new(
        id: InstrumentId,
        instrument: Option<&InstrumentAny>,
        native: bool,
    ) -> Result<Self> {
        let (price_precision, size_precision) = if native {
            let precision = nautilus_model::types::fixed::FIXED_PRECISION;
            (precision, precision)
        } else {
            let instrument = instrument
                .context("INSTRUMENT_PARAMETERS_MISSING; use --native-precision for raw records")?;
            (instrument.price_precision(), instrument.size_precision())
        };
        Ok(Self {
            id,
            price_precision,
            size_precision,
        })
    }
}

fn field<'a>(row: &'a Row, name: &str) -> Result<&'a Field> {
    row.get_column_iter()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, value)| value)
        .with_context(|| format!("SOURCE_COLUMN_MISSING: {name}"))
}

impl Stream {
    fn bar_interval(&self, id: InstrumentId) -> Result<(BarType, u64)> {
        match (self.bar_seconds, self.bar_milliseconds) {
            (Some(seconds), None) => {
                Ok((bar_type(id, seconds)?, u64::from(seconds) * 1_000_000_000))
            }
            (None, Some(milliseconds)) => {
                ensure!(
                    milliseconds > 0 && milliseconds <= 86_400_000,
                    "BAR_INTERVAL_UNSUPPORTED"
                );
                Ok((
                    BarType::from_str(&format!("{id}-{milliseconds}-MILLISECOND-LAST-EXTERNAL"))?,
                    u64::from(milliseconds) * 1_000_000,
                ))
            }
            (None, None) => anyhow::bail!("BAR_INTERVAL_REQUIRED"),
            (Some(_), Some(_)) => anyhow::bail!("BAR_INTERVAL_UNITS_CONFLICT"),
        }
    }

    pub(super) fn field<'a>(&self, row: &'a Row, name: &str) -> Result<&'a Field> {
        field(
            row,
            self.columns
                .get(name)
                .with_context(|| format!("COLUMN_MAPPING_MISSING: {name}"))?,
        )
    }

    pub(super) fn matches(&self, row: &Row) -> Result<bool> {
        for (filters, excluded) in [(&self.equals, false), (&self.not_equals, true)] {
            for (name, expected) in filters {
                let actual = field(row, name)?;
                let matches = match (actual, expected) {
                    (_, serde_json::Value::Array(_) | serde_json::Value::Object(_)) => {
                        anyhow::bail!("FILTER_SCALAR_REQUIRED")
                    }
                    (Field::Bool(a), serde_json::Value::Bool(b)) => a == b,
                    (Field::Str(a), serde_json::Value::Bool(b)) => a.parse::<bool>()? == *b,
                    (Field::Str(a), serde_json::Value::String(b)) => a == b,
                    (Field::Null, serde_json::Value::Null) => true,
                    (Field::Null, _) => false,
                    (_, serde_json::Value::Number(b)) => {
                        decimal(actual)? == Decimal::from_str_exact(&b.to_string())?
                    }
                    _ => false,
                };
                if matches == excluded {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

pub(super) fn scalar(value: &Field) -> Result<String> {
    Ok(match value {
        Field::Str(v) => v.clone(),
        Field::Decimal(_) => value.to_string(),
        Field::Byte(v) => v.to_string(),
        Field::Short(v) => v.to_string(),
        Field::Int(v) => v.to_string(),
        Field::Long(v) | Field::TimestampMillis(v) | Field::TimestampMicros(v) => v.to_string(),
        Field::UByte(v) => v.to_string(),
        Field::UShort(v) => v.to_string(),
        Field::UInt(v) => v.to_string(),
        Field::ULong(v) => v.to_string(),
        _ => anyhow::bail!("EXACT_SCALAR_REQUIRED"),
    })
}

pub(super) fn timestamp(value: &Field, unit: &str) -> Result<u64> {
    if unit == "rfc3339" {
        let at = chrono::DateTime::parse_from_rfc3339(&scalar(value)?)?;
        return Ok(u64::try_from(
            at.timestamp_nanos_opt().context("TIMESTAMP_RANGE")?,
        )?);
    }
    ensure!(
        !matches!(value, Field::TimestampMillis(_)) || unit == "ms",
        "TIMESTAMP_UNIT_MISMATCH"
    );
    ensure!(
        !matches!(value, Field::TimestampMicros(_)) || unit == "us",
        "TIMESTAMP_UNIT_MISMATCH"
    );
    let multiplier = match unit {
        "s" => 1_000_000_000,
        "ms" => 1_000_000,
        "us" => 1_000,
        "ns" => 1,
        _ => anyhow::bail!("TIMESTAMP_UNIT_UNSUPPORTED"),
    };
    scalar(value)?
        .parse::<u64>()?
        .checked_mul(multiplier)
        .context("TIMESTAMP_RANGE")
}

fn decimal(value: &Field) -> Result<Decimal> {
    Ok(Decimal::from_str_exact(&scalar(value)?)?)
}

fn price_with_precision(value: &Field, precision: u8) -> Result<Price> {
    let original = decimal(value)?;
    let native = Price::from_decimal_dp(original, precision)?;
    ensure!(native.as_decimal() == original, "PRICE_PRECISION_LOSS");
    ensure!(original > Decimal::ZERO, "NONPOSITIVE_MARKET_PRICE");
    Ok(native)
}

fn quantity(value: &Field, precision: u8) -> Result<Quantity> {
    let original = decimal(value)?;
    let native = Quantity::from_decimal_dp(original, precision)?;
    ensure!(native.as_decimal() == original, "SIZE_PRECISION_LOSS");
    Ok(native)
}

pub(super) fn read_rows(
    path: &Path,
    format: &str,
    mut consume: impl FnMut(Row) -> Result<()>,
) -> Result<()> {
    match format {
        "parquet" => {
            let reader = SerializedFileReader::new(File::open(path)?)?;
            for row in reader.get_row_iter(None)? {
                consume(row?)?;
            }
        }
        "csv" | "csv-lz4" => {
            let file = File::open(path)?;
            let input: Box<dyn Read> = if format == "csv-lz4" {
                Box::new(lz4_flex::frame::FrameDecoder::new(file))
            } else {
                Box::new(file)
            };
            let mut reader = csv::Reader::from_reader(input);
            let headers = reader.headers()?.clone();
            for record in reader.records() {
                consume(Row::new(
                    headers
                        .iter()
                        .zip(record?.iter())
                        .map(|(key, value)| {
                            (
                                key.to_owned(),
                                if value.is_empty() {
                                    Field::Null
                                } else {
                                    Field::Str(value.to_owned())
                                },
                            )
                        })
                        .collect(),
                ))?;
            }
        }
        _ => anyhow::bail!("SOURCE_FORMAT_UNSUPPORTED"),
    }
    Ok(())
}

fn bar_type(id: InstrumentId, seconds: u32) -> Result<BarType> {
    ensure!(seconds > 0 && seconds <= 86400, "BAR_INTERVAL_UNSUPPORTED");
    let (divisor, unit) = [
        (86400, "DAY"),
        (3600, "HOUR"),
        (60, "MINUTE"),
        (1, "SECOND"),
    ]
    .into_iter()
    .find(|(divisor, _)| seconds.is_multiple_of(*divisor))
    .context("BAR_INTERVAL_UNSUPPORTED")?;
    Ok(BarType::from_str(&format!(
        "{id}-{}-{unit}-LAST-EXTERNAL",
        seconds / divisor
    ))?)
}

fn snapshot(
    encoding: &Encoding,
    sides: [Vec<(Price, Quantity)>; 2],
    at: UnixNanos,
    init: UnixNanos,
    report: &mut Report,
) -> Result<Vec<Data>> {
    let id = encoding.id;
    let mut clear = OrderBookDelta::clear(id, 0, at, init);
    // Clear has no price or size; its metadata must match the snapshot encoding.
    clear.order.price = Price::zero(encoding.price_precision);
    clear.order.size = Quantity::zero(encoding.size_precision);
    let mut data = vec![Data::Delta(clear)];
    for (levels, side) in sides.iter().zip([OrderSide::Buy, OrderSide::Sell]) {
        for (p, q) in levels {
            data.push(Data::Delta(OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(side, *p, *q, 0),
                RecordFlag::F_SNAPSHOT as u8,
                0,
                at,
                init,
            )));
        }
    }
    let Some(Data::Delta(last)) = data.last_mut() else {
        unreachable!()
    };
    last.flags |= RecordFlag::F_LAST as u8;
    match (sides[0].first(), sides[1].first()) {
        (Some((bid, bid_size)), Some((ask, ask_size))) => {
            ensure!(bid <= ask, "BOOK_CROSSED");
            data.push(Data::Quote(QuoteTick::new_checked(
                id, *bid, *ask, *bid_size, *ask_size, at, init,
            )?));
        }
        (None, None) => report.empty_books += 1,
        _ => report.one_sided_books += 1,
    }
    Ok(data)
}

pub(super) fn convert(
    stream: &Stream,
    row: &Row,
    encoding: &Encoding,
    event: u64,
    available: u64,
    report: &mut Report,
) -> Result<Vec<Data>> {
    let at: UnixNanos = event.into();
    let init: UnixNanos = available.into();
    let p = |name| price_with_precision(stream.field(row, name)?, encoding.price_precision);
    let q = |name| quantity(stream.field(row, name)?, encoding.size_precision);
    let id = encoding.id;
    Ok(match stream.kind.as_str() {
        "trades" => {
            let side = scalar(stream.field(row, "side")?)?;
            let side = stream.sides.get(&side).context("UNMAPPED_AGGRESSOR_SIDE")?;
            let side = AggressorSide::from_str(side)?;
            let trade_id = TradeId::new_checked(scalar(stream.field(row, "trade_id")?)?)?;
            let size = q("size")?;
            ensure!(size.as_decimal() > Decimal::ZERO, "NONPOSITIVE_TRADE_SIZE");
            vec![Data::Trade(TradeTick::new_checked(
                id,
                p("price")?,
                size,
                side,
                trade_id,
                at,
                init,
            )?)]
        }
        "bars" => {
            let (kind, interval_ns) = stream.bar_interval(id)?;
            let label = match stream.bar_timestamp.as_deref() {
                Some("start") => event
                    .checked_add(interval_ns)
                    .context("BAR_TIMESTAMP_RANGE")?,
                Some("end") => event,
                _ => anyhow::bail!("BAR_TIMESTAMP_CONVENTION_REQUIRED"),
            };
            vec![Data::Bar(Bar::new_checked(
                kind,
                p("open")?,
                p("high")?,
                p("low")?,
                p("close")?,
                q("volume")?,
                label.into(),
                available.max(label).into(),
            )?)]
        }
        "books" => {
            let mut sides = [Vec::new(), Vec::new()];
            for (name, parsed) in ["bids", "asks"].into_iter().zip(&mut sides) {
                let Field::ListInternal(levels) = stream.field(row, name)? else {
                    anyhow::bail!("BOOK_LEVEL_LIST_REQUIRED")
                };
                for level in levels.elements() {
                    let Field::Group(level) = level else {
                        anyhow::bail!("BOOK_LEVEL_STRUCT_REQUIRED")
                    };
                    let p = price_with_precision(
                        stream.field(level, "price")?,
                        encoding.price_precision,
                    )?;
                    let q = quantity(stream.field(level, "size")?, encoding.size_precision)?;
                    ensure!(q.as_decimal() > Decimal::ZERO, "NONPOSITIVE_BOOK_SIZE");
                    if let Some((previous, _)) = parsed.last() {
                        ensure!(
                            if name == "bids" {
                                *previous > p
                            } else {
                                *previous < p
                            },
                            "BOOK_LEVEL_ORDER_INVALID"
                        );
                    }
                    parsed.push((p, q));
                }
            }
            snapshot(encoding, sides, at, init, report)?
        }
        "context" => {
            let mut data = Vec::new();
            for name in ["funding_rate", "mark_price", "index_price"] {
                if stream.columns.contains_key(name)
                    && !matches!(stream.field(row, name)?, Field::Null)
                {
                    data.push(match name {
                        "funding_rate" => Data::FundingRate(FundingRateUpdate::new(
                            id,
                            decimal(stream.field(row, name)?)?,
                            None,
                            None,
                            at,
                            init,
                        )),
                        // Valuation prices need not lie on an executable instrument tick.
                        "mark_price" => Data::MarkPrice(MarkPriceUpdate::new(
                            id,
                            price_with_precision(
                                stream.field(row, name)?,
                                nautilus_model::types::fixed::FIXED_PRECISION,
                            )?,
                            at,
                            init,
                        )),
                        _ => Data::IndexPrice(IndexPriceUpdate::new(
                            id,
                            price_with_precision(
                                stream.field(row, name)?,
                                nautilus_model::types::fixed::FIXED_PRECISION,
                            )?,
                            at,
                            init,
                        )),
                    });
                }
            }
            data
        }
        _ => anyhow::bail!("SOURCE_KIND_UNSUPPORTED"),
    })
}

struct Bucket {
    builder: BarBuilder,
    kind: BarType,
    end: u64,
    available: u64,
    volume: Quantity,
    precision: (u8, u8),
    last_event: u64,
}

pub(super) fn with_bar_precision(mut bar: Bar, precision: (u8, u8)) -> Bar {
    for price in [&mut bar.open, &mut bar.high, &mut bar.low, &mut bar.close] {
        *price = Price::from_raw(price.raw, precision.0);
    }
    bar.volume = Quantity::from_raw(bar.volume.raw, precision.1);
    bar
}

impl Bucket {
    fn build(&mut self) -> Bar {
        let bar = with_bar_precision(
            self.builder.build(self.end.into(), self.available.into()),
            self.precision,
        );
        Bar {
            bar_type: self.kind,
            ..bar
        }
    }
}

pub(super) struct Aggregation {
    seconds: Option<u32>,
    milliseconds: Option<u32>,
    buckets: BTreeMap<InstrumentId, Bucket>,
}

impl Aggregation {
    pub(super) fn new(seconds: Option<u32>) -> Self {
        Self {
            seconds,
            milliseconds: None,
            buckets: BTreeMap::new(),
        }
    }

    pub(super) fn with_milliseconds(
        seconds: Option<u32>,
        milliseconds: Option<u32>,
    ) -> Result<Self> {
        ensure!(
            seconds.is_none() || milliseconds.is_none(),
            "BAR_INTERVAL_UNITS_CONFLICT"
        );
        if let Some(interval) = milliseconds {
            ensure!(
                interval > 0 && interval <= 86_400_000,
                "BAR_INTERVAL_UNSUPPORTED"
            );
        }
        let mut result = Self::new(seconds);
        result.milliseconds = milliseconds;
        Ok(result)
    }

    pub(super) fn update(&mut self, trade: &TradeTick) -> Result<Option<Bar>> {
        let (kind, interval) = match (self.seconds, self.milliseconds) {
            (Some(seconds), None) => (
                bar_type(trade.instrument_id, seconds)?,
                u64::from(seconds) * 1_000_000_000,
            ),
            (None, Some(milliseconds)) => (
                BarType::from_str(&format!(
                    "{}-{milliseconds}-MILLISECOND-LAST-EXTERNAL",
                    trade.instrument_id
                ))?,
                u64::from(milliseconds) * 1_000_000,
            ),
            (None, None) => return Ok(None),
            _ => anyhow::bail!("BAR_INTERVAL_UNITS_CONFLICT"),
        };
        let event = trade.ts_event.as_u64();
        let end = (event / interval + 1)
            .checked_mul(interval)
            .context("BAR_TIMESTAMP_RANGE")?;
        // Use one exact encoding while building; publish each interval at its maximum observed precision.
        let price = Price::from_raw(trade.price.raw, FIXED_PRECISION);
        let size = Quantity::from_raw(trade.size.raw, FIXED_PRECISION);
        let bucket = self.buckets.entry(trade.instrument_id).or_insert(Bucket {
            builder: BarBuilder::new(
                BarType::new(
                    kind.instrument_id(),
                    kind.spec(),
                    AggregationSource::Internal,
                ),
                FIXED_PRECISION,
                FIXED_PRECISION,
            ),
            kind,
            end,
            available: end,
            volume: Quantity::zero(FIXED_PRECISION),
            precision: (trade.price.precision, trade.size.precision),
            last_event: event,
        });
        ensure!(
            event >= bucket.last_event,
            "TRADE_AGGREGATION_ORDER_INVALID"
        );
        let completed = if bucket.end != end {
            let bar = bucket.build();
            bucket.end = end;
            bucket.available = end;
            bucket.volume = Quantity::zero(FIXED_PRECISION);
            bucket.precision = (trade.price.precision, trade.size.precision);
            Some(bar)
        } else {
            None
        };
        bucket.precision.0 = bucket.precision.0.max(trade.price.precision);
        bucket.precision.1 = bucket.precision.1.max(trade.size.precision);
        bucket.volume = bucket
            .volume
            .checked_add(size)
            .context("BAR_VOLUME_RANGE")?;
        bucket.available = bucket.available.max(trade.ts_init.as_u64());
        bucket.last_event = event;
        bucket.builder.update(price, size, trade.ts_event);
        Ok(completed)
    }

    pub(super) fn finish(self) -> Result<Vec<Bar>> {
        Ok(self.buckets.into_values().map(|mut b| b.build()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_filters_reject_nonscalar_conditions_including_on_null_rows() {
        for filter in ["equals", "not_equals"] {
            for expected in [serde_json::json!([]), serde_json::json!({})] {
                let mut value = serde_json::json!({
                    "name":"trades", "kind":"trades", "format":"csv", "time_unit":"ms", "columns":{}
                });
                value[filter] = serde_json::json!({"direction":expected});
                let stream: Stream = serde_json::from_value(value).unwrap();
                for actual in [Field::Null, Field::Str("Buy".into())] {
                    let row = Row::new(vec![("direction".into(), actual)]);
                    assert_eq!(
                        stream.matches(&row).unwrap_err().to_string(),
                        "FILTER_SCALAR_REQUIRED"
                    );
                }
            }
        }
    }

    #[test]
    fn csv_millisecond_bars_preserve_interval_values_and_receipt_time() {
        let stream: Stream = serde_json::from_value(serde_json::json!({
            "name":"bars", "kind":"bars", "format":"csv", "time_unit":"ms",
            "bar_milliseconds":100, "bar_timestamp":"start",
            "columns":{"symbol":"coin", "time":"time", "available_at":"received",
                "open":"open", "high":"high", "low":"low", "close":"close", "volume":"volume"}
        }))
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let csv = root.path().join("bars.csv");
        std::fs::write(&csv, "coin,time,received,open,high,low,close,volume\nAUD/USD,1000,1150,1.1,1.2,1.0,1.15,2.5\n").unwrap();
        let id = InstrumentId::from_str("AUD/USD.SIM").unwrap();
        let encoding = Encoding::new(id, None, true).unwrap();
        let mut sink = super::super::writer::Writer::new(&root.path().join("native"), &[]).unwrap();
        let mut report = Report::default();
        let mut rows = 0;
        read_rows(&csv, &stream.format, |row| {
            let event = timestamp(stream.field(&row, "time")?, &stream.time_unit)?;
            let receipt = timestamp(stream.field(&row, "available_at")?, &stream.time_unit)?;
            let records = convert(&stream, &row, &encoding, event, receipt, &mut report)?;
            let Data::Bar(bar) = &records[0] else {
                panic!("expected BAR")
            };
            assert_eq!(
                bar.bar_type.to_string(),
                "AUD/USD.SIM-100-MILLISECOND-LAST-EXTERNAL"
            );
            assert_eq!(bar.ts_event.as_u64(), 1_100_000_000);
            assert_eq!(bar.ts_init.as_u64(), 1_150_000_000);
            assert_eq!(bar.close.as_decimal(), Decimal::from_str("1.15").unwrap());
            assert_eq!(bar.volume.as_decimal(), Decimal::from_str("2.5").unwrap());
            for record in records {
                sink.push(record)?;
            }
            rows += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(rows, 1);
        sink.finish().unwrap(); // Writer compares the actual native readback to the original BAR.
    }

    #[test]
    fn supplied_bar_interval_requires_one_positive_unit() {
        let id = InstrumentId::from_str("AUD/USD.SIM").unwrap();
        let base = serde_json::json!({"name":"bars","kind":"bars","format":"csv","time_unit":"ms","columns":{}});
        for fields in [
            serde_json::json!({}),
            serde_json::json!({"bar_milliseconds":0}),
            serde_json::json!({"bar_milliseconds":86_400_001}),
            serde_json::json!({"bar_seconds":1,"bar_milliseconds":100}),
        ] {
            let mut value = base.clone();
            value
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let stream: Stream = serde_json::from_value(value).unwrap();
            assert!(stream.bar_interval(id).is_err());
        }
    }

    #[test]
    fn timestamps_preserve_units_and_large_integer_precision() {
        assert_eq!(
            timestamp(&Field::TimestampMicros(1_700_000_000_123_456), "us").unwrap(),
            1_700_000_000_123_456_000
        );
        assert!(timestamp(&Field::TimestampMicros(1), "ms").is_err());
        assert!(timestamp(&Field::Double(1.0), "s").is_err());
        assert!(timestamp(&Field::ULong(u64::MAX), "s").is_err());
        assert_eq!(
            scalar(&Field::ULong(9_007_199_254_740_993)).unwrap(),
            "9007199254740993"
        );
    }

    #[test]
    fn archive_trades_aggregate_at_100ms_boundaries_with_native_readback() {
        let id = InstrumentId::from_str("AUD/USD.SIM").unwrap();
        let mut aggregation = Aggregation::with_milliseconds(None, Some(100)).unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut sink = super::super::writer::Writer::new(&root.path().join("native"), &[]).unwrap();
        let mut bars = Vec::new();
        for (event_ms, receipt_ms, px, size) in [
            (99_u64, 120_u64, "1.1", "2"),
            (100, 250, "1.2", "3"),
            (350, 390, "1.3", "4"),
        ] {
            let trade = TradeTick::new_checked(
                id,
                Price::from(px),
                Quantity::from(size),
                AggressorSide::Buy,
                TradeId::new(event_ms.to_string()),
                (event_ms * 1_000_000).into(),
                (receipt_ms * 1_000_000).into(),
            )
            .unwrap();
            if let Some(bar) = aggregation.update(&trade).unwrap() {
                bars.push(bar);
            }
        }
        bars.extend(aggregation.finish().unwrap());
        assert_eq!(
            bars.iter()
                .map(|bar| bar.ts_event.as_u64())
                .collect::<Vec<_>>(),
            vec![100_000_000, 200_000_000, 400_000_000]
        );
        assert_eq!(
            bars.iter()
                .map(|bar| bar.ts_init.as_u64())
                .collect::<Vec<_>>(),
            vec![120_000_000, 250_000_000, 400_000_000]
        );
        assert_eq!(
            bars.iter()
                .map(|bar| bar.volume.as_decimal())
                .collect::<Vec<_>>(),
            vec![Decimal::from(2), Decimal::from(3), Decimal::from(4)]
        );
        for bar in bars {
            assert_eq!(
                bar.bar_type.to_string(),
                "AUD/USD.SIM-100-MILLISECOND-LAST-EXTERNAL"
            );
            sink.push(Data::Bar(bar)).unwrap();
        }
        sink.finish().unwrap();
        assert!(Aggregation::with_milliseconds(Some(1), Some(100)).is_err());
        assert!(Aggregation::with_milliseconds(None, Some(0)).is_err());
    }

    #[test]
    fn observed_trade_aggregation_does_not_fill_empty_intervals() {
        let id = InstrumentId::from_str("AUD/USD.SIM").unwrap();
        let mut aggregation = Aggregation::new(Some(60));
        for event in [1_u64, 20] {
            let trade = TradeTick::new_checked(
                id,
                Price::from("0.12345"),
                Quantity::from("10"),
                AggressorSide::Buy,
                TradeId::new(event.to_string()),
                (event * 1_000_000_000).into(),
                (event * 1_000_000_000).into(),
            )
            .unwrap();
            assert!(aggregation.update(&trade).unwrap().is_none());
        }
        let late = TradeTick::new_checked(
            id,
            Price::from("0.12346"),
            Quantity::from("5"),
            AggressorSide::Sell,
            TradeId::new("later"),
            181_000_000_000.into(),
            250_000_000_000.into(),
        )
        .unwrap();
        let first = aggregation.update(&late).unwrap().unwrap();
        assert_eq!(first.volume.as_decimal().to_string(), "20");
        assert_eq!(first.ts_event.as_u64(), 60_000_000_000);
        let last = aggregation.finish().unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].ts_event.as_u64(), 240_000_000_000);
        assert_eq!(last[0].ts_init.as_u64(), 250_000_000_000);
    }

    #[test]
    fn aggregation_preserves_changes_within_and_between_intervals() {
        let id = InstrumentId::from_str("TOKEN/QUOTE.VENUE").unwrap();
        let mut aggregation = Aggregation::new(Some(60));
        let mut bars = Vec::new();
        for (event, price, size) in [
            (1_u64, "1.23", "2"),
            (20, "1.234", "3.125"),
            (61, "1.2", "4"),
            (82, "1.19", "0.75"),
        ] {
            let at = (event * 1_000_000_000).into();
            let trade = TradeTick::new_checked(
                id,
                Price::from(price),
                Quantity::from(size),
                AggressorSide::Buy,
                TradeId::new(event.to_string()),
                at,
                at,
            )
            .unwrap();
            if let Some(bar) = aggregation.update(&trade).unwrap() {
                bars.push(bar);
            }
        }
        bars.extend(aggregation.finish().unwrap());
        assert_eq!(bars.len(), 2);
        assert_eq!(
            bars[0].open.as_decimal(),
            Decimal::from_str_exact("1.23").unwrap()
        );
        assert_eq!(
            bars[0].close.as_decimal(),
            Decimal::from_str_exact("1.234").unwrap()
        );
        assert_eq!(
            bars[0].volume.as_decimal(),
            Decimal::from_str_exact("5.125").unwrap()
        );
        assert_eq!(bars[0].open.precision, 3);
        assert_eq!(bars[0].volume.precision, 3);
        assert_eq!(
            bars[1].open.as_decimal(),
            Decimal::from_str_exact("1.2").unwrap()
        );
        assert_eq!(
            bars[1].close.as_decimal(),
            Decimal::from_str_exact("1.19").unwrap()
        );
        assert_eq!(
            bars[1].volume.as_decimal(),
            Decimal::from_str_exact("4.75").unwrap()
        );
        assert_eq!(bars[1].open.precision, 2);
        assert_eq!(bars[1].volume.precision, 2);
    }

    #[test]
    fn a_one_sided_snapshot_keeps_levels_without_inventing_a_quote() {
        let id = InstrumentId::from_str("AUD/USD.SIM").unwrap();
        let encoding = Encoding::new(id, None, true).unwrap();
        let mut report = Report::default();
        let data = snapshot(
            &encoding,
            [
                vec![],
                vec![(
                    price_with_precision(&Field::Str("0.12345".into()), encoding.price_precision)
                        .unwrap(),
                    quantity(&Field::Str("7".into()), encoding.size_precision).unwrap(),
                )],
            ],
            1.into(),
            2.into(),
            &mut report,
        )
        .unwrap();
        assert_eq!(report.one_sided_books, 1);
        assert_eq!(data.len(), 2);
        assert!(matches!(&data[0], Data::Delta(d) if d.action == BookAction::Clear));
        assert!(
            matches!(&data[1], Data::Delta(d) if d.order.side == Some(OrderSide::Sell) && d.flags & RecordFlag::F_LAST as u8 != 0)
        );
        let root = tempfile::tempdir().unwrap();
        let mut sink = super::super::writer::Writer::new(&root.path().join("output"), &[]).unwrap();
        for item in snapshot(&encoding, [vec![], vec![]], 0.into(), 0.into(), &mut report)
            .unwrap()
            .into_iter()
            .chain(data)
        {
            sink.push(item).unwrap();
        }
        sink.finish().unwrap();
        assert_eq!(sink.counts["book_deltas"][&encoding.id.to_string()], 3);
        assert_eq!(report.empty_books, 1);
    }

    #[test]
    fn raw_native_encoding_preserves_ten_decimal_values_without_definitions() {
        let id = InstrumentId::from_str("TOKEN/QUOTE.VENUE").unwrap();
        let encoding = Encoding::new(id, None, true).unwrap();
        let value = Field::Str("0.0000041769".into());
        assert_eq!(
            quantity(&value, encoding.size_precision)
                .unwrap()
                .as_decimal(),
            decimal(&value).unwrap()
        );
        assert!(quantity(&value, 9).is_err());
        assert!(Encoding::new(id, None, false).is_err());
    }

    #[test]
    fn exact_values_do_not_depend_on_venue_or_symbol() {
        let id = InstrumentId::from_str("AUD/USD.SIM").unwrap();
        assert_eq!(
            price_with_precision(&Field::Str("0.12345".into()), 5)
                .unwrap()
                .as_decimal()
                .to_string(),
            "0.12345"
        );
        assert!(price_with_precision(&Field::Str("0.123456".into()), 5).is_err());
        assert!(decimal(&Field::Str("0.12345678901234567890123456789".into())).is_err());
        assert_eq!(
            bar_type(id, 60).unwrap().to_string(),
            "AUD/USD.SIM-1-MINUTE-LAST-EXTERNAL"
        );
    }
}
