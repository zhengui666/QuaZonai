//! Explicit economic estimates over derived cohorts, separate from factor evidence.
use crate::{ticks::*, DbCounter, DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickEconomicScenarioV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_model: Option<NativeTickEconomicExecutionModelV1>,
    pub initial_usdt: DecimalValue,
    pub usdt0_per_usdt: DecimalValue,
    pub spot_fraction: DecimalValue,
    pub gross_fraction: DecimalValue,
    pub maximum_asset_weight: DecimalValue,
    pub participation_fraction: DecimalValue,
    pub perp_taker_fee_bps: DecimalValue,
    pub spot_taker_fee_bps: DecimalValue,
    pub extra_cost_bps: DecimalValue,
    pub initial_margin_fraction: DecimalValue,
    pub maintenance_margin_fraction: DecimalValue,
    pub decision_interval_ns: DbCounter,
    pub base_latency_ns: DbCounter,
    pub jitter_min_ns: DbCounter,
    pub jitter_max_ns: DbCounter,
    pub random_seed: DbCounter,
    pub maximum_mark_age_ns: DbCounter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum NativeTickEconomicExecutionModelV1 {
    #[serde(rename = "SYNTHETIC_QUOTE_IOC_V1")]
    SyntheticQuoteIocV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickSmallCapitalPolicyV1 {
    pub maximum_entry_notional_usdc: DecimalValue,
    pub maximum_holding_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickEconomicRequestV1 {
    pub schema_version: SchemaV1,
    /// The selected model's original prepared input and feature parameters.
    pub research: NativeTradeTickFactorEvaluationRequestV1,
    pub scenario: NativeTickEconomicScenarioV1,
    pub fx_instrument_ids: Vec<String>,
    /// Catalog-relative chronological JSONL of actual instrument_id/event_ns/rate.
    pub funding_source: String,
    /// Explicit optional oracle/mark observations in the independent economic catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_source: Option<String>,
    /// Optional catalog-relative spot-valuation.jsonl for daily valuation only.
    /// Each row: instrument_id, received_ns, bid and ask; rows are ordered per instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spot_valuation_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_capital_policy: Option<NativeTickSmallCapitalPolicyV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickEconomicDayV1 {
    pub day_start_ns: DbCounter,
    pub equity_usdt: Option<DecimalValue>,
    pub net_return: Option<f64>,
    pub unknown_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickEconomicAssetV1 {
    pub instrument_id: String,
    pub native_kind: String,
    pub cohorts: DbCounter,
    pub decisions: DbCounter,
    pub execution_attempts: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickEconomicMetricV1 {
    pub partition: crate::factors::NativeFactorPartitionV1,
    pub regime: NativeTickRegimeV1,
    pub days: DbCounter,
    pub valid_return_days: DbCounter,
    pub sharpe_365: Option<f64>,
    pub conditional_annualized_return_365: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickSmallCapitalPositionV1 {
    pub instrument_id: String,
    pub quantity: DecimalValue,
    pub entry_decision_ns: DbCounter,
    pub first_fill_ns: DbCounter,
    pub exit_decision_ns: Option<DbCounter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickSmallCapitalDiagnosticsV1 {
    pub entry_attempts: DbCounter,
    pub unfilled_entries: DbCounter,
    pub positions_opened: DbCounter,
    pub positions_closed: DbCounter,
    pub exit_attempts: DbCounter,
    pub maximum_exit_delay_ns: Option<DbCounter>,
    pub remaining_positions: Vec<NativeTickSmallCapitalPositionV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickEconomicResultV1 {
    pub schema_version: SchemaV1,
    pub request: NativeTickEconomicRequestV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_capital_diagnostics: Option<NativeTickSmallCapitalDiagnosticsV1>,
    pub model: String,
    pub methods: BTreeMap<String, String>,
    pub coverage: NativeTradeTickCoverageV1,
    pub assets: Vec<NativeTickEconomicAssetV1>,
    pub days: Vec<NativeTickEconomicDayV1>,
    pub metrics: Vec<NativeTickEconomicMetricV1>,
    pub conversion_ns: Option<DbCounter>,
    pub initial_usdc_after_cost: Option<DecimalValue>,
    pub funding_usdc: DecimalValue,
    pub fees_usdc_equivalent: Option<DecimalValue>,
    pub fees_by_currency: BTreeMap<String, DecimalValue>,
    pub initial_conversion_cost_usdc: DecimalValue,
    pub nominal_usd_settled_usdc: DecimalValue,
    pub native_fills: DbCounter,
    pub native_rejections: DbCounter,
    pub native_perp_pnl_rounding_events: DbCounter,
    pub perp_pnl_rounding_bound_usdc: DecimalValue,
    pub funding_missing_instruments: Vec<String>,
    pub targets: Vec<crate::portfolio::AllocationTargetV1>,
    pub target_asof_ns: Option<DbCounter>,
    pub target_cash_weight: Option<DecimalValue>,
    pub skipped_cash: DbCounter,
    pub skipped_minimum: DbCounter,
    pub consumed_fuel: Option<DbCounter>,
    pub limitations: Vec<String>,
}
