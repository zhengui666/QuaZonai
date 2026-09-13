//! Catalog-backed portfolio execution, not permission to use an Alpha version.
use super::{NativeBarSelectionV1, NativeForecastParametersV1};
use crate::{brief::TargetKind, portfolio::*, DbCounter, DecimalValue, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PortfolioWeightsSourceV1 {
    ForwardSnapshot {
        downstream_id: Id,
        external_message_id: String,
    },
    LastTarget {
        candidate_id: Id,
    },
}

/// Observable target-only weights. Source identity is verified by Store, not by the numerical job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PortfolioCurrentWeightsV1 {
    pub schema_version: SchemaV1,
    pub source: PortfolioWeightsSourceV1,
    pub asof_ns: DbCounter,
    pub available_ns: DbCounter,
    pub valid_until_ns: DbCounter,
    pub base_currency: String,
    pub cash_weight: DecimalValue,
    #[schema(min_items = 1, max_items = 256)]
    pub weights: Vec<AllocationTargetV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioAlphaV1 {
    pub alpha_id: Id,
    pub alpha_version_id: Id,
    pub model_artifact_id: Id,
    pub calibration_artifact_id: Option<Id>,
    pub target_kind: TargetKind,
    pub ensemble_weight: DecimalValue,
    pub parameters: NativeForecastParametersV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioBuildRequestV1 {
    pub schema_version: SchemaV1,
    pub selection: NativeBarSelectionV1,
    pub mandate: MandateContentV1,
    pub current_weights_artifact_id: Id,
    pub current_weights: PortfolioCurrentWeightsV1,
    #[schema(min_items = 1, max_items = 256)]
    pub assets: Vec<AllocationAssetV1>,
    #[schema(min_items = 2, max_items = 256)]
    pub members: Vec<NativePortfolioAlphaV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioBuildResultV1 {
    pub schema_version: SchemaV1,
    /// Observable original numerical inputs generated inside the fixed native job.
    pub input: AllocationInputV1,
    pub allocation: AllocationResultV1,
    pub consumed_fuel: DbCounter,
}
