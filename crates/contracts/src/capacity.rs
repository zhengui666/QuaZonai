//! Conditional BAR participation capacity and account sizing, never a profit forecast.
use crate::{science::*, DbCounter, DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapacityRequestV1 {
    pub schema_version: SchemaV1,
    pub simulation: NativeSimulationRequestV1,
    pub policy: NativeRollingBarLiquidityPolicyV1,
    pub capitals: Vec<DecimalValue>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityScenarioV1 {
    pub capital: DecimalValue,
    pub required_turnover: Option<DecimalValue>,
    pub maximum_participation: Option<DecimalValue>,
    pub usable_capital: Option<DecimalValue>,
    pub unallocated_cash: Option<DecimalValue>,
    pub status: String,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityAssumptionsV1 {
    pub participation: DecimalValue,
    pub maximum_age_seconds: u32,
    pub execution_window: String,
    pub exit: String,
    pub fees: String,
    pub depth: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityReportV1 {
    pub status: String,
    pub reason: Option<String>,
    pub scope: String,
    pub currency: String,
    pub basis: String,
    pub maximum_capital: Option<DecimalValue>,
    pub minimum_capital: Option<DecimalValue>,
    pub binding_instrument: Option<String>,
    pub observation_start_ns: Option<DbCounter>,
    pub observation_end_ns: Option<DbCounter>,
    pub decision_clock_ns: Option<DbCounter>,
    pub valid_until_ns: Option<DbCounter>,
    pub assumptions: NativeCapacityAssumptionsV1,
    pub scenarios: Vec<NativeCapacityScenarioV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSizingLiquidityV1 {
    pub instrument_id: String,
    pub currency: String,
    pub event_ns: DbCounter,
    pub available_ns: DbCounter,
    pub notional: DecimalValue,
    pub lot_notional: DecimalValue,
    pub minimum_notional: DecimalValue,
    pub taker_fee: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSizingHoldingV1 {
    pub instrument_id: String,
    pub value: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeAccountSizingRequestV1 {
    pub schema_version: SchemaV1,
    #[serde(default)]
    pub projection_only: bool,
    pub equity: DecimalValue,
    /// Outstanding reservation only; reconciled withdrawals are already in equity.
    pub reserved: DecimalValue,
    pub cash_buffer: DecimalValue,
    pub maximum_capital: DecimalValue,
    pub minimum_capital: Option<DecimalValue>,
    pub participation: DecimalValue,
    pub maximum_age_seconds: u32,
    pub currency: String,
    pub clock_ns: DbCounter,
    pub target: NativeTargetPointV1,
    pub holdings: Vec<NativeSizingHoldingV1>,
    pub liquidity: Vec<NativeSizingLiquidityV1>,
    pub members: Vec<NativeSizingMemberV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSizingMemberV1 {
    pub factor_id: String,
    pub weight: DecimalValue,
    #[serde(default)]
    pub pool_weight: Option<DecimalValue>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeSizedMemberV1 {
    pub factor_id: String,
    pub capital: DecimalValue,
    pub scale: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeAccountSizingResultV1 {
    pub status: String,
    pub reason: Option<String>,
    pub budget: DecimalValue,
    pub capital: DecimalValue,
    pub residual_cash: DecimalValue,
    pub target: Option<NativeTargetPointV1>,
    pub members: Vec<NativeSizedMemberV1>,
}
