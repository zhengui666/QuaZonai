//! Read an already-authorized, immutable native catalog mounted into this job.
//! Native time ordering is necessary, not sufficient, evidence of historical availability.
#[cfg(feature = "market-history")]
pub mod aggregation;
pub mod coverage;
pub mod currencies;
mod file_batches;
pub mod publish;
pub use file_batches::{file_batch_session, FileBatchSession};

use anyhow::{ensure, Result};
use contracts::{
    execution::{NativeDataQualityReportV1, NativeDatasetQualityV1, NativeDatasetSelectionV1},
    science::NativeBarSelectionV1,
    DbCounter, SchemaV1,
};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarType, DataBatch, NautilusDataType},
    enums::{BarAggregation, PriceType},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog, catalog::types::CatalogQuery,
};
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
    pub execution_fees: Option<contracts::catalogs::FrozenMakerTakerFeesV1>,
    pub fee_selection: Option<NativeBarSelectionV1>,
}

/// The close of an explicitly right-open, internally aggregated native time bar.
/// External venue bars do not imply this convention.
/// Availability remains `ts_init` and may be later than this boundary.
pub fn right_open_bar_close_ns(bar: &Bar) -> Result<u64> {
    ensure!(
        bar.bar_type.is_internally_aggregated()
            && !bar.bar_type.is_composite()
            && bar.bar_type.spec().is_time_aggregated(),
        "RIGHT_OPEN_INTERNAL_TIME_BAR_REQUIRED"
    );
    bar.ts_event
        .as_u64()
        .checked_add(nautilus_model::data::bar::get_bar_interval_ns(&bar.bar_type).as_u64())
        .ok_or_else(|| anyhow::anyhow!("BAR_CLOSE_TIME_RANGE"))
}

/// A frozen aggregate is supplied from outside the consuming backtest runtime.
/// Keep the source type in its catalog; the official engine/risk cache consumes
/// this declared transport projection rather than synthesizing quotes or trades.
pub fn runtime_bar_type(source: BarType) -> BarType {
    if source.is_externally_aggregated() {
        return source;
    }
    BarType::new(
        source.instrument_id(),
        source.spec(),
        nautilus_model::enums::AggregationSource::External,
    )
}

pub fn runtime_bar(mut source: Bar) -> Result<Bar> {
    if source.bar_type.is_internally_aggregated() {
        let close = right_open_bar_close_ns(&source)?;
        ensure!(
            close <= source.ts_init.as_u64(),
            "CATALOG_BAR_AVAILABLE_BEFORE_CLOSE"
        );
        source.ts_event = close.into();
        source.bar_type = runtime_bar_type(source.bar_type);
    }
    Ok(source)
}

#[cfg(test)]
mod clock_tests {
    use super::*;
    use nautilus_model::types::{Price, Quantity};

    fn source(kind: &str, event: u64, available: u64) -> Bar {
        let price = Price::from("1.25");
        Bar::new(
            kind.parse().unwrap(),
            price,
            price,
            price,
            price,
            Quantity::from("2"),
            event.into(),
            available.into(),
        )
    }

    #[test]
    fn runtime_projection_keeps_source_open_and_actual_availability_separate() {
        let original = source("TOKEN/QUOTE.VENUE-1-SECOND-LAST-INTERNAL", 0, 3_000_000_000);
        let runtime = runtime_bar(original).unwrap();
        assert!(original.bar_type.is_internally_aggregated());
        assert_eq!(original.ts_event.as_u64(), 0);
        assert!(runtime.bar_type.is_externally_aggregated());
        assert_eq!(runtime.ts_event.as_u64(), 1_000_000_000);
        assert_eq!(runtime.ts_init, original.ts_init);
        assert_eq!(
            (runtime.open, runtime.close, runtime.volume),
            (original.open, original.close, original.volume)
        );
        let external = source("TOKEN/QUOTE.VENUE-1-SECOND-LAST-EXTERNAL", 100, 200);
        assert_eq!(runtime_bar(external).unwrap(), external);
    }

    #[test]
    fn a_frozen_aggregate_cannot_be_delivered_before_its_native_close() {
        let early = source("TOKEN/QUOTE.VENUE-1-SECOND-LAST-INTERNAL", 0, 999_999_999);
        assert!(runtime_bar(early).is_err());
        assert!(validate_price_bar(&early, None).is_err());
    }
}

