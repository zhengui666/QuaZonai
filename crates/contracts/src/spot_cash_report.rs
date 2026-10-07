//! Separate report-currency evidence, never a replacement for native canonical data.
use crate::{
    DbCounter, DecimalValue, SchemaV1,
    science::NativeStatisticV1,
    spot_cash::{
        NativeSpotBarEventTimeV1, NativeSpotCashFlowEvidenceV1, NativeSpotCashMoneyV1,
        NativeSpotCashSessionV1, NativeSpotInstrumentV1, NativeSpotSnapshotOriginV1,
        NativeSpotValuationFrameV1, ReportCurrencyDailyReturnsV1, ReportCurrencyValuationV1,
    },
    spot_fees::FrozenSpotFeeScheduleV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub use crate::spot_fees::SpotCashFeeAcceptanceV1;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashReportV1 {
    pub schema_version: SchemaV1,
    pub session: NativeSpotCashSessionV1,
    /// Original serialization of the official completed BacktestResult. Its
    /// instance/run UUIDs are not present in deterministic canonical_result.
    pub native_run_receipt: serde_json::Value,
    pub instruments: Vec<NativeSpotInstrumentV1>,
    /// Complete ordered observer tape, including official decision-time snapshots.
    pub observations: Vec<NativeSpotCashObservationV1>,
    pub flow_evidence: NativeSpotCashFlowEvidenceV1,
    pub daily_returns: ReportCurrencyDailyReturnsV1,
    /// Official statistics evaluated only on complete report-currency daily returns.
    /// They are separate from the engine's native-currency statistics.
    pub statistics: Vec<NativeStatisticV1>,
    pub fee_schedule: FrozenSpotFeeScheduleV1,
    pub fee_acceptance: SpotCashFeeAcceptanceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_evidence: Option<crate::spot_cash_source::NativeSpotCashSourceEvidenceV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeSpotCashObservationV1 {
    pub sequence: DbCounter,
    pub native_clock_ns: DbCounter,
    #[serde(flatten)]
    pub record: NativeSpotCashObservationKindV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum NativeSpotCashObservationKindV1 {
    Bar {
        event_time: NativeSpotBarEventTimeV1,
        /// Original serialization of the complete official Bar, including precision.
        native: serde_json::Value,
        source_row_key: String,
        bar_open_ns: DbCounter,
        bar_close_ns: DbCounter,
    },
    Snapshot {
        origin: NativeSpotSnapshotOriginV1,
        /// Original complete PortfolioSnapshot, with its actual native event UUID.
        native: serde_json::Value,
        frame: NativeSpotValuationFrameV1,
        valuation: ReportCurrencyValuationV1,
    },
}

/// Read-only report projection. Balance mark changes are not native account PnL.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashSummaryV1 {
    pub report_currency: String,
    pub valuation_basis: String,
    pub valuation_change: Option<DecimalValue>,
    pub valuation_change_reason: Option<String>,
    /// All original native fee currencies; never aggregated at an assumed FX rate.
    pub commissions_by_currency: Vec<NativeSpotCashMoneyV1>,
    pub commissions_reason: Option<String>,
    pub native_pnl_by_currency: Vec<NativeStatisticV1>,
    pub fee_acceptance: SpotCashFeeAcceptanceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_evidence: Option<crate::spot_cash_source::NativeSpotCashSourceEvidenceV1>,
}
