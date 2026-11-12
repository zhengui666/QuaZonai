//! Read-only discovery through the pinned official catalog API.
use super::open_catalog;
use anyhow::{ensure, Result};
use contracts::{catalog_coverage::*, DbCounter, SchemaV1};
use nautilus_model::{
    data::{BarType, NautilusDataType},
    instruments::Instrument,
};
use nautilus_persistence::catalog::types::CatalogDataType;
use std::path::Path;

pub fn query(
    root: &Path,
    request: NativeCatalogInspectRequestV1,
) -> Result<NativeCatalogInspectResultV1> {
    let page_size = request.page_size.unwrap_or(100) as usize;
    ensure!(page_size > 0, "CATALOG_INSPECT_PAGE_SIZE_INVALID");
    let mut instrument_ids = request.instrument_ids.clone().unwrap_or_else(|| {
        request
            .cursor
            .as_ref()
            .map(|c| c.instrument_ids.clone())
            .unwrap_or_default()
    });
    instrument_ids.sort();
    instrument_ids.dedup();
    let raw_types = request.data_types.clone().unwrap_or_else(|| {
        request
            .cursor
            .as_ref()
            .map(|c| c.data_types.clone())
            .unwrap_or_default()
    });
    let mut data_types = raw_types
        .iter()
        .map(|v| Ok(v.parse::<NautilusDataType>()?.to_string()))
        .collect::<Result<Vec<_>>>()?;
    data_types.sort();
    data_types.dedup();
    if let Some(cursor) = &request.cursor {
        ensure!(
            cursor.instrument_ids == instrument_ids && cursor.data_types == data_types,
            "CATALOG_INSPECT_CURSOR_FILTER_MISMATCH"
        );
    }
    let catalog = open_catalog(root)?;
    let native_instruments = catalog.query_instruments(if instrument_ids.is_empty() {
        None
    } else {
        Some(&instrument_ids)
    })?;
    let mut instruments = Vec::new();
    for instrument in native_instruments {
        instruments.push(NativeCatalogInstrumentVersionV1 {
            instrument: serde_json::to_value(&instrument)?,
            ts_init_ns: DbCounter::new(instrument.ts_init().as_u64())
                .map_err(anyhow::Error::msg)?,
            ts_event_ns: DbCounter::new(instrument.ts_event().as_u64())
                .map_err(anyhow::Error::msg)?,
        });
    }
    let mut selectors = Vec::new();
    for kind in catalog.list_data_types()? {
        // Records and instrument definitions are not market-data selectors.
        if !matches!(kind, CatalogDataType::Data(_)) {
            continue;
        }
        if !data_types.is_empty() && !data_types.contains(&kind.to_string()) {
            continue;
        }
        for identifier in catalog.list_instruments(&kind)? {
            let instrument_id = if kind == CatalogDataType::Data(NautilusDataType::Bar) {
                identifier.parse::<BarType>()?.instrument_id().to_string()
            } else {
                identifier.clone()
            };
            if !instrument_ids.is_empty() && !instrument_ids.contains(&instrument_id) {
                continue;
            }
            let file_intervals = catalog
                .get_intervals(&kind, Some(&identifier))?
                .into_iter()
                .map(|(start, end)| {
                    Ok(NativeCatalogIntervalV1 {
                        start_ns: DbCounter::new(start).map_err(anyhow::Error::msg)?,
                        end_ns: DbCounter::new(end).map_err(anyhow::Error::msg)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            selectors.push(NativeCatalogInspectSeriesV1 {
                data_type: kind.to_string(),
                identifier,
                file_intervals,
            });
        }
    }
    selectors.sort_by(|a, b| (&a.data_type, &a.identifier).cmp(&(&b.data_type, &b.identifier)));
    // This is projection pagination over official Vec-returning APIs, not upstream streaming.
    instruments.sort_by_key(|row| {
        (
            row.instrument.to_string(),
            row.ts_init_ns.get(),
            row.ts_event_ns.get(),
        )
    });
    for selector in &mut selectors {
        selector
            .file_intervals
            .sort_by_key(|i| (i.start_ns.get(), i.end_ns.get()));
    }
    let total = instruments.len()
        + selectors
            .iter()
            .map(|s| s.file_intervals.len().max(1))
            .sum::<usize>();
    let offset = request.cursor.as_ref().map_or(0, |c| c.offset.get());
    ensure!(
        offset <= total as u64,
        "CATALOG_INSPECT_CURSOR_OUT_OF_RANGE"
    );
    if let Some(cursor) = &request.cursor {
        ensure!(
            cursor.total_items.get() == total as u64,
            "CATALOG_INSPECT_CATALOG_CHANGED_RESTART"
        );
    }
    let start = offset as usize;
    let end = start.saturating_add(page_size).min(total);
    let instrument_count = instruments.len();
    let page_instruments = instruments
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| (i >= start && i < end).then_some(v))
        .collect();
    let mut position = instrument_count;
    let mut page_selectors = Vec::new();
    for mut selector in selectors {
        let empty = selector.file_intervals.is_empty();
        let count = selector.file_intervals.len().max(1);
        let selected = position < end && position + count > start;
        selector.file_intervals = selector
            .file_intervals
            .into_iter()
            .enumerate()
            .filter_map(|(i, v)| (position + i >= start && position + i < end).then_some(v))
            .collect();
        position += count;
        if selected && (empty || !selector.file_intervals.is_empty()) {
            page_selectors.push(selector);
        }
    }
    let counter = |n: usize| DbCounter::new(n as u64).map_err(anyhow::Error::msg);
    Ok(NativeCatalogInspectResultV1 {
        schema_version: SchemaV1, timestamp_basis: "ts_init".into(), interval_bounds: "inclusive".into(),
        coverage_basis: "native_file_intervals_not_event_completeness".into(),
        instruments: page_instruments, selectors: page_selectors,
        page: NativeCatalogInspectPageV1 { offset: counter(start)?, returned_items: counter(end-start)?, total_items: counter(total)?,
            unit: "instrument_version_or_selector_interval".into(), consistency: "projection_only_requires_unchanged_catalog_not_a_snapshot_same_selector_may_span_pages".into() },
        next_cursor: if end < total { Some(NativeCatalogInspectCursorV1 { offset: counter(end)?, total_items: counter(total)?, instrument_ids, data_types }) } else { None },
    })
}
