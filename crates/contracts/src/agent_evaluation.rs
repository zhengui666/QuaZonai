//! Uploaded Agent runner evidence. These reports never create scientific qualification.
use crate::{DbCounter, DecimalValue, SchemaV1, Timestamp};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, pattern = r"^[0-9a-f]{64}(?![\s\S])")]
pub struct EvaluationSha256(String);
impl TryFrom<String> for EvaluationSha256 {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(value))
        } else {
            Err("expected lowercase SHA-256")
        }
    }
}
impl From<EvaluationSha256> for String {
    fn from(value: EvaluationSha256) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentEvaluationReportKind {
    AgentEvaluation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentEvaluationMode {
    Live,
    ProtocolOnly,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentEvaluationStatus {
    Pass,
    Fail,
    Blocked,
    Unrun,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentEvaluationSplit {
    Tuning,
    HeldOut,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationIdentityV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub name: String,
    #[schema(min_length = 1, max_length = 200)]
    pub version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationPolicyV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub model: String,
    #[schema(min_length = 1, max_length = 200)]
    pub reasoning_effort: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationObservedV1 {
    pub settings: AgentEvaluationPolicyV1,
    /// Public native invocation identity, never hidden reasoning or credentials.
    #[schema(min_length = 1, max_length = 200)]
    pub invocation_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationDatasetV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub id: String,
    pub sha256: EvaluationSha256,
    #[schema(min_items = 1)]
    pub case_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationAssertionV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub id: String,
    pub passed: bool,
    /// Hash of the externally retained, auditable observation; not its raw content.
    pub evidence_sha256: EvaluationSha256,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationCostV1 {
    pub amount: DecimalValue,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub currency: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationMeasurementsV1 {
    /// Null means unknown, including when a provider does not report usage.
    pub input_tokens: Option<DbCounter>,
    pub output_tokens: Option<DbCounter>,
    pub elapsed_ms: Option<DbCounter>,
    pub tool_calls: Option<DbCounter>,
    /// Actual reported amount and currency, never a hardcoded pricing estimate.
    pub cost: Option<AgentEvaluationCostV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationCaseV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub id: String,
    pub split: AgentEvaluationSplit,
    pub scenario_sha256: EvaluationSha256,
    pub status: AgentEvaluationStatus,
    #[schema(min_length = 1, max_length = 2000)]
    pub reason: String,
    pub observed: Option<AgentEvaluationObservedV1>,
    pub measurements: AgentEvaluationMeasurementsV1,
    /// IDs fixed by the suite before execution; PASS requires every one with evidence.
    #[schema(min_items = 1)]
    pub required_assertions: Vec<String>,
    pub assertions: Vec<AgentEvaluationAssertionV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvaluationReportV1 {
    pub schema_version: SchemaV1,
    pub report_kind: AgentEvaluationReportKind,
    pub mode: AgentEvaluationMode,
    pub status: AgentEvaluationStatus,
    #[schema(value_type = String, format = DateTime)]
    pub recorded_at: Timestamp,
    pub runner: AgentEvaluationIdentityV1,
    pub subject: AgentEvaluationIdentityV1,
    pub requested: AgentEvaluationPolicyV1,
    #[schema(min_length = 1, max_length = 200)]
    pub source_revision: String,
    pub source_sha256: EvaluationSha256,
    #[schema(min_length = 1, max_length = 200)]
    pub suite_id: String,
    pub suite_sha256: EvaluationSha256,
    pub tuning: AgentEvaluationDatasetV1,
    pub held_out: AgentEvaluationDatasetV1,
    #[schema(min_items = 2)]
    pub cases: Vec<AgentEvaluationCaseV1>,
}
