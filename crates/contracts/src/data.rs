//! Operator-managed data sources, explicit licenses and immutable native registrations.
//! Client requests never assign origin, PIT status, row counts or scientific qualification.
use crate::{
    catalogs::DataRevisionPolicy,
    research::{DataOrigin, DataPartition, DataUse, PitStatus},
    runtime::RuntimeDataKind,
    DbCounter, Id, Revision, SchemaV1,
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
    #[schema(schema_with = data_validation_limits_schema)]
    pub limits: crate::lifecycle::JobLimitsV1,
}

fn data_validation_limits_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::{
        openapi::schema::{AllOfBuilder, ObjectBuilder, Type},
        PartialSchema,
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
                .property("cpu_seconds", crate::scalars::positive_db_counter_schema())
                .property(
                    "wall_seconds",
                    ObjectBuilder::new()
                        .schema_type(Type::Integer)
                        .minimum(Some(1.0))
                        .maximum(Some(86400.0)),
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
                    crate::scalars::bounded_bigint_schema(
                        crate::runtime_jobs::MAX_JOB_OUTPUT_BYTES,
                        true,
                    ),
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
