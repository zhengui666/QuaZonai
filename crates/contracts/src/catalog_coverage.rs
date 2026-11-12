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
