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
        /// Historical observations retain their original advertised versions.
        #[schema(schema_with = historical_downstream_capabilities_schema)]
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
    #[schema(value_type = Vec<crate::strategy_portfolio::TargetPackageVersionV2>, max_items = 1)]
    pub available_package_versions: Vec<PackageSchemaVersion>,
    pub available_environments: Vec<crate::forward::ForwardEnvironmentV1>,
}

/// Native observation only. This does not authorize approval or delivery.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamCapabilitiesV1 {
    pub schema_version: crate::SchemaV1,
    pub delivery_mode: DownstreamDeliveryModeV1,
    #[schema(schema_with = crate::settings::active_package_versions_schema)]
    pub accepted_package_versions: Vec<PackageSchemaVersion>,
    #[schema(value_type = std::collections::BTreeSet<crate::forward::ForwardEnvironmentV1>, min_items = 1, max_items = 2)]
    pub environments: Vec<crate::forward::ForwardEnvironmentV1>,
    #[schema(schema_with = market_capability_versions_schema)]
    pub market_capability_versions: Vec<String>,
    pub accepting_targets: bool,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

// The active capability contract advertises V2 only. A persisted observation is
// audit data and may retain V1; it never restores permission for a new delivery.
fn historical_downstream_capabilities_schema(
) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::{
        openapi::{
            schema::{ArrayBuilder, ObjectBuilder, Schema, Type},
            RefOr,
        },
        PartialSchema,
    };
    let RefOr::T(Schema::Object(mut object)) = DownstreamCapabilitiesV1::schema() else {
        unreachable!("downstream capabilities is an object schema");
    };
    object.properties.insert(
        "accepted_package_versions".into(),
        ArrayBuilder::new()
            .min_items(Some(1))
            .max_items(Some(2))
            .unique_items(true)
            .items(
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .enum_values(Some(["1", "2"])),
            )
            .into(),
    );
    RefOr::T(Schema::Object(object))
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
    #[schema(min_items = 1, max_items = 256)]
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
    /// Original Build weights, never a newly observed or fresh-cash account.
    pub current_weights: crate::science::PortfolioCurrentWeightsV1,
    /// Original Build execution assumptions, not current downstream settings.
    pub execution_settings: crate::science::NativeSimulationSettingsV1,
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

/// Historical immutable bytes only. Never accepted by an active claim or package envelope.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetPackageV1 {
    pub release_id: Id,
    #[serde(
        serialize_with = "serialize_package_v1",
        deserialize_with = "deserialize_package_v1"
    )]
    #[schema(schema_with = package_v1_schema)]
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

fn serialize_package_v1<S: serde::Serializer>(
    version: &PackageSchemaVersion,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if *version != PackageSchemaVersion::V1 {
        return Err(serde::ser::Error::custom(
            "forecast package requires version 1",
        ));
    }
    serializer.serialize_str("1")
}

fn deserialize_package_v1<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<PackageSchemaVersion, D::Error> {
    match PackageSchemaVersion::deserialize(deserializer)? {
        PackageSchemaVersion::V1 => Ok(PackageSchemaVersion::V1),
        PackageSchemaVersion::V2 => Err(serde::de::Error::custom(
            "forecast package requires version 1",
        )),
    }
}

fn package_v1_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::String)
        .enum_values(Some(["1"]))
        .into()
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

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffOfferV1 {
    pub schema_version: crate::SchemaV1,
    pub release_id: Id,
    pub approval_id: Id,
    pub supersedes_handoff_id: Option<Id>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
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

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffClaimV1 {
    pub schema_version: crate::SchemaV1,
    pub external_claim_id: String,
    #[serde(
        serialize_with = "crate::settings::serialize_active_package_version",
        deserialize_with = "crate::settings::deserialize_active_package_version"
    )]
    #[schema(value_type = crate::strategy_portfolio::TargetPackageVersionV2)]
    pub package_schema_version: PackageSchemaVersion,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffClaimViewV1 {
    pub handoff: HandoffViewV1,
    pub package: ForecastTargetPackageV2,
}

/// Consume one existing model-capital root for an original accepted Paper claim.
/// The caller cannot supply capital, targets, artifact locators or a receipt time.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PaperInitialExecutionConsumeV1 {
    pub schema_version: crate::SchemaV1,
    pub paper_initialization: crate::science::PaperInitializationRefV1,
    pub release_id: Id,
    #[schema(min_length = 1, max_length = 200)]
    pub external_claim_id: String,
    /// Newly generated by the current service process. Never restored from a
    /// configuration, claim, journal or earlier execution receipt.
    pub owner_instance_id: Id,
}

/// An immutable attempt-consumption fact, not proof that a native engine started.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaperInitialExecutionStateV1 {
    Consumed,
}

/// Audit data only, never a serializable execution permit. Only the current
/// authenticated HTTP response with CommandResult.replayed=false may be checked
/// for an in-process, non-Clone one-use permit. Files/replays cannot mint one.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PaperInitialExecutionViewV1 {
    pub schema_version: crate::SchemaV1,
    pub state: PaperInitialExecutionStateV1,
    pub paper_initialization: crate::science::PaperInitializationRefV1,
    /// Resolved by Store from the original release, never supplied by the host.
    pub package_artifact_id: Id,
    pub owner_instance_id: Id,
    /// Existing credential that claimed the target and consumed this attempt.
    pub consuming_credential_id: Id,
    /// Real server receipt time, never the model's historical initial cutoff.
    pub consumed_at: chrono::DateTime<chrono::Utc>,
    /// Original immutable HANDOFF_CLAIM receipt body. A new consumer uses these
    /// canonical target bytes, not an owner-edited local package with matching IDs.
    pub claim: Box<crate::strategy_portfolio::HandoffClaimViewV2>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffAckOutcomeV1 {
    Acknowledged,
    Rejected,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffAckV1 {
    pub schema_version: crate::SchemaV1,
    pub external_ack_id: String,
    pub external_claim_id: Option<String>,
    pub outcome: HandoffAckOutcomeV1,
    pub reason_code: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRevokeV1 {
    pub schema_version: crate::SchemaV1,
    pub expected_latest_revocation_id: Option<Id>,
    pub effective_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reason_code: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRevocationViewV1 {
    pub id: Id,
    pub approval_id: Id,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub reason_code: Option<String>,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutomationModeV1 {
    Manual,
    AutoPaper,
    AutoHandoff,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationPolicyContentV1 {
    pub mode: AutomationModeV1,
    pub mandate_id: Id,
    pub downstream_id: Id,
    pub required_paper_observations: u32,
    pub minimum_paper_elapsed_seconds: crate::DbCounter,
    pub max_feedback_age_seconds: crate::DbCounter,
    pub promotion_metric_requirements: Vec<crate::evidence::MetricRequirementV1>,
    pub degradation_metric_requirements: Vec<crate::evidence::MetricRequirementV1>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub enabled_for_new_rebalances: bool,
    pub max_rebalances_per_day: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationAuthorizeV1 {
    pub schema_version: crate::SchemaV1,
    pub expected_project_revision: crate::Revision,
    pub content: AutomationPolicyContentV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationPolicyViewV1 {
    pub id: Id,
    pub project_id: Id,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub authorized_at: chrono::DateTime<chrono::Utc>,
    pub content: AutomationPolicyContentV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyRevokeV1 {
    pub schema_version: crate::SchemaV1,
    pub expected_latest_revocation_id: Option<Id>,
    pub effective_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyRevocationViewV1 {
    pub id: Id,
    pub automation_policy_id: Id,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
}
