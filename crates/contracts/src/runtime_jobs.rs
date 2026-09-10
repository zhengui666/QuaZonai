//! Control-plane-to-runtime wire contracts. Only trusted services can submit these.
//! Neither a terminal process nor a declared artifact grants scientific qualification.
use crate::{
    research::{ArtifactInputRole, DataPartition},
    runs::RunKind,
    runtime::RuntimeArtifactSchemaV1,
    DbCounter, Id, Revision, SchemaV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobLimitsV1 {
    #[schema(minimum = 1, maximum = 1024)]
    pub cpu: u16,
    #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
    pub cpu_seconds: DbCounter,
    #[schema(minimum = 1, maximum = 4294967295u64, format = Int64)]
    pub memory_mib: u32,
    #[schema(minimum = 1, maximum = 4294967295u64, format = Int64)]
    pub wall_seconds: u32,
    #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
    pub output_bytes: DbCounter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum RuntimeInputV1 {
    Dataset {
        revision_id: Id,
        /// Exact registry key, never a filesystem path or a URL to fetch.
        #[schema(min_length = 1, max_length = 512)]
        registered_ref: String,
        #[schema(min_length = 1, max_length = 120)]
        storage_version: String,
        role: DataPartition,
    },
    Artifact {
        artifact_id: Id,
        #[schema(min_length = 1, max_length = 120)]
        storage_version: String,
        #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
        byte_count: DbCounter,
        role: ArtifactInputRole,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobSpecV1 {
    pub schema_version: SchemaV1,
    pub run_id: Id,
    #[schema(minimum = 1, maximum = 4294967295u64, format = Int64)]
    pub attempt_no: u32,
    /// The first dispatch owner's epoch. Takeover never rewrites this frozen spec.
    pub owner_epoch: Revision,
    #[schema(min_length = 38, max_length = 47)]
    pub external_job_id: String,
    pub job_kind: RunKind,
    #[schema(min_length = 1, max_length = 512)]
    pub image_ref: String,
    pub input_set_id: Id,
    #[schema(min_items = 1, max_items = 256)]
    pub inputs: Vec<RuntimeInputV1>,
    /// Trusted native configuration stored under an immutable Artifact identity.
    /// It does not change frozen InputSet membership or provide arbitrary commands.
    pub parameters_artifact_id: Id,
    pub limits: RuntimeJobLimitsV1,
    pub deadline_at: DateTime<Utc>,
    #[schema(min_items = 1, max_items = 64)]
    pub requested_output_schemas: Vec<RuntimeArtifactSchemaV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeJobState {
    Accepted,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
}
impl RuntimeJobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobStatusV1 {
    pub schema_version: SchemaV1,
    pub run_id: Id,
    #[schema(minimum = 1, maximum = 4294967295u64, format = Int64)]
    pub attempt_no: u32,
    pub external_job_id: String,
    pub state: RuntimeJobState,
    pub has_result: bool,
    pub submitted_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Cancellation is durable even when the original submission has not arrived yet.
/// The native identity is closed; a later matching or conflicting POST cannot start it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCancelV1 {
    pub schema_version: SchemaV1,
    pub run_id: Id,
    #[schema(minimum = 1, maximum = 4294967295u64, format = Int64)]
    pub attempt_no: u32,
    pub owner_epoch: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeResultState {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeOutputKind {
    Model,
    Signals,
    Targets,
    Report,
    Metrics,
    DataQuality,
}
impl RuntimeOutputKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Model => "MODEL",
            Self::Signals => "SIGNALS",
            Self::Targets => "TARGETS",
            Self::Report => "REPORT",
            Self::Metrics => "METRICS",
            Self::DataQuality => "DATA_QUALITY",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeOutputV1 {
    pub kind: RuntimeOutputKind,
    pub schema: RuntimeArtifactSchemaV1,
    /// Runtime-owned immutable object key. Downloads stay under this exact job.
    pub storage_ref: Id,
    pub storage_version: Revision,
    #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
    pub byte_count: DbCounter,
    #[schema(min_length = 1, max_length = 120)]
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeResourceUsageV1 {
    pub wall_milliseconds: DbCounter,
    /// Native cgroup measurement, or null when no exact final sample is available.
    pub cpu_nanoseconds: Option<DbCounter>,
    pub peak_memory_bytes: Option<DbCounter>,
    /// Sum of the immutable output payload sizes, excluding the manifest envelope.
    pub output_bytes: DbCounter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeFailureClass {
    RetryableInfra,
    PermanentConfig,
    InvalidInput,
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeFailureCode {
    EngineUnavailable,
    ImageUnavailable,
    ContractUnsupported,
    InputUnavailable,
    InvalidInput,
    NativeJobFailed,
    InvalidOutput,
    CpuLimit,
    MemoryLimit,
    OutputLimit,
    DeadlineExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobErrorV1 {
    pub class: RuntimeFailureClass,
    pub code: RuntimeFailureCode,
    /// A code-derived static message, never native stderr, input text or an HTTP body.
    #[schema(min_length = 1, max_length = 256)]
    pub safe_message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultManifestV1 {
    pub schema_version: SchemaV1,
    pub run_id: Id,
    #[schema(minimum = 1, maximum = 4294967295u64, format = Int64)]
    pub attempt_no: u32,
    pub external_job_id: String,
    pub input_set_id: Id,
    pub state: RuntimeResultState,
    #[schema(schema_with = crate::runtime::engine_versions_schema)]
    pub engine_versions: BTreeMap<String, String>,
    /// Null only when native execution was never started.
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: DateTime<Utc>,
    pub resource_usage: RuntimeResourceUsageV1,
    #[schema(max_items = 64)]
    pub artifacts: Vec<RuntimeOutputV1>,
    pub error: Option<RuntimeJobErrorV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeObjectReceiptV1 {
    pub schema_version: SchemaV1,
    pub artifact_id: Id,
    #[schema(min_length = 1, max_length = 120)]
    pub storage_version: String,
    #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
    pub byte_count: DbCounter,
}
