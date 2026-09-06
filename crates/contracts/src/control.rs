//! Strict control-plane DTOs; machine identity and human authority stay separate.
use crate::{runs::ProjectState, Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Page<T> {
    pub schema_version: SchemaV1,
    pub items: Vec<T>,
    pub next_cursor: Option<Id>,
}
fn default_limit() -> u16 {
    50
}
#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub cursor: Option<Id>,
    #[serde(default = "default_limit")]
    #[schema(default = 50, minimum = 1, maximum = 100)]
    pub limit: u16,
}
impl Default for ListQuery {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 50,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectCreate {
    pub schema_version: SchemaV1,
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    #[schema(max_length = 8000)]
    pub description: String,
    pub fork_from_project_id: Option<Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectUpdate {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    #[schema(max_length = 8000)]
    pub description: String,
    pub state: ProjectState,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectView {
    pub id: Id,
    pub root_lineage_id: Id,
    pub name: String,
    pub description: String,
    pub state: ProjectState,
    pub current_brief_id: Option<Id>,
    pub current_automation_policy_id: Option<Id>,
    pub created_by: ProjectOrigin,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: Revision,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectOrigin {
    Operator,
    Import,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandResult<T> {
    pub schema_version: SchemaV1,
    pub replayed: bool,
    pub resource: T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrincipalKind {
    Cli,
    Downstream,
    Automation,
    Mission,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssignablePrincipalKind {
    Cli,
    Downstream,
    Automation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MachineScope {
    ResearchRead,
    ExperimentSubmit,
    ArtifactSubmit,
    EvidenceRead,
    RunRead,
    RunCancel,
    DownstreamClaim,
    DownstreamAck,
    ForwardSubmit,
    DoctorRead,
}
impl MachineScope {
    pub fn code(self) -> &'static str {
        match self {
            Self::ResearchRead => "RESEARCH_READ",
            Self::ExperimentSubmit => "EXPERIMENT_SUBMIT",
            Self::ArtifactSubmit => "ARTIFACT_SUBMIT",
            Self::EvidenceRead => "EVIDENCE_READ",
            Self::RunRead => "RUN_READ",
            Self::RunCancel => "RUN_CANCEL",
            Self::DownstreamClaim => "DOWNSTREAM_CLAIM",
            Self::DownstreamAck => "DOWNSTREAM_ACK",
            Self::ForwardSubmit => "FORWARD_SUBMIT",
            Self::DoctorRead => "DOCTOR_READ",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PrincipalCreate {
    pub schema_version: SchemaV1,
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    pub kind: AssignablePrincipalKind,
    pub project_id: Option<Id>,
    pub downstream_id: Option<Id>,
    pub enabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PrincipalUpdate {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    pub enabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PrincipalView {
    pub id: Id,
    pub name: String,
    pub kind: PrincipalKind,
    pub project_id: Option<Id>,
    pub downstream_id: Option<Id>,
    pub run_id: Option<Id>,
    pub enabled: bool,
    pub credential_epoch: Revision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: Revision,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialIssue {
    pub schema_version: SchemaV1,
    #[schema(min_items = 1, max_items = 10)]
    pub scope_codes: Vec<MachineScope>,
    pub expires_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialIssueIntent {
    pub schema_version: SchemaV1,
    pub principal_id: Id,
    pub request: CredentialIssue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialRevoke {
    pub schema_version: SchemaV1,
    #[schema(min_length = 1, max_length = 2000)]
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialView {
    pub id: Id,
    pub principal_id: Id,
    pub public_token_id: Id,
    pub principal_epoch: Revision,
    pub scope_codes: Vec<MachineScope>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}
// Secret-bearing response intentionally has no Debug implementation.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialCreated {
    pub schema_version: SchemaV1,
    pub replayed: bool,
    pub resource: CredentialView,
    /// Only the initial response contains the token; it is never recoverable.
    pub token: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperatorOperation {
    BriefCreate,
    BriefUpdate,
    ProjectCreate,
    ProjectUpdate,
    PrincipalCreate,
    PrincipalUpdate,
    CredentialIssue,
    CredentialRevoke,
    InputSetCreate,
    EvaluationPolicyCreate,
}
impl OperatorOperation {
    pub fn code(self) -> &'static str {
        match self {
            Self::BriefCreate => "BRIEF_CREATE",
            Self::BriefUpdate => "BRIEF_UPDATE",
            Self::ProjectCreate => "PROJECT_CREATE",
            Self::ProjectUpdate => "PROJECT_UPDATE",
            Self::PrincipalCreate => "PRINCIPAL_CREATE",
            Self::PrincipalUpdate => "PRINCIPAL_UPDATE",
            Self::CredentialIssue => "CREDENTIAL_ISSUE",
            Self::CredentialRevoke => "CREDENTIAL_REVOKE",
            Self::InputSetCreate => "INPUT_SET_CREATE",
            Self::EvaluationPolicyCreate => "EVALUATION_POLICY_CREATE",
        }
    }
    pub fn creates(self) -> bool {
        matches!(
            self,
            Self::BriefCreate
                | Self::ProjectCreate
                | Self::PrincipalCreate
                | Self::CredentialIssue
                | Self::InputSetCreate
                | Self::EvaluationPolicyCreate
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "operation",
    content = "request",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum OperatorCommand {
    BriefCreate(Box<crate::brief::BriefCreateIntent>),
    BriefUpdate(Box<crate::brief::BriefUpdate>),
    ProjectCreate(ProjectCreate),
    ProjectUpdate(ProjectUpdate),
    PrincipalCreate(PrincipalCreate),
    PrincipalUpdate(PrincipalUpdate),
    CredentialIssue(CredentialIssueIntent),
    CredentialRevoke(CredentialRevoke),
    InputSetCreate(crate::research::InputSetCreate),
    EvaluationPolicyCreate(Box<crate::research::EvaluationPolicyCreate>),
}
impl OperatorCommand {
    pub fn operation(&self) -> OperatorOperation {
        match self {
            Self::BriefCreate(_) => OperatorOperation::BriefCreate,
            Self::BriefUpdate(_) => OperatorOperation::BriefUpdate,
            Self::ProjectCreate(_) => OperatorOperation::ProjectCreate,
            Self::ProjectUpdate(_) => OperatorOperation::ProjectUpdate,
            Self::PrincipalCreate(_) => OperatorOperation::PrincipalCreate,
            Self::PrincipalUpdate(_) => OperatorOperation::PrincipalUpdate,
            Self::CredentialIssue(_) => OperatorOperation::CredentialIssue,
            Self::CredentialRevoke(_) => OperatorOperation::CredentialRevoke,
            Self::InputSetCreate(_) => OperatorOperation::InputSetCreate,
            Self::EvaluationPolicyCreate(_) => OperatorOperation::EvaluationPolicyCreate,
        }
    }
    pub fn normalized_request(&self) -> Result<serde_json::Value, serde_json::Error> {
        match self {
            Self::BriefCreate(v) => serde_json::to_value(v),
            Self::BriefUpdate(v) => serde_json::to_value(v),
            Self::ProjectCreate(v) => serde_json::to_value(v),
            Self::ProjectUpdate(v) => serde_json::to_value(v),
            Self::PrincipalCreate(v) => serde_json::to_value(v),
            Self::PrincipalUpdate(v) => serde_json::to_value(v),
            Self::CredentialIssue(v) => serde_json::to_value(v),
            Self::CredentialRevoke(v) => serde_json::to_value(v),
            Self::InputSetCreate(v) => serde_json::to_value(v),
            Self::EvaluationPolicyCreate(v) => serde_json::to_value(v),
        }
    }
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorGrantRequest {
    pub schema_version: SchemaV1,
    pub command: OperatorCommand,
    pub target_id: Option<Id>,
    #[schema(min_length = 6, max_length = 6, pattern = "^[0-9]{6}$")]
    pub code: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorGrantView {
    pub id: Id,
    pub credential_id: Id,
    pub operation: OperatorOperation,
    pub target_id: Id,
    pub auth_epoch: Revision,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Public identity of this verified machine credential, never a secret lookup.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineSessionView {
    pub schema_version: SchemaV1,
    pub credential_id: Id,
    pub kind: PrincipalKind,
    pub project_id: Option<Id>,
    pub downstream_id: Option<Id>,
    pub run_id: Option<Id>,
    pub scope_codes: Vec<MachineScope>,
    pub expires_at: DateTime<Utc>,
}
