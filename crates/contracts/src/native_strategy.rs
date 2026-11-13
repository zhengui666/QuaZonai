//! Configuration for compiled Nautilus strategies, including user crates; not a strategy language.
use crate::{catalog_coverage::NativeCatalogSelectorV1, DbCounter, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeStrategyBacktestRequestV1 {
    pub schema_version: SchemaV1,
    pub instrument_id: String,
    pub selectors: Vec<NativeCatalogSelectorV1>,
    /// Half-open source event window; availability is independently bounded.
    pub event_start_ns: DbCounter,
    pub event_end_ns: DbCounter,
    /// Inclusive availability lower bound, independently of the source event window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_start_ns: Option<DbCounter>,
    pub available_end_ns: DbCounter,
    /// Default: definitions available at the historical window start. Static scenarios keep the
    /// original definition timestamps and explicitly assume its rules across the whole window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_definition: Option<NativeInstrumentDefinitionV1>,
    pub venue: NativeStrategyVenueV1,
    pub strategy: NativeCompiledStrategyV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeStrategyVenueV1 {
    pub account_type: String,
    pub oms_type: String,
    pub book_type: String,
    /// Official SimulatedVenueConfig option. None retains the upstream default (true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_execution: Option<bool>,
    pub starting_balances: Vec<String>,
    pub base_currency: Option<String>,
    pub leverage: String,
    /// Explicit simulation assumptions, never inferred historical venue fees.
    pub maker_fee: String,
    pub taker_fee: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "mode", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum NativeInstrumentDefinitionV1 {
    HistoricalAsOfStart,
    StaticScenario {
        ts_init_ns: DbCounter,
        ts_event_ns: DbCounter,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeInstrumentDefinitionBasisV1 {
    pub mode: String,
    pub ts_init_ns: DbCounter,
    pub ts_event_ns: DbCounter,
    pub scope: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum NativeCompiledStrategyV1 {
    /// User crate statically linked into the runner; configuration is decoded by that crate.
    Registered {
        name: String,
        config: serde_json::Value,
    },
    EmaCross {
        /// Original official StrategyConfig JSON, decoded directly upstream.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<serde_json::Value>,
        trade_size: String,
        fast_period: usize,
        slow_period: usize,
    },
    /// Compiled official GridMarketMaker. All strategy-specific fields map directly upstream.
    GridMarketMaker {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<serde_json::Value>,
        trade_size: Option<String>,
        num_levels: usize,
        grid_step_bps: u32,
        skew_factor: f64,
        max_position: String,
        requote_threshold_bps: u32,
        expire_time_secs: Option<u64>,
        on_cancel_resubmit: bool,
    },
    /// Compiled official HurstVpinDirectional; original BAR/quote/trade semantics.
    HurstVpinDirectional {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<serde_json::Value>,
        bar_type: String,
        trade_size: String,
        hurst_window: usize,
        hurst_lags: Vec<usize>,
        hurst_enter: f64,
        hurst_exit: f64,
        vpin_window: usize,
        vpin_threshold: f64,
        max_holding_secs: u64,
    },
    OrderBookImbalance {
        /// Original official StrategyConfig JSON, decoded directly upstream.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<serde_json::Value>,
        max_trade_size: String,
        trigger_min_size: String,
        trigger_imbalance_ratio: String,
        min_seconds_between_triggers: f64,
        time_in_force: String,
        dry_run: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeStrategyBacktestResultV1 {
    pub schema_version: SchemaV1,
    pub request: NativeStrategyBacktestRequestV1,
    pub native_version: String,
    pub instrument_definition: NativeInstrumentDefinitionBasisV1,
    pub canonical_result: serde_json::Value,
    pub equity_performance: NativeEquityPerformanceV1,
    pub data_rows_by_type: std::collections::BTreeMap<String, DbCounter>,
    pub notes: Vec<String>,
}

/// Official snapshot-only analysis, separate from canonical position-return fallback.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeEquityPerformanceV1 {
    pub source: String,
    pub status: String,
    pub reason: Option<String>,
    pub currency: Option<String>,
    pub returns: Vec<NativeEquityReturnV1>,
    pub statistics: Vec<NativeEquityStatisticV1>,
    pub first_snapshot_ns: Option<String>,
    pub last_snapshot_ns: Option<String>,
    pub snapshot_count: String,
    pub observed_utc_days: String,
    pub unpriced_snapshot_count: String,
    pub return_days: String,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeEquityReturnV1 {
    pub ts_event_ns: String,
    pub value: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeEquityStatisticV1 {
    pub native_key: String,
    pub annualization_days: usize,
    pub value: Option<f64>,
}

/// Configuration owned by a statically linked user crate; not a QZ mirror of Nautilus builders.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeConfiguredBacktestRequestV1 {
    pub schema_version: SchemaV1,
    pub name: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NativeConfiguredBacktestResultV1 {
    pub schema_version: SchemaV1,
    pub request: NativeConfiguredBacktestRequestV1,
    pub native_version: String,
    /// Original upstream BacktestResult serialization; no QZ ranking or aggregation.
    pub native_results: serde_json::Value,
    /// Present only if the user's official node configuration retains the engine.
    pub canonical_result: Option<serde_json::Value>,
    pub canonical_unavailable_reason: Option<String>,
    /// Official snapshot-only account-equity statistics, never position-return fallback.
    /// Absent when the configured run does not retain a completed native engine.
    pub equity_performance: Option<NativeEquityPerformanceV1>,
    pub equity_performance_unavailable_reason: Option<String>,
}
