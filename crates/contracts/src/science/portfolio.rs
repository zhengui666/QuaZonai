//! Catalog-backed portfolio execution, not permission to use an Alpha version.
use super::{NativeBarSelectionV1, NativeForecastParametersV1};
use crate::{brief::TargetKind, portfolio::*, DbCounter, DecimalValue, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Exact immutable qz.portfolio_targets/1 document, not an account snapshot.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PortfolioTargetsV1 {
    pub schema_version: SchemaV1,
    pub candidate_id: Id,
    pub base_currency: String,
    pub asof: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub cash_weight: DecimalValue,
    #[schema(min_items = 1, max_items = 256)]
    pub targets: Vec<AllocationTargetV1>,
}

/// Trusted original availability and object identity, not operator-authored weights.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioTargetSourceV1 {
    pub candidate_id: Id,
    pub candidate_available_ns: DbCounter,
    pub target_artifact_id: Id,
}

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
pub struct NativePortfolioLiquidityV1 {
    pub schema_version: SchemaV1,
    pub assumption: crate::execution_assumptions::BarLiquidityAssumptionV1,
    pub source: crate::execution::NativeDatasetSelectionV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioBuildRequestV1 {
    pub schema_version: SchemaV1,
    pub selection: NativeBarSelectionV1,
    pub mandate: MandateContentV1,
    pub current_weights_artifact_id: Id,
    pub current_weights: PortfolioCurrentWeightsV1,
    pub execution_settings: super::NativeSimulationSettingsV1,
    pub bar_liquidity: Option<NativePortfolioLiquidityV1>,
    #[schema(min_items = 1, max_items = 256)]
    pub assets: Vec<AllocationAssetV1>,
    #[schema(min_items = 2, max_items = 256)]
    pub members: Vec<NativePortfolioAlphaV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioBuildResultV1 {
    pub schema_version: SchemaV1,
    #[schema(max_items = 256)]
    pub slippage_references: Vec<NativePortfolioSlippageReferenceV1>,
    /// Observable original numerical inputs generated inside the fixed native job.
    pub input: AllocationInputV1,
    pub allocation: AllocationResultV1,
    pub consumed_fuel: DbCounter,
}

/// Offline model-driven research. The simulated account, not the caller, owns weights.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioStudyRequestV1 {
    pub schema_version: SchemaV1,
    pub source_selection: NativeBarSelectionV1,
    pub evaluation_start_ns: DbCounter,
    pub research_available_through_ns: DbCounter,
    pub mandate: MandateContentV1,
    pub execution_settings: super::NativeSimulationSettingsV1,
    #[schema(min_items = 1, max_items = 256)]
    pub assets: Vec<AllocationAssetV1>,
    #[schema(min_items = 2, max_items = 256)]
    pub members: Vec<NativePortfolioAlphaV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioStudyFrameV1 {
    pub cutoff_ns: DbCounter,
    pub input: AllocationInputV1,
    pub allocation: AllocationResultV1,
    pub slippage_references: Vec<NativePortfolioSlippageReferenceV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioStudyResultV1 {
    pub schema_version: SchemaV1,
    pub consumed_fuel: DbCounter,
    #[schema(max_items = 256)]
    pub frames: Vec<NativePortfolioStudyFrameV1>,
    /// Actual generated points, never the schedule's provisional all-cash placeholders.
    pub simulation_request: Option<super::NativeSimulationRequestV1>,
    pub simulation: Option<super::NativeSimulationResultV1>,
}

/// Original last-known native BAR and tick, used only for proportional cost planning.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioSlippageReferenceV1 {
    pub instrument_id: String,
    pub currency: String,
    pub event_ns: DbCounter,
    pub available_ns: DbCounter,
    pub close_price: DecimalValue,
    pub price_increment: DecimalValue,
}