/// Select original baseline plus updates without copying earlier or future versions.
/// This same selection is used for traded series and their selected instrument updates.
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
        native_version: "nautilus-persistence/0.69.0".into(),
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

pub(crate) fn selected_types(selection: &NativeBarSelectionV1) -> Result<Vec<BarType>> {
    ensure!(
        !selection.bar_types.is_empty()
            && selection.event_start_ns < selection.event_end_ns
            && selection.event_end_ns <= selection.decision_cutoff_ns,
        "CATALOG_SELECTION_INVALID"
    );
    bar_types(&selection.bar_types)
}

/// Shared canonical native bar parsing, independent of any catalog path.
pub(crate) fn bar_types(values: &[String]) -> Result<Vec<BarType>> {
    ensure!(!values.is_empty(), "CATALOG_SELECTION_INVALID");
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
            !bar_type.is_composite()
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

pub(crate) fn catalog_root(root: &Path) -> Result<std::path::PathBuf> {
    let metadata = std::fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "CATALOG_ROOT_INVALID"
    );
    Ok(root.canonicalize()?)
}

pub(crate) fn open_catalog(root: &Path) -> Result<ParquetDataCatalog> {
    let root = catalog_root(root)?;
    currencies::load(&root)?;
    let text = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("CATALOG_ROOT_INVALID"))?;
    // Use the fallible native constructor. Cloud storage support is not enabled.
    let catalog = ParquetDataCatalog::from_uri(text, None, Some(4096), None, None)?;
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
    Ok(catalog)
}

/// Load executable market data with original instrument definitions.
pub fn load_catalog(root: &Path, selection: &NativeBarSelectionV1) -> Result<NativeMarketData> {
    load_catalog_mode(root, selection, false)
}

