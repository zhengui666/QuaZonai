//! Restricted native fold evidence, not a policy decision or qualification.
use super::{NativeForecastPointV1, NativeForecastRequestV1};
use crate::{
    DbCounter, SchemaV1, brief::TargetKind, evidence::MetricStatus, research::SplitPolicyV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAlphaValidationRequestV1 {
    pub schema_version: SchemaV1,
    pub forecast: NativeForecastRequestV1,
    pub split_policy: SplitPolicyV1,
    pub target_kind: TargetKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeValidationPointV1 {
    pub observation: NativeForecastPointV1,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub expected_return: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCalibrationV1 {
    pub status: MetricStatus,
    pub reason_code: Option<String>,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub intercept: Option<f64>,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub slope: Option<f64>,
    pub training_observations: DbCounter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeAlphaMetricKind {
    PearsonIc,
    ReturnRmse,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeValidationMetricV1 {
    pub kind: NativeAlphaMetricKind,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub status: MetricStatus,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeValidationFoldV1 {
    pub instrument_id: String,
    pub bar_type: String,
    pub source_row_count: DbCounter,
    #[schema(minimum = 0)]
    pub fold_index: u16,
    #[schema(min_items = 3)]
    pub training_ordinals: Vec<u32>,
    /// Actual availability of the last complete training label, not event time.
    pub training_end_available_ns: DbCounter,
    #[schema(min_items = 1)]
    pub test_points: Vec<NativeValidationPointV1>,
    /// No calibration is fitted for a model already declaring expected returns.
    pub calibration: Option<NativeCalibrationV1>,
    #[schema(min_items = 2, max_items = 2)]
    pub metrics: Vec<NativeValidationMetricV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAlphaValidationResultV1 {
    pub schema_version: SchemaV1,
    pub native_versions: BTreeMap<String, String>,
    /// None means some execution was unmetered, not a measured zero.
    #[serde(deserialize_with = "crate::science::deserialize_consumed_fuel")]
    #[schema(required = true)]
    pub consumed_fuel: Option<DbCounter>,
    /// Distinct asset/ordinal pairs, NOT a claim of statistical independence.
    pub unique_test_observations: DbCounter,
    #[schema(min_items = 1)]
    pub folds: Vec<NativeValidationFoldV1>,
}

/// Restricted persisted native coefficients. No labels, refitting or verdict.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFrozenCalibrationV1 {
    pub schema_version: SchemaV1,
    pub source_report_artifact_id: crate::Id,
    pub estimator_kind: String,
    pub estimator_version: String,
    pub selection_rule: String,
    pub horizon_observations: DbCounter,
    pub fit_end_available_ns: DbCounter,
    #[schema(min_items = 1)]
    pub assets: Vec<NativeAssetCalibrationV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAssetCalibrationV1 {
    pub instrument_id: String,
    pub bar_type: String,
    pub fold_index: u16,
    #[schema(min_items = 3)]
    pub training_ordinals: Vec<u32>,
    pub training_end_available_ns: DbCounter,
    pub calibration: NativeCalibrationV1,
}
