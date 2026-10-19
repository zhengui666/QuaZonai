//! Conditional BAR participation capacity and account sizing, never a profit forecast.
use crate::{science::*, DbCounter, DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCapacityRequestV1 {
    pub schema_version: SchemaV1,
    pub simulation: NativeSimulationRequestV1,
    pub policy: NativeRollingBarLiquidityPolicyV1,
    pub capitals: Vec<DecimalValue>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityScenarioV1 {
    pub capital: DecimalValue,
    pub required_turnover: Option<DecimalValue>,
    pub maximum_participation: Option<DecimalValue>,
    pub usable_capital: Option<DecimalValue>,
    pub unallocated_cash: Option<DecimalValue>,
    pub status: String,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_rebalance_turnover: Option<DecimalValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_turnover: Option<DecimalValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_rebalance_participation: Option<DecimalValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_participation: Option<DecimalValue>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityMethodologyV1 {
    pub id: String,
    pub formula: String,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCapacityConstraintKindV1 {
    EntryRebalance,
    FullExit,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityConstraintV1 {
    pub kind: NativeCapacityConstraintKindV1,
    pub maximum_capital: DecimalValue,
    pub instrument_id: String,
    pub decision_ns: DbCounter,
    pub bar_event_ns: DbCounter,
    pub bar_available_ns: DbCounter,
    pub observed_notional: DecimalValue,
    pub required_weight: DecimalValue,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityCoverageV1 {
    pub decision_count: usize,
    pub assessed_decision_count: usize,
    pub required_observations: usize,
    pub covered_observations: usize,
    pub unique_bars: usize,
    pub reused_bar_observations: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityVolumeStressV1 {
    pub volume_multiplier: DecimalValue,
    pub maximum_capital: Option<DecimalValue>,
    pub status: String,
    pub reason: Option<String>,
    pub scenarios: Vec<NativeCapacityScenarioV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityEconomicV1 {
    pub status: String,
    pub maximum_capital: Option<DecimalValue>,
    pub reason: String,
    pub missing_inputs: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityAssumptionsV1 {
    pub participation: DecimalValue,
    pub maximum_age_seconds: u32,
    pub execution_window: String,
    pub exit: String,
    pub fees: String,
    pub depth: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeCapacityReportV1 {
    pub status: String,
    pub reason: Option<String>,
    pub scope: String,
    pub currency: String,
    pub basis: String,
    pub maximum_capital: Option<DecimalValue>,
    pub minimum_capital: Option<DecimalValue>,
    pub binding_instrument: Option<String>,
    pub observation_start_ns: Option<DbCounter>,
    pub observation_end_ns: Option<DbCounter>,
    pub decision_clock_ns: Option<DbCounter>,
    pub valid_until_ns: Option<DbCounter>,
    pub assumptions: NativeCapacityAssumptionsV1,
    pub scenarios: Vec<NativeCapacityScenarioV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methodology: Option<NativeCapacityMethodologyV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Vec<NativeCapacityConstraintV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<NativeCapacityCoverageV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_stress: Option<Vec<NativeCapacityVolumeStressV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub economic_capacity: Option<NativeCapacityEconomicV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSizingLiquidityV1 {
    pub instrument_id: String,
    pub currency: String,
    pub event_ns: DbCounter,
    pub available_ns: DbCounter,
    pub notional: DecimalValue,
    pub lot_notional: DecimalValue,
    pub minimum_notional: DecimalValue,
    pub taker_fee: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSizingHoldingV1 {
    pub instrument_id: String,
    pub value: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeAccountSizingRequestV1 {
    pub schema_version: SchemaV1,
    #[serde(default)]
    pub projection_only: bool,
    pub equity: DecimalValue,
    /// Outstanding reservation only; reconciled withdrawals are already in equity.
    pub reserved: DecimalValue,
    pub cash_buffer: DecimalValue,
    pub maximum_capital: DecimalValue,
    pub minimum_capital: Option<DecimalValue>,
    pub participation: DecimalValue,
    pub maximum_age_seconds: u32,
    pub currency: String,
    pub clock_ns: DbCounter,
    pub target: NativeTargetPointV1,
    pub holdings: Vec<NativeSizingHoldingV1>,
    pub liquidity: Vec<NativeSizingLiquidityV1>,
    pub members: Vec<NativeSizingMemberV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSizingMemberV1 {
    pub factor_id: String,
    pub weight: DecimalValue,
    #[serde(default)]
    pub pool_weight: Option<DecimalValue>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeSizedMemberV1 {
    pub factor_id: String,
    pub capital: DecimalValue,
    pub scale: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeAccountSizingResultV1 {
    pub status: String,
    pub reason: Option<String>,
    pub budget: DecimalValue,
    pub capital: DecimalValue,
    pub residual_cash: DecimalValue,
    pub target: Option<NativeTargetPointV1>,
    pub members: Vec<NativeSizedMemberV1>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn historical_capacity_reports_without_diagnostics_remain_readable() {
        let original = json!({
            "status":"ESTIMATED", "reason":null, "scope":"original-factor", "currency":"USD", "basis":"HISTORICAL_BAR_ESTIMATE",
            "maximumCapital":"1000.000000000000000001", "minimumCapital":"1", "bindingInstrument":"A",
            "observationStartNs":"9007199254740993", "observationEndNs":"9007199254740993", "decisionClockNs":null, "validUntilNs":null,
            "assumptions":{"participation":"0.01", "maximumAgeSeconds":120, "executionWindow":"original", "exit":"original", "fees":"original", "depth":"BAR_NOTIONAL_ONLY"},
            "scenarios":[{"capital":"1000.000000000000000001", "requiredTurnover":"100", "maximumParticipation":"0.01", "usableCapital":"1000.000000000000000001", "unallocatedCash":"0", "status":"FEASIBLE", "reason":null}]
        });
        let report: NativeCapacityReportV1 = serde_json::from_value(original.clone()).unwrap();
        assert!(
            report.methodology.is_none()
                && report.coverage.is_none()
                && report.constraints.is_none()
        );
        assert!(report.volume_stress.is_none() && report.economic_capacity.is_none());
        assert_eq!(serde_json::to_value(report).unwrap(), original);
    }
}
