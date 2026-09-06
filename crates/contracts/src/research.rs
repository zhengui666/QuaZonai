//! Immutable research preparation. Registration is not native capability or PASS evidence.
use crate::{evidence::MetricRequirementV1, DbCounter, DecimalValue, Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldIssue {
    pub field: String,
    pub code: String,
    pub message: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InputPurpose {
    Discovery,
    Validation,
    Sealed,
    Portfolio,
    Forward,
}
impl InputPurpose {
    pub fn code(self) -> &'static str {
        match self {
            Self::Discovery => "DISCOVERY",
            Self::Validation => "VALIDATION",
            Self::Sealed => "SEALED",
            Self::Portfolio => "PORTFOLIO",
            Self::Forward => "FORWARD",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataUse {
    Research,
    ResearchAndPaper,
    ResearchPaperLive,
}
impl DataUse {
    pub fn code(self) -> &'static str {
        match self {
            Self::Research => "RESEARCH",
            Self::ResearchAndPaper => "RESEARCH_AND_PAPER",
            Self::ResearchPaperLive => "RESEARCH_PAPER_LIVE",
        }
    }
    pub fn permits_preparation(self, purpose: InputPurpose) -> bool {
        !matches!(purpose, InputPurpose::Portfolio | InputPurpose::Forward)
            || matches!(self, Self::ResearchAndPaper | Self::ResearchPaperLive)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataPartition {
    Discovery,
    Validation,
    Sealed,
    Forward,
}
impl DataPartition {
    pub fn code(self) -> &'static str {
        match self {
            Self::Discovery => "DISCOVERY",
            Self::Validation => "VALIDATION",
            Self::Sealed => "SEALED",
            Self::Forward => "FORWARD",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtifactInputRole {
    Code,
    Parameters,
    Signals,
    Targets,
    Model,
    Report,
    Metrics,
    DataQuality,
}
impl ArtifactInputRole {
    pub fn code(self) -> &'static str {
        match self {
            Self::Code => "CODE",
            Self::Parameters => "PARAMETERS",
            Self::Signals => "SIGNALS",
            Self::Targets => "TARGETS",
            Self::Model => "MODEL",
            Self::Report => "REPORT",
            Self::Metrics => "METRICS",
            Self::DataQuality => "DATA_QUALITY",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum InputItemV1 {
    Dataset {
        dataset_revision_id: Id,
        role: DataPartition,
    },
    Artifact {
        artifact_id: Id,
        role: ArtifactInputRole,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataOrigin {
    Real,
    Synthetic,
    Fixture,
    LegacyUnknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PitStatus {
    Verified,
    Unverified,
    Invalid,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InputSetCreate {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub purpose: InputPurpose,
    pub decision_cutoff: DateTime<Utc>,
    #[schema(min_items = 1, max_items = 256)]
    pub items: Vec<InputItemV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InputSetSummary {
    pub id: Id,
    pub project_id: Id,
    pub purpose: InputPurpose,
    pub decision_cutoff: DateTime<Utc>,
    pub frozen_at: DateTime<Utc>,
    pub revision: Revision,
    pub created_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InputItemView {
    pub id: Id,
    #[schema(minimum = 0, maximum = 255)]
    pub ordinal: u16,
    pub item: InputItemV1,
    pub origin: DataOrigin,
    pub pit_status: Option<PitStatus>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InputSetView {
    pub header: InputSetSummary,
    #[schema(min_items = 1, max_items = 256)]
    pub items: Vec<InputItemView>,
}
fn limit() -> u16 {
    50
}
#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResearchListQuery {
    pub project_id: Id,
    pub cursor: Option<Id>,
    #[serde(default = "limit")]
    #[schema(default = 50, minimum = 1, maximum = 100)]
    pub limit: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SplitKind {
    WalkForward,
    CpcvFixedHorizon,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SplitPolicyV1 {
    pub schema_version: SchemaV1,
    pub kind: SplitKind,
    pub train_size: DbCounter,
    pub test_size: DbCounter,
    pub step_size: Option<DbCounter>,
    #[schema(minimum = 2, maximum = 65535)]
    pub group_count: Option<u16>,
    #[schema(minimum = 1, maximum = 65534)]
    pub test_group_count: Option<u16>,
    pub purge_observations: DbCounter,
    pub embargo_observations: DbCounter,
    pub label_horizon_observations: Option<DbCounter>,
    #[schema(schema_with = required_interval_schema)]
    pub interval_validation_required: bool,
    pub sealed_revision_id: Id,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SelectionEvaluationKind {
    WalkForward,
    Sealed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SelectionDirection {
    Maximize,
    Minimize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComparableScope {
    FamilyLineage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SelectionTieBreak {
    ExperimentIdAsc,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MissingSelectionMetric {
    Inconclusive,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionParametersV1 {
    pub evaluation_kind: SelectionEvaluationKind,
    #[schema(min_length = 1, max_length = 120)]
    pub metric_code: String,
    #[schema(min_length = 1, max_length = 120)]
    pub metric_scope: String,
    #[schema(min_length = 1, max_length = 120)]
    pub method_id: String,
    #[schema(min_length = 1, max_length = 120)]
    pub method_version: String,
    #[schema(min_length = 1, max_length = 120)]
    pub unit: String,
    #[schema(min_length = 1, max_length = 120)]
    pub frequency: String,
    pub direction: SelectionDirection,
    #[schema(minimum = 1, maximum = 65535)]
    pub candidate_count: u16,
}
// Keep the frozen wire shape flat as specified in DESIGN; serde flatten plus
// deny_unknown_fields would weaken the input boundary. The public create DTO
// carries only SelectionParametersV1, never these trusted identity fields.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionRuleV1 {
    pub schema_version: SchemaV1,
    pub comparable_scope: ComparableScope,
    pub root_lineage_id: Id,
    pub family_id: Id,
    pub comparison_input_set_id: Id,
    pub execution_assumptions_id: Id,
    pub evaluation_kind: SelectionEvaluationKind,
    #[schema(min_length = 1, max_length = 120)]
    pub metric_code: String,
    #[schema(min_length = 1, max_length = 120)]
    pub metric_scope: String,
    #[schema(min_length = 1, max_length = 120)]
    pub method_id: String,
    #[schema(min_length = 1, max_length = 120)]
    pub method_version: String,
    #[schema(min_length = 1, max_length = 120)]
    pub unit: String,
    #[schema(min_length = 1, max_length = 120)]
    pub frequency: String,
    pub direction: SelectionDirection,
    #[schema(minimum = 1, maximum = 65535)]
    pub candidate_count: u16,
    pub tie_break: SelectionTieBreak,
    pub missing_required_metric: MissingSelectionMetric,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluationPolicyCreate {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    #[schema(min_length = 1, max_length = 8000)]
    pub question: String,
    pub comparison_input_set_id: Id,
    pub execution_assumptions_id: Id,
    pub selection: SelectionParametersV1,
    pub split_policy: SplitPolicyV1,
    #[schema(min_items = 1, max_items = 64)]
    pub metric_requirements: Vec<MetricRequirementV1>,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub minimum_observations: u32,
    pub maximum_missing_fraction: DecimalValue,
    pub require_real_data: bool,
    #[schema(max_items = 64)]
    pub required_capabilities: Vec<String>,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub maximum_sealed_uses_per_lineage: u32,
    pub validity_seconds: DbCounter,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluationPolicyView {
    pub id: Id,
    pub project_id: Id,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub version: u32,
    pub created_at: DateTime<Utc>,
    #[schema(min_length = 1, max_length = 8000)]
    pub question: String,
    pub selection_rule: SelectionRuleV1,
    pub split_policy: SplitPolicyV1,
    #[schema(min_items = 1, max_items = 64)]
    pub metric_requirements: Vec<MetricRequirementV1>,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub minimum_observations: u32,
    pub maximum_missing_fraction: DecimalValue,
    pub require_real_data: bool,
    #[schema(max_items = 64)]
    pub required_capabilities: Vec<String>,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub maximum_sealed_uses_per_lineage: u32,
    pub validity_seconds: DbCounter,
}

fn required_interval_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::Boolean)
        .enum_values(Some([true]))
        .into()
}
