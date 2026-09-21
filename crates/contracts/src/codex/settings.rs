//! Public, nonsecret application profile and observation contracts. Native OAuth
//! tokens and App Server history are deliberately absent from these projections.
use super::{ConnectionMode, ModelCapabilityV1, ProfileOrigin, SavedModelSettingsV1};
use crate::{Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "mode", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum CodexConnectionCreateV1 {
    System {},
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexProfileCreateV1 {
    pub schema_version: SchemaV1,
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    #[schema(
        min_length = 1,
        max_length = 64,
        pattern = "^[A-Za-z0-9][A-Za-z0-9_.-]*(?![\\s\\S])"
    )]
    pub home_binding: String,
    pub profile_origin: ProfileOrigin,
    pub connection: CodexConnectionCreateV1,
    pub model_settings: SavedModelSettingsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexProfileUpdateV1 {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    pub model_settings: SavedModelSettingsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexProfileViewV1 {
    pub id: Id,
    pub name: String,
    /// An opaque deployment label, never a path. None identifies a historical
    /// unregistered reference, which must not be exposed or guessed into a mount.
    pub home_binding: Option<String>,
    pub profile_origin: ProfileOrigin,
    pub connection_mode: ConnectionMode,
    pub model_settings: SavedModelSettingsV1,
    pub revision: Revision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexHomeBindingV1 {
    pub reference: String,
    pub label: String,
    pub profile_origin: ProfileOrigin,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexProbeRequestV1 {
    pub schema_version: SchemaV1,
    pub profile_id: Id,
    pub expected_revision: Revision,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexProfileQueryV1 {
    pub profile_id: Id,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodexAuthenticationKind {
    ApiKey,
    Chatgpt,
    AmazonBedrock,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexAccountV1 {
    pub requires_openai_auth: bool,
    pub authentication_kind: Option<CodexAuthenticationKind>,
    pub plan_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexEffectiveSettingsV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub model: String,
    #[schema(min_length = 1, max_length = 200)]
    pub provider: String,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexServiceTierV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub id: String,
    #[schema(min_length = 1, max_length = 200)]
    pub name: String,
    #[schema(max_length = 8192)]
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexAdvertisedModelV1 {
    pub capability: ModelCapabilityV1,
    #[schema(max_items = 64)]
    pub service_tiers: Vec<CodexServiceTierV1>,
    pub default_service_tier: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodexProbeFailureV1 {
    DeploymentUnavailable,
    NativeUnavailable,
    VersionUnsupported,
    ContractUnsupported,
    AuthenticationRequired,
    ModelSettingsUnsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "status",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum CodexProbeOutcomeV1 {
    Available {
        native_version: String,
        account: CodexAccountV1,
        effective: CodexEffectiveSettingsV1,
        /// Model from the override-free native Thread. Absent only in historical
        /// observations; never infer it from the post-override effective model.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(min_length = 1, max_length = 200)]
        native_default_model: Option<String>,
        #[schema(min_items = 1, max_items = 4096)]
        models: Vec<CodexAdvertisedModelV1>,
    },
    Unavailable {
        reason: CodexProbeFailureV1,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexProbeViewV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub profile_id: Id,
    pub profile_revision: Revision,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub outcome: CodexProbeOutcomeV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodexObservationStateV1 {
    NeverProbed,
    Stale,
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexObservationV1 {
    pub schema_version: SchemaV1,
    pub profile_id: Id,
    pub profile_revision: Revision,
    pub state: CodexObservationStateV1,
    pub observation: Option<CodexProbeViewV1>,
}
