//! Run transport contracts. These DTOs confer no execution or approval authority.
use crate::{runs::RunState, DbCounter, Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobLimitsV1 {
    pub schema_version: SchemaV1,
    #[schema(minimum=1, maximum=4294967295u64, format=Int64)]
    pub experiments: u32,
    pub cpu_seconds: DbCounter,
    #[schema(minimum=1, maximum=4294967295u64, format=Int64)]
    pub wall_seconds: u32,
    #[schema(minimum=1, maximum=4294967295u64, format=Int64)]
    pub memory_mib: u32,
    pub output_bytes: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunCancelV1 {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
}
fn default_limit() -> u16 {
    50
}
#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunListQuery {
    pub project_id: Option<Id>,
    pub state: Option<RunState>,
    pub cursor: Option<Id>,
    #[serde(default = "default_limit")]
    #[schema(default = 50, minimum = 1, maximum = 100)]
    pub limit: u16,
}
impl Default for RunListQuery {
    fn default() -> Self {
        Self {
            project_id: None,
            state: None,
            cursor: None,
            limit: 50,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunReason {
    Admitted,
    DispatchReserved,
    RuntimeRunning,
    LeaseTakenOver,
    CancelRequested,
    CancelledBeforeDispatch,
    RuntimeSucceeded,
    RuntimeFailed,
    RuntimeCancelled,
    ResultDiscardedAfterCancel,
    DeadlineExceeded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum RunEventKind {
    #[serde(rename = "run.created")]
    Created,
    #[serde(rename = "run.state_changed")]
    StateChanged,
}
impl RunEventKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Created => "run.created",
            Self::StateChanged => "run.state_changed",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunStatePayload {
    pub schema_version: SchemaV1,
    pub state: RunState,
    pub reason: RunReason,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunEventV1 {
    pub schema_version: SchemaV1,
    pub run_id: Id,
    pub seq: DbCounter,
    pub attempt_id: Option<Id>,
    pub event_type: RunEventKind,
    pub occurred_at: DateTime<Utc>,
    pub payload: RunStatePayload,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunEventBatchV1 {
    pub schema_version: SchemaV1,
    pub run_id: Id,
    pub events: Vec<RunEventV1>,
    pub last_event_seq: DbCounter,
    pub state: RunState,
}
