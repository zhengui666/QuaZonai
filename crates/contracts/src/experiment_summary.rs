//! Read-only views of an adopted independent frozen-policy replay.
use crate::{
    equity_curve::{EquityPointV1, EquityResolution},
    evidence::MetricStatus,
    science::NativeAccountKind,
    DbCounter, DecimalValue, Id, SchemaV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

pub const MAX_EXPERIMENT_PREVIEW_POINTS: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentSummaryV1 {
    pub schema_version: SchemaV1,
    pub experiment_id: Id,
    pub run_id: Id,
    pub report_artifact_id: Id,
    pub dataset_revision_id: Id,
    pub model_artifact_id: Id,
    #[schema(min_items = 1)]
    pub feature_artifact_ids: Vec<Id>,
    pub native_versions: BTreeMap<String, String>,
    pub instrument_id: String,
    /// Independent frozen-policy replay; no fitted-model, external-training PIT,
    /// Alpha qualification or investment-performance claim follows from it.
    pub interpretation: String,
    /// Each fold starts with fresh capital. No aggregate/average Sharpe or joined equity.
    #[schema(min_items = 1)]
    pub folds: Vec<ExperimentFoldSummaryV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentFoldSummaryV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spot_cash: Option<crate::spot_cash_report::NativeSpotCashSummaryV1>,
    pub fold_index: u16,
    pub native_version: String,
    #[schema(min_length = 1, max_length = 200)]
    pub account_id: String,
    pub account_kind: NativeAccountKind,
    pub base_currency: String,
    pub starting_capital: DecimalValue,
    pub period_start_ns: DbCounter,
    pub period_end_ns: DbCounter,
    pub decision_count: DbCounter,
    pub orders: DbCounter,
    pub positions: DbCounter,
    pub daily_return_count: DbCounter,
    pub return_frequency: String,
    /// Native account PnL (total), including the engine's unrealized-PnL argument.
    /// Never recomputed from preview points or relabeled as economic net profit.
    pub native_account_pnl: ExperimentSummaryStatisticV1,
    /// Native UTC daily portfolio returns, zero risk-free rate, frozen annualization.
    pub sharpe_ratio: ExperimentSummaryStatisticV1,
    /// Native UTC daily return drawdown, negative fraction: -0.20 means 20%.
    pub max_drawdown: ExperimentSummaryStatisticV1,
    /// Exact native account commissions in base currency; absent is unknown, not zero.
    pub commissions: Option<DecimalValue>,
    pub commissions_reason: Option<String>,
    pub equity_preview: ExperimentEquityPreviewV1,
}

/// Shared-account native replay projection, also used by experiment fold summaries.
/// This view selects existing statistics and observations; it does not rerun science.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSimulationSummaryV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spot_cash: Option<crate::spot_cash_report::NativeSpotCashSummaryV1>,
    pub native_version: String,
    #[schema(min_length = 1, max_length = 200)]
    pub account_id: String,
    pub account_kind: NativeAccountKind,
    pub base_currency: String,
    pub starting_capital: DecimalValue,
    pub period_start_ns: DbCounter,
    pub period_end_ns: DbCounter,
    pub decision_count: DbCounter,
    pub orders: DbCounter,
    pub positions: DbCounter,
    pub daily_return_count: DbCounter,
    pub return_frequency: String,
    /// Native account PnL (total), including the engine's unrealized-PnL argument.
    /// Never recomputed from preview points or relabeled as economic net profit.
    pub native_account_pnl: ExperimentSummaryStatisticV1,
    /// Native UTC daily portfolio returns, zero risk-free rate, frozen annualization.
    pub sharpe_ratio: ExperimentSummaryStatisticV1,
    /// Native UTC daily return drawdown, negative fraction: -0.20 means 20%.
    pub max_drawdown: ExperimentSummaryStatisticV1,
    /// Exact native account commissions in base currency; absent is unknown, not zero.
    pub commissions: Option<DecimalValue>,
    pub commissions_reason: Option<String>,
    pub equity_preview: ExperimentEquityPreviewV1,
}

/// A direct selection from native statistics, with original missingness retained.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentSummaryStatisticV1 {
    pub native_key: String,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub status: MetricStatus,
    pub reason_code: Option<String>,
    pub unit: String,
    pub scope: String,
    pub method_id: String,
    pub annualization_days: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentEquityPreviewV1 {
    pub source_point_count: DbCounter,
    pub distinct_point_count: DbCounter,
    /// Native, or the existing UTC bucket-end projection before preview selection.
    pub source_resolution: EquityResolution,
    pub sampled: bool,
    /// Display only: first/last and uniformly spaced indices of the projected series.
    /// No interpolated values/timestamps; never use preview points to estimate metrics.
    pub sampling_method: String,
    pub points: Vec<EquityPointV1>,
}
