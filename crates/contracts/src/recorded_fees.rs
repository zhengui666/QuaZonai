//! Transaction-linked cost replay, not an inferred historical fee schedule.
use crate::{DbCounter, DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const POLYMARKET_V2_SOURCE: &str = "TimeSeventeen/Polymarket-v2";
pub const RECORDED_FEES_FILE: &str = "recorded-fees.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedTradeFeeParametersV1 {}

/// Original normalized source values. Null fees cannot deserialize as zero.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedTradeFeeV1 {
    pub id: String,
    pub instrument_id: String,
    pub ts_event_ns: DbCounter,
    /// Source token_amount retained before the existing native amount normalization.
    pub source_quantity: DecimalValue,
    /// Same-ID original native TradeTick quantity, not a newly fitted amount.
    pub quantity: DecimalValue,
    pub fee_usdc: DecimalValue,
    pub usdc_amount: DecimalValue,
    pub price: DecimalValue,
    /// Provider-derived columns retained verbatim, not asserted aggressor roles.
    pub maker_direction: String,
    pub taker_direction: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedTradeFeesV1 {
    pub schema_version: SchemaV1,
    pub source: String,
    pub revision: String,
    /// V2's legacy *_usdc column names do not imply a USDC conversion.
    pub currency: String,
    pub records: Vec<RecordedTradeFeeV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeeAllocationV1 {
    pub original: RecordedTradeFeeV1,
    pub allocated_quantity: DecimalValue,
    /// Exact rational amount before the single per-fill currency rounding.
    pub allocated_fee_numerator: String,
    pub allocated_fee_denominator: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeeFillV1 {
    pub client_order_id: String,
    pub instrument_id: String,
    /// Actual shared native simulation clock at commission calculation.
    pub fill_ns: DbCounter,
    /// Source-cost binding: latest completed one-minute boundary at fill_ns.
    pub bar_end_ns: DbCounter,
    pub order_side: String,
    pub fill_quantity: DecimalValue,
    pub fill_price: DecimalValue,
    /// Rounded once at the native quote-currency precision, then booked natively.
    pub commission: DecimalValue,
    pub allocations: Vec<RecordedFeeAllocationV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFeeReplayV1 {
    pub source: String,
    pub revision: String,
    pub currency: String,
    pub allocation_rule: String,
    pub fills: Vec<RecordedFeeFillV1>,
}
