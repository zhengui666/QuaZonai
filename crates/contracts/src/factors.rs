//! Factor-only scientific evidence. No targets, calibration, accounts or qualification.
use crate::{
    science::{NativeForecastPointV1, NativeForecastRequestV1},
    DbCounter, SchemaV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorPointV1 {
    /// Unmodified native score, label, event/availability clocks and missing masks.
    /// Labels are evaluation evidence only; none is supplied to the Wasm model.
    pub observation: NativeForecastPointV1,
    /// Membership is determined by event time, never an asset's row ordinal.
    pub partition: NativeFactorPartitionV1,
    /// Original future bar's event clock; null for warmup or incomplete labels.
    pub label_event_ns: Option<DbCounter>,
    /// Null means this score/label pair is included in its partition's IC.
    pub exclusion_reason: Option<NativeFactorExclusionReasonV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeFactorIcStatusV1 {
    Available,
    InsufficientSamples,
    ConstantScore,
    ConstantLabel,
    NonfiniteData,
    NonfiniteResult,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorIcV1 {
    /// Raw Pearson correlation of arbitrary finite scores with future returns.
    /// No score-to-return fitting, normalization, clamping or RMSE is performed.
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub status: NativeFactorIcStatusV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorCountsV1 {
    pub observations: DbCounter,
    pub included: DbCounter,
    pub excluded: DbCounter,
    /// Exclusion categories are disjoint and sum to excluded.
    pub indicator_warmup: DbCounter,
    pub incomplete_labels: DbCounter,
    pub cross_boundary_labels: DbCounter,
    pub prediction_outside_partition: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorMetricV1 {
    pub counts: NativeFactorCountsV1,
    pub pearson_ic: NativeFactorIcV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorAssetMetricV1 {
    pub instrument_id: String,
    pub metric: NativeFactorMetricV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorPartitionReportV1 {
    pub partition: NativeFactorPartitionV1,
    pub start_ns: DbCounter,
    /// Exclusive for both event and availability clocks, including future labels.
    pub end_ns: DbCounter,
    /// Pooled qualifying score/return pairs across assets, not cross-sectional IC.
    pub metric: NativeFactorMetricV1,
    pub assets: Vec<NativeFactorAssetMetricV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorEvaluationResultV1 {
    pub schema_version: SchemaV1,
    /// Original validated request, including the selection and shared temporal cuts.
    pub request: NativeFactorEvaluationRequestV1,
    pub native_versions: BTreeMap<String, String>,
    pub methods: BTreeMap<String, String>,
    /// Explicit null means unmetered execution, not a measured zero.
    #[serde(deserialize_with = "crate::science::deserialize_consumed_fuel")]
    #[schema(required = true)]
    pub consumed_fuel: Option<DbCounter>,
    pub points: Vec<NativeFactorPointV1>,
    /// Always TRAIN, VALIDATION, HELDOUT, including empty/unavailable metrics.
    pub partitions: Vec<NativeFactorPartitionReportV1>,
}

/// CLI summary projection of the same native calculation; no point rows on stdout.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorEvaluationSummaryV1 {
    pub schema_version: SchemaV1,
    pub request: NativeFactorEvaluationRequestV1,
    pub native_versions: BTreeMap<String, String>,
    pub methods: BTreeMap<String, String>,
    #[serde(deserialize_with = "crate::science::deserialize_consumed_fuel")]
    #[schema(required = true)]
    pub consumed_fuel: Option<DbCounter>,
    pub partitions: Vec<NativeFactorPartitionReportV1>,
}

impl From<NativeFactorEvaluationResultV1> for NativeFactorEvaluationSummaryV1 {
    fn from(value: NativeFactorEvaluationResultV1) -> Self {
        Self {
            schema_version: value.schema_version,
            request: value.request,
            native_versions: value.native_versions,
            methods: value.methods,
            consumed_fuel: value.consumed_fuel,
            partitions: value.partitions,
        }
    }
}
