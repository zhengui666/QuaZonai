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
    SealedEvidence,
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
    pub primary_key: Vec<String>,
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
    pub match_type: HistoricalForeignKeyMatchV1,
    pub constraint: String,
    pub source_table: String,
    pub target_table: String,
    pub source_columns: Vec<String>,
    pub target_columns: Vec<String>,
    pub orphan_rows: DbCounter,
}

/// A local artifact selection report, not database-wide coverage or import completion.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalArtifactExportV1 {
    pub schema_version: SchemaV1,
    pub source_installation_id: Id,
    pub exported_at: DateTime<Utc>,
    pub artifacts: Vec<HistoricalArtifactCopyV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalArtifactCopyV1 {
    pub identity: HistoricalIdentityV1,
    pub outcome: HistoricalArtifactOutcomeV1,
    pub object_ref: Option<Id>,
    pub byte_count: Option<DbCounter>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HistoricalArtifactOutcomeV1 {
    Copied,
    Missing,
    Unsupported,
    Unreadable,
    SealedRetained,
    ManualReviewRequired,
}

/// Native CSV projections accompany the retained complete backup. Never an import result.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalRowExportV1 {
    pub schema_version: SchemaV1,
    pub source_installation_id: Id,
    pub inspection: HistoricalSourceInspectionV1,
    pub missing_tables: Vec<String>,
    pub tables: Vec<HistoricalTableExportV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalTableExportV1 {
    pub table: String,
    pub source_rows: DbCounter,
    pub projected_rows: DbCounter,
    pub object_ref: Option<Id>,
    pub byte_count: Option<DbCounter>,
    pub columns: Vec<String>,
    pub excluded_columns: Vec<HistoricalColumnExclusionV1>,
    pub unsupported_schema: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalColumnExclusionV1 {
    pub column: String,
    pub reason: HistoricalExclusionReasonV1,
}

/// Native old primary-key values, including bigint and composite identities.
/// Values use PostgreSQL canonical text, never a new-system UUID parser or float.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalOriginalKeyV1 {
    pub source_installation_id: Id,
    pub source_table: String,
    pub values: std::collections::BTreeMap<String, String>,
}

/// Read-only historical field values. SQL NULL stays distinct from an empty string.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalProjectedRowV1 {
    pub key: HistoricalOriginalKeyV1,
    pub fields: std::collections::BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalImportRequestV1 {
    pub schema_version: SchemaV1,
    pub export_ref: Id,
    pub dry_run: bool,
}

/// Import of reviewed projections is not approval of excluded or legacy evidence.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalImportReportV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub export_ref: Id,
    pub source_installation_id: Id,
    pub dry_run: bool,
    pub projected_rows: DbCounter,
    pub new_rows: DbCounter,
    pub existing_rows: DbCounter,
    pub checked_relationships: DbCounter,
    pub unverified_relationships: Vec<String>,
    pub manual_review_required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HistoricalForeignKeyMatchV1 {
    Simple,
    Full,
}
