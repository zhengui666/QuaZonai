//! Operator-authored research contract. Saving a draft is not proof of native readiness.
use crate::{
    budget::{BudgetV1, StopRuleV1},
    research::DataPartition,
    DbCounter, Id, Revision, SchemaV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TargetKind {
    Score,
    ExpectedReturn,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HorizonKind {
    FixedBars,
    FixedDuration,
    VariableInterval,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataAccess {
    MetadataOnly,
    ResearchRead,
    EvaluatorOnly,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BriefState {
    Draft,
    Frozen,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefContentV1 {
    #[schema(min_length = 1, max_length = 8000)]
    pub hypothesis: String,
    #[schema(min_length = 1, max_length = 8000)]
    pub economic_rationale: String,
    pub universe_version_id: Id,
    pub target_kind: TargetKind,
    pub horizon_kind: HorizonKind,
    pub horizon_value: Option<DbCounter>,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    pub base_currency: String,
    pub benchmark_ref: Option<Id>,
    pub evaluation_policy_id: Id,
    pub execution_assumptions_id: Id,
    pub budget: BudgetV1,
    pub stop_rule: StopRuleV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefBindingV1 {
    pub dataset_revision_id: Id,
    pub role: DataPartition,
    pub access_policy: DataAccess,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefCreate {
    pub schema_version: SchemaV1,
    pub content: BriefContentV1,
    #[schema(min_items = 1, max_items = 64)]
    pub bindings: Vec<BriefBindingV1>,
    pub supersedes_id: Option<Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefCreateIntent {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub request: BriefCreate,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefUpdate {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    pub content: BriefContentV1,
    #[schema(min_items = 1, max_items = 64)]
    pub bindings: Vec<BriefBindingV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefView {
    pub id: Id,
    pub project_id: Id,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub version: u32,
    pub revision: Revision,
    pub state: BriefState,
    pub content: BriefContentV1,
    #[schema(min_items = 1, max_items = 64)]
    pub bindings: Vec<BriefBindingV1>,
    pub supersedes_id: Option<Id>,
    pub frozen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
