//! Application references to human-initiated native account operations. Native
//! OAuth tokens and canonical auth files never enter this public contract.
use super::CodexAccountV1;
use crate::{control::CommandResult, Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodexAccountActionV1 {
    Login,
    Logout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodexAccountOperationStateV1 {
    Requested,
    Waiting,
    CancelRequested,
    Succeeded,
    Cancelled,
    Failed,
    Unknown,
}
impl CodexAccountOperationStateV1 {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Cancelled | Self::Failed | Self::Unknown
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodexAccountReasonV1 {
    NativeLoginCompleted,
    NativeLoginRejected,
    NativeLogoutCompleted,
    NativeCancelConfirmed,
    ConfirmedNotSent,
    NativeResponseUnknown,
    NativeContractUnsupported,
    NativeVersionUnsupported,
    DeploymentUnavailable,
    ProfileChanged,
    WaitWindowEnded,
    OwnerUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexAccountRequestV1 {
    pub schema_version: SchemaV1,
    pub profile_id: Id,
    pub expected_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexLoginCancelV1 {
    pub schema_version: SchemaV1,
    pub operation_id: Id,
    pub expected_revision: Revision,
}

/// Immutable acceptance reference; a receipt is never rewritten to track progress.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexAccountOperationRefV1 {
    pub id: Id,
    pub profile_id: Id,
    pub profile_revision: Revision,
    pub action: CodexAccountActionV1,
    pub created_at: DateTime<Utc>,
    /// Local bounded wait deadline, not an assertion about the native device code.
    pub deadline_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexAccountOperationV1 {
    pub schema_version: SchemaV1,
    pub operation: CodexAccountOperationRefV1,
    pub state: CodexAccountOperationStateV1,
    pub revision: Revision,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub reason: Option<CodexAccountReasonV1>,
    /// Present only after a corresponding actual native account/read observation.
    pub account: Option<CodexAccountV1>,
}

// Deliberately no Debug: the one-time native code is only for the initiating UI.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexDeviceCodeV1 {
    #[schema(min_length = 1, max_length = 2048)]
    pub verification_url: String,
    #[schema(min_length = 1, max_length = 200)]
    pub user_code: String,
}

// The code is never written into the immutable acceptance receipt or a GET response.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodexAccountStartV1 {
    pub schema_version: SchemaV1,
    pub acceptance: CommandResult<CodexAccountOperationRefV1>,
    pub current: CodexAccountOperationV1,
    pub device_code: Option<CodexDeviceCodeV1>,
}
