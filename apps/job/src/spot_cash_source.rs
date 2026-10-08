//! Offline reconstruction of selected candles from an existing immutable source.
//! A parsed REST response's actual observation proves current retrieval only.
//! No Hyper network collection, metadata refresh or polling is exposed here.
use crate::{catalog, spot_cash_capture::ClosedBarSourceRow};
use anyhow::{Context, Result, ensure};
use contracts::{
    DbCounter, SchemaV1, science::NativeBarSelectionV1, spot_cash::NativeSpotBarEventTimeV1,
    spot_cash_source::*,
};
use nautilus_hyperliquid::websocket::{messages::CandleData, parse::parse_ws_candle};
use nautilus_model::{
    data::{Bar, BarType},
    instruments::{Instrument, InstrumentAny},
    types::Currency,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub const SIDECAR: &str = SPOT_CANDLE_SOURCE_FILE;
const MINUTE_MS: u64 = 60_000;

pub struct VerifiedSpotSource {
    pub rows: Vec<ClosedBarSourceRow>,
    pub evidence: NativeSpotCashSourceEvidenceV1,
}
fn count(n: u64) -> Result<DbCounter> {
    DbCounter::new(n).map_err(anyhow::Error::msg)
}
fn nanos(ms: u64) -> Result<u64> {
    ms.checked_mul(1_000_000).context("SPOT_SOURCE_CLOCK_RANGE")
}
fn read_bundle(root: &Path) -> Result<FrozenSpotCandleSourceV1> {
    ensure!(
        !fs::symlink_metadata(root)?.file_type().is_symlink(),
        "SPOT_SOURCE_ROOT_SYMLINK"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let mut file = options
        .open(root.join(SIDECAR))
        .context("SPOT_CASH_FROZEN_SOURCE_REQUIRED")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() > 0,
        "SPOT_SOURCE_FILE_LIMIT"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(metadata.nlink() == 1, "SPOT_SOURCE_FILE_IDENTITY");
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == metadata.len(),
        "SPOT_SOURCE_FILE_CHANGED"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn write_bundle(root: &Path, bundle: &FrozenSpotCandleSourceV1) -> Result<()> {
    let bytes = serde_json::to_vec(bundle)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(SIDECAR))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}
fn bar_key(bar: &Bar) -> (String, u64, u64) {
    (
        bar.bar_type.to_string(),
        bar.ts_event.as_u64(),
        bar.ts_init.as_u64(),
    )
}
fn same_native(left: &impl serde::Serialize, right: &impl serde::Serialize) -> Result<bool> {
    // Native Price/Quantity PartialEq does not include precision.
    Ok(serde_json::to_value(left)? == serde_json::to_value(right)?)
}

/// Recompute locators from exact one-closed-window responses. No unselected
/// historical/future rows ride into the native job alongside selected Parquet.
/// A current observation is never relabeled as arrival at a historical close.
fn source_rows(
    bundle: &FrozenSpotCandleSourceV1,
) -> Result<(
    InstrumentAny,
    BTreeMap<FrozenSpotCandleRowV1, ClosedBarSourceRow>,
)> {
    ensure!(
        bundle.native_version == "0.63.0" && !bundle.responses.is_empty(),
        "SPOT_SOURCE_VERSION_OR_RESPONSES"
    );
    let instrument: InstrumentAny = serde_json::from_value(bundle.instrument_definition.clone())?;
    let InstrumentAny::CurrencyPair(pair) = &instrument else {
        anyhow::bail!("SPOT_SOURCE_CURRENCY_PAIR_REQUIRED")
    };
    ensure!(
        pair.id.to_string().ends_with("-SPOT.HYPERLIQUID")
            && !pair.id.to_string().contains(':')
            && pair.quote_currency == Currency::USDC()
            && pair.quote_currency.precision == Currency::USDC().precision
            && pair.base_currency != pair.quote_currency
            && instrument.multiplier().as_decimal() == rust_decimal::Decimal::ONE,
        "SPOT_SOURCE_ORDINARY_USDC_PAIR_REQUIRED"
    );
    ensure!(
        bundle.instrument_request_started_ns <= bundle.instrument_received_ns
            && instrument.ts_init().as_u64() <= bundle.instrument_received_ns.get(),
        "SPOT_SOURCE_INSTRUMENT_OBSERVATION_CLOCK"
    );
    let raw_symbol = instrument.raw_symbol().to_string();
    let indexed_spot = raw_symbol.strip_prefix('@').is_some_and(|index| {
        index
            .parse::<u64>()
            .is_ok_and(|parsed| parsed.to_string() == index)
    });
    ensure!(
        indexed_spot || raw_symbol == "PURR/USDC",
        "SPOT_SOURCE_API_COIN_MUST_BE_SPOT"
    );
    let expected_type = format!("{}-1-MINUTE-LAST-EXTERNAL", instrument.id());
    ensure!(bundle.bar_type == expected_type, "SPOT_SOURCE_BAR_TYPE");
    let bar_type: BarType = bundle.bar_type.parse()?;
    let mut result = BTreeMap::new();
    let mut all_closed = BTreeMap::<u64, Value>::new();
    let mut selected_windows = BTreeSet::new();
    let mut previous_receipt = bundle.instrument_received_ns.get();
    let mut previous_selected_open = 0;
    let mut previous_sequence = 0;
    for response in &bundle.responses {
        ensure!(
            response.sequence.get() > previous_sequence
                && response.coin == instrument.raw_symbol().to_string()
                && response.interval == "1m",
            "SPOT_SOURCE_REQUEST_BINDING"
        );
        ensure!(
            response.requested_start_ms < response.requested_end_ms
                && nanos(response.requested_end_ms.get())? <= response.request_started_ns.get()
                && response.request_started_ns <= response.received_ns
                && previous_receipt <= response.request_started_ns.get()
                && instrument.ts_init().as_u64() <= response.request_started_ns.get(),
            "SPOT_SOURCE_RECEIPT_CLOCK"
        );
        previous_receipt = response.received_ns.get();
        previous_sequence = response.sequence.get();
        ensure!(
            response.requested_start_ms.get() % MINUTE_MS == 0
                && response.requested_start_ms.get().checked_add(MINUTE_MS - 1)
                    == Some(response.requested_end_ms.get())
                && nanos(
                    response
                        .requested_end_ms
                        .get()
                        .checked_add(1)
                        .context("SPOT_SOURCE_CLOCK_RANGE")?
                )? <= response.request_started_ns.get(),
            "SPOT_SOURCE_ONE_CLOSED_WINDOW_REQUIRED"
        );
        let values = response
            .response
            .as_array()
            .context("SPOT_SOURCE_RESPONSE_ARRAY")?;
        ensure!(values.len() == 1, "SPOT_SOURCE_EXACT_ONE_RESPONSE_ROW");
        let mut windows = BTreeSet::new();
        let mut newest: Option<(u64, FrozenSpotCandleRowV1, ClosedBarSourceRow)> = None;
        for (index, value) in values.iter().enumerate() {
            let candle: CandleData = serde_json::from_value(value.clone())?;
            let close_ms = candle
                .close_time
                .checked_add(1)
                .context("SPOT_SOURCE_CLOSE_RANGE")?;
            ensure!(
                candle.s.to_string() == response.coin
                    && candle.i.as_str() == "1m"
                    && candle.t % MINUTE_MS == 0
                    && candle.t.checked_add(MINUTE_MS) == Some(close_ms)
                    && windows.insert(candle.t),
                "SPOT_SOURCE_RAW_ROW_IDENTITY_OR_BOUNDARY"
            );
            let close_ns = nanos(close_ms)?;
            ensure!(
                candle.t == response.requested_start_ms.get()
                    && candle.close_time == response.requested_end_ms.get()
                    && close_ns <= response.received_ns.get(),
                "SPOT_SOURCE_RESPONSE_OUTSIDE_FROZEN_WINDOW"
            );
            if let Some(prior) = all_closed.get(&candle.t) {
                ensure!(prior == value, "SPOT_SOURCE_CLOSED_ROW_REWRITTEN");
            } else {
                all_closed.insert(candle.t, value.clone());
            }
            // The official public parser uses an unchecked Bar constructor.
            // Validate the untrusted source domain before calling it, without
            // replacing its native parsing or changing upstream implementation.
            ensure!(
                candle.l > rust_decimal::Decimal::ZERO
                    && candle.h >= candle.l
                    && candle.h >= candle.o.max(candle.c)
                    && candle.l <= candle.o.min(candle.c)
                    && candle.v >= rust_decimal::Decimal::ZERO,
                "SPOT_SOURCE_OHLCV_INVALID"
            );
            let native = parse_ws_candle(
                &candle,
                &instrument,
                &bar_type,
                response.received_ns.get().into(),
            )?;
            ensure!(
                native.ts_event.as_u64() == nanos(candle.t)?
                    && native.ts_init.as_u64() == response.received_ns.get(),
                "SPOT_SOURCE_NATIVE_CLOCK_CHANGED"
            );
            let locator = FrozenSpotCandleRowV1 {
                response_sequence: response.sequence,
                source_row_index: count(index as u64)?,
            };
            let row = ClosedBarSourceRow {
                event_time: NativeSpotBarEventTimeV1::Open,
                source_row_key: format!(
                    "{}:{}:{}",
                    bundle.capture_id,
                    response.sequence.get(),
                    index
                ),
                bar_open_ns: count(nanos(candle.t)?)?,
                bar_close_ns: count(close_ns)?,
                native,
            };
            if newest.as_ref().is_none_or(|(open, _, _)| candle.t > *open) {
                newest = Some((candle.t, locator, row));
            }
        }
        if let Some((open, locator, row)) = newest {
            ensure!(
                open >= previous_selected_open,
                "SPOT_SOURCE_CLOSED_WINDOWS_REVERSED"
            );
            previous_selected_open = open;
            if selected_windows.insert(open) {
                result.insert(locator, row);
            }
        }
    }
    ensure!(!result.is_empty(), "SPOT_SOURCE_NO_OBSERVED_CLOSED_ROWS");
    Ok((instrument, result))
}

fn verified(
    bundle: &FrozenSpotCandleSourceV1,
    root: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<(VerifiedSpotSource, Vec<FrozenSpotCandleRowV1>)> {
    ensure!(
        !bundle.selected_rows.is_empty(),
        "SPOT_SOURCE_SELECTED_ROW_COUNT"
    );
    let (instrument, mut originals) = source_rows(bundle)?;
    let mut selected = BTreeMap::new();
    for locator in &bundle.selected_rows {
        let row = originals
            .remove(locator)
            .context("SPOT_SOURCE_DUPLICATE_OR_UNKNOWN_LOCATOR")?;
        ensure!(
            selected
                .insert(bar_key(&row.native), (locator.clone(), row))
                .is_none(),
            "SPOT_SOURCE_DUPLICATE_NATIVE_BAR"
        );
    }
    ensure!(
        originals.values().all(|row| {
            row.native.ts_event.as_u64() < selection.event_start_ns.get()
                || row.native.ts_event.as_u64() >= selection.event_end_ns.get()
                || row.native.ts_init.as_u64() > selection.decision_cutoff_ns.get()
        }),
        "SPOT_SOURCE_ELIGIBLE_ROW_OMITTED"
    );
    let market = catalog::load_catalog(root, selection)?;
    ensure!(
        market.series.len() == 1
            && market.series[0].instrument_updates.is_empty()
            && same_native(&market.series[0].instrument, &instrument)?,
        "SPOT_SOURCE_NATIVE_INSTRUMENT_MISMATCH"
    );
    let mut rows = Vec::new();
    let mut locators = Vec::new();
    for actual in &market.series[0].bars {
        let (locator, row) = selected
            .remove(&bar_key(actual))
            .context("SPOT_SOURCE_NATIVE_ROW_MISSING")?;
        ensure!(
            same_native(actual, &row.native)?,
            "SPOT_SOURCE_NATIVE_ROW_CHANGED"
        );
        locators.push(locator);
        rows.push(row);
    }
    // Extra source rows outside the explicit event/availability selection are
    // harmless original evidence. An unexplained selected row inside it is not.
    ensure!(
        selected.values().all(|(_, row)| {
            row.native.ts_event.as_u64() < selection.event_start_ns.get()
                || row.native.ts_event.as_u64() >= selection.event_end_ns.get()
                || row.native.ts_init.as_u64() > selection.decision_cutoff_ns.get()
        }),
        "SPOT_SOURCE_SELECTED_ROW_NOT_IN_CATALOG"
    );
    ensure!(
        !rows.is_empty() && rows.len() == market.rows,
        "SPOT_SOURCE_NATIVE_COVERAGE"
    );
    let first = rows
        .iter()
        .map(|row| row.native.ts_init.as_u64())
        .min()
        .unwrap();
    let last = rows
        .iter()
        .map(|row| row.native.ts_init.as_u64())
        .max()
        .unwrap();
    let responses = locators
        .iter()
        .map(|row| row.response_sequence)
        .collect::<BTreeSet<_>>()
        .len();
    let evidence = NativeSpotCashSourceEvidenceV1 {
        schema_version: SchemaV1,
        capture_id: bundle.capture_id,
        method: bundle.method,
        network: bundle.network,
        native_version: bundle.native_version.clone(),
        historical_availability: bundle.historical_availability,
        first_received_ns: count(first)?,
        last_received_ns: count(last)?,
        response_count: count(responses as u64)?,
        selected_row_count: count(rows.len() as u64)?,
    };
    Ok((VerifiedSpotSource { rows, evidence }, locators))
}

pub fn load_closed_rows(
    root: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<VerifiedSpotSource> {
    let bundle = read_bundle(root)?;
    Ok(verified(&bundle, root, selection)?.0)
}

pub fn preserve_for_selection(
    source: &Path,
    destination: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<()> {
    let mut bundle = read_bundle(source)?;
    let (_, selected) = verified(&bundle, source, selection)?;
    let keep = selected
        .iter()
        .map(|row| row.response_sequence)
        .collect::<BTreeSet<_>>();
    bundle
        .responses
        .retain(|response| keep.contains(&response.sequence));
    bundle.selected_rows = selected;
    verified(&bundle, destination, selection)?;
    write_bundle(destination, &bundle)
}

#[cfg(test)]
mod tests;
