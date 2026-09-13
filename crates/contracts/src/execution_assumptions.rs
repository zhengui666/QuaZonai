//! Immutable Operator assumptions, distinct from native market observations or approval.
use crate::{science::NativeSimulationSettingsV1, Id, Revision, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BarLiquidityAssumptionV1 {
    pub schema_version: SchemaV1,
    pub report_artifact_id: Id,
    #[schema(minimum = 1)]
    pub maximum_age_seconds: u32,
    pub participation_limit: crate::DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionAssumptionsCreateV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub runtime_id: Id,
    pub expected_runtime_revision: Revision,
    pub input_set_id: Id,
    pub dataset_revision_id: Id,
    pub settings: NativeSimulationSettingsV1,
    pub bar_liquidity: Option<BarLiquidityAssumptionV1>,
    #[schema(min_length = 1, max_length = 200)]
    pub settlement_rule_ref: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionAssumptionsViewV1 {
    pub id: Id,
    pub project_id: Id,
    pub input_set_id: Id,
    pub dataset_revision_id: Id,
    pub runtime_id: Id,
    pub capability_snapshot_artifact_id: Id,
    pub fee_schedule_artifact_id: Id,
    pub engine_image_ref: String,
    pub venue_capability_ref: String,
    pub calendar_version: String,
    pub settlement_rule_ref: String,
    /// This entrypoint freezes declared models, never self-asserted DATA_BACKED.
    pub cost_assumption_status: ConservativeAssumption,
    pub settings: NativeSimulationSettingsV1,
    pub bar_liquidity: Option<BarLiquidityAssumptionV1>,
    pub bar_liquidity_valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
pub enum ConservativeAssumption {
    #[serde(rename = "CONSERVATIVE_ASSUMPTION")]
    Conservative,
}
