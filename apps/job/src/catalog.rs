//! Read an already-authorized, immutable native catalog mounted into this job.
//! Native time ordering is necessary, not sufficient, evidence of historical availability.
use anyhow::{ensure, Result};
use contracts::science::NativeBarSelectionV1;
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarType, Data},
    enums::{BarAggregation, PriceType},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use std::{collections::BTreeMap, path::Path, str::FromStr};

pub struct NativeBarSeries {
    pub instrument: InstrumentAny,
    pub bar_type: BarType,
    pub bars: Vec<Bar>,
}

pub struct NativeMarketData {
    pub series: Vec<NativeBarSeries>,
    pub rows: usize,
}

fn selected_types(selection: &NativeBarSelectionV1) -> Result<Vec<BarType>> {
    ensure!(
        (1..=256).contains(&selection.bar_types.len())
            && (1..=1_000_000).contains(&selection.maximum_rows)
            && selection.event_start_ns < selection.event_end_ns
            && selection.event_end_ns <= selection.decision_cutoff_ns,
        "CATALOG_SELECTION_INVALID"
    );
    let mut instruments = std::collections::BTreeSet::new();
    let mut types = Vec::with_capacity(selection.bar_types.len());
    for text in &selection.bar_types {
        ensure!(
            (1..=300).contains(&text.len()) && !text.chars().any(char::is_control),
            "CATALOG_BAR_TYPE_INVALID"
        );
        let bar_type = BarType::from_str(text)?;
        let spec = bar_type.spec();
        ensure!(bar_type.to_string() == *text, "NONCANONICAL_BAR_TYPE");
        ensure!(
            bar_type.is_externally_aggregated()
                && spec.price_type == PriceType::Last
                && matches!(
                    spec.aggregation,
                    BarAggregation::Millisecond
                        | BarAggregation::Second
                        | BarAggregation::Minute
                        | BarAggregation::Hour
                        | BarAggregation::Day
                        | BarAggregation::Week
                ),
            "UNSUPPORTED_BAR_CONTRACT"
        );
        ensure!(
            instruments.insert(bar_type.instrument_id().to_string()),
            "DUPLICATE_CATALOG_INSTRUMENT"
        );
        types.push(bar_type);
    }
    Ok(types)
}

/// The path comes only from the trusted runtime's registered read-only mount.
/// No HTTP/MCP caller can provide a path, URI, SQL expression or storage options.
pub fn load_catalog(root: &Path, selection: &NativeBarSelectionV1) -> Result<NativeMarketData> {
    let types = selected_types(selection)?;
    let metadata = std::fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "CATALOG_ROOT_INVALID"
    );
    let root = root.canonicalize()?;
    let text = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("CATALOG_ROOT_INVALID"))?;
    // Use the fallible native constructor. Cloud storage support is not enabled.
    let mut catalog = ParquetDataCatalog::from_uri(text, None, Some(4096), None, None)?;
    let ids = types
        .iter()
        .map(|kind| kind.instrument_id().to_string())
        .collect::<Vec<_>>();
    let instruments = catalog.instruments(
        Some(&ids),
        None,
        Some(UnixNanos::from(selection.decision_cutoff_ns.get())),
    )?;
    // The upstream query filters ts_init through the actual availability cutoff.
    // Also push the half-open event interval into that same native query before
    // applying the decoded-row cap. This preserves in-range late arrivals while
    // excluding unrelated newer rows; callers cannot supply a SQL expression.
    let event_interval = format!(
        "ts_event >= {} AND ts_event < {}",
        selection.event_start_ns.get(),
        selection.event_end_ns.get()
    );
    // Native decoding is bounded instead of first materializing an arbitrary Vec.
    // The query engine itself still runs under external job memory/CPU limits.
    let query = catalog.query::<Bar>(
        Some(selection.bar_types.clone()),
        Some(UnixNanos::from(selection.event_start_ns.get())),
        Some(UnixNanos::from(selection.decision_cutoff_ns.get())),
        Some(&event_interval),
        None,
        true,
    )?;
    let mut bars = Vec::new();
    for item in query.take(selection.maximum_rows as usize + 1) {
        ensure!(
            bars.len() < selection.maximum_rows as usize,
            "CATALOG_ROW_LIMIT"
        );
        let Data::Bar(bar) = item? else {
            anyhow::bail!("CATALOG_NATIVE_TYPE_MISMATCH");
        };
        bars.push(bar);
    }
    validate_native(instruments, bars, selection)
}

