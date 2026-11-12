//! Walk-forward orchestration of existing native backtests, not a fitted model or qualification.
use crate::{
    native_strategy::{NativeStrategyBacktestRequestV1, NativeStrategyBacktestResultV1},
    DbCounter, SchemaV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeStrategyWalkForwardRequestV1 {
    pub schema_version: SchemaV1,
    pub candidates: Vec<NativeStrategyBacktestRequestV1>,
    pub split: NativeWalkForwardSplitV1,
    pub objective: NativeWalkForwardObjectiveV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeWalkForwardSplitV1 {
    pub n_splits: usize,
    pub test_size: Option<usize>,
    pub gap: usize,
    pub max_train_size: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeWalkForwardObjectiveV1 {
    pub native_key: String,
    #[serde(default)]
    pub metric_source: NativeMetricSourceV1,
    pub annualization_days: Option<usize>,
    pub currency: Option<String>,
    pub direction: NativeObjectiveDirectionV1,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeObjectiveDirectionV1 {
    Maximize,
    Minimize,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeWalkForwardTrialV1 {
    pub candidate_index: usize,
    pub metric_value: Option<f64>,
    pub error: Option<String>,
    pub result: Option<NativeStrategyBacktestResultV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeWalkForwardFoldV1 {
    pub index: usize,
    /// Inclusive availability clocks; same-clock source records are never split.
    pub train_start_ns: DbCounter,
    pub train_end_ns: DbCounter,
    pub test_start_ns: DbCounter,
    pub test_end_ns: DbCounter,
    pub trials: Vec<NativeWalkForwardTrialV1>,
    pub selected_candidate_index: Option<usize>,
    pub test_result: Option<NativeStrategyBacktestResultV1>,
    pub status: String,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeStrategyWalkForwardResultV1 {
    pub schema_version: SchemaV1,
    pub request: NativeStrategyWalkForwardRequestV1,
    pub native_version: String,
    pub splitter_version: String,
    pub performance_scope: String,
    pub notes: Vec<String>,
    pub folds: Vec<NativeWalkForwardFoldV1>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeMetricSourceV1 {
    #[default]
    Canonical,
    EquityPerformance,
}
