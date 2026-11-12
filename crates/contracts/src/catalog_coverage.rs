//! Native file coverage is not evidence of event completeness or historical availability.
use crate::{DbCounter, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogCoverageRequestV1 {
    /// Legacy BAR-only shorthand. May be combined with explicit native selectors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bar_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<NativeCatalogSelectorV1>,
    /// Inclusive ts_init bounds, not the half-open ts_event selection used by backtests.
    pub start_ns: DbCounter,
    pub end_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogSelectorV1 {
    /// Parsed by the pinned official NautilusDataType parser, including its native aliases.
    pub data_type: String,
    /// Official instrument ID, BAR type or custom-data identifier. No cross-identifier union.
    pub identifier: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogIntervalV1 {
    pub start_ns: DbCounter,
    pub end_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogSeriesCoverageV1 {
    /// Retained for exact BAR selectors and legacy BAR-only callers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_type: Option<String>,
    pub selector: NativeCatalogSelectorV1,
    /// All native file intervals for this selector, with inclusive endpoints.
    pub file_intervals: Vec<NativeCatalogIntervalV1>,
    /// Missing file coverage within the requested inclusive range. Not missing market events.
    pub missing_intervals: Vec<NativeCatalogIntervalV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogCoverageResultV1 {
    pub schema_version: SchemaV1,
    pub request: NativeCatalogCoverageRequestV1,
    pub timestamp_basis: String,
    pub interval_bounds: String,
    pub coverage_basis: String,
    pub series: Vec<NativeCatalogSeriesCoverageV1>,
}

/// Discovery reads only the bound catalog; no caller-supplied path or guessed selector.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogInspectRequestV1 {
    /// Exact instrument IDs; also matches the instrument component of BAR identifiers.
    #[serde(default)]
    pub instrument_ids: Option<Vec<String>>,
    /// Official NautilusDataType names/aliases; filters market-data selectors, not definitions.
    #[serde(default)]
    pub data_types: Option<Vec<String>>,
    /// Projection records per page, default 100. Not an upstream catalog limit.
    pub page_size: Option<u32>,
    /// Return next_cursor unchanged. Omitted filters inherit its scope. Requires unchanged catalog.
    pub cursor: Option<NativeCatalogInspectCursorV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogInspectCursorV1 {
    pub offset: DbCounter,
    pub total_items: DbCounter,
    pub instrument_ids: Vec<String>,
    pub data_types: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeCatalogInspectPageV1 {
    pub offset: DbCounter,
    pub returned_items: DbCounter,
    pub total_items: DbCounter,
    pub unit: String,
    pub consistency: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeCatalogInspectSeriesV1 {
    pub data_type: String,
    pub identifier: String,
    pub file_intervals: Vec<NativeCatalogIntervalV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeCatalogInstrumentVersionV1 {
    pub instrument: serde_json::Value,
    pub ts_init_ns: DbCounter,
    pub ts_event_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeCatalogInspectResultV1 {
    pub schema_version: SchemaV1,
    pub timestamp_basis: String,
    pub interval_bounds: String,
    pub coverage_basis: String,
    /// Official instrument serialization, with every returned definition retained.
    pub instruments: Vec<NativeCatalogInstrumentVersionV1>,
    pub selectors: Vec<NativeCatalogInspectSeriesV1>,
    pub page: NativeCatalogInspectPageV1,
    /// Non-null means incomplete: continue to read required metadata. Same selector can span pages.
    pub next_cursor: Option<NativeCatalogInspectCursorV1>,
}
