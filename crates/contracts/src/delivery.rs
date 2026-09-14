//! Immutable target-only package body, never an approval or execution instruction.
use crate::{portfolio::PortfolioConstraintsV1, settings::PackageSchemaVersion, DecimalValue, Id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageOriginV1 {
    Demo,
    Real,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageTargetV1 {
    pub instrument_id: String,
    pub target_weight: DecimalValue,
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetPackageV1 {
    pub release_id: Id,
    pub package_schema_version: PackageSchemaVersion,
    pub environment_origin: PackageOriginV1,
    pub project_id: Id,
    pub candidate_id: Id,
    pub mandate_id: Id,
    #[schema(min_items = 2, max_items = 256)]
    pub qualification_refs: Vec<Id>,
    #[schema(min_items = 1, max_items = 256)]
    pub evaluation_refs: Vec<Id>,
    #[schema(min_items = 1, max_items = 256)]
    pub input_revision_refs: Vec<Id>,
    pub engine_versions: BTreeMap<String, String>,
    pub asof: chrono::DateTime<chrono::Utc>,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub current_weights_source: crate::portfolio::CandidateWeightsSourceV1,
    #[schema(min_items = 1, max_items = 256)]
    pub targets: Vec<PackageTargetV1>,
    pub cash_weight: DecimalValue,
    pub constraints_summary: PortfolioConstraintsV1,
    pub exposure_tolerance: DecimalValue,
    pub cost_assumption_ref: Id,
    #[schema(min_items = 1, max_items = 64)]
    pub compatible_market_capabilities: Vec<String>,
    #[schema(max_items = 64)]
    pub limitations: Vec<String>,
    #[schema(min_items = 1, max_items = 256)]
    pub provenance_artifact_refs: Vec<Id>,
}
