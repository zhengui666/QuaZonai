//! Content identity of the actually loaded causal native data prefix.
use crate::{science::NativeBarSelectionV1, DbCounter, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogPrefixSeriesV1 {
    pub instrument_id: String,
    pub bar_type: String,
    pub row_count: DbCounter,
    pub instrument_version_count: u32,
    pub content_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogPrefixResultV1 {
    pub schema_version: SchemaV1,
    /// Version freezes exact original native serde encoding, object-key sorting
    /// and ordered series/definition/BAR arrays. No filesystem path is hashed.
    pub method: String,
    pub selection: NativeBarSelectionV1,
    pub series_count: u32,
    pub row_count: DbCounter,
    pub content_sha256: String,
    pub series: Vec<NativeCatalogPrefixSeriesV1>,
}
