//! Registered data partitions retained by catalog preparation; no IC/result contract.
use crate::{science::NativeForecastRequestV1, DbCounter, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorEvaluationRequestV1 {
    pub schema_version: SchemaV1,
    pub forecast: NativeForecastRequestV1,
    /// Shared exclusive train boundary, strictly inside the selected event range.
    pub train_end_ns: DbCounter,
    /// Shared exclusive validation boundary: train_end < validation_end <= event_end.
    /// Equality means iterative evaluation without reading a held-out suffix.
    pub validation_end_ns: DbCounter,
}

/// Complete registered catalog scope, without a fixed instrument shortlist.
/// Catalog paths remain trusted local CLI arguments, never scientific input fields.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogFactorScanRequestV1 {
    pub schema_version: SchemaV1,
    pub event_start_ns: DbCounter,
    pub train_end_ns: DbCounter,
    pub validation_end_ns: DbCounter,
    pub event_end_ns: DbCounter,
    pub decision_cutoff_ns: DbCounter,
    /// Legacy row allowance retained in frozen requests; no longer limits reads.
    #[serde(default)]
    pub maximum_rows: u32,
    pub parameters: crate::science::NativeForecastParametersV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeFactorPartitionV1 {
    Train,
    Validation,
    Heldout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeFactorExclusionReasonV1 {
    IndicatorWarmup,
    LabelNotComplete,
    PredictionOutsidePartition,
    LabelCrossesPartition,
}
