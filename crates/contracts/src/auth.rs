//! Nonsecret metadata for an automatically established local browser session.
use crate::SchemaV1;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserSession {
    pub schema_version: SchemaV1,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
