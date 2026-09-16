use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{DbCounter, Id, Revision, SchemaV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectState {
    Draft,
    Active,
    Paused,
    Archived,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunState {
    Queued,
    Dispatching,
    Running,
    Reconciling,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunKind {
    AgentResearch,
    DataValidate,
    AlphaEvaluate,
    PortfolioBuild,
    PortfolioSimulate,
    ForwardEvaluate,
    Export,
    Import,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunSnapshotV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub project_id: Id,
    pub cycle_id: Option<Id>,
    pub kind: RunKind,
    pub input_set_id: Id,
    pub state: RunState,
    #[schema(format = Int64, maximum = 4294967295u64)]
    pub current_attempt_no: u32,
    pub active_attempt_id: Option<Id>,
    pub last_event_seq: DbCounter,
    pub deadline_at: DateTime<Utc>,
    pub cancellation_requested_at: Option<DateTime<Utc>>,
    pub terminal_reason_code: Option<String>,
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub revision: Revision,
}

/// Historical automatic rebalance facts, never current policy authority.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunRebalanceViewV1 {
    pub schema_version: SchemaV1,
    pub rebalance: Option<RunRebalanceV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunRebalanceV1 {
    pub build_run_id: Id,
    pub policy_id: Id,
    pub downstream_id: Id,
    pub source_candidate_id: Id,
    pub decision_cutoff: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub request: crate::portfolio::PortfolioBuildRequestV1,
    pub study_run_id: Option<Id>,
    pub release_id: Option<Id>,
}
