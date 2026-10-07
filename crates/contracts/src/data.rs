//! Operator-managed data sources, explicit licenses and immutable native registrations.
//! Client requests never assign origin, PIT status, row counts or scientific qualification.
use crate::{
    DbCounter, Id, Revision, SchemaV1,
    catalogs::DataRevisionPolicy,
    research::{DataOrigin, DataPartition, DataUse, PitStatus},
    runtime::RuntimeDataKind,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataProviderKind {
    NautilusCatalog,
}
impl DataProviderKind {
    pub fn code(self) -> &'static str {
        "NAUTILUS_CATALOG"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataSourceCreate {
    pub schema_version: SchemaV1,
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    pub runtime_id: Id,
    /// Exact Runtime registry key, not an HTTP URL or local filesystem path.
    #[schema(schema_with = native_catalog_key_schema)]
    pub native_catalog_ref: String,
    pub provider_kind: DataProviderKind,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataSourceUpdate {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataSourceView {
    pub id: Id,
    pub name: String,
    pub runtime_id: Id,
    pub native_catalog_ref: String,
    /// Historical native provider names remain visible, not silently reclassified.
    pub provider_kind: String,
    pub enabled: bool,
    pub revision: Revision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataGrantCreate {
    pub schema_version: SchemaV1,
    /// Also bound in the normalized command so a one-time CLI grant cannot be
    /// replayed against a different source merely by changing the route.
    pub source_id: Id,
    #[schema(min_length = 1, max_length = 2000)]
    pub license_reference: String,
    pub evidence_artifact_id: Id,
    pub allowed_uses: DataUse,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataGrantRevoke {
    pub schema_version: SchemaV1,
    /// None takes effect at the authoritative database clock. Explicit times may
    /// only be future-effective; the caller cannot rewrite historical access.
    pub effective_at: Option<DateTime<Utc>>,
    #[schema(min_length = 1, max_length = 120)]
    pub reason_code: String,
    #[schema(min_length = 1, max_length = 2000)]
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataLicenseState {
    Active,
    NotYetValid,
    Expired,
    Revoked,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataGrantView {
    pub id: Id,
    pub source_id: Id,
    pub version: Revision,
    pub license_reference: String,
    pub evidence_artifact_id: Id,
    pub allowed_uses: DataUse,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub license_state: DataLicenseState,
    /// This read-time observation is not a durable readiness or license extension.
    pub checked_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataGrantRevocationView {
    pub id: Id,
    pub grant_id: Id,
    pub effective_at: DateTime<Utc>,
    pub reason_code: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DatasetRegister {
    pub schema_version: SchemaV1,
    pub source_id: Id,
    pub grant_id: Id,
    pub expected_source_revision: Revision,
    pub expected_runtime_revision: Revision,
    #[schema(min_length = 1, max_length = 120)]
    pub native_storage_version: String,
    /// Reuse an immutable Universe only when its full native definition matches.
    /// This permits Discovery/Validation/Sealed revisions to share one frozen universe.
    pub existing_universe_version_id: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DatasetView {
    pub id: Id,
    pub source_id: Id,
    pub data_use_grant_id: Id,
    pub native_snapshot_ref: String,
    pub storage_version: String,
    pub universe_version_id: Id,
    pub schema_version: String,
    pub data_kind: RuntimeDataKind,
    pub partition: DataPartition,
    pub event_start: DateTime<Utc>,
    pub event_end: DateTime<Utc>,
    pub available_through: DateTime<Utc>,
    pub row_count: DbCounter,
    pub timezone: String,
    pub quality_artifact_id: Id,
    pub pit_status: PitStatus,
    pub revision_policy: DataRevisionPolicy,
    pub origin: DataOrigin,
    pub created_at: DateTime<Utc>,
    pub native_metadata_artifact_id: Option<Id>,
    pub registration_observed_at: Option<DateTime<Utc>>,
    pub source_enabled: bool,
    pub runtime_enabled: bool,
    pub license_state: DataLicenseState,
    pub checked_at: DateTime<Utc>,
}

/// Owner-only audit projection of the two immutable registration documents.
/// Reading this summary does not revalidate the native snapshot, attest historical
/// availability, extend its license, or change the registered origin/PIT labels.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DatasetEvidenceViewV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub source_id: Id,
    pub data_use_grant_id: Id,
    pub native_metadata_artifact_id: Id,
    pub quality_artifact_id: Id,
    pub registration_observed_at: DateTime<Utc>,
    pub provider_kind: DataProviderKind,
    pub partition: DataPartition,
    pub origin: DataOrigin,
    /// The persisted label, not a new historical-availability attestation.
    pub pit_status: PitStatus,
    pub revision_policy: DataRevisionPolicy,
    pub source_enabled: bool,
    pub runtime_enabled: bool,
    pub license_state: DataLicenseState,
    /// Read-time license observation only; does not authorize new data use.
    pub checked_at: DateTime<Utc>,
    pub quality: DatasetQualitySummaryV1,
}

/// Original registration-time measurements only. No prices, quantities,
/// instruments, settlements, samples, or raw native documents are returned.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DatasetQualitySummaryV1 {
    pub native_version: String,
    pub checked_at: DateTime<Utc>,
    pub row_count: DbCounter,
    #[schema(minimum = 1, maximum = 256)]
    pub instrument_count: u16,
    pub first_event_ns: DbCounter,
    pub last_event_ns: DbCounter,
    pub available_through_ns: DbCounter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UniverseRegistrationState {
    NativeMetadata,
    LegacyUnverified,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UniverseView {
    pub id: Id,
    pub name: String,
    /// Derived from formal Dataset registration evidence, not from a legacy label.
    /// Native registration does not imply REAL data, verified PIT or qualification.
    pub registration_state: UniverseRegistrationState,
    pub membership_artifact_id: Id,
    pub instrument_definitions_artifact_id: Id,
    pub calendar_artifact_id: Option<Id>,
    pub calendar_ref: String,
    pub calendar_version: String,
    pub selection_asof: DateTime<Utc>,
    pub has_historical_membership: bool,
    pub coverage_start: DateTime<Utc>,
    pub coverage_end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Start fixed native catalog validation for an already frozen project InputSet.
/// No image, arbitrary path, origin, PIT claim or result is supplied by the caller.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataValidateRequest {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub input_set_id: Id,
    pub runtime_id: Id,
    pub expected_runtime_revision: Revision,
    #[schema(schema_with = bounded_native_limits_schema)]
    pub limits: crate::lifecycle::JobLimitsV1,
}

pub(crate) fn bounded_native_limits_schema()
-> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::{
        PartialSchema,
        openapi::schema::{AllOfBuilder, ObjectBuilder, OneOfBuilder, Type},
    };
    AllOfBuilder::new()
        .item(crate::lifecycle::JobLimitsV1::schema())
        .item(
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .property(
                    "experiments",
                    ObjectBuilder::new()
                        .schema_type(Type::Integer)
                        .enum_values(Some([0])),
                )
                .property(
                    "cpu_seconds",
                    crate::scalars::optional_positive_db_counter_schema(),
                )
                .property(
                    "wall_seconds",
                    OneOfBuilder::new()
                        .item(ObjectBuilder::new().schema_type(Type::Null))
                        .item(
                            ObjectBuilder::new()
                                .schema_type(Type::Integer)
                                .minimum(Some(1.0))
                                .maximum(Some(86400.0)),
                        ),
                )
                .property(
                    "memory_mib",
                    ObjectBuilder::new()
                        .schema_type(Type::Integer)
                        .minimum(Some(1.0))
                        .maximum(Some(1048576.0)),
                )
                .property(
                    "output_bytes",
                    OneOfBuilder::new()
                        .item(ObjectBuilder::new().schema_type(Type::Null))
                        .item(crate::scalars::bounded_bigint_schema(
                            crate::runtime_jobs::MAX_JOB_OUTPUT_BYTES,
                            true,
                        )),
                ),
        )
        .into()
}

fn native_catalog_key_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    // Rust trim's Unicode White_Space set, not JavaScript's differing BOM rule.
    // Slash-separated names retain their original spelling; dot/traversal/empty
    // components and control/URL delimiters are not registry keys.
    ObjectBuilder::new()
        .schema_type(Type::String)
        .min_length(Some(1))
        .max_length(Some(512))
        .pattern(Some(r"^(?![ \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000])(?![\s\S]*[ \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000](?![\s\S]))(?!\.{1,2}(?:/|(?![\s\S])))(?![\s\S]*/\.{1,2}(?:/|(?![\s\S])))[^/\\?#\u0000-\u001f\u007f-\u009f]+(?:/[^/\\?#\u0000-\u001f\u007f-\u009f]+)*(?![\s\S])"))
        .into()
}

fn default_limit() -> u16 {
    50
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DataListQuery {
    pub source_id: Option<Id>,
    pub partition: Option<DataPartition>,
    pub cursor: Option<Id>,
    #[serde(default = "default_limit")]
    #[schema(default = 50, minimum = 1, maximum = 100)]
    pub limit: u16,
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
    /// 1..=2097152 encoded UTF-8 bytes; whitespace and final newline are retained.
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

/// Project research-read projection of frozen registration evidence. Historical
/// reads do not imply that a grant is currently valid for new scientific use.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeatureViewV1 {
    pub artifact_id: Id,
    pub project_id: Id,
    pub source_binding: RecordedFeatureSourceBindingV1,
    pub partition: DataPartition,
    pub source_selection_start_ns: DbCounter,
    pub source_selection_end_ns: DbCounter,
    /// Original fragment descriptor including byte size, count and exact clocks.
    pub fragment: crate::catalogs::RecordedFeatureFragmentV1,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeatureListQuery {
    pub project_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeatureListV1 {
    #[schema(max_items = 16)]
    pub items: Vec<RecordedFeatureViewV1>,
}
