//! Portable historical trace metadata. This format carries no executable authority.
use crate::{DbCounter, Id, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HistoricalKindV1 {
    Research,
    Run,
    Artifact,
    Strategy,
    Evaluation,
    Evidence,
    Approval,
    Handoff,
}

/// Old UUIDs are native UUIDs, not new-system UUIDv7 identities.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalIdentityV1 {
    pub kind: HistoricalKindV1,
    pub source_table: String,
    pub source_id: uuid::Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalRelationV1 {
    pub field: String,
    pub target: HistoricalIdentityV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalRecordV1 {
    pub identity: HistoricalIdentityV1,
    pub label: String,
    pub source_state: String,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub relations: Vec<HistoricalRelationV1>,
    /// Opaque object key in the trusted export, never an old host path or URL.
    pub object_ref: Option<Id>,
    pub object_bytes: Option<DbCounter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalExclusionV1 {
    pub source_table: String,
    pub row_count: DbCounter,
    pub reason: HistoricalExclusionReasonV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HistoricalExclusionReasonV1 {
    Credentials,
    InternalChat,
    UnreviewedFields,
    UnsupportedSchema,
}

/// Trace manifest accompanies the original preserved export; it is not a full backup.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalManifestV1 {
    pub schema_version: SchemaV1,
    pub source_installation_id: Id,
    pub source_schema_version: String,
    pub exported_at: DateTime<Utc>,
    pub records: Vec<HistoricalRecordV1>,
    pub exclusions: Vec<HistoricalExclusionV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalIssueV1 {
    pub identity: HistoricalIdentityV1,
    pub field: String,
    pub code: HistoricalIssueCodeV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HistoricalIssueCodeV1 {
    InvalidIdentity,
    DuplicateIdentity,
    InvalidField,
    UnsupportedTable,
    InvalidTime,
    MissingRelation,
    DuplicateRelation,
    InvalidRelation,
    RequiredRelation,
    InvalidArtifact,
}

/// Native database inspection only; no claim that artifacts or import mapping are complete.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalSourceInspectionV1 {
    pub schema_version: SchemaV1,
    pub source_schema_version: String,
    pub inspected_at: DateTime<Utc>,
    pub tables: Vec<HistoricalTableCountV1>,
    pub foreign_keys: Vec<HistoricalForeignKeyCheckV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalTableCountV1 {
    pub table: String,
    pub rows: DbCounter,
    pub columns: Vec<HistoricalColumnV1>,
}

/// Native type includes numeric and timestamp precision; defaults and row values are omitted.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalColumnV1 {
    pub name: String,
    pub postgres_type: String,
    pub nullable: bool,
    pub identity: bool,
    pub generated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalForeignKeyCheckV1 {
    pub constraint: String,
    pub source_table: String,
    pub target_table: String,
    pub source_columns: Vec<String>,
    pub target_columns: Vec<String>,
    pub orphan_rows: DbCounter,
}
