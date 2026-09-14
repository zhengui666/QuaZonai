//! Immutable target-only package body, never an approval or execution instruction.
use crate::{portfolio::PortfolioConstraintsV1, settings::PackageSchemaVersion, DecimalValue, Id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownstreamDeliveryModeV1 {
    TargetOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamProbeRequestV1 {
    pub schema_version: crate::SchemaV1,
    pub expected_revision: crate::Revision,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "status",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum DownstreamProbeOutcomeV1 {
    Available {
        capabilities: DownstreamCapabilitiesV1,
    },
    Unavailable {
        reason: crate::runtime::RuntimeProbeFailure,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamProbeViewV1 {
    pub id: Id,
    pub downstream_id: Id,
    pub integration_revision: crate::Revision,
    pub snapshot_artifact_id: Id,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub outcome: DownstreamProbeOutcomeV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownstreamReadinessState {
    NotChecked,
    Disabled,
    Stale,
    Unavailable,
    Available,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamReadinessV1 {
    pub schema_version: crate::SchemaV1,
    pub downstream_id: Id,
    pub integration_revision: crate::Revision,
    pub state: DownstreamReadinessState,
    pub latest_observation: Option<DownstreamProbeViewV1>,
    pub available_package_versions: Vec<PackageSchemaVersion>,
    pub available_environments: Vec<crate::forward::ForwardEnvironmentV1>,
}

/// Native observation only. This does not authorize approval or delivery.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamCapabilitiesV1 {
    pub schema_version: crate::SchemaV1,
    pub delivery_mode: DownstreamDeliveryModeV1,
    #[schema(value_type = std::collections::BTreeSet<PackageSchemaVersion>, min_items = 1, max_items = 1)]
    pub accepted_package_versions: Vec<PackageSchemaVersion>,
    #[schema(value_type = std::collections::BTreeSet<crate::forward::ForwardEnvironmentV1>, min_items = 1, max_items = 2)]
    pub environments: Vec<crate::forward::ForwardEnvironmentV1>,
    #[schema(schema_with = market_capability_versions_schema)]
    pub market_capability_versions: Vec<String>,
    pub accepting_targets: bool,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

fn market_capability_versions_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Type};
    ArrayBuilder::new()
        .min_items(Some(1))
        .max_items(Some(64))
        .unique_items(true)
        .items(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(1))
                .max_length(Some(200)),
        )
        .into()
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCreateV1 {
    pub schema_version: crate::SchemaV1,
    pub candidate_id: Id,
    pub evaluation_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseApproveV1 {
    pub schema_version: crate::SchemaV1,
    pub downstream_id: Id,
    pub environment: crate::forward::ForwardEnvironmentV1,
    pub expected_downstream_revision: crate::Revision,
    pub expected_latest_decision_id: Option<Id>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalViewV1 {
    pub id: Id,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub project_id: Id,
    pub candidate_id: Id,
    pub release_id: Id,
    pub downstream_id: Id,
    pub environment: crate::forward::ForwardEnvironmentV1,
    pub authority_kind: String,
    pub automation_policy_id: Option<Id>,
    pub evidence_set_id: Id,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    /// None on original historical rows; never sufficient for new delivery.
    pub downstream_revision: Option<crate::Revision>,
    pub decision_ordinal: Option<u32>,
    pub readiness_observation_id: Option<Id>,
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
