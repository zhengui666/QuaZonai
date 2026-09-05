//! Application settings, not a handwritten replacement for Codex's native schema.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{Revision, SchemaV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectionMode {
    System,
    CustomProvider,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileOrigin {
    ManagedVolume,
    OperatorMount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SavedModelSettingsV1 {
    pub schema_version: SchemaV1,
    pub use_default_model_settings: bool,
    pub saved_model: Option<String>,
    pub saved_reasoning_effort: Option<String>,
    pub saved_fast_mode: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReasoningEffortCapability {
    pub reasoning_effort: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelCapabilityV1 {
    pub schema_version: SchemaV1,
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub hidden: bool,
    pub default_reasoning_effort: String,
    pub supported_reasoning_efforts: Vec<ReasoningEffortCapability>,
    pub is_default: bool,
    pub fetched_at: DateTime<Utc>,
    pub profile_revision: Revision,
}
