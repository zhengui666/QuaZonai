//! Instance password, browser sessions and revocable owner CLI devices.
use crate::{Id, SchemaV1};
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

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthStatus {
    pub schema_version: SchemaV1,
    pub setup_required: bool,
}

// Credential-bearing inputs deliberately do not implement Debug.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordLogin {
    pub schema_version: SchemaV1,
    #[schema(min_length = 8, max_length = 1024)]
    pub password: String,
    pub remember_device: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordChange {
    pub schema_version: SchemaV1,
    pub current_password: String,
    #[schema(min_length = 8, max_length = 1024)]
    pub new_password: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CliLogin {
    pub schema_version: SchemaV1,
    pub password: String,
    #[schema(min_length = 1, max_length = 100)]
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CliDevice {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CliLoginResult {
    pub device: CliDevice,
    pub token: String,
}
