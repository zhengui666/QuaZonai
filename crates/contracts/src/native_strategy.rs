//! Explicit configuration for compiled upstream strategies, not a strategy language.
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
    pub data_rows_by_type: std::collections::BTreeMap<String, DbCounter>,
    pub notes: Vec<String>,
}
