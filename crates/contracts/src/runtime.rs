//! Native data kinds and result-schema identifiers, not a Runtime service API.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeDataKind {
    Bar,
    Quote,
    Trade,
    OrderBook,
    Fundamental,
    Event,
    DerivedFeature,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeArtifactSchemaV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    #[schema(min_length = 1, max_length = 40)]
    pub version: String,
}
