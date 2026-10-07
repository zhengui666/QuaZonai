//! Explicit opt-in contracts for cash inventory and separate report valuation.
//!
//! The mode is optional on existing simulation settings. Native results and
//! native snapshots remain unchanged; report valuation is a separate artifact.
use crate::{DbCounter, DecimalValue, Id, SchemaV1, science::NativeAccountKind};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use utoipa::ToSchema;
use uuid::{Uuid, Variant, Version};

/// An identity copied from the official engine, never a generated QZ Id (UUIDv7).
/// Deliberately has no constructor that generates or reserves an identity.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema,
)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, format = Uuid, pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$")]
pub struct NativeSpotRuntimeIdV1(Uuid);

impl TryFrom<Uuid> for NativeSpotRuntimeIdV1 {
    type Error = String;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        if value.get_version() != Some(Version::Random) || value.get_variant() != Variant::RFC4122 {
            return Err("expected an official native UUIDv4 identity".into());
        }
        Ok(Self(value))
    }
}

impl FromStr for NativeSpotRuntimeIdV1 {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let value = Uuid::parse_str(text).map_err(|_| "invalid UUID".to_owned())?;
        if !value.hyphenated().to_string().eq_ignore_ascii_case(text) {
            return Err("expected a canonical native UUIDv4 identity".into());
        }
        value.try_into()
    }
}

