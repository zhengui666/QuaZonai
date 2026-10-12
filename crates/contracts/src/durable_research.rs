//! Thin transport contracts. Native execution views stay pi-durable-owned JSON.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DurableResearchStartV1 {
    pub request_id: String,
    pub input: DurableResearchInputV1,
    #[serde(flatten)]
    pub payload: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct DurableResearchInputV1 {
    pub goal: String,
    pub model: DurableResearchModelV1,
    /// Extension-owned data: no core asset, source or factor enum.
    pub data: Map<String, Value>,
    /// Original evaluation/selection/stop criteria and optional user limits.
    pub policy: Map<String, Value>,
    #[serde(flatten)]
    pub payload: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DurableResearchModelV1 {
    pub provider: String,
    pub model_id: String,
    #[serde(flatten)]
    pub payload: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DurableSubmissionReceiptV1 {
    #[schema(minimum = 0, maximum = 9007199254740991u64)]
    pub conversation_id: u64,
    #[schema(minimum = 0, maximum = 9007199254740991u64)]
    pub submission_id: u64,
    #[serde(flatten)]
    pub payload: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DurableModelViewV1 {
    pub provider: String,
    pub model_id: String,
    pub name: String,
    pub reasoning: bool,
    #[serde(flatten)]
    pub payload: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct DurableResearchErrorV1 {
    pub error: String,
    #[serde(flatten)]
    pub payload: Map<String, Value>,
}

/// Native admission closure or an unavailable/unknown transport outcome.
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(untagged)]
pub enum DurableResearchAdmissionUnavailableV1 {
    Native(DurableResearchErrorV1),
    Transport(Box<crate::http::Problem>),
}
