//! Immutable target-only package body, never an approval or execution instruction.
use crate::{portfolio::PortfolioConstraintsV1, settings::PackageSchemaVersion, DecimalValue, Id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCreateV1 {
    pub schema_version: crate::SchemaV1,
    pub candidate_id: Id,
    pub evaluation_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseViewV1 {
    pub id: Id,
    pub project_id: Id,
    pub candidate_id: Id,
    pub mandate_id: Id,
    pub evaluation_id: Id,
    pub package_artifact_id: Id,
    pub package_schema_version: PackageSchemaVersion,
    pub market_capability_version: String,
    pub asof: chrono::DateTime<chrono::Utc>,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub environment: PackageOriginV1,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

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

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRejectV1 {
    pub schema_version: crate::SchemaV1,
    pub downstream_id: Id,
    pub environment: crate::forward::ForwardEnvironmentV1,
    pub expected_latest_decision_id: Option<Id>,
    #[schema(min_length = 1, max_length = 120)]
    pub reason_code: String,
    #[schema(min_length = 1, max_length = 2000)]
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseReopenV1 {
    pub schema_version: crate::SchemaV1,
    pub expected_latest_decision_id: Id,
    #[schema(min_length = 1, max_length = 120)]
    pub reason_code: String,
    #[schema(min_length = 1, max_length = 2000)]
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReleaseDecisionV1 {
    Reject,
    Reopen,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDecisionViewV1 {
    pub id: Id,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub project_id: Id,
    pub release_id: Id,
    pub candidate_id: Id,
    pub downstream_id: Id,
    pub environment: crate::forward::ForwardEnvironmentV1,
    pub ordinal: u32,
    pub decision: ReleaseDecisionV1,
    pub supersedes_decision_id: Option<Id>,
    #[schema(min_length = 1, max_length = 120)]
    pub reason_code: String,
    #[schema(min_length = 1, max_length = 2000)]
    pub reason: String,
    pub decided_at: chrono::DateTime<chrono::Utc>,
    pub decided_by: String,
}
