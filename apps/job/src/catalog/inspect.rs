//! Read-only discovery through the pinned official catalog API.
use super::open_catalog;
use anyhow::Result;
use contracts::{catalog_coverage::*, DbCounter, SchemaV1};
use nautilus_model::instruments::Instrument;
use nautilus_persistence::catalog::types::CatalogDataType;
use std::path::Path;

pub fn query(
    root: &Path,
    _request: NativeCatalogInspectRequestV1,
) -> Result<NativeCatalogInspectResultV1> {
    let catalog = open_catalog(root)?;
    let native_instruments = catalog.query_instruments(None)?;
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
        for identifier in catalog.list_instruments(&kind)? {
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
    Ok(NativeCatalogInspectResultV1 {
        schema_version: SchemaV1,
        timestamp_basis: "ts_init".into(),
        interval_bounds: "inclusive".into(),
        coverage_basis: "native_file_intervals_not_event_completeness".into(),
        instruments,
        selectors,
    })
}
