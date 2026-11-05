//! Hash the same original native prefix used by forecast/fit/study, not a path.
use anyhow::Result;
use contracts::{catalog_prefix::*, science::NativeBarSelectionV1};
use std::path::Path;

pub fn digest(
    catalog: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<NativeCatalogPrefixResultV1> {
    #[cfg(any(feature = "catalog-prepare", feature = "polymarket-history"))]
    {
        use contracts::{DbCounter, SchemaV1};
        use nautilus_model::instruments::Instrument;
        use sha2::{Digest, Sha256};
        // Same bounds, availability, definition-history and precision checks as
        // science. Copying identical source rows to another directory is valid;
        // changing a past mark, clock or instrument definition changes identity.
        let market = crate::catalog::load_catalog(catalog, selection)?;
        let mut series = Vec::with_capacity(market.series.len());
        let mut contents = Vec::with_capacity(market.series.len());
        for original in market.series {
            let mut content = serde_json::json!({
                "instrument":original.instrument,
                "instrument_updates":original.instrument_updates,
                "bar_type":original.bar_type,
                "bars":original.bars,
            });
            content.sort_all_objects();
            let encoded = serde_json::to_vec(&content)?;
            series.push(NativeCatalogPrefixSeriesV1 {
                instrument_id: original.instrument.id().to_string(),
                bar_type: original.bar_type.to_string(),
                row_count: DbCounter::new(original.bars.len() as u64)
                    .map_err(anyhow::Error::msg)?,
                instrument_version_count: u32::try_from(original.instrument_updates.len() + 1)?,
                content_sha256: format!("{:x}", Sha256::digest(encoded)),
            });
            contents.push(content);
        }
        // Include domain separation/version so an unrelated SHA256 document
        // cannot be mistaken for this scientific content-identity protocol.
        let method = "NAUTILUS_SELECTED_PREFIX_SHA256_V2";
        let mut combined = serde_json::json!({
            "method":method,
            "series":contents,
            "execution_fees":market.execution_fees,
        });
        combined.sort_all_objects();
        Ok(NativeCatalogPrefixResultV1 {
            schema_version: SchemaV1,
            method: method.into(),
            selection: selection.clone(),
            series_count: u32::try_from(series.len())?,
            row_count: DbCounter::new(market.rows as u64).map_err(anyhow::Error::msg)?,
            content_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(&combined)?)),
            series,
        })
    }
    #[cfg(not(any(feature = "catalog-prepare", feature = "polymarket-history")))]
    {
        let _ = (catalog, selection);
        Err(anyhow::anyhow!(
            "CATALOG_PREFIX_DIGEST_CAPABILITY_NOT_BUILT"
        ))
    }
}
