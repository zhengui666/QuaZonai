//! Native job inputs and observable outputs. No path, credential or PASS authority.
use crate::{portfolio::AllocationTargetV1, DbCounter, DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeBarSelectionV1 {
    pub schema_version: SchemaV1,
    /// Complete native BarType strings, never SQL or directory prefixes.
    #[schema(min_items = 1, max_items = 256)]
    pub bar_types: Vec<String>,
    pub event_start_ns: DbCounter,
    /// Exclusive event-time boundary.
    pub event_end_ns: DbCounter,
    pub decision_cutoff_ns: DbCounter,
    #[schema(minimum = 1, maximum = 1000000)]
    pub maximum_rows: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeForecastParametersV1 {
    pub schema_version: SchemaV1,
    #[schema(minimum = 1, maximum = 10000)]
    pub fast_period: u32,
    #[schema(minimum = 2, maximum = 10000)]
    pub slow_period: u32,
    #[schema(minimum = 1, maximum = 100000)]
    pub label_horizon_observations: u32,
    pub total_fuel: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeForecastRequestV1 {
    pub schema_version: SchemaV1,
    pub selection: NativeBarSelectionV1,
    pub parameters: NativeForecastParametersV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForecastMissingReason {
    IndicatorWarmup,
    LabelNotComplete,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeForecastPointV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    #[schema(minimum = 0, maximum = 999999)]
    pub ordinal: u32,
    pub event_ns: DbCounter,
    pub available_ns: DbCounter,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub forecast: Option<f64>,
    pub forecast_reason: Option<ForecastMissingReason>,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub label_return: Option<f64>,
    pub label_available_ns: Option<DbCounter>,
    pub label_reason: Option<ForecastMissingReason>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeForecastResultV1 {
    pub schema_version: SchemaV1,
    pub native_versions: BTreeMap<String, String>,
    pub consumed_fuel: DbCounter,
    #[schema(min_items = 1, max_items = 1000000)]
    pub points: Vec<NativeForecastPointV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeAccountKind {
    Cash,
    Margin,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFeeRateV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    pub maker: DecimalValue,
    pub taker: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSimulationSettingsV1 {
    pub schema_version: SchemaV1,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub base_currency: String,
    pub starting_capital: DecimalValue,
    pub account_kind: NativeAccountKind,
    pub leverage: DecimalValue,
    pub insert_latency_ns: DbCounter,
    #[schema(minimum = 1, maximum = 86400000)]
    pub snapshot_interval_ms: u32,
    pub exposure_tolerance: DecimalValue,
    #[schema(min_items = 1, max_items = 256)]
    pub fee_rates: Vec<NativeFeeRateV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTargetPointV1 {
    pub schema_version: SchemaV1,
    pub asof_ns: DbCounter,
    pub valid_until_ns: DbCounter,
    #[schema(min_items = 1, max_items = 256)]
    pub targets: Vec<AllocationTargetV1>,
    pub cash_weight: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSimulationRequestV1 {
    pub schema_version: SchemaV1,
    pub selection: NativeBarSelectionV1,
    pub settings: NativeSimulationSettingsV1,
    #[schema(min_items = 1, max_items = 10000)]
    pub target_points: Vec<NativeTargetPointV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeStatisticGroup {
    Pnl,
    Returns,
    General,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeStatisticV1 {
    pub group: NativeStatisticGroup,
    pub native_key: String,
    pub currency: Option<String>,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReturnV1 {
    pub timestamp_ns: DbCounter,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub reason_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeReturnsKind {
    PortfolioDaily,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSimulationResultV1 {
    pub schema_version: SchemaV1,
    pub native_version: String,
    pub iterations: DbCounter,
    pub events: DbCounter,
    pub orders: DbCounter,
    pub positions: DbCounter,
    pub consumed_target_points: DbCounter,
    pub summary: BTreeMap<String, String>,
    pub statistics: Vec<NativeStatisticV1>,
    /// UTC daily equity changes from native snapshots, never per-position fallback.
    pub returns_kind: NativeReturnsKind,
    pub returns_status: crate::evidence::MetricStatus,
    pub returns_reason: Option<String>,
    pub returns: Vec<NativeReturnV1>,
    /// Unmodified native canonical document; stored as restricted simulation evidence.
    pub canonical_result: serde_json::Value,
}
