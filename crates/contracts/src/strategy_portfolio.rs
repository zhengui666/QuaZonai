//! Weight strategies share the existing Alpha, Run, candidate and delivery
//! aggregates. Their outputs are target weights, never return forecasts.
use crate::{
    evidence::AlphaVersionView,
    portfolio::{AllocationTargetV1, PortfolioConstraintsV1},
    research::{DataOrigin, PitStatus},
    science::{
        FeatureDefinitionV1, NativeBarSelectionV1, NativeSimulationRequestV1,
        NativeSimulationResultV1, NativeSimulationSettingsV1, NativeTargetPointV1,
    },
    DbCounter, DecimalValue, Id, Revision, SchemaV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyOutputKindV1 {
    TargetWeight,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyAlphaAdoptV1 {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    #[schema(min_length = 1, max_length = 200)]
    pub name: String,
    /// Selects an original fold; its clocks and ordinals are resolved by Store.
    pub source_fold_index: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyAlphaAdoptIntentV1 {
    pub schema_version: SchemaV1,
    pub experiment_id: Id,
    pub request: StrategyAlphaAdoptV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptedExperimentSourceV1 {
    pub experiment_id: Id,
    pub evaluation_run_id: Id,
    pub accepted_attempt_id: Id,
    pub report_artifact_id: Id,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureReplayInitializationV1 {
    pub source_fold_index: u16,
    pub first_ordinal: u32,
    pub first_event_ns: DbCounter,
    pub first_decision_ns: DbCounter,
}

/// A fresh instance replays every decision from the original fold start. It
/// retains WASM globals/memory and original ordinals between predictions.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenTargetPolicyV1 {
    pub schema_version: SchemaV1,
    pub output_kind: StrategyOutputKindV1,
    pub source: AcceptedExperimentSourceV1,
    pub code_artifact_id: Id,
    pub model_artifact_id: Id,
    pub parameter_artifact_id: Id,
    pub dataset_revision_id: Id,
    pub feature_artifact_ids: Vec<Id>,
    pub feature_schema: Vec<FeatureDefinitionV1>,
    pub instrument_id: String,
    pub base_currency: String,
    pub model_abi: String,
    pub initialization: FeatureReplayInitializationV1,
    pub target_ttl_ns: DbCounter,
    pub runtime_image_ref: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyAlphaVersionV1 {
    pub schema_version: SchemaV1,
    pub output_kind: StrategyOutputKindV1,
    pub id: Id,
    pub project_id: Id,
    pub alpha_id: Id,
    pub version: Revision,
    pub experiment_id: Id,
    pub root_lineage_id: Id,
    pub policy: FrozenTargetPolicyV1,
    pub created_at: DateTime<Utc>,
}

/// Legacy forecast bytes keep their original shape; weight strategies carry an
/// explicit output discriminator and no inapplicable forecast fields.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum AlphaVersionEnvelopeV2 {
    Forecast(AlphaVersionView),
    TargetWeight(StrategyAlphaVersionV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyAllocationMethodV1 {
    FixedTargetWeights,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyPortfolioSourceV1 {
    StrategyAlpha,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyReleaseSourceV1 {
    NativeTargetDecision,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyMandateContentV1 {
    pub schema_version: SchemaV1,
    pub allocation_method: StrategyAllocationMethodV1,
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub universe_version_id: Id,
    pub execution_assumptions_id: Id,
    pub constraints: PortfolioConstraintsV1,
    pub exposure_tolerance: DecimalValue,
    pub max_input_age_seconds: u32,
    pub target_ttl_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyMandateCreateV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub runtime_id: Id,
    pub expected_runtime_revision: Revision,
    pub content: StrategyMandateContentV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyMandateViewV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub project_id: Id,
    pub version: u32,
    pub content: StrategyMandateContentV1,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum MandateCreateEnvelopeV2 {
    Forecast(Box<crate::portfolio::MandateCreateV1>),
    Strategy(Box<StrategyMandateCreateV1>),
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum MandateViewEnvelopeV2 {
    Forecast(Box<crate::portfolio::MandateViewV1>),
    Strategy(Box<StrategyMandateViewV1>),
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyMemberSelectionV1 {
    pub alpha_version_id: Id,
    pub ensemble_weight: DecimalValue,
}

/// Declared initialization of the next fresh Paper session, not an observed
/// balance or a snapshot from a different account/session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FreshPaperCashV1 {
    pub downstream_id: Id,
    pub trader_id: String,
    pub account_id: String,
    pub base_currency: String,
    pub starting_capital: DecimalValue,
    pub execution_assumptions_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyCurrentInputsV1 {
    pub alpha_version_id: Id,
    /// New observations retain the frozen feature dictionary and source meaning.
    pub feature_artifact_ids: Vec<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "purpose",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum StrategyPortfolioPurposeV1 {
    HistoricalReplay {},
    CurrentDecision {
        account_start: FreshPaperCashV1,
        member_inputs: Vec<StrategyCurrentInputsV1>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyPortfolioBuildV1 {
    pub schema_version: SchemaV1,
    pub source_kind: StrategyPortfolioSourceV1,
    pub cycle_id: Id,
    pub mandate_id: Id,
    pub input_set_id: Id,
    pub runtime_id: Id,
    pub expected_runtime_revision: Revision,
    #[schema(min_items = 1, max_items = 256)]
    pub members: Vec<StrategyMemberSelectionV1>,
    pub purpose: StrategyPortfolioPurposeV1,
    #[schema(schema_with = crate::data::bounded_native_limits_schema)]
    pub limits: crate::lifecycle::JobLimitsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum PortfolioBuildEnvelopeV2 {
    Forecast(crate::portfolio::PortfolioBuildRequestV1),
    Strategy(StrategyPortfolioBuildV1),
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeStrategyMemberV1 {
    pub alpha_id: Id,
    pub alpha_version_id: Id,
    pub ensemble_weight: DecimalValue,
    pub policy: FrozenTargetPolicyV1,
    pub feature_artifact_ids: Vec<Id>,
}

/// Market/feature provenance remains separate from the simulated account.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyInputProvenanceV1 {
    pub dataset_revision_id: Id,
    pub market_data_origin: DataOrigin,
    pub pit_status: PitStatus,
    pub revision_policy: crate::catalogs::DataRevisionPolicy,
    pub feature_artifact_origins: BTreeMap<Id, DataOrigin>,
    /// Only registered features have bindings. None preserves old reports;
    /// new fields require a matching Job binary as well as Server/Runtime.
    /// A current decision may also retain bindings for its historical policy Dataset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_source_bindings: Option<BTreeMap<Id, crate::data::RecordedFeatureSourceBindingV1>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeStrategyCompositionRequestV1 {
    pub schema_version: SchemaV1,
    pub selection: NativeBarSelectionV1,
    pub mandate: StrategyMandateContentV1,
    pub settings: NativeSimulationSettingsV1,
    pub purpose: StrategyPortfolioPurposeV1,
    pub input_provenance: StrategyInputProvenanceV1,
    #[schema(min_items = 1, max_items = 256)]
    pub members: Vec<NativeStrategyMemberV1>,
    pub total_fuel: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "purpose",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum StrategyCompositionOutcomeV1 {
    HistoricalReplay {
        simulation_request: Box<NativeSimulationRequestV1>,
        simulation: Box<NativeSimulationResultV1>,
    },
    CurrentDecision {
        account_start: Box<FreshPaperCashV1>,
        target: NativeTargetPointV1,
        /// Counts include warmup; only the final target is publishable.
        predictions_per_member: BTreeMap<Id, DbCounter>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeStrategyCompositionResultV1 {
    pub schema_version: SchemaV1,
    pub request: NativeStrategyCompositionRequestV1,
    pub native_versions: BTreeMap<String, String>,
    pub consumed_fuel: DbCounter,
    pub outcome: StrategyCompositionOutcomeV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyPortfolioCandidateV1 {
    pub schema_version: SchemaV1,
    pub source_kind: StrategyPortfolioSourceV1,
    pub id: Id,
    pub project_id: Id,
    pub mandate_id: Id,
    pub input_set_id: Id,
    pub run_id: Id,
    pub accepted_attempt_id: Id,
    pub report_artifact_id: Id,
    pub allocation_method: StrategyAllocationMethodV1,
    pub purpose: StrategyPortfolioPurposeV1,
    pub input_provenance: StrategyInputProvenanceV1,
    pub members: Vec<StrategyMemberSelectionV1>,
    pub targets: Vec<AllocationTargetV1>,
    pub cash_weight: DecimalValue,
    pub decision_asof: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Bounded read-only projection of a published candidate's accepted native report.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyPortfolioSummaryV1 {
    pub schema_version: SchemaV1,
    pub candidate_id: Id,
    pub project_id: Id,
    pub run_id: Id,
    pub accepted_attempt_id: Id,
    pub report_artifact_id: Id,
    pub input_provenance: StrategyInputProvenanceV1,
    pub native_versions: BTreeMap<String, String>,
    /// Accepted execution is not scientific qualification, PIT or delivery approval.
    pub interpretation: String,
    pub outcome: StrategyPortfolioSummaryOutcomeV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "purpose",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum StrategyPortfolioSummaryOutcomeV1 {
    HistoricalReplay {
        /// One original shared-capital simulation; never summed member curves.
        simulation: Box<crate::experiment_summary::NativeSimulationSummaryV1>,
    },
    CurrentDecision {
        /// A target-only decision has no simulated returns or historical equity.
        reason_code: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum PortfolioCandidateEnvelopeV2 {
    Forecast(crate::portfolio::CandidateDetailV1),
    Strategy(StrategyPortfolioCandidateV1),
}

/// List rows preserve the original flat forecast header, unlike detail responses.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum PortfolioCandidateListEnvelopeV2 {
    Forecast(crate::portfolio::CandidateViewV1),
    Strategy(StrategyPortfolioCandidateV1),
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyReleaseCreateV1 {
    pub schema_version: SchemaV1,
    pub source_kind: StrategyReleaseSourceV1,
    pub candidate_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum ReleaseCreateEnvelopeV2 {
    Forecast(crate::delivery::ReleaseCreateV1),
    TargetDecision(StrategyReleaseCreateV1),
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTargetDecisionSourceV1 {
    pub run_id: Id,
    pub accepted_attempt_id: Id,
    pub report_artifact_id: Id,
    pub alpha_version_ids: Vec<Id>,
    pub input_provenance: StrategyInputProvenanceV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
pub enum TargetPackageVersionV2 {
    #[serde(rename = "2")]
    V2,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetPackageV2 {
    pub release_id: Id,
    pub package_schema_version: TargetPackageVersionV2,
    pub project_id: Id,
    pub candidate_id: Id,
    pub mandate_id: Id,
    pub source_kind: StrategyReleaseSourceV1,
    pub source: NativeTargetDecisionSourceV1,
    pub execution_environment: crate::forward::ForwardEnvironmentV1,
    pub account_start: FreshPaperCashV1,
    pub input_revision_refs: Vec<Id>,
    pub engine_versions: BTreeMap<String, String>,
    pub asof: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub targets: Vec<crate::delivery::PackageTargetV1>,
    pub cash_weight: DecimalValue,
    pub constraints_summary: PortfolioConstraintsV1,
    pub exposure_tolerance: DecimalValue,
    pub cost_assumption_ref: Id,
    pub execution_settings: NativeSimulationSettingsV1,
    pub compatible_market_capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub provenance_artifact_refs: Vec<Id>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum TargetPackageEnvelopeV2 {
    Forecast(Box<crate::delivery::TargetPackageV1>),
    TargetDecision(Box<TargetPackageV2>),
}

impl<'de> Deserialize<'de> for TargetPackageEnvelopeV2 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value
            .get("package_schema_version")
            .and_then(serde_json::Value::as_str)
        {
            Some("1") => serde_json::from_value(value).map(Self::Forecast),
            Some("2") => serde_json::from_value(value).map(Self::TargetDecision),
            _ => {
                return Err(serde::de::Error::custom(
                    "unsupported target package version",
                ))
            }
        }
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffClaimViewV2 {
    pub handoff: crate::delivery::HandoffViewV1,
    pub package: TargetPackageEnvelopeV2,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyReleaseViewV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub project_id: Id,
    pub candidate_id: Id,
    pub mandate_id: Id,
    pub package_artifact_id: Id,
    pub package_schema_version: TargetPackageVersionV2,
    pub source_kind: StrategyReleaseSourceV1,
    pub source: NativeTargetDecisionSourceV1,
    pub execution_environment: crate::forward::ForwardEnvironmentV1,
    pub market_capability_version: String,
    pub asof: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum ReleaseViewEnvelopeV2 {
    Forecast(crate::delivery::ReleaseViewV1),
    TargetDecision(StrategyReleaseViewV1),
}
