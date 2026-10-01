//! Shared exact spot-candle construction and original native catalog publication.
use anyhow::{ensure, Context, Result};
use bigdecimal::BigDecimal;
use nautilus_model::{
    data::{Bar, BarType, Data},
    instruments::{Instrument, InstrumentAny},
    types::{Price, Quantity},
};
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    str::FromStr,
};

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn no_symlinks(path: &Path) -> Result<()> {
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

pub(super) fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
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
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "SOURCE_FILE_LIMIT"
    );
    let mut bytes = Vec::new();
    (&mut file).take(limit + 1).read_to_end(&mut bytes)?;
    no_symlinks(path)?;
    let after = file.metadata()?;
    let current = fs::metadata(path)?;
    ensure!(
        same_file(&metadata, &after) && same_file(&after, &current),
        "SOURCE_FILE_CHANGED"
    );
    ensure!(bytes.len() as u64 == metadata.len(), "SOURCE_FILE_CHANGED");
    Ok(bytes)
}

pub(super) fn exact(text: &str) -> Result<Decimal> {
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

pub(super) fn definitions(
    bytes: &[u8],
    venue: &str,
    symbol: &str,
    base: &str,
    quote: &str,
) -> Result<Vec<InstrumentAny>> {
    let values: Vec<Value> = serde_json::from_slice(bytes)?;
    let originals: Vec<InstrumentAny> = serde_json::from_slice(bytes)?;
    ensure!(
        serde_json::to_value(&originals)? == Value::Array(values.clone()),
        "ORIGINAL_NATIVE_DEFINITIONS_REQUIRED"
    );
    let chains = domain::catalogs::instrument_versions(&values)?;
    ensure!(chains.len() == 1, "ONE_SOURCE_PRODUCT_REQUIRED");
    for instrument in &originals {
        let InstrumentAny::CurrencyPair(pair) = instrument else {
            anyhow::bail!("SPOT_CURRENCY_PAIR_REQUIRED")
        };
        ensure!(
            instrument.venue().as_str() == venue
                && instrument.raw_symbol().as_str() == symbol
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

pub(super) fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
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

pub(super) fn bar_type(instruments: &[InstrumentAny], interval: u64) -> Result<BarType> {
    ensure!(
        !instruments.is_empty() && interval > 0 && interval.is_multiple_of(60),
        "SOURCE_INTERVAL"
    );
    let (step, unit) = if interval.is_multiple_of(86400) {
        (interval / 86400, "DAY")
    } else if interval.is_multiple_of(3600) {
        (interval / 3600, "HOUR")
    } else {
        (interval / 60, "MINUTE")
    };
    Ok(BarType::from_str(&format!(
        "{}-{step}-{unit}-LAST-EXTERNAL",
        instruments[0].id()
    ))?)
}

pub(super) fn candle(
    kind: BarType,
    values: [Decimal; 5],
    ts_event: u64,
    received: u64,
    instruments: &[InstrumentAny],
) -> Result<Bar> {
    let instrument = instruments
        .iter()
        .rev()
        .find(|definition| definition.ts_init().as_u64() <= received)
        .context("INSTRUMENT_DEFINITION_FROM_FUTURE")?;
    ensure!(
        !instruments
            .iter()
            .skip(1)
            .any(|definition| definition.ts_init().as_u64() == received),
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
    ensure!(ts_event <= received, "CANDLE_EVENT_AFTER_OBSERVATION");
    Bar::new_checked(
        kind,
        price(values[0])?,
        price(values[1])?,
        price(values[2])?,
        price(values[3])?,
        volume,
        ts_event.into(),
        received.into(),
    )
}

pub(super) fn write_catalog(
    root: &Path,
    instruments: &[InstrumentAny],
    bars: &[Bar],
) -> Result<()> {
    let cutoff = bars.last().context("SOURCE_EMPTY")?.ts_init;
    let mut native = super::native(root)?;
    native.write_instruments(instruments.to_vec())?;
    native.write_to_parquet(bars, None, None, None)?;
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
        serde_json::to_value(readback_definitions)? == serde_json::to_value(instruments)?,
        "NATIVE_DEFINITION_READBACK_MISMATCH"
    );
    Ok(())
}

fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (
            a.dev(),
            a.ino(),
            a.len(),
            a.mtime(),
            a.mtime_nsec(),
            a.ctime(),
            a.ctime_nsec(),
        ) == (
            b.dev(),
            b.ino(),
            b.len(),
            b.mtime(),
            b.mtime_nsec(),
            b.ctime(),
            b.ctime_nsec(),
        )
    }
    #[cfg(not(unix))]
    {
        a.len() == b.len() && a.modified().ok() == b.modified().ok() && a.is_file() == b.is_file()
    }
}
