//! Display-only projections of adopted simulated equity, never account accounting.
use crate::{research::DataOrigin, DbCounter, DecimalValue, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

pub const MAX_EQUITY_POINTS: usize = 10_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EquityResolution {
    #[default]
    Auto,
    Native,
    Day,
    Week,
    Month,
}

/// Inclusive UTC nanosecond bounds; absent bounds select the complete native run.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct EquityCurveQuery {
    pub start_ns: Option<DbCounter>,
    pub end_ns: Option<DbCounter>,
    #[serde(default)]
    pub resolution: EquityResolution,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EquityPointV1 {
    pub timestamp_ns: DbCounter,
    pub value: Option<DecimalValue>,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EquitySeriesV1 {
    pub native_version: String,
    pub base_currency: String,
    pub starting_capital: DecimalValue,
    pub period_start_ns: DbCounter,
    pub period_end_ns: DbCounter,
    pub source_point_count: DbCounter,
    pub window_point_count: DbCounter,
    /// Actual resolution, never AUTO. Bucket ends retain their native timestamp.
    pub resolution: EquityResolution,
    pub sampled: bool,
    #[schema(max_items = 10000)]
    pub points: Vec<EquityPointV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EquityUnavailableReason {
    SimulationFailed,
    InvalidEvidence,
    NoSimulation,
    LegacySnapshotsUnavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum EquityCurveDataV1 {
    Ready {
        source_artifact_id: Id,
        series: EquitySeriesV1,
    },
    Unavailable {
        reason_code: EquityUnavailableReason,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EquityCurveV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub candidate_id: Id,
    pub evaluation_id: Id,
    pub run_id: Id,
    pub origin: DataOrigin,
    pub curve: EquityCurveDataV1,
}
