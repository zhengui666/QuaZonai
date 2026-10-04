//! Frozen external features and independently rerun target policies. These are
//! research evidence, not Alpha forecasts, fitted models, qualification or orders.
use super::{
    NativeBarSelectionV1, NativeSimulationRequestV1, NativeSimulationResultV1,
    NativeSimulationSettingsV1,
};
use crate::{
    research::{DataPartition, SplitPolicyV1},
    DbCounter, DecimalValue, Id, SchemaV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

pub const MAX_FEATURE_ARTIFACTS: usize = 16;
pub const MAX_FEATURE_INPUT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_EXPERIMENT_FEATURES: usize = 64;
pub const MAX_FEATURE_OBSERVATIONS: usize = 100_000;
pub const MAX_EXPERIMENT_ROWS: u32 = 100_000;
pub const MAX_EXPERIMENT_FOLDS: usize = 32;
pub const MAX_EXPERIMENT_DECISIONS: usize = 10_000;
pub const FEATURE_MODEL_ABI_V2: &str =
    "qz_set_feature_v2(i32,f64,i32,i64,i64)->();qz_predict_v2(i64,i64,i32,i32,i32)->f64";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "basis",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum FeatureAvailabilityV1 {
    Observed,
    /// A conditional schedule, never proof of historical observed availability.
    ModeledLag {
        lag_ns: DbCounter,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureDefinitionV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub feature_key: String,
    /// Caller-declared immutable source reference. Not a fetch URL or PIT grant.
    #[schema(min_length = 1, max_length = 512)]
    pub source_ref: String,
    #[schema(min_length = 1, max_length = 200)]
    pub source_key: String,
    pub availability: FeatureAvailabilityV1,
    /// Maximum decision minus source event age; null explicitly permits any age.
    pub max_age_ns: Option<DbCounter>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureObservationV1 {
    #[schema(minimum = 0, maximum = 63)]
    pub feature_index: u16,
    pub event_ns: DbCounter,
    /// Actual historical receive/publication clock, if known; never synthesized.
    pub observed_available_ns: Option<DbCounter>,
    /// Unique within a feature; makes corrections/ties deterministic.
    pub sequence: DbCounter,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub missing_reason: Option<String>,
}

/// Stored by the existing PARAMETERS upload. Its 2 MiB UTF-8 cap still applies
/// in addition to these count bounds. It is never native SIGNALS evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureObservationsV1 {
    pub schema_version: SchemaV1,
    pub partition: DataPartition,
    #[schema(min_items = 1, max_items = 64)]
    pub feature_schema: Vec<FeatureDefinitionV1>,
    #[schema(min_items = 1, max_items = 100000)]
    pub observations: Vec<FeatureObservationV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExperimentDecisionOutputV1 {
    /// The frozen model itself returns an unlevered long-only weight in [0,1].
    TargetWeight,
}

/// The immutable proposal parameter artifact. The server selects catalog and
/// split from the frozen Brief, not caller-supplied paths or fold boundaries.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentEvaluationParametersV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    #[schema(min_items = 1, max_items = 16)]
    pub feature_artifact_ids: Vec<Id>,
    pub instrument_id: String,
    #[schema(min_items = 1, max_items = 64)]
    pub feature_schema: Vec<FeatureDefinitionV1>,
    #[schema(minimum = 1, maximum = 100000)]
    pub label_horizon_observations: u32,
    pub total_fuel: DbCounter,
    pub target_ttl_ns: DbCounter,
    pub decision_output: ExperimentDecisionOutputV1,
    pub settings: NativeSimulationSettingsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeExperimentEvaluationRequestV1 {
    pub schema_version: SchemaV1,
    pub selection: NativeBarSelectionV1,
    pub instrument_id: String,
    #[schema(min_items = 1, max_items = 64)]
    pub feature_schema: Vec<FeatureDefinitionV1>,
    pub split_policy: SplitPolicyV1,
    #[schema(minimum = 1, maximum = 100000)]
    pub label_horizon_observations: u32,
    pub total_fuel: DbCounter,
    pub target_ttl_ns: DbCounter,
    pub decision_output: ExperimentDecisionOutputV1,
    pub settings: NativeSimulationSettingsV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeatureMissingReasonV1 {
    NotYetAvailable,
    Expired,
    SourceMissing,
}
impl FeatureMissingReasonV1 {
    /// Zero is reserved for a present finite value, including observed zero.
    pub fn mask(self) -> i32 {
        match self {
            Self::NotYetAvailable => 1,
            Self::Expired => 2,
            Self::SourceMissing => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureValueV1 {
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub missing_reason: Option<FeatureMissingReasonV1>,
    pub event_ns: Option<DbCounter>,
    pub observed_available_ns: Option<DbCounter>,
    pub effective_available_ns: Option<DbCounter>,
    pub sequence: Option<DbCounter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeExperimentDecisionV1 {
    pub ordinal: u32,
    pub event_ns: DbCounter,
    pub decision_ns: DbCounter,
    /// Actual end of the N-observed-bar label, never inferred seconds.
    pub label_end_ns: Option<DbCounter>,
    pub label_available_ns: Option<DbCounter>,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub label_return: Option<f64>,
    #[schema(min_items = 1, max_items = 64)]
    pub features: Vec<FeatureValueV1>,
    pub target_weight: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeExperimentFoldV1 {
    #[schema(minimum = 0, maximum = 31)]
    pub fold_index: u16,
    pub training_ordinals: Vec<u32>,
    pub training_end_available_ns: DbCounter,
    #[schema(min_items = 1, max_items = 10000)]
    pub decisions: Vec<NativeExperimentDecisionV1>,
    /// Exact generated targets replayed through the existing native engine.
    pub simulation_request: NativeSimulationRequestV1,
    pub simulation: NativeSimulationResultV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeExperimentEvaluationResultV1 {
    pub schema_version: SchemaV1,
    pub native_versions: BTreeMap<String, String>,
    pub dataset_revision_id: Id,
    pub model_artifact_id: Id,
    /// Original frozen request, including ordered feature meanings and limits.
    pub request: NativeExperimentEvaluationRequestV1,
    #[schema(min_items = 1, max_items = 16)]
    pub feature_artifact_ids: Vec<Id>,
    pub instrument_id: String,
    pub consumed_fuel: DbCounter,
    pub source_row_count: DbCounter,
    #[schema(minimum = 1, maximum = 64)]
    pub feature_count: u16,
    #[schema(min_items = 1, max_items = 32)]
    pub folds: Vec<NativeExperimentFoldV1>,
}
