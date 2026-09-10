//! Human research startup and durable cycle views. None of these requests can
//! inject a worker result, qualification, expanded budget, or a data permission.
use crate::{
    brief::BriefView, budget::BudgetV1, runs::RunSnapshotV1, DbCounter, Id, Revision, SchemaV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefExecutionContextV1 {
    pub schema_version: SchemaV1,
    pub runtime_id: Id,
    pub runtime_revision: Revision,
    pub discovery_input_set_id: Id,
    pub validation_input_set_id: Id,
    pub sealed_input_set_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefFreezeV1 {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    pub execution_context: BriefExecutionContextV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenBriefV1 {
    pub schema_version: SchemaV1,
    pub brief: BriefView,
    pub execution_context: BriefExecutionContextV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CycleStartV1 {
    pub schema_version: SchemaV1,
    pub brief_id: Id,
    /// The Project revision; the Brief and execution context are immutable.
    pub expected_revision: Revision,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CycleStartIntent {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub request: CycleStartV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CycleState {
    Queued,
    Running,
    WaitingInput,
    Pausing,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CycleTrigger {
    Operator,
    Schedule,
    Degradation,
    NewData,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CycleOutcome {
    QualifiedCandidates,
    NoSupportedCandidate,
    BudgetExhausted,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CycleReadAction {
    ViewBrief,
    ViewRuns,
    ViewExperiments,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CycleViewV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub project_id: Id,
    pub brief_id: Id,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub ordinal: u32,
    pub revision: Revision,
    pub trigger: CycleTrigger,
    pub state: CycleState,
    pub outcome: Option<CycleOutcome>,
    pub budget: BudgetV1,
    #[schema(format = Int64, minimum = 0, maximum = 4294967295u64)]
    pub reserved_experiments: u32,
    #[schema(format = Int64, minimum = 0, maximum = 4294967295u64)]
    pub used_experiments: u32,
    pub reserved_cpu_seconds: DbCounter,
    pub initial_run_id: Option<Id>,
    pub next_action: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub available_actions: Vec<CycleReadAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CycleStartedV1 {
    pub schema_version: SchemaV1,
    pub cycle: CycleViewV1,
    pub run: RunSnapshotV1,
}
