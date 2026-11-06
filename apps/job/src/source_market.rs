//! Exact published market rows and complete L2 snapshots shared by import and replay.
use anyhow::{ensure, Context, Result};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{BookOrder, Data, OrderBookDelta, OrderBookDeltas, QuoteTick},
    enums::{BookAction, OrderSide, RecordFlag},
    identifiers::InstrumentId,
    instruments::{Instrument, InstrumentAny},
    types::{Price, Quantity},
};
use parquet::{
    file::reader::SerializedFileReader,
    record::{reader::RowIter, Field, Row},
};
use rust_decimal::Decimal;
use std::{collections::BTreeMap, fs::File, path::Path};

#[derive(Clone, Copy)]
pub struct Encoding {
    pub id: InstrumentId,
    pub price_precision: u8,
    pub size_precision: u8,
}

impl Encoding {
    pub fn new(id: InstrumentId, instrument: Option<&InstrumentAny>, native: bool) -> Result<Self> {
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

pub fn field<'a>(row: &'a Row, name: &str) -> Result<&'a Field> {
    row.get_column_iter()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, value)| value)
        .with_context(|| format!("SOURCE_COLUMN_MISSING: {name}"))
}

pub fn mapped_field<'a>(
    row: &'a Row,
    columns: &BTreeMap<String, String>,
    name: &str,
) -> Result<&'a Field> {
    field(
        row,
        columns
            .get(name)
            .with_context(|| format!("COLUMN_MAPPING_MISSING: {name}"))?,
    )
}

