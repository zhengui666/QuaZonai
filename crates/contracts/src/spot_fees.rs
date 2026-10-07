//! Explicit research fee inputs. A frozen schedule is not account fee evidence.
use crate::{DbCounter, DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotFeeOrderSideV1 {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotFeeLiquidityV1 {
    Maker,
    Taker,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotFeeBasisV1 {
    BaseQuantity,
    QuoteNotional,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotFeeCurrencyV1 {
    Base,
    Quote,
}

/// Exact decimal arithmetic, rounded once per fill before native Money construction.
/// This is an explicit research convention, not a verified exchange rounding rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotFeeRoundingV1 {
    HalfEvenCurrencyPrecision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotFeeEvidenceStatusV1 {
    /// Reserved: current public rates alone do not prove conservative window coverage.
    ConservativeAssumption,
    /// Current public-rate research scenario only. Applicable-window coverage is unverified.
    /// This fee-assumption state does not change the origin of underlying market data.
    PublicRateScenarioUnverifiedApplicability,
    /// Declared hypothetical/fixture fees; separate from the market data origin.
    Synthetic,
    /// Reserved: cannot be admitted until an independent source verifier exists.
    DataBacked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenSpotFeeSourceV1 {
    pub status: SpotFeeEvidenceStatusV1,
    /// Source/capture identity for traceability, not proof of applicable-window coverage.
    /// A settings copy's artifact ID does not establish independent fee evidence.
    #[schema(min_length = 1, max_length = 200)]
    pub source_ref: String,
    #[schema(min_length = 1, max_length = 500)]
    pub source_uri: String,
    pub observed_at_ns: DbCounter,
    /// State unverified tier, discounts, fee-currency and rounding assumptions.
    #[schema(min_length = 1, max_length = 2000)]
    pub assumptions: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenSpotFeeRuleV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    pub order_side: SpotFeeOrderSideV1,
    pub liquidity: SpotFeeLiquidityV1,
    /// Signed fraction, not percentage points: 0.0007 means 0.070%.
    pub rate: DecimalValue,
    pub basis: SpotFeeBasisV1,
    pub fee_currency: SpotFeeCurrencyV1,
    /// Bind to the untouched native currency definition, including precision.
    #[schema(min_length = 1, max_length = 40)]
    pub currency_code: String,
    pub currency_precision: u8,
    pub rounding: SpotFeeRoundingV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenSpotFeeScheduleV1 {
    pub schema_version: SchemaV1,
    pub source: FrozenSpotFeeSourceV1,
    /// Declared half-open scenario window; source applicability is NOT verified.
    /// Containment here is never evidence of historical, present or future fees.
    pub valid_from_ns: DbCounter,
    pub valid_until_ns: DbCounter,
    /// Exactly four rules per admitted instrument: Buy/Sell x Maker/Taker.
    #[schema(min_items = 4, max_items = 1024)]
    pub rules: Vec<FrozenSpotFeeRuleV1>,
}

/// Explicit frozen research-policy acceptance, never an applicability assertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotCashFeeAcceptanceV1 {
    PublicRateScenarioUnverifiedApplicability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenSpotFeeScenarioParametersV1 {
    pub acceptance: SpotCashFeeAcceptanceV1,
    pub schedule: FrozenSpotFeeScheduleV1,
}