/// Revalidate exact identities after native directory discovery, which can match substrings.
/// This function does not set origin, PIT status, license grants or qualification.
pub fn validate_native(
    instruments: Vec<InstrumentAny>,
    bars: Vec<Bar>,
    selection: &NativeBarSelectionV1,
) -> Result<NativeMarketData> {
    let types = selected_types(selection)?;
    ensure!(
        instruments.len() == types.len(),
        "CATALOG_INSTRUMENT_VERSION_MISMATCH"
    );
    ensure!(
        bars.len() <= selection.maximum_rows as usize,
        "CATALOG_ROW_LIMIT"
    );
    let mut definitions = BTreeMap::new();
    for instrument in instruments {
        ensure!(
            definitions
                .insert(instrument.id().to_string(), instrument)
                .is_none(),
            "CATALOG_DUPLICATE_INSTRUMENT_VERSION"
        );
    }
    let mut series = Vec::with_capacity(types.len());
    let mut indices = BTreeMap::new();
    for bar_type in types {
        let instrument = definitions
            .remove(&bar_type.instrument_id().to_string())
            .ok_or_else(|| anyhow::anyhow!("CATALOG_INSTRUMENT_MISSING"))?;
        indices.insert(bar_type.to_string(), series.len());
        series.push(NativeBarSeries {
            instrument,
            bar_type,
            bars: Vec::new(),
        });
    }
    ensure!(definitions.is_empty(), "CATALOG_EXTRA_INSTRUMENT");
    let mut rows = 0;
    for bar in bars {
        let index = indices
            .get(&bar.bar_type.to_string())
            .ok_or_else(|| anyhow::anyhow!("CATALOG_UNREQUESTED_BAR_TYPE"))?;
        let event = bar.ts_event.as_u64();
        let available = bar.ts_init.as_u64();
        ensure!(
            event <= available && available <= selection.decision_cutoff_ns.get(),
            "CATALOG_TIME_INVALID"
        );
        // Native catalog time filters use ts_init; recheck the requested event interval.
        if event < selection.event_start_ns.get() || event >= selection.event_end_ns.get() {
            continue;
        }
        let selected = &mut series[*index];
        ensure!(
            selected.instrument.ts_init() <= bar.ts_init,
            "INSTRUMENT_DEFINITION_FROM_FUTURE"
        );
        if let Some(previous) = selected.bars.last() {
            ensure!(
                previous.ts_event < bar.ts_event && previous.ts_init < bar.ts_init,
                "CATALOG_NONUNIQUE_OR_REVISED_BAR"
            );
        }
        let precision = selected.instrument.price_precision();
        for price in [bar.open, bar.high, bar.low, bar.close] {
            ensure!(
                price.precision == precision,
                "CATALOG_PRICE_PRECISION_MISMATCH"
            );
            selected.instrument.try_normalize_price(price)?;
            ensure!(
                price.as_f64().is_finite() && price.as_f64() > 0.0,
                "UNSUPPORTED_NONPOSITIVE_PRICE"
            );
        }
        ensure!(
            bar.high >= bar.open
                && bar.high >= bar.close
                && bar.high >= bar.low
                && bar.low <= bar.open
                && bar.low <= bar.close,
            "CATALOG_OHLC_INVALID"
        );
        selected.instrument.try_normalize_qty(bar.volume)?;
        selected.bars.push(bar);
        rows += 1;
    }
    ensure!(
        rows > 0 && series.iter().all(|series| !series.bars.is_empty()),
        "CATALOG_EMPTY_SELECTION"
    );
    Ok(NativeMarketData { series, rows })
}
