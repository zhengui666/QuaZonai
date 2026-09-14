//! Target-only downstream observations; no account ledger or execution credentials.
use crate::{
    portfolio::AllocationTargetV1, science::PortfolioCurrentWeightsV1, DbCounter, DecimalValue, Id,
    SchemaV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForwardEnvironmentV1 {
    Paper,
    Live,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamWeightsSubmitV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub environment: ForwardEnvironmentV1,
    #[schema(min_length = 1, max_length = 200)]
    pub external_message_id: String,
    pub asof_ns: DbCounter,
    pub available_ns: DbCounter,
    pub valid_until_ns: DbCounter,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub base_currency: String,
    pub cash_weight: DecimalValue,
    #[schema(min_items = 1, max_items = 256)]
    pub weights: Vec<AllocationTargetV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DownstreamWeightsViewV1 {
    pub id: Id,
    pub project_id: Id,
    pub downstream_id: Id,
    pub environment: ForwardEnvironmentV1,
    pub report_artifact_id: Id,
    pub content: PortfolioCurrentWeightsV1,
    pub received_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForwardReportContentV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub handoff_id: Id,
    pub external_claim_id: String,
    pub issuer_version: String,
    pub stream_id: String,
    pub sequence: DbCounter,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub message_revision: u32,
    pub supersedes_message_id: Option<Id>,
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_end: chrono::DateTime<chrono::Utc>,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub complete: bool,
    #[schema(max_items = 10000)]
    pub returns: Vec<crate::science::NativeReturnV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForwardMessageSubmitV1 {
    pub schema_version: SchemaV1,
    pub external_message_id: String,
    pub report: ForwardReportContentV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForwardReportV1 {
    pub schema_version: SchemaV1,
    pub downstream_id: Id,
    pub release_id: Id,
    pub environment: ForwardEnvironmentV1,
    pub content: ForwardReportContentV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForwardCoverageV1 {
    Complete,
    Partial,
    Correction,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForwardMessageViewV1 {
    pub id: Id,
    pub project_id: Id,
    pub release_id: Id,
    pub downstream_id: Id,
    pub handoff_id: Id,
    pub external_message_id: String,
    pub stream_id: String,
    pub sequence: DbCounter,
    pub message_revision: u32,
    pub supersedes_message_id: Option<Id>,
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_end: chrono::DateTime<chrono::Utc>,
    pub coverage_status: ForwardCoverageV1,
    pub observation_count: DbCounter,
    pub report_artifact_id: Id,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub received_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForwardWindowQueryV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub stream_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForwardWindowReasonV1 {
    NoMessages,
    Partial,
    MissingReturns,
    SequenceGap,
    WindowGap,
    WindowOverlap,
    SampleOverlap,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForwardWindowViewV1 {
    pub handoff_id: Id,
    pub stream_id: String,
    pub latest_message_ids: Vec<Id>,
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
    pub complete_observations: DbCounter,
    pub is_contiguous: bool,
    pub reason_codes: Vec<ForwardWindowReasonV1>,
}