impl TryFrom<String> for NativeSpotRuntimeIdV1 {
    type Error = String;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<NativeSpotRuntimeIdV1> for String {
    fn from(value: NativeSpotRuntimeIdV1) -> Self {
        value.0.to_string()
    }
}

impl fmt::Display for NativeSpotRuntimeIdV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

// An omitted native base-currency field is not evidence of explicit native null.
fn required_native_base_currency<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

fn required_external_flow_count<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<DbCounter>, D::Error> {
    Option::<DbCounter>::deserialize(deserializer)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeSpotCashModeV1 {
    MultiCurrencyCash,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashPolicyV1 {
    pub schema_version: SchemaV1,
    pub mode: NativeSpotCashModeV1,
    pub report_currency: String,
    pub price_method: NativeSpotPriceMethodV1,
    pub allowed_instrument_ids: Vec<String>,
    pub returns_policy: ReportCurrencyDailyPolicyV1,
    /// Native intraday timers emit only while positions are open; never infer a
    /// complete intraday grid from snapshot_interval_ms.
    pub daily_sampling: NativeSpotDailySamplingV1,
    /// Frozen maximum age. Never use a current/live price for historical equity.
    pub maximum_price_age_ns: DbCounter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeSpotPriceMethodV1 {
    ClosedBarClose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportCurrencyDailyPolicyV1 {
    FreshSimulationNoExternalFlows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeSpotDailySamplingV1 {
    /// First native publication at each UTC midnight; later same-clock events
    /// belong to the next half-open day. Decision builds never replace boundaries.
    NativeUtcMidnightBoundaries,
}

/// Output of the pure cash-account planning rule, not an engine configuration.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashAccountPlanV1 {
    pub account_kind: NativeAccountKind,
    /// Must be None when constructing the official multi-currency CashAccount.
    #[serde(deserialize_with = "required_native_base_currency")]
    #[schema(required = true)]
    pub native_base_currency: Option<String>,
    pub allow_cash_borrowing: bool,
    pub leverage: DecimalValue,
    pub starting_balances: Vec<NativeSpotCashMoneyV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashMoneyV1 {
    pub currency: String,
    pub amount: DecimalValue,
}

/// Execution-local facts for one fresh, exclusively owned observer session.
/// The native run identity is available only in its eventual completion receipt.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashSessionV1 {
    /// QZ observer session UUIDv7; not the official native run or instance ID.
    pub session_id: Id,
    pub native_instance_id: NativeSpotRuntimeIdV1,
    pub dataset_revision_id: Id,
    pub account_id: String,
    pub venue: String,
    pub period_start_ns: DbCounter,
    pub period_end_ns: DbCounter,
}

/// Narrow projection of an authoritative native instrument, never a symbol guess.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotInstrumentV1 {
    pub instrument_id: String,
    pub venue: String,
    /// Adapter must copy the native enum variant; only CurrencyPair is admitted.
    pub native_kind: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub is_inverse: bool,
    pub has_expiration: bool,
    pub multiplier: DecimalValue,
}

/// A row from one unchanged native account snapshot. Total already includes locked.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashBalanceV1 {
    pub currency: String,
    pub total: DecimalValue,
    pub free: DecimalValue,
    pub locked: DecimalValue,
}

/// Actual native observation path. Both variants retain the untouched snapshot.
/// The source adapter states which original native event clock the Bar uses.
/// Its complete Bar stays unchanged; valuation uses the source-bound close.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeSpotBarEventTimeV1 {
    Open,
    CloseExclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeSpotSnapshotOriginV1 {
    NativePublication,
    DecisionBuild,
}

/// Identity of the original snapshot, retained in every valuation observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotSnapshotBindingV1 {
    pub origin: NativeSpotSnapshotOriginV1,
    pub native_instance_id: NativeSpotRuntimeIdV1,
    /// Copy snapshot.event_id directly from the official PortfolioSnapshot.
    pub event_id: NativeSpotRuntimeIdV1,
    pub dataset_revision_id: Id,
    /// Global replay sequence at capture, in the same ordering as price observations.
    pub snapshot_sequence: DbCounter,
    pub session_id: Id,
    pub account_id: String,
    pub venue: String,
    pub asof_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashSnapshotV1 {
    pub binding: NativeSpotSnapshotBindingV1,
    pub account_kind: NativeAccountKind,
    #[serde(deserialize_with = "required_native_base_currency")]
    #[schema(required = true)]
    pub native_base_currency: Option<String>,
    /// Adapter must establish that every native currency row was retained.
    pub balances_complete: bool,
    pub balances: Vec<NativeSpotCashBalanceV1>,
}

/// Exact source binding and both clocks for a known direct spot close price.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotPriceV1 {
    pub session_id: Id,
    pub native_instance_id: NativeSpotRuntimeIdV1,
    pub dataset_revision_id: Id,
    /// Exact row in the frozen dataset revision. The adapter must establish
    /// native Bar equality with that row; a supplied key alone proves nothing.
    pub source_row_key: String,
    pub instrument_id: String,
    pub method: NativeSpotPriceMethodV1,
    /// Global replay ordering within this session, copied by the native adapter.
    pub observed_sequence: DbCounter,
    pub bar_open_ns: DbCounter,
    /// Closed-bar end, not the candle's opening timestamp.
    pub event_ns: DbCounter,
    pub available_ns: DbCounter,
    pub price: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotValuationFrameV1 {
    pub snapshot: NativeSpotCashSnapshotV1,
    /// At most one already selected as-of price per instrument. The adapter must
    /// select from the frozen catalog, not accept arbitrary user-asserted marks.
    pub prices: Vec<NativeSpotPriceV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportValuationFailureV1 {
    InvalidSnapshotBinding,
    UnsupportedAccount,
    IncompleteBalances,
    InvalidBalance,
    DuplicateCurrency,
    UnknownCurrency,
    InvalidPriceSource,
    UnknownPriceInstrument,
    DuplicatePrice,
    MissingPrice,
    FuturePrice,
    AmbiguousPriceOrder,
    PriceMethodMismatch,
    ReplaySequenceConflict,
    ReplayClockConflict,
    PriceSourceConflict,
    StalePrice,
    NonPositivePrice,
    ArithmeticOutOfRange,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReportCurrencyValuationLegV1 {
    pub currency: String,
    pub native_total: DecimalValue,
    pub report_value: DecimalValue,
    /// None only for the report currency itself or an exactly zero balance.
    pub price: Option<NativeSpotPriceV1>,
}

/// A separate QZ derived artifact. Never embed this in nativeSnapshot or pretend
/// it is the official engine's single-currency portfolio equity.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "status",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum ReportCurrencyValuationOutcomeV1 {
    Complete {
        total: DecimalValue,
        legs: Vec<ReportCurrencyValuationLegV1>,
    },
    /// No subtotal or partially valued legs may accompany an unavailable total.
    Unavailable { reason: ReportValuationFailureV1 },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReportCurrencyValuationV1 {
    pub schema_version: SchemaV1,
    pub snapshot: NativeSpotSnapshotBindingV1,
    pub report_currency: String,
    /// Copied from the same immutable simulation settings used by this context.
    pub policy: NativeSpotCashPolicyV1,
    pub outcome: ReportCurrencyValuationOutcomeV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportCurrencyReturnsKindV1 {
    ReportCurrencyDaily,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportCurrencyReturnFailureV1 {
    SnapshotGap,
    ValuationUnavailable,
    NonPositiveOpeningEquity,
    ArithmeticOutOfRange,
}

/// Runtime facts projected only by the owner of a completed fresh one-shot run.
/// The owner must establish closed engine ownership and inspect the actual run
/// receipt and complete native event stream. A generic snapshot observer cannot
/// produce this evidence. These wire fields bind that proof; they do not
/// authenticate it, and a caller-supplied count is not itself proof of no flows.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashFlowEvidenceV1 {
    /// The actual official run ID from the completed engine receipt, not an ID
    /// reserved before the engine has publicly supplied it.
    pub native_run_id: NativeSpotRuntimeIdV1,
    pub native_instance_id: NativeSpotRuntimeIdV1,
    pub session_id: Id,
    pub dataset_revision_id: Id,
    pub account_id: String,
    pub venue: String,
    pub period_start_ns: DbCounter,
    pub period_end_ns: DbCounter,
    pub opening_snapshot: NativeSpotSnapshotBindingV1,
    pub closing_snapshot: NativeSpotSnapshotBindingV1,
    pub observed_snapshot_count: DbCounter,
    /// Initial fresh capital precedes this window; buys/sells/fees are not flows.
    /// None means unknown. Only the owning runner can establish Some(0).
    #[serde(deserialize_with = "required_external_flow_count")]
    #[schema(required = true)]
    pub external_flow_count: Option<DbCounter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReportCurrencyDailyReturnV1 {
    pub day_start_ns: DbCounter,
    pub day_end_ns: DbCounter,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub reason: Option<ReportCurrencyReturnFailureV1>,
    pub opening_snapshot: Option<NativeSpotSnapshotBindingV1>,
    pub closing_snapshot: Option<NativeSpotSnapshotBindingV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReportCurrencyDailyReturnsV1 {
    pub schema_version: SchemaV1,
    pub kind: ReportCurrencyReturnsKindV1,
    pub report_currency: String,
    pub policy: NativeSpotCashPolicyV1,
    pub annualization_days: u16,
    pub flow_evidence: NativeSpotCashFlowEvidenceV1,
    /// Only complete UTC calendar days inside the requested window. Missing
    /// observations remain None with a reason, never zero or carried forward.
    pub days: Vec<ReportCurrencyDailyReturnV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_native_null_is_required_in_the_generated_schema() {
        use utoipa::PartialSchema;
        for schema in [
            NativeSpotCashAccountPlanV1::schema(),
            NativeSpotCashSnapshotV1::schema(),
        ] {
            let value = serde_json::to_value(schema).unwrap();
            assert!(
                value["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|name| name == "native_base_currency")
            );
        }
    }

    #[test]
    fn unknown_modes_and_partial_totals_are_rejected() {
        assert!(serde_json::from_str::<NativeSpotCashPolicyV1>(
            r#"{"schema_version":1,"mode":"MARGIN","report_currency":"USDC","maximum_price_age_ns":"1"}"#
        ).is_err());
        assert!(
            serde_json::from_str::<ReportCurrencyValuationOutcomeV1>(
                r#"{"status":"UNAVAILABLE","reason":"MISSING_PRICE","total":"100"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn native_runtime_ids_accept_only_canonical_rfc4122_uuid4() {
        let text = "550e8400-e29b-41d4-a716-446655440000";
        let id: NativeSpotRuntimeIdV1 = text.parse().unwrap();
        assert_eq!(id.to_string(), text);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{text}\""));
        assert_eq!(
            NativeSpotRuntimeIdV1::try_from(Uuid::parse_str(text).unwrap()).unwrap(),
            id
        );
        assert!(Id::try_from(text.to_owned()).is_err());
        for invalid in [
            Id::new().to_string(),
            "00000000-0000-0000-0000-000000000000".into(),
            "550e8400-e29b-41d4-7716-446655440000".into(),
            "550e8400e29b41d4a716446655440000".into(),
        ] {
            assert!(invalid.parse::<NativeSpotRuntimeIdV1>().is_err());
            assert!(
                serde_json::from_value::<NativeSpotRuntimeIdV1>(serde_json::json!(invalid))
                    .is_err()
            );
        }
    }

    #[test]
    fn sampling_policy_and_runtime_identity_schemas_are_explicit() {
        use utoipa::PartialSchema;
        let schema = serde_json::to_value(NativeSpotCashPolicyV1::schema()).unwrap();
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "daily_sampling")
        );
        let schema = serde_json::to_value(NativeSpotRuntimeIdV1::schema()).unwrap();
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["format"], "uuid");
        assert!(schema["pattern"].as_str().unwrap().contains("-4["));
        assert_eq!(
            serde_json::to_value(NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries).unwrap(),
            "NATIVE_UTC_MIDNIGHT_BOUNDARIES"
        );
        assert!(
            serde_json::from_value::<NativeSpotDailySamplingV1>(serde_json::json!(
                "SNAPSHOT_INTERVAL_GRID"
            ))
            .is_err()
        );
    }
}
