//! Explicit counterfactual replay from an observed spot inventory, never account writes.
use crate::{account_observation::NativePortfolioSnapshotV1, science::*, DbCounter, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStartV1 {
    pub schema_version: SchemaV1,
    pub snapshot_evidence_ref: String,
    pub source_native_version: String,
    pub snapshot: NativePortfolioSnapshotV1,
    pub available_ns: DbCounter,
    pub open_orders_evidence_ref: String,
    pub open_orders_account_id: String,
    pub open_orders_asof_ns: DbCounter,
    pub open_orders_available_ns: DbCounter,
    /// Required original observation. Missing never means zero.
    pub open_orders_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotComparisonRequestV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub initial_state: NativeObservedSpotStartV1,
    pub simulation: NativeSimulationRequestV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStudyRequestV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub initial_state: NativeObservedSpotStartV1,
    pub study: NativePortfolioStudyRequestV1,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeObservedSpotLegStatusV1 {
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotSimulationLegV1 {
    pub status: NativeObservedSpotLegStatusV1,
    pub result: Option<NativeSimulationResultV1>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStudyLegV1 {
    pub status: NativeObservedSpotLegStatusV1,
    /// Infeasible solver frames remain evidence, even without a completed replay.
    pub result: Option<NativePortfolioStudyResultV1>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotComparisonResultV1 {
    pub schema_version: SchemaV1,
    pub kind: String,
    pub basis: String,
    pub request: NativeObservedSpotComparisonRequestV1,
    pub hold: NativeObservedSpotSimulationLegV1,
    pub rebalance: NativeObservedSpotSimulationLegV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStudyResultV1 {
    pub schema_version: SchemaV1,
    pub kind: String,
    pub basis: String,
    pub request: NativeObservedSpotStudyRequestV1,
    pub hold: NativeObservedSpotSimulationLegV1,
    pub rebalance: NativeObservedSpotStudyLegV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotValuationRequestV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub initial_state: NativeObservedSpotStartV1,
    pub selection: NativeBarSelectionV1,
    pub settings: NativeSimulationSettingsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotValuationLegV1 {
    pub currency: String,
    pub native_total: crate::DecimalValue,
    pub report_value: crate::DecimalValue,
    pub instrument_id: Option<String>,
    pub price: Option<crate::DecimalValue>,
    pub price_event_ns: Option<DbCounter>,
    pub price_available_ns: Option<DbCounter>,
    pub source_row_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotValuationResultV1 {
    pub schema_version: SchemaV1,
    pub kind: String,
    pub basis: String,
    pub request: NativeObservedSpotValuationRequestV1,
    pub report_currency: String,
    pub asof_ns: DbCounter,
    pub available_ns: DbCounter,
    pub total: crate::DecimalValue,
    pub legs: Vec<NativeObservedSpotValuationLegV1>,
    pub source_evidence: Option<crate::spot_cash_source::NativeSpotCashSourceEvidenceV1>,
}

/// One causal forward solve. No replay, fabricated opening account or supplied current weights.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotBuildRequestV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub initial_state: NativeObservedSpotStartV1,
    pub selection: NativeBarSelectionV1,
    pub mandate: crate::portfolio::MandateContentV1,
    pub execution_settings: NativeSimulationSettingsV1,
    pub rolling_liquidity: Option<NativeRollingBarLiquidityPolicyV1>,
    /// Zero current_weight placeholders; the original inventory replaces them.
    pub assets: Vec<crate::portfolio::AllocationAssetV1>,
    pub members: Vec<NativePortfolioAlphaV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeForwardModeledOutcomeV1 {
    pub gross_expected_return: f64,
    pub linear_turnover_cost: f64,
    pub net_expected_return: f64,
    /// Squared return per original forecast horizon, not annualized volatility.
    pub variance: f64,
    /// Original minimized MAX_UTILITY objective: lambda * variance + cost - return.
    pub objective_value: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeForwardModeledComparisonV1 {
    pub method: String,
    pub basis: String,
    pub forecast_unit: String,
    pub hold: NativeForwardModeledOutcomeV1,
    pub proposed: Option<NativeForwardModeledOutcomeV1>,
    pub limitations: Vec<String>,
}

/// Exact product representation for continuous estimates, not a persisted
/// NUMERIC(38,18) amount. Two source decimals need at most 40 integer digits
/// and 36 fractional digits; conversion never rounds either operand.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NativeForwardExactNotionalV1(String);

impl utoipa::PartialSchema for NativeForwardExactNotionalV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some("Exact derived notional product; up to 40 integer digits and 36 fractional digits without rounding. Not a NUMERIC(38,18) balance."))
            .min_length(Some(1)).max_length(Some(80))
            .pattern(Some(r"^-?[0-9]{1,40}(?:\.[0-9]{1,36})?(?![\s\S])"))
            .into()
    }
}
impl ToSchema for NativeForwardExactNotionalV1 {}

impl NativeForwardExactNotionalV1 {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for NativeForwardExactNotionalV1 {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let unsigned = value.strip_prefix('-').unwrap_or(&value);
        let mut parts = unsigned.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();
        if value.len() > 80
            || whole.is_empty()
            || whole.len() > 40
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || parts.next().is_some()
            || fraction.is_some_and(|part| {
                part.is_empty() || part.len() > 36 || !part.bytes().all(|b| b.is_ascii_digit())
            })
        {
            return Err("invalid exact notional product: at most 40 integer and 36 fractional digits, without rounding".into());
        }
        Ok(Self(value))
    }
}
impl From<NativeForwardExactNotionalV1> for String {
    fn from(value: NativeForwardExactNotionalV1) -> Self {
        value.0
    }
}

/// Static trade-size estimate at retained marks, never an order or fill.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeForwardSpotTradeEstimateV1 {
    pub instrument_id: String,
    pub current_quantity: crate::DecimalValue,
    pub current_notional: crate::DecimalValue,
    /// Continuous optimizer intent; not the lot-rounded inventory value.
    /// Exact product of original NAV and weight; may need up to 36 decimals.
    pub target_notional: NativeForwardExactNotionalV1,
    /// Continuous optimizer intent minus original marked inventory.
    pub net_trade_notional: NativeForwardExactNotionalV1,
    /// Signed base quantity after the existing toward-zero lot rule.
    pub rounded_trade_quantity: crate::DecimalValue,
    /// Original observed base quantity plus the signed rounded trade quantity.
    pub post_rounding_quantity: crate::DecimalValue,
    /// Post-rounding base quantity times this same retained source mark.
    pub post_rounding_notional: crate::DecimalValue,
    /// Signed rounded trade quantity times this same retained source mark.
    pub rounded_trade_notional: crate::DecimalValue,
    pub price: crate::DecimalValue,
    pub price_event_ns: DbCounter,
    pub price_available_ns: DbCounter,
    pub source_row_key: String,
    pub modeled_linear_cost: crate::DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotBuildResultV1 {
    pub schema_version: SchemaV1,
    pub kind: String,
    pub basis: String,
    pub request: NativeObservedSpotBuildRequestV1,
    /// Cutoff-price valuation of original observed quantities. The source snapshot
    /// economic clock is unchanged; this is not a newly observed account NAV.
    pub valuation: NativeObservedSpotValuationResultV1,
    pub build: NativePortfolioBuildResultV1,
    pub modeled_comparison: NativeForwardModeledComparisonV1,
    /// Same original-NAV horizon model applied to lot-rounded inventory.
    /// Absent when no admissible target was produced; never realized returns.
    pub rounded_modeled_comparison: Option<NativeForwardModeledComparisonV1>,
    /// Original official definitions, including lot/minimum/fee precision rules.
    pub instrument_definitions: Vec<serde_json::Value>,
    pub trade_estimates: Vec<NativeForwardSpotTradeEstimateV1>,
    pub target: Option<NativeTargetPointV1>,
    pub target_block_reason: Option<String>,
}
