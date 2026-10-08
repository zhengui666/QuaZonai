//! Read an already-authorized, immutable native catalog mounted into this job.
//! Native time ordering is necessary, not sufficient, evidence of historical availability.
use anyhow::{Result, ensure};
use contracts::{
    DbCounter, SchemaV1,
    execution::{NativeDataQualityReportV1, NativeDatasetQualityV1, NativeDatasetSelectionV1},
    science::NativeBarSelectionV1,
};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarType, Data},
    enums::{BarAggregation, PriceType},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
    path::Path,
    str::FromStr,
};

pub struct NativeBarSeries {
    /// Latest definition strictly before event start, or the earliest original definition.
    pub instrument: InstrumentAny,
    /// Original tick-only changes after the baseline, through decision cutoff.
    pub instrument_updates: Vec<InstrumentAny>,
    pub bar_type: BarType,
    pub bars: Vec<Bar>,
}

impl NativeBarSeries {
    pub fn instrument_at(&self, ts_init: u64) -> Result<&InstrumentAny> {
        ensure!(
            self.instrument.ts_init().as_u64() <= ts_init,
            "INSTRUMENT_DEFINITION_FROM_FUTURE"
        );
        Ok(self
            .instrument_updates
            .iter()
            .rev()
            .find(|instrument| instrument.ts_init().as_u64() <= ts_init)
            .unwrap_or(&self.instrument))
    }
}

pub struct NativeMarketData {
    pub series: Vec<NativeBarSeries>,
    pub rows: usize,
}

/// Select original baseline plus updates without copying earlier or future versions.
/// This same selection is used for traded series and original settlement siblings.
pub fn select_instrument_versions(
    instruments: Vec<InstrumentAny>,
    selection: &NativeBarSelectionV1,
) -> Result<BTreeMap<String, Vec<InstrumentAny>>> {
    let values = instruments
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    domain::catalogs::instrument_versions(&values)?;
    let mut chains: BTreeMap<String, Vec<InstrumentAny>> = BTreeMap::new();
    for instrument in instruments {
        ensure!(
            instrument.ts_init().as_u64() <= selection.decision_cutoff_ns.get(),
            "INSTRUMENT_DEFINITION_FROM_FUTURE"
        );
        chains
            .entry(instrument.id().to_string())
            .or_default()
            .push(instrument);
    }
    for versions in chains.values_mut() {
        let preceding = versions.partition_point(|instrument| {
            instrument.ts_init().as_u64() < selection.event_start_ns.get()
        });
        // An update at the boundary remains an event, including its ordering checks.
        // A static catalog may begin with an empty interval before its first definition.
        // Keep the original time; each BAR must still be available after this baseline.
        *versions = versions.split_off(preceding.saturating_sub(1));
    }
    Ok(chains)
}

/// Measure the same native records for operator preparation and formal DATA_VALIDATE.
/// This does not certify coverage, source permissions, historical availability or PIT.
pub fn measure_catalog(
    root: &Path,
    selected: &NativeDatasetSelectionV1,
    measure_notionals: bool,
) -> Result<(NativeMarketData, NativeDatasetQualityV1)> {
    let data = load_catalog(root, &selected.selection)?;
    crate::prediction::catalog_closes(root, &data, &selected.selection, &selected.settlements)?;
    let mut first = u64::MAX;
    let mut last = 0;
    let mut available = 0;
    let mut instrument_ids = Vec::with_capacity(data.series.len());
    let last_bar_notionals = measure_notionals
        .then(|| last_bar_notionals(&data))
        .transpose()?;
    for series in &data.series {
        instrument_ids.push(series.instrument.id().to_string());
        for bar in &series.bars {
            first = first.min(bar.ts_event.as_u64());
            last = last.max(bar.ts_event.as_u64());
            available = available.max(bar.ts_init.as_u64());
        }
    }
    let count = |value| DbCounter::new(value).map_err(anyhow::Error::msg);
    let quality = NativeDatasetQualityV1 {
        settlements: selected.settlements.clone(),
        dataset_revision_id: selected.dataset_revision_id,
        selection: selected.selection.clone(),
        row_count: count(data.rows as u64)?,
        instrument_ids,
        first_event_ns: count(first)?,
        last_event_ns: count(last)?,
        available_through_ns: count(available)?,
        last_bar_notionals,
    };
    Ok((data, quality))
}

pub fn quality_report(datasets: Vec<NativeDatasetQualityV1>) -> Result<NativeDataQualityReportV1> {
    Ok(NativeDataQualityReportV1 {
        schema_version: SchemaV1,
        native_version: "nautilus-persistence/0.63.0".into(),
        checked_at: chrono::DateTime::from_timestamp_micros(chrono::Utc::now().timestamp_micros())
            .ok_or_else(|| anyhow::anyhow!("NATIVE_CLOCK"))?,
        datasets,
    })
}

