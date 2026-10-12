//! Immutable target-only package body, never an approval or execution instruction.
use crate::{portfolio::PortfolioConstraintsV1, DecimalValue, Id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageOriginV1 {
    Demo,
    Real,
    /// Virtual account capital only; it never changes market-source provenance.
    Synthetic,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageTargetV1 {
    pub instrument_id: String,
    pub target_weight: DecimalValue,
    pub currency: String,
}

/// Forecast qualification and evaluation remain distinct from native weight decisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForecastReleaseSourceV2 {
    ForecastEvaluation,
}

/// All Build identities refer to the Candidate's original accepted Build, not its
/// later Study. Study evidence remains in evaluation_refs and provenance refs.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForecastEvaluationSourceV2 {
    pub build_run_id: Id,
    pub build_accepted_attempt_id: Id,
    pub build_parameters_artifact_id: Id,
    pub build_report_artifact_id: Id,
    pub build_input_set_id: Id,
    /// Original research Build fact; delivery still uses the approved Handoff environment.
    pub build_environment: crate::forward::ForwardEnvironmentV1,
    /// Original Build task Dataset, checked against its unique Forward binding.
    pub forward_dataset_revision_id: Id,
    pub forward_metadata_artifact_id: Id,
    pub current_weights_artifact_id: Id,
}

/// Claim-contained projection of the original registered Forward Dataset. It
/// contains only bounded metadata and complete original definition histories for
/// the frozen portfolio's assets, including zero targets and existing holdings.
/// No catalog locator, raw market rows, credentials or current eligibility is carried.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenForwardDatasetV2 {
    pub dataset_revision_id: Id,
    pub native_metadata_artifact_id: Id,
    #[schema(min_length = 1, max_length = 120)]
    pub storage_version: String,
    pub data_kind: crate::runtime::RuntimeDataKind,
    pub partition: crate::research::DataPartition,
    pub origin: crate::research::DataOrigin,
    pub pit_status: crate::research::PitStatus,
    pub revision_policy: crate::catalogs::DataRevisionPolicy,
    pub event_start: chrono::DateTime<chrono::Utc>,
    pub event_end: chrono::DateTime<chrono::Utc>,
    pub available_through: chrono::DateTime<chrono::Utc>,
    pub row_count: crate::DbCounter,
    pub selection: crate::science::NativeBarSelectionV1,
    /// Original externally tagged Nautilus InstrumentAny values, never rewritten.
    #[schema(min_items = 1)]
    pub instrument_definitions: Vec<serde_json::Value>,
}

/// Version-two forecast target delivery. No caller-supplied account initialization.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForecastTargetPackageV2 {
    pub release_id: Id,
    pub package_schema_version: crate::strategy_portfolio::TargetPackageVersionV2,
    pub source_kind: ForecastReleaseSourceV2,
    pub source: ForecastEvaluationSourceV2,
    pub forward_dataset: FrozenForwardDatasetV2,
    pub environment_origin: PackageOriginV1,
    pub project_id: Id,
    pub candidate_id: Id,
    pub mandate_id: Id,
    #[schema(min_items = 2)]
    pub qualification_refs: Vec<Id>,
    #[schema(min_items = 1)]
    pub evaluation_refs: Vec<Id>,
    #[schema(min_items = 1)]
    pub input_revision_refs: Vec<Id>,
    pub engine_versions: BTreeMap<String, String>,
    pub asof: chrono::DateTime<chrono::Utc>,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub current_weights_source: crate::portfolio::CandidateWeightsSourceV1,
    /// Original Build weights, never a newly observed or fresh-cash account.
    pub current_weights: crate::science::PortfolioCurrentWeightsV1,
    /// Original Build execution assumptions, not current downstream settings.
    pub execution_settings: crate::science::NativeSimulationSettingsV1,
    #[schema(min_items = 1)]
    pub targets: Vec<PackageTargetV1>,
    pub cash_weight: DecimalValue,
    pub constraints_summary: PortfolioConstraintsV1,
    pub exposure_tolerance: DecimalValue,
    pub cost_assumption_ref: Id,
    #[schema(min_items = 1, max_items = 64)]
    pub compatible_market_capabilities: Vec<String>,
    #[schema(max_items = 64)]
    pub limitations: Vec<String>,
    #[schema(min_items = 1)]
    pub provenance_artifact_refs: Vec<Id>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffStateV1 {
    Offered,
    Claimed,
    Acknowledged,
    Rejected,
    Revoked,
    Expired,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffViewV1 {
    pub id: Id,
    pub project_id: Id,
    pub candidate_id: Id,
    pub mandate_id: Id,
    pub release_id: Id,
    pub approval_id: Id,
    pub downstream_id: Id,
    pub environment: crate::forward::ForwardEnvironmentV1,
    pub delivery_sequence: crate::DbCounter,
    pub revision: crate::Revision,
    pub state: HandoffStateV1,
    pub supersedes_handoff_id: Option<Id>,
    pub offered_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub claimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub external_claim_id: Option<String>,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
}
