//! Catalog-backed portfolio execution, not permission to use an Alpha version.
use super::{NativeBarSelectionV1, NativeForecastParametersV1};
use crate::{DbCounter, DecimalValue, Id, SchemaV1, brief::TargetKind, portfolio::*};
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
    #[schema(min_items = 1)]
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

/// Immutable initialization lineage, not an account balance or execution ledger.
/// The artifact is the original initial-weights document. The scope deliberately
/// excludes project, mandate and session IDs, so those cannot reset capital.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PaperInitializationRefV1 {
    pub artifact_id: Id,
    pub downstream_id: Id,
    #[schema(min_length = 1, max_length = 200)]
    pub trader_id: String,
    #[schema(min_length = 1, max_length = 200)]
    pub account_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PortfolioWeightsSourceV1 {
    /// New model initial condition. It is not a historical account observation.
    /// Only this source interprets asof/available as the frozen model cutoff;
    /// the actual initialization receipt retains its real creation timestamp.
    PaperInitialCapital {
        account_start: crate::strategy_portfolio::FreshPaperCashV1,
    },
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
    /// None preserves historical bytes. A new Paper lineage must carry the
    /// original server-resolved reference through snapshots and last targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper_initialization: Option<PaperInitializationRefV1>,
    pub asof_ns: DbCounter,
    pub available_ns: DbCounter,
    pub valid_until_ns: DbCounter,
    pub base_currency: String,
    pub cash_weight: DecimalValue,
    #[schema(min_items = 1)]
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
    pub rolling_liquidity: Option<NativeRollingBarLiquidityPolicyV1>,
    #[schema(min_items = 1)]
    pub assets: Vec<AllocationAssetV1>,
    #[schema(min_items = 2)]
    pub members: Vec<NativePortfolioAlphaV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioBuildResultV1 {
    pub schema_version: SchemaV1,
    pub bar_notionals: Vec<crate::execution::NativeBarNotionalV1>,
    pub slippage_references: Vec<NativePortfolioSlippageReferenceV1>,
    /// Observable original numerical inputs generated inside the fixed native job.
    pub input: AllocationInputV1,
    pub allocation: AllocationResultV1,
    /// None means some execution was unmetered, not a measured zero.
    #[serde(deserialize_with = "crate::science::deserialize_consumed_fuel")]
    #[schema(required = true)]
    pub consumed_fuel: Option<DbCounter>,
}

/// Original per-rebalance measurement policy, not a previously measured snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeRollingBarLiquidityPolicyV1 {
    pub schema_version: SchemaV1,
    #[schema(minimum = 1)]
    pub maximum_age_seconds: u32,
    pub participation_limit: DecimalValue,
}

/// Original UTC session boundaries, including native early closes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCalendarSessionV1 {
    pub open_ns: DbCounter,
    pub close_ns: DbCounter,
}

/// Original complete session data, not QZ-generated holiday rules.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCalendarSessionsV1 {
    pub schema_version: SchemaV1,
    pub calendar_ref: String,
    pub calendar_version: String,
    pub timezone: String,
    pub source_reference: String,
    pub available_at_ns: DbCounter,
    pub coverage_start_ns: DbCounter,
    pub coverage_end_ns: DbCounter,
    #[schema(min_items = 1)]
    pub sessions: Vec<NativeCalendarSessionV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioCalendarV1 {
    pub artifact_id: Id,
    pub calendar: NativeCalendarSessionsV1,
}

/// Offline model-driven research. The simulated account, not the caller, owns weights.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioStudyRequestV1 {
    /// Complete original condition payouts; not inferred from a last bar or expiry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settlements: Vec<crate::settlement::NativeSettlementGroupV1>,
    pub schema_version: SchemaV1,
    pub source_selection: NativeBarSelectionV1,
    pub evaluation_start_ns: DbCounter,
    #[schema(min_items = 2)]
    pub manual_cutoffs_ns: Option<Vec<DbCounter>>,
    pub calendar: Option<NativePortfolioCalendarV1>,
    pub research_available_through_ns: DbCounter,
    pub mandate: MandateContentV1,
    pub execution_settings: super::NativeSimulationSettingsV1,
    pub rolling_liquidity: Option<NativeRollingBarLiquidityPolicyV1>,
    #[schema(min_items = 1)]
    pub assets: Vec<AllocationAssetV1>,
    #[schema(min_items = 2)]
    pub members: Vec<NativePortfolioAlphaV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioStudyFrameV1 {
    pub cutoff_ns: DbCounter,
    pub input: AllocationInputV1,
    pub allocation: AllocationResultV1,
    pub slippage_references: Vec<NativePortfolioSlippageReferenceV1>,
    pub bar_notionals: Vec<crate::execution::NativeBarNotionalV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioStudyResultV1 {
    pub schema_version: SchemaV1,
    /// None means some execution was unmetered, not a measured zero.
    #[serde(deserialize_with = "crate::science::deserialize_consumed_fuel")]
    #[schema(required = true)]
    pub consumed_fuel: Option<DbCounter>,
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
