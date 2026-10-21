//! Conditional all-catalog ensemble research; paths and market shortlists are absent.
use crate::{
    science::{NativeSimulationResultV1, NativeSimulationSettingsV1},
    DbCounter, DecimalValue, SchemaV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogEnsembleMemberV1 {
    pub weight: DecimalValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_fuel: Option<DbCounter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogEnsembleRequestV1 {
    pub schema_version: SchemaV1,
    pub event_start_ns: DbCounter,
    /// Warm the full causal prefix, but start a fresh shared account at this cut.
    pub evaluation_start_ns: DbCounter,
    /// Exclusive original BAR-time end. No artificial market or trade cap.
    pub event_end_ns: DbCounter,
    pub decision_cutoff_ns: DbCounter,
    pub source: String,
    pub revision: String,
    /// The source has no observed receive clock; this is a disclosed scenario.
    pub modeled_lag_ns: DbCounter,
    pub score_max_age_ns: Option<DbCounter>,
    pub score_threshold: f64,
    #[schema(min_items = 2)]
    pub members: Vec<NativeCatalogEnsembleMemberV1>,
    pub gross_exposure: DecimalValue,
    pub target_ttl_ns: DbCounter,
    pub settings: NativeSimulationSettingsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogEnsembleResultV1 {
    pub schema_version: SchemaV1,
    pub source: String,
    pub revision: String,
    pub observed_markets: DbCounter,
    pub bars: DbCounter,
    pub source_records: DbCounter,
    /// One item per member. Null explicitly means native fuel was unmetered.
    pub consumed_fuel: Vec<Option<DbCounter>>,
    pub simulation: NativeSimulationResultV1,
    pub assumptions: Vec<String>,
}
