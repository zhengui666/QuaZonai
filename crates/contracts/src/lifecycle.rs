//! Run transport contracts. These DTOs confer no execution or approval authority.
use crate::{runs::RunState, DbCounter, Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobLimitsV1 {
    pub schema_version: SchemaV1,
    /// Zero only for trusted non-research management jobs.
    #[schema(minimum=0, maximum=4294967295u64, format=Int64)]
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
#[serde(try_from = "RunEventWire")]
pub struct RunEventV1 {
    pub schema_version: SchemaV1,
    pub run_id: Id,
    pub seq: DbCounter,
    pub attempt_id: Option<Id>,
    #[schema(
        min_length = 1,
        max_length = 120,
        pattern = "^[a-z][a-z0-9_.]*(?![\\s\\S])"
    )]
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    /// Public schema-v1 object. Known state event payloads retain strict validation.
    #[schema(schema_with = event_payload_schema)]
    pub payload: serde_json::Value,
}

// Producer enums stay closed; a consumer preserves compatible future envelopes.
// Never turn an unrecognized public event into a state transition.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunEventWire {
    schema_version: SchemaV1,
    run_id: Id,
    seq: DbCounter,
    attempt_id: Option<Id>,
    event_type: String,
    occurred_at: DateTime<Utc>,
    payload: serde_json::Value,
}
impl TryFrom<RunEventWire> for RunEventV1 {
    type Error = &'static str;
    fn try_from(wire: RunEventWire) -> Result<Self, Self::Error> {
        let event = Self {
            schema_version: wire.schema_version,
            run_id: wire.run_id,
            seq: wire.seq,
            attempt_id: wire.attempt_id,
            event_type: wire.event_type,
            occurred_at: wire.occurred_at,
            payload: wire.payload,
        };
        event.validate()?;
        Ok(event)
    }
}
impl RunEventV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        let name = self.event_type.as_bytes();
        if !(1..=120).contains(&name.len())
            || !name[0].is_ascii_lowercase()
            || !name
                .iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'.'))
        {
            return Err("unsupported event name");
        }
        if !self.payload.is_object()
            || self.payload.get("schema_version") != Some(&serde_json::json!(1))
            || serde_json::to_vec(&self.payload)
                .map_err(|_| "invalid event payload")?
                .len()
                > 65_536
        {
            return Err("unsupported event payload");
        }
        if matches!(
            self.event_type.as_str(),
            "run.created" | "run.state_changed"
        ) {
            serde_json::from_value::<RunStatePayload>(self.payload.clone())
                .map_err(|_| "unsupported state event payload")?;
        }
        Ok(())
    }
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

fn event_payload_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::{
        openapi::schema::{AdditionalProperties, ObjectBuilder, Type},
        PartialSchema,
    };
    ObjectBuilder::new().schema_type(Type::Object)
        .property("schema_version",SchemaV1::schema()).required("schema_version")
        .additional_properties(Some(AdditionalProperties::FreeForm(true)))
        .description(Some("Public extensible schema-v1 JSON object; serialized UTF-8 is limited to 65536 bytes. Known event types additionally validate their specific payload contract."))
        .into()
}