/// Scoped replay can retain an unobserved, untraded market's original metadata.
pub(crate) fn load_scoped_catalog(
    root: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<NativeMarketData> {
    load_catalog_mode(root, selection, true)
}

fn load_catalog_mode(
    root: &Path,
    selection: &NativeBarSelectionV1,
    allow_empty_series: bool,
) -> Result<NativeMarketData> {
    let types = selected_types(selection)?;
    let catalog = open_catalog(root)?;
    let ids = types
        .iter()
        .map(|kind| kind.instrument_id().to_string())
        .collect::<Vec<_>>();
    let instruments = catalog.query_instruments_filtered(
        Some(&ids),
        None,
        Some(UnixNanos::from(selection.decision_cutoff_ns.get())),
    )?;
    let bars = read_bars(&catalog, selection)?;
    let mut market = validate_native_mode(instruments, bars, selection, allow_empty_series)?;
    market.execution_fees = crate::fee_models::catalog_fees::load(root, selection)?;
    market.fee_selection = Some(selection.clone());
    Ok(market)
}

fn read_bars(catalog: &ParquetDataCatalog, selection: &NativeBarSelectionV1) -> Result<Vec<Bar>> {
    // Filter availability and the half-open event interval in Arrow before native
    // decoding. Same-timestamp records cannot enlarge a decoded batch beyond its
    // fixed size; all rows in the original source/time selection are retained.
    let query = CatalogQuery::new(NautilusDataType::Bar)
        .with_identifiers(Some(selection.bar_types.clone()))
        .with_range(
            Some(UnixNanos::from(selection.event_start_ns.get())),
            Some(UnixNanos::from(selection.decision_cutoff_ns.get())),
        );
    let mut session = file_batch_session(
        catalog,
        query,
        Some(selection.event_start_ns.get()..selection.event_end_ns.get()),
        4096,
    )?;
    let mut bars = Vec::new();
    while let Some(batch) = session.next_batch()? {
        let DataBatch::Bar(batch) = batch else {
            anyhow::bail!("CATALOG_NATIVE_TYPE_MISMATCH");
        };
        for bar in batch.as_ref() {
            bars.push(*bar);
        }
    }
    // File discovery groups series by path. Stable ordering preserves each
    // file's FIFO for equal availability.
    if !bars.is_sorted_by_key(|bar| bar.ts_init) {
        bars.sort_by_key(|bar| bar.ts_init);
    }
    Ok(bars)
}

/// Factor statistics need observed prices/volume, not invented trading definitions.
/// Simulation and calibrated forecasts continue to use load_catalog.
pub(crate) fn load_price_bars(
    root: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<Vec<(BarType, Vec<Bar>)>> {
    let types = selected_types(selection)?;
    let catalog = open_catalog(root)?;
    let mut series = types
        .into_iter()
        .map(|kind| (kind, Vec::<Bar>::new()))
        .collect::<Vec<_>>();
    let indices = series
        .iter()
        .enumerate()
        .map(|(index, (kind, _))| (kind.to_string(), index))
        .collect::<BTreeMap<_, _>>();
    for bar in read_bars(&catalog, selection)? {
        let index = indices
            .get(&bar.bar_type.to_string())
            .ok_or_else(|| anyhow::anyhow!("CATALOG_UNREQUESTED_BAR_TYPE"))?;
        let event = bar.ts_event.as_u64();
        let available = bar.ts_init.as_u64();
        ensure!(
            event <= available && available <= selection.decision_cutoff_ns.get(),
            "CATALOG_TIME_INVALID"
        );
        if event < selection.event_start_ns.get() || event >= selection.event_end_ns.get() {
            continue;
        }
        let bars = &mut series[*index].1;
        validate_price_bar(&bar, bars.last())?;
        bars.push(bar);
    }
    ensure!(
        series.iter().all(|(_, bars)| !bars.is_empty()),
        "CATALOG_EMPTY_SELECTION"
    );
    Ok(series)
}

pub(crate) fn validate_price_bar(bar: &Bar, previous: Option<&Bar>) -> Result<()> {
    if bar.bar_type.is_internally_aggregated() {
        ensure!(
            right_open_bar_close_ns(bar)? <= bar.ts_init.as_u64(),
            "CATALOG_BAR_AVAILABLE_BEFORE_CLOSE"
        );
    }
    if let Some(previous) = previous {
        ensure!(
            previous.ts_event < bar.ts_event && previous.ts_init <= bar.ts_init,
            "CATALOG_NONUNIQUE_OR_REVISED_BAR"
        );
    }
    ensure!(!bar.volume.is_undefined(), "CATALOG_VOLUME_UNDEFINED");
    for price in [bar.open, bar.high, bar.low, bar.close] {
        ensure!(!price.is_undefined(), "CATALOG_PRICE_UNDEFINED");
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
    Ok(())
}

/// Revalidate exact identities after native directory discovery, which can match substrings.
/// This function does not set origin, PIT status, license grants or qualification.
pub fn validate_native(
    instruments: Vec<InstrumentAny>,
    bars: Vec<Bar>,
    selection: &NativeBarSelectionV1,
) -> Result<NativeMarketData> {
    validate_native_mode(instruments, bars, selection, false)
}

fn validate_native_mode(
    instruments: Vec<InstrumentAny>,
    bars: Vec<Bar>,
    selection: &NativeBarSelectionV1,
    allow_empty_series: bool,
) -> Result<NativeMarketData> {
    let types = selected_types(selection)?;
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
        validate_price_bar(&bar, selected.bars.last())?;
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
        }
        instrument.try_normalize_qty(bar.volume)?;
        selected.bars.push(bar);
        rows += 1;
    }
    ensure!(
        rows > 0 && (allow_empty_series || series.iter().all(|series| !series.bars.is_empty())),
        "CATALOG_EMPTY_SELECTION"
    );
    Ok(NativeMarketData {
        series,
        rows,
        execution_fees: None,
        fee_selection: None,
    })
}

/// Validate supplied owner metadata without scientific scoring.
pub fn validate_source(
    request: &contracts::review_science::NativeReviewSourceRequestV1,
) -> Result<contracts::review_science::NativeReviewSourceResultV1> {
    let metadata: contracts::catalogs::RuntimeCatalogMetadataV1 =
        serde_json::from_value(request.metadata.clone())?;
    domain::catalogs::metadata(&metadata, request.observed_at)?;
    domain::execution::native_quality(&metadata.quality)?;
    Ok(contracts::review_science::NativeReviewSourceResultV1 {
        schema_version: contracts::SchemaV1,
        method_version: "qz.review-source.v1".into(),
        validated: true,
        metadata: request.metadata.clone(),
    })
}

mod scope;
pub use scope::catalog_factor_scope;
