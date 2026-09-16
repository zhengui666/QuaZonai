//! Browser authentication DTOs. Secret-bearing inputs deliberately do not derive
//! Debug; no API ever reads back a stored key, verifier or provider credential.
use crate::{Id, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BootstrapStart {
    pub schema_version: SchemaV1,
    pub capability_id: Id,
    #[schema(min_length = 43, max_length = 43)]
    pub capability: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BootstrapEnrollment {
    pub schema_version: SchemaV1,
    pub enrollment_id: Id,
    pub expires_at: DateTime<Utc>,
    pub provisioning_uri: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BootstrapConfirm {
    pub schema_version: SchemaV1,
    pub enrollment_id: Id,
    #[schema(min_length = 6, max_length = 6, pattern = "^[0-9]{6}$")]
    pub code: String,
    pub trust_device: bool,
    #[schema(max_length = 120)]
    pub device_label: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub schema_version: SchemaV1,
    #[schema(min_length = 6, max_length = 6, pattern = "^[0-9]{6}$")]
    pub code: String,
    pub trust_device: bool,
    #[schema(max_length = 120)]
    pub device_label: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    pub schema_version: SchemaV1,
    #[schema(min_length = 6, max_length = 6, pattern = "^[0-9]{6}$")]
    pub code: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BootstrapStatus {
    pub schema_version: SchemaV1,
    pub initialized: bool,
    pub setup_allowed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserSession {
    pub schema_version: SchemaV1,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub trusted_device_id: Option<Id>,
    pub recent_authentication_required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TrustedDevice {
    pub id: Id,
    pub label: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceList {
    pub schema_version: SchemaV1,
    pub items: Vec<TrustedDevice>,
    pub next_cursor: Option<Id>,
}

#[derive(Clone, Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceCursor {
    pub cursor: Option<Id>,
}
