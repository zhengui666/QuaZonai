//! Shared HTTP error wire contract used by the server, native CLI and generated clients.
use crate::{Id, Revision};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    #[schema(minimum = 100, maximum = 599)]
    pub status: u16,
    pub code: String,
    pub detail: String,
    pub request_id: Id,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<Revision>,
    pub field_errors: Vec<FieldError>,
    pub safe_next_actions: Vec<String>,
}