pub fn scalar(value: &Field) -> Result<String> {
    Ok(match value {
        Field::Bool(v) => v.to_string(),
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

pub fn timestamp(value: &Field, unit: &str) -> Result<u64> {
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

pub fn decimal(value: &Field) -> Result<Decimal> {
    Ok(Decimal::from_str_exact(&scalar(value)?)?)
}

pub fn price_with_precision(value: &Field, precision: u8) -> Result<Price> {
    let original = decimal(value)?;
    let native = Price::from_decimal_dp(original, precision)?;
    ensure!(native.as_decimal() == original, "PRICE_PRECISION_LOSS");
    ensure!(original > Decimal::ZERO, "NONPOSITIVE_MARKET_PRICE");
    Ok(native)
}

pub fn quantity(value: &Field, precision: u8) -> Result<Quantity> {
    let original = decimal(value)?;
    let native = Quantity::from_decimal_dp(original, precision)?;
    ensure!(native.as_decimal() == original, "SIZE_PRECISION_LOSS");
    Ok(native)
}

pub fn parquet_rows(path: &Path, format: &str) -> Result<RowIter<'static>> {
    let file = match format {
        "parquet" => File::open(path)?,
        "parquet-zstd" => {
            let mut decoded = tempfile::tempfile()?;
            std::io::copy(
                &mut zstd::stream::read::Decoder::new(File::open(path)?)?,
                &mut decoded,
            )?;
            decoded
        }
        _ => anyhow::bail!("SOURCE_FORMAT_UNSUPPORTED"),
    };
    Ok(RowIter::from_file_into(Box::new(
        SerializedFileReader::new(file)?,
    )))
}

pub struct BookSnapshot {
    pub deltas: OrderBookDeltas,
    pub quote: Option<QuoteTick>,
    pub empty: bool,
    pub one_sided: bool,
}

impl BookSnapshot {
    pub fn into_data(self) -> Vec<Data> {
        self.deltas
            .deltas
            .into_iter()
            .map(Data::BookDelta)
            .chain(self.quote.map(Data::Quote))
            .collect()
    }
}

pub fn snapshot(
    encoding: &Encoding,
    sides: [Vec<(Price, Quantity)>; 2],
    at: UnixNanos,
    init: UnixNanos,
) -> Result<BookSnapshot> {
    let id = encoding.id;
    let mut clear = OrderBookDelta::clear(id, 0, at, init);
    // A clear shares the snapshot's native encoding even though it has no level.
    clear.order.price = Price::zero(encoding.price_precision);
    clear.order.size = Quantity::zero(encoding.size_precision);
    let mut deltas = vec![clear];
    for (levels, side) in sides.iter().zip([OrderSide::Buy, OrderSide::Sell]) {
        for (price, size) in levels {
            deltas.push(OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(side, *price, *size, 0),
                RecordFlag::F_SNAPSHOT as u8,
                0,
                at,
                init,
            ));
        }
    }
    deltas.last_mut().unwrap().flags |= RecordFlag::F_LAST as u8;
    let quote = match (sides[0].first(), sides[1].first()) {
        (Some((bid, bid_size)), Some((ask, ask_size))) => {
            ensure!(bid <= ask, "BOOK_CROSSED");
            Some(QuoteTick::new_checked(
                id, *bid, *ask, *bid_size, *ask_size, at, init,
            )?)
        }
        _ => None,
    };
    Ok(BookSnapshot {
        deltas: OrderBookDeltas::new_checked(id, deltas)?,
        quote,
        empty: sides[0].is_empty() && sides[1].is_empty(),
        one_sided: sides[0].is_empty() != sides[1].is_empty(),
    })
}

struct FlatSnapshot {
    encoding: Encoding,
    event: u64,
    available: u64,
    rows: Vec<Row>,
}

impl FlatSnapshot {
    fn convert(self, columns: &BTreeMap<String, String>) -> Result<BookSnapshot> {
        let mut sides = [Vec::new(), Vec::new()];
        for row in &self.rows {
            ensure!(
                scalar(mapped_field(row, columns, "event_type")?)? == "snapshot",
                "BOOK_SNAPSHOT_REQUIRED"
            );
            let side = scalar(mapped_field(row, columns, "side")?)?;
            if side == "noop" {
                ensure!(
                    self.rows.len() == 1
                        && decimal(mapped_field(row, columns, "price")?)? == Decimal::ZERO
                        && decimal(mapped_field(row, columns, "size")?)? == Decimal::ZERO,
                    "EMPTY_BOOK_SNAPSHOT_INVALID"
                );
                continue;
            }
            let index = match side.as_str() {
                "bid" => 0,
                "ask" => 1,
                _ => anyhow::bail!("BOOK_SIDE_UNSUPPORTED"),
            };
            let price = price_with_precision(
                mapped_field(row, columns, "price")?,
                self.encoding.price_precision,
            )?;
            let size = quantity(
                mapped_field(row, columns, "size")?,
                self.encoding.size_precision,
            )?;
            ensure!(size.as_decimal() > Decimal::ZERO, "NONPOSITIVE_BOOK_SIZE");
            sides[index].push((price, size));
        }
        sides[0].sort_by(|a, b| b.0.cmp(&a.0));
        sides[1].sort_by(|a, b| a.0.cmp(&b.0));
        ensure!(
            sides
                .iter()
                .all(|levels| levels.windows(2).all(|pair| pair[0].0 != pair[1].0)),
            "DUPLICATE_BOOK_LEVEL"
        );
        snapshot(
            &self.encoding,
            sides,
            self.event.into(),
            self.available.into(),
        )
    }
}

#[derive(Default)]
pub struct FlatSnapshots {
    pending: Option<FlatSnapshot>,
}

impl FlatSnapshots {
    pub fn push(
        &mut self,
        columns: &BTreeMap<String, String>,
        row: Row,
        encoding: Encoding,
        event: u64,
        available: u64,
    ) -> Result<Option<BookSnapshot>> {
        let completed = if self.pending.as_ref().is_some_and(|current| {
            (current.encoding.id, current.event, current.available)
                != (encoding.id, event, available)
        }) {
            self.pending.take()
        } else {
            None
        };
        self.pending
            .get_or_insert_with(|| FlatSnapshot {
                encoding,
                event,
                available,
                rows: Vec::new(),
            })
            .rows
            .push(row);
        completed
            .map(|snapshot| snapshot.convert(columns))
            .transpose()
    }

    pub fn finish(&mut self, columns: &BTreeMap<String, String>) -> Result<Option<BookSnapshot>> {
        self.pending
            .take()
            .map(|snapshot| snapshot.convert(columns))
            .transpose()
    }
}
