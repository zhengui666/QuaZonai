//! Public research uploads are not native evidence or delivery packages.
use crate::{research::DataOrigin, DbCounter, Id, SchemaV1, Timestamp};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_UPLOAD_BODY_BYTES: usize = MAX_UPLOAD_BYTES * 6 + 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResearchArtifactKind {
    Code,
    Parameters,
    Report,
}
impl ResearchArtifactKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Code => "CODE",
            Self::Parameters => "PARAMETERS",
            Self::Report => "REPORT",
        }
    }
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Code => "text/x-rust",
            Self::Parameters | Self::Report => "application/json",
        }
    }
    pub fn schema_name(self) -> &'static str {
        match self {
            Self::Code => "qz.rust_source",
            Self::Parameters => "qz.research_parameters",
            Self::Report => "qz.research_report",
        }
    }
}

// Content may contain proprietary source; do not derive Debug for upload bodies.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCreate {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub kind: ResearchArtifactKind,
    /// At most 2 MiB of UTF-8, including JSON whitespace; the original bytes are preserved.
    #[schema(min_length = 1, max_length = 2097152)]
    pub content: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtifactAccess {
    Operator,
    Research,
    EvaluatorOnly,
    Delivery,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtifactProducer {
    Operator,
    Runtime,
    Agent,
    Import,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactView {
    pub id: Id,
    pub project_id: Id,
    pub producer_run_id: Option<Id>,
    pub producer_attempt_id: Option<Id>,
    pub kind: String,
    pub media_type: String,
    pub schema_name: String,
    pub schema_version: String,
    pub byte_count: DbCounter,
    pub access_class: ArtifactAccess,
    pub origin: DataOrigin,
    pub created_by: ArtifactProducer,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: Timestamp,
}
