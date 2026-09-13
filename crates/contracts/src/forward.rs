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
