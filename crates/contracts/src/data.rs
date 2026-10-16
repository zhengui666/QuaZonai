//! Recorded native feature provenance and calculation limit schemas.
use crate::{
    catalogs::DataRevisionPolicy,
    research::{DataOrigin, PitStatus},
    DbCounter, Id, SchemaV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataLicenseState {
    Active,
    NotYetValid,
    Expired,
    Revoked,
}

/// Register an existing recorded attachment using its exact UTF-8 bytes. The
/// frozen Dataset metadata is authoritative; no client origin/PIT/license claims.
/// Deliberately no Debug, as with ArtifactCreate, to avoid logging the raw content.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeatureRegisterV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    /// Must equal the Dataset ID in the route and normalized operator intent.
    pub dataset_revision_id: Id,
    #[schema(
        min_length = 1,
        max_length = 120,
        pattern = r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}(?![\s\S])"
    )]
    pub feature_part_key: String,
    /// Original UTF-8 bytes; whitespace and final newline are retained.
    /// This is a string, never a parsed JSON Value to be reserialized for storage.
    #[schema(min_length = 1)]
    pub content: String,
}

impl RecordedFeatureRegisterV1 {
    /// Bind the project, route identity, part and original UTF-8 size.
    /// Store separately compares original bytes when replaying a registration.
    pub fn intent(&self) -> Result<RecordedFeatureRegisterIntentV1, String> {
        Ok(RecordedFeatureRegisterIntentV1 {
            schema_version: self.schema_version,
            project_id: self.project_id,
            dataset_revision_id: self.dataset_revision_id,
            feature_part_key: self.feature_part_key.clone(),
            byte_count: DbCounter::new(self.content.len() as u64)?,
        })
    }
}

/// Small nonsecret approval/receipt input derived from the original upload.
/// A matching intent does not replace Store's original-byte replay comparison.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeatureRegisterIntentV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub dataset_revision_id: Id,
    #[schema(
        min_length = 1,
        max_length = 120,
        pattern = r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}(?![\s\S])"
    )]
    pub feature_part_key: String,
    #[schema(schema_with = crate::catalogs::recorded_feature_bytes_schema)]
    pub byte_count: DbCounter,
}

/// Immutable source identity resolved from the registered feature relationship.
/// It is never supplied by a registration client and never extends grant scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeatureSourceBindingV1 {
    pub dataset_revision_id: Id,
    pub source_id: Id,
    pub data_use_grant_id: Id,
    pub native_metadata_artifact_id: Id,
    #[schema(
        min_length = 1,
        max_length = 120,
        pattern = r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}(?![\s\S])"
    )]
    pub feature_part_key: String,
    pub origin: DataOrigin,
    pub pit_status: PitStatus,
    pub revision_policy: DataRevisionPolicy,
}
