//! Explicit counterfactual replay from an observed spot inventory, never account writes.
use crate::{account_observation::NativePortfolioSnapshotV1, science::*, DbCounter, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStartV1 {
    pub schema_version: SchemaV1,
    pub snapshot_evidence_ref: String,
    pub source_native_version: String,
    pub snapshot: NativePortfolioSnapshotV1,
    pub available_ns: DbCounter,
    pub open_orders_evidence_ref: String,
    pub open_orders_account_id: String,
    pub open_orders_asof_ns: DbCounter,
    pub open_orders_available_ns: DbCounter,
    /// Required original observation. Missing never means zero.
    pub open_orders_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotComparisonRequestV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub initial_state: NativeObservedSpotStartV1,
    pub simulation: NativeSimulationRequestV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStudyRequestV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub initial_state: NativeObservedSpotStartV1,
    pub study: NativePortfolioStudyRequestV1,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeObservedSpotLegStatusV1 {
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotSimulationLegV1 {
    pub status: NativeObservedSpotLegStatusV1,
    pub result: Option<NativeSimulationResultV1>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStudyLegV1 {
    pub status: NativeObservedSpotLegStatusV1,
    /// Infeasible solver frames remain evidence, even without a completed replay.
    pub result: Option<NativePortfolioStudyResultV1>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotComparisonResultV1 {
    pub schema_version: SchemaV1,
    pub kind: String,
    pub basis: String,
    pub request: NativeObservedSpotComparisonRequestV1,
    pub hold: NativeObservedSpotSimulationLegV1,
    pub rebalance: NativeObservedSpotSimulationLegV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotStudyResultV1 {
    pub schema_version: SchemaV1,
    pub kind: String,
    pub basis: String,
    pub request: NativeObservedSpotStudyRequestV1,
    pub hold: NativeObservedSpotSimulationLegV1,
    pub rebalance: NativeObservedSpotStudyLegV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotValuationRequestV1 {
    pub schema_version: SchemaV1,
    pub dataset_revision_id: Id,
    pub initial_state: NativeObservedSpotStartV1,
    pub selection: NativeBarSelectionV1,
    pub settings: NativeSimulationSettingsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotValuationLegV1 {
    pub currency: String,
    pub native_total: crate::DecimalValue,
    pub report_value: crate::DecimalValue,
    pub instrument_id: Option<String>,
    pub price: Option<crate::DecimalValue>,
    pub price_event_ns: Option<DbCounter>,
    pub price_available_ns: Option<DbCounter>,
    pub source_row_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeObservedSpotValuationResultV1 {
    pub schema_version: SchemaV1,
    pub kind: String,
    pub basis: String,
    pub request: NativeObservedSpotValuationRequestV1,
    pub report_currency: String,
    pub asof_ns: DbCounter,
    pub available_ns: DbCounter,
    pub total: crate::DecimalValue,
    pub legs: Vec<NativeObservedSpotValuationLegV1>,
    pub source_evidence: Option<crate::spot_cash_source::NativeSpotCashSourceEvidenceV1>,
}
