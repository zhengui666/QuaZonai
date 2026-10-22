//! Signal-only fitting over original native factor evidence, never asset allocation.
use crate::{
    factors::NativeFactorEvaluationResultV1,
    science::{NativeForecastRequestV1, NativeForecastResultV1},
    DbCounter, DecimalValue, SchemaV1,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFitMemberV1 {
    pub factor_version_id: String,
    /// Immutable original report identity; the host must verify this association.
    pub source_evidence_ref: String,
    pub report: NativeFactorEvaluationResultV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFoldV1 {
    /// Exclusive boundary for training predictions AND completed labels.
    pub train_end_ns: DbCounter,
    pub validation_start_ns: DbCounter,
    /// Exclusive boundary for validation predictions AND completed labels.
    pub validation_end_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFitRequestV1 {
    pub schema_version: SchemaV1,
    /// Exclusive fit cutoff, no later than every original report's TRAIN end.
    pub fit_end_ns: DbCounter,
    pub folds: Vec<NativeEnsembleFoldV1>,
    pub members: Vec<NativeEnsembleFitMemberV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleCalibrationV1 {
    pub instrument_id: String,
    pub intercept: f64,
    pub slope: f64,
    pub training_observations: DbCounter,
    pub last_label_available_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFittedFactorV1 {
    pub factor_version_id: String,
    pub source_evidence_ref: String,
    /// Nonnegative simplex coefficient, including exact zero.
    pub weight: DecimalValue,
    pub calibrations: Vec<NativeEnsembleCalibrationV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFoldEvidenceV1 {
    pub fold: NativeEnsembleFoldV1,
    pub training_observations: DbCounter,
    pub validation_observations: DbCounter,
    pub training_last_label_available_ns: DbCounter,
    /// Each member's calibration was fitted independently inside this fold.
    pub member_calibrations: Vec<Vec<NativeEnsembleCalibrationV1>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFitResultV1 {
    pub schema_version: SchemaV1,
    pub method: String,
    pub solver_version: String,
    pub calibration_version: String,
    /// Explicit numerical reporting policy; no covariance jitter or ridge added.
    pub weight_decimal_places: u32,
    pub fit_end_ns: DbCounter,
    pub forecast_contract: NativeForecastRequestV1,
    pub members: Vec<NativeFittedFactorV1>,
    pub folds: Vec<NativeEnsembleFoldEvidenceV1>,
    pub oof_observations: DbCounter,
    /// Diagnostic only: OOF rows also selected weights, not independent validation.
    pub oof_mean_squared_error: f64,
    pub solver_objective: f64,
    pub primal_residual: f64,
    pub dual_residual: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleApplyMemberV1 {
    pub factor_version_id: String,
    pub forecast_request: NativeForecastRequestV1,
    pub forecast: NativeForecastResultV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleApplyRequestV1 {
    pub schema_version: SchemaV1,
    pub fit: NativeEnsembleFitResultV1,
    pub members: Vec<NativeEnsembleApplyMemberV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleApplyResultV1 {
    pub schema_version: SchemaV1,
    /// Gross price return per the original common label horizon; not net account PnL.
    pub forecast: NativeForecastResultV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFreezeMemberV1 {
    pub factor_version_id: String,
    /// The actual immutable native report object, not a generated placeholder.
    pub report_artifact_id: crate::Id,
    pub report: NativeFactorEvaluationResultV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFreezeRequestV1 {
    pub schema_version: SchemaV1,
    pub fit: NativeEnsembleFitResultV1,
    pub members: Vec<NativeEnsembleFreezeMemberV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFrozenMemberV1 {
    pub factor_version_id: String,
    pub calibration: crate::science::NativeFrozenCalibrationV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeEnsembleFreezeResultV1 {
    pub schema_version: SchemaV1,
    pub members: Vec<NativeEnsembleFrozenMemberV1>,
}
