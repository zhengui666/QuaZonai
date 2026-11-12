//! Explicit native indicator strategies. No model artifact, DSL or live-trading authority.
use crate::{
    science::{NativeBarSelectionV1, NativeSimulationSettingsV1},
    DbCounter, DecimalValue, SchemaV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum NativeIndicatorSignalV1 {
    /// Long while the fast EMA is strictly above the slow EMA; otherwise cash.
    EmaCross { fast_period: u32, slow_period: u32 },
    /// Enter below entry_below, exit above exit_above; retain state between thresholds.
    /// Nautilus RSI uses Wilder smoothing and the native 0..100 scale.
    Rsi {
        period: u32,
        entry_below: f64,
        exit_above: f64,
    },
    /// Long while MACD is strictly above its signal line; otherwise cash.
    Macd {
        fast_period: u32,
        slow_period: u32,
        signal_period: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeIndicatorStrategyRequestV1 {
    pub schema_version: SchemaV1,
    /// Exactly one BAR series. All selected data contributes to full-selection performance.
    /// Before decision_start_ns the account stays in cash while indicators warm up.
    pub selection: NativeBarSelectionV1,
    pub settings: NativeSimulationSettingsV1,
    pub signal: NativeIndicatorSignalV1,
    pub decision_start_ns: DbCounter,
    /// Long allocation in [0, 1], rebalanced on each ready availability-time batch
    /// at/after decision_start_ns only when prior native orders/targets have settled.
    /// The remainder remains cash. No leverage or shorting.
    pub long_weight: DecimalValue,
    pub target_ttl_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observations: Option<NativeIndicatorObservationsConfigV1>,
}

/// Optional observations only; none of these indicators invents an additional trade rule.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeIndicatorObservationsConfigV1 {
    /// Wilder ATR with previous close, no value floor. Periods count selected BARs.
    pub atr_period: u32,
    /// Native ratio %D, no %K slowing; periods count selected BARs.
    pub stochastics_k_period: u32,
    pub stochastics_d_period: u32,
    pub donchian_period: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeIndicatorObservationV1 {
    /// Runtime BAR event clock (INTERNAL source bars are projected to close).
    pub event_ns: DbCounter,
    pub available_ns: DbCounter,
    /// None means the native indicator is not initialized, never a substituted zero.
    pub atr: Option<f64>,
    /// Cumulative typical-price VWAP since selection start; NOT session/daily VWAP.
    /// Native zero-volume rows do not initialize it or add volume.
    pub vwap: Option<f64>,
    pub stochastics_k: Option<f64>,
    pub stochastics_d: Option<f64>,
    pub donchian_upper: Option<f64>,
    pub donchian_middle: Option<f64>,
    pub donchian_lower: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeIndicatorStrategyResultV1 {
    /// Complete selection, including pre-decision cash periods, not decision-only performance.
    pub performance_scope: String,
    /// One decision at the end of each same-ts_init batch, only with a settled account.
    pub decision_policy: String,
    pub skipped_unsettled_decisions: DbCounter,
    pub merged_same_time_bars: DbCounter,
    pub request: NativeIndicatorStrategyRequestV1,
    pub simulation: crate::science::NativeSimulationResultV1,
    pub observations: Vec<NativeIndicatorObservationV1>,
}

/// One explicit native invocation. Every request must share selection and settings;
/// signals/observations may vary. A failure fails the whole batch, not a partial success.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeIndicatorStrategyBatchRequestV1 {
    pub schema_version: SchemaV1,
    pub requests: Vec<NativeIndicatorStrategyRequestV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeIndicatorStrategyBatchResultV1 {
    pub schema_version: SchemaV1,
    pub native_version: String,
    pub reuse: String,
    /// Same order as requests; no independently invented run or task identity.
    pub results: Vec<NativeIndicatorStrategyResultV1>,
}
