//! Measured native replays at automatically selected capital points.
//! A tested point is evidence, never an interval or an economic capacity claim.
use crate::{
    science::{NativeSimulationRequestV1, NativeSimulationResultV1},
    DbCounter, DecimalValue, Id, SchemaV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalBasisV1 {
    pub currency: String,
    pub equity: DecimalValue,
    pub reserved: DecimalValue,
    pub owner_liquidity_reserve: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalCurveBudgetV1 {
    /// Host resource allowance, not a user-authored capital grid.
    #[schema(minimum = 1, maximum = 64)]
    pub maximum_simulations: u32,
    /// Monotonic elapsed-time admission limit checked before each replay.
    /// This does not cancel an in-flight replay. The official task/process owner
    /// retains cancellation authority; no implicit process deadline is installed.
    pub maximum_elapsed_ms: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalCurveRequestV1 {
    pub schema_version: SchemaV1,
    pub simulation: NativeSimulationRequestV1,
    pub capital_basis: NativeCapitalBasisV1,
    pub budget: NativeCapitalCurveBudgetV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalMinimumSourceV1 {
    pub instrument_id: String,
    pub currency: String,
    pub decision_ns: DbCounter,
    pub bar_event_ns: DbCounter,
    pub bar_available_ns: DbCounter,
    pub instrument_available_ns: DbCounter,
    pub minimum_notional: DecimalValue,
    pub entry_weight: DecimalValue,
    /// A candidate from original first-entry lot/notional rules and absolute
    /// exposure (including supported margin shorts), excluding fees
    /// and later holding drift. Native replay determines what actually happens.
    pub candidate_capital: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalCurveGenerationV1 {
    pub method: String,
    pub minimum_candidate_capital: Option<DecimalValue>,
    pub minimum_reason_code: Option<String>,
    pub minimum_sources: Vec<NativeCapitalMinimumSourceV1>,
    /// Numeric exploration anchor, NOT a tested amount or feasible lower bound.
    /// Zero is used when first-entry sizing cannot identify a funded lower anchor.
    pub lower_planning_anchor: DecimalValue,
    pub planned_capitals: Vec<DecimalValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCapitalPointKindV1 {
    TestedPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCapitalReplayStatusV1 {
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalCurvePointV1 {
    pub capital: DecimalValue,
    pub kind: NativeCapitalPointKindV1,
    /// Both equal capital. No interpolation or monotonic feasibility assertion.
    pub lower: DecimalValue,
    pub upper: DecimalValue,
    /// COMPLETED means native execution returned, not target achievement,
    /// positive profit, or validated economic capacity.
    pub status: NativeCapitalReplayStatusV1,
    pub elapsed_ms: DbCounter,
    pub result: Option<NativeSimulationResultV1>,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalCurveResourcesV1 {
    pub attempted_simulations: u32,
    pub completed_simulations: u32,
    pub failed_simulations: u32,
    pub elapsed_ms: DbCounter,
    /// Only returned native results are measurable; failed executions may have
    /// consumed additional events. This is not a total CPU/memory/fuel meter.
    pub completed_native_events: DbCounter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCapitalCurveTerminationV1 {
    Completed,
    PlannedOnly,
    TimeBudgetExhausted,
    NoDeployableCapital,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCapitalEconomicStatusV1 {
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapitalCurveResultV1 {
    pub schema_version: SchemaV1,
    /// Exact frozen configuration and targets; a replay changes only starting_capital.
    pub request: NativeCapitalCurveRequestV1,
    pub dataset_revision_id: Option<Id>,
    pub deployable_capital: DecimalValue,
    pub generation: NativeCapitalCurveGenerationV1,
    pub points: Vec<NativeCapitalCurvePointV1>,
    pub resources: NativeCapitalCurveResourcesV1,
    pub termination: NativeCapitalCurveTerminationV1,
    pub economic_capacity_status: NativeCapitalEconomicStatusV1,
    pub limitations: Vec<String>,
}
