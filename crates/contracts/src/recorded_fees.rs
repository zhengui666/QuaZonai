//! Transaction-linked cost replay, not an inferred historical fee schedule.
use crate::{DbCounter, DecimalValue, SchemaV1};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use utoipa::ToSchema;

/// Exact original price/notional/fee text, before native account rounding.
/// Source decimals need not fit the accounting NUMERIC(38,18) contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
pub struct RecordedSourceDecimal(String);

impl FromStr for RecordedSourceDecimal {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty()
            || text
                .bytes()
                .any(|byte| !byte.is_ascii_digit() && !matches!(byte, b'.' | b'-' | b'+'))
        {
            return Err("expected a finite plain source decimal string".into());
        }
        BigDecimal::from_str(text).map_err(|_| "invalid source decimal")?;
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for RecordedSourceDecimal {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<RecordedSourceDecimal> for String {
    fn from(value: RecordedSourceDecimal) -> Self {
        value.0
    }
}

impl RecordedSourceDecimal {
    pub fn is_positive(&self) -> bool {
        BigDecimal::from_str(&self.0).expect("validated source decimal") > BigDecimal::from(0)
    }

    pub fn to_decimal(&self) -> BigDecimal {
        BigDecimal::from_str(&self.0).expect("validated source decimal")
    }
}

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
    /// Exact original source fee, before proportional allocation and fill rounding.
    pub fee_usdc: RecordedSourceDecimal,
    pub usdc_amount: RecordedSourceDecimal,
    pub price: RecordedSourceDecimal,
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