/// Same last-known native BAR valuation for quality reports and rolling research.
pub(crate) fn last_bar_notionals(
    data: &NativeMarketData,
) -> Result<Vec<contracts::execution::NativeBarNotionalV1>> {
    data.series
        .iter()
        .map(|series| {
            let bar = series
                .bars
                .last()
                .ok_or_else(|| anyhow::anyhow!("CATALOG_EMPTY_SELECTION"))?;
            let notional = series.instrument.try_calculate_notional_value(
                bar.volume,
                bar.close,
                Some(false),
            )?;
            Ok(contracts::execution::NativeBarNotionalV1 {
                instrument_id: series.instrument.id().to_string(),
                currency: notional.currency.to_string(),
                event_ns: contracts::DbCounter::new(bar.ts_event.as_u64())
                    .map_err(anyhow::Error::msg)?,
                available_ns: contracts::DbCounter::new(bar.ts_init.as_u64())
                    .map_err(anyhow::Error::msg)?,
                close_price: bar
                    .close
                    .as_decimal()
                    .to_string()
                    .parse()
                    .map_err(anyhow::Error::msg)?,
                traded_volume: bar
                    .volume
                    .as_decimal()
                    .to_string()
                    .parse()
                    .map_err(anyhow::Error::msg)?,
                notional_value: notional
                    .as_decimal()
                    .to_string()
                    .parse()
                    .map_err(anyhow::Error::msg)?,
            })
        })
        .collect()
}

fn selected_types(selection: &NativeBarSelectionV1) -> Result<Vec<BarType>> {
    ensure!(
        selection.bar_types.len() >= 1
            && selection.maximum_rows >= 1
            && selection.event_start_ns < selection.event_end_ns
            && selection.event_end_ns <= selection.decision_cutoff_ns,
        "CATALOG_SELECTION_INVALID"
    );
    bar_types(&selection.bar_types)
}

/// Shared canonical native bar parsing, independent of any catalog path.
pub(crate) fn bar_types(values: &[String]) -> Result<Vec<BarType>> {
    ensure!(values.len() >= 1, "CATALOG_SELECTION_INVALID");
    let mut instruments = std::collections::BTreeSet::new();
    let mut types = Vec::with_capacity(values.len());
    for text in values {
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
    // The pinned native push decoder subtracts this untrusted length unchecked.
    // Check only the fixed-size footer boundary; native Parquet still parses all
    // metadata and pages. Runtime mounts are already scoped and immutable.
    // ponytail: one footer read per mounted file; remove when the pinned decoder checks bounds.
    for path in catalog.list_parquet_files("data")? {
        let mut file = std::fs::File::open(root.join(path))?;
        let length = file.metadata()?.len();
        ensure!(length >= 8, "CATALOG_PARQUET_FOOTER_INVALID");
        file.seek(SeekFrom::End(-8))?;
        let mut footer = [0_u8; 8];
        file.read_exact(&mut footer)?;
        let metadata_length = u32::from_le_bytes(footer[..4].try_into()?);
        ensure!(
            u64::from(metadata_length) <= length - 8,
            "CATALOG_PARQUET_FOOTER_INVALID"
        );
    }
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
    for item in query {
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
        bars.len() <= selection.maximum_rows as usize,
        "CATALOG_ROW_LIMIT"
    );
    let mut definitions = select_instrument_versions(instruments, selection)?;
    let mut series = Vec::with_capacity(types.len());
    let mut indices = BTreeMap::new();
    for bar_type in types {
        let mut versions = definitions
            .remove(&bar_type.instrument_id().to_string())
            .ok_or_else(|| anyhow::anyhow!("CATALOG_INSTRUMENT_MISSING"))?;
        let instrument = versions.remove(0);
        indices.insert(bar_type.to_string(), series.len());
        series.push(NativeBarSeries {
            instrument,
            instrument_updates: versions,
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
        // Native records have no shared source ordinal to disambiguate this tie.
        ensure!(
            !selected
                .instrument_updates
                .iter()
                .any(|instrument| instrument.ts_init() == bar.ts_init),
            "CATALOG_AMBIGUOUS_INSTRUMENT_UPDATE"
        );
        let instrument = selected.instrument_at(available)?;
        if let Some(previous) = selected.bars.last() {
            ensure!(
                // A source response may publish several distinct historical bars
                // together. Native APIs preserve equal receipt times; never invent
                // per-row offsets. Event identities and availability order still
                // reject duplicates, revisions and time travel.
                previous.ts_event < bar.ts_event && previous.ts_init <= bar.ts_init,
                "CATALOG_NONUNIQUE_OR_REVISED_BAR"
            );
        }
        let precision = instrument.price_precision();
        for price in [bar.open, bar.high, bar.low, bar.close] {
            ensure!(
                price.precision == precision,
                "CATALOG_PRICE_PRECISION_MISMATCH"
            );
            instrument.try_normalize_price(price)?;
            ensure!(
                instrument
                    .min_price()
                    .is_none_or(|minimum| price >= minimum)
                    && instrument
                        .max_price()
                        .is_none_or(|maximum| price <= maximum),
                "CATALOG_PRICE_OUTSIDE_INSTRUMENT_BOUNDS"
            );
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
        instrument.try_normalize_qty(bar.volume)?;
        selected.bars.push(bar);
        rows += 1;
    }
    ensure!(
        rows > 0 && series.iter().all(|series| !series.bars.is_empty()),
        "CATALOG_EMPTY_SELECTION"
    );
    Ok(NativeMarketData { series, rows })
}
