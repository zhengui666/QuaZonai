//! Explicit integration configuration and native secret references, never a generic settings blob.
use crate::{runs::RunKind, Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

mod schema;

/// A wire-shape minimum, not an assertion about credential entropy.
pub const RUNTIME_CREDENTIAL_MIN_LENGTH: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationSecretPurpose {
    Runtime,
    Downstream,
    CustomProvider,
    TlsCa,
}
impl IntegrationSecretPurpose {
    pub fn code(self) -> &'static str {
        match self {
            Self::Runtime => "RUNTIME",
            Self::Downstream => "DOWNSTREAM",
            Self::CustomProvider => "CUSTOM_PROVIDER",
            Self::TlsCa => "TLS_CA",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IntegrationSecretIntent {
    pub schema_version: SchemaV1,
    pub purpose: IntegrationSecretPurpose,
    #[schema(min_length = 1, max_length = 120)]
    pub label: String,
}

// Deliberately no Debug: this write-only request contains a secret.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationSecretCreate {
    pub intent: IntegrationSecretIntent,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IntegrationSecretView {
    /// Native immutable SecretVault object identity; never a path or a plaintext read capability.
    pub id: Id,
    pub purpose: IntegrationSecretPurpose,
    pub label: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TlsPolicy {
    SystemCa,
    PinnedCa,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfigurationV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    /// Only an origin. Registration does not send network traffic or attest readiness.
    #[schema(min_length = 1, max_length = 2048)]
    pub endpoint: String,
    pub tls_policy: TlsPolicy,
    #[schema(value_type = std::collections::BTreeSet<RunKind>, min_items = 1, max_items = 8)]
    pub allowed_capabilities: Vec<RunKind>,
    pub enabled: bool,
    /// Both this setting and the deployment must explicitly permit literal-loopback HTTP.
    pub development_http: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCreate {
    pub schema_version: SchemaV1,
    pub configuration: RuntimeConfigurationV1,
    pub credential_ref: Id,
    pub ca_certificate_ref: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeUpdate {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    pub configuration: RuntimeConfigurationV1,
    /// None retains the current immutable credential object; not a request for unauthenticated access.
    pub credential_ref: Option<Id>,
    /// None retains the current CA for PINNED_CA. SYSTEM_CA explicitly clears only the binding.
    pub ca_certificate_ref: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeView {
    pub id: Id,
    pub configuration: RuntimeConfigurationV1,
    pub protocol_version: SchemaV1,
    pub credential_configured: bool,
    pub ca_configured: bool,
    pub last_capability_snapshot_artifact_id: Option<Id>,
    pub revision: Revision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownstreamEnvironments {
    Paper,
    Live,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
pub enum PackageSchemaVersion {
    #[serde(rename = "1")]
    V1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamConfigurationV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    #[schema(min_length = 1, max_length = 2048)]
    pub endpoint: String,
    #[schema(value_type = std::collections::BTreeSet<PackageSchemaVersion>, min_items = 1, max_items = 1)]
    pub accepted_package_versions: Vec<PackageSchemaVersion>,
    pub environments: DownstreamEnvironments,
    pub enabled: bool,
    pub development_http: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamCreate {
    pub schema_version: SchemaV1,
    pub configuration: DownstreamConfigurationV1,
    pub credential_ref: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamUpdate {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    pub configuration: DownstreamConfigurationV1,
    /// None retains the current native credential version.
    pub credential_ref: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamView {
    pub id: Id,
    pub configuration: DownstreamConfigurationV1,
    pub credential_configured: bool,
    pub revision: Revision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
