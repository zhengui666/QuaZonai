//! Finite native metrics and frozen comparison requirements; no numeric estimator.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{DbCounter, DecimalValue, Id, Revision, SchemaV1};

/// Explicit Cycle-funded evaluation of an existing immutable Alpha, not a new trial.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AlphaEvaluateRequestV1 {
    pub schema_version: SchemaV1,
    pub cycle_id: Id,
    pub policy_id: Id,
    pub input_set_id: Id,
    pub runtime_id: Id,
    pub expected_runtime_revision: Revision,
    #[schema(schema_with = crate::data::bounded_native_limits_schema)]
    pub limits: crate::lifecycle::JobLimitsV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlphaLifecycle {
    Research,
    Qualified,
    Suspended,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForecastUnit {
    ReturnPerHorizon,
    ResidualReturnPerHorizon,
    UnitlessScore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvaluationKind {
    Discovery,
    WalkForward,
    Sealed,
    Portfolio,
    Forward,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AlphaView {
    pub id: Id,
    pub project_id: Id,
    pub name: String,
    pub lifecycle: AlphaLifecycle,
    pub active_version_id: Option<Id>,
    pub active_version: Option<Revision>,
    pub revision: Revision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AlphaVersionView {
    pub id: Id,
    pub project_id: Id,
    pub alpha_id: Id,
    pub version: Revision,
    pub experiment_id: Id,
    pub root_lineage_id: Id,
    pub code_artifact_id: Id,
    pub model_artifact_id: Option<Id>,
    pub signal_contract_version: String,
    pub signal_kind: crate::brief::TargetKind,
    pub horizon_kind: crate::brief::HorizonKind,
    pub horizon_value: Option<DbCounter>,
    pub forecast_unit: ForecastUnit,
    pub calibration_id: Option<Id>,
    pub runtime_image_ref: String,
    pub origin: Option<crate::research::DataOrigin>,
    pub created_at: DateTime<Utc>,
}

/// Metadata only. The source Validation still evaluates its original version.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CalibrationView {
    pub id: Id,
    pub alpha_version_id: Id,
    pub estimator_kind: String,
    pub estimator_version: String,
    pub model_artifact_id: Id,
    pub train_input_set_id: Id,
    /// Conservative microsecond ceiling; exact nanoseconds stay in the model.
    pub fit_end_available_at: DateTime<Utc>,
    pub output_unit: ForecastUnit,
    pub horizon_kind: crate::brief::HorizonKind,
    pub horizon_value: DbCounter,
    pub validation: EvaluationView,
    pub created_at: DateTime<Utc>,
}

/// Original grant history, not a current portfolio admission decision.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct QualificationView {
    pub id: Id,
    pub alpha_version_id: Id,
    pub policy_id: Id,
    pub qualifying_evaluation_id: Id,
    pub granted_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub checked_at: DateTime<Utc>,
    /// Checks only the grant interval and revocation, not policy, data or lifecycle.
    pub grant_window_open: bool,
    /// Earliest revocation, including a scheduled future effective time.
    pub revocation: Option<QualificationRevocationView>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct QualificationRevocationView {
    pub id: Id,
    pub effective_at: DateTime<Utc>,
    pub reason_code: String,
    pub evidence_evaluation_id: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluationView {
    pub id: Id,
    pub project_id: Id,
    pub subject_alpha_version_id: Option<Id>,
    pub subject_candidate_id: Option<Id>,
    pub input_set_id: Id,
    pub policy_id: Id,
    pub run_id: Id,
    pub evaluation_kind: EvaluationKind,
    pub execution_status: crate::runtime_jobs::RuntimeResultState,
    pub evidence_status: EvidenceStatus,
    pub decision: Decision,
    pub report_artifact_id: Id,
    pub method_versions_artifact_id: Id,
    pub origin: crate::research::DataOrigin,
    pub concluded_at: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub checked_at: DateTime<Utc>,
    pub unexpired_at_read: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MetricStatus {
    Ok,
    InsufficientData,
    Unsupported,
    InvalidInput,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceStatus {
    Valid,
    Invalid,
    Incomplete,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Pass,
    Reject,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Comparator {
    Gt,
    Ge,
    Lt,
    Le,
    Between,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricRequirementV1 {
    pub schema_version: SchemaV1,
    #[schema(min_length = 1, max_length = 120)]
    pub metric_code: String,
    #[schema(min_length = 1, max_length = 120)]
    pub scope: String,
    pub comparator: Comparator,
    pub threshold_low: Option<DecimalValue>,
    pub threshold_high: Option<DecimalValue>,
    pub required: bool,
    pub minimum_observations: DbCounter,
    #[schema(schema_with = method_allowlist_schema)]
    pub method_allowlist: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricValueV1 {
    pub schema_version: SchemaV1,
    pub evaluation_id: Id,
    pub metric_code: String,
    pub scope: String,
    #[serde(
        serialize_with = "serialize_finite_optional",
        deserialize_with = "deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub status: MetricStatus,
    pub reason_code: Option<String>,
    pub unit: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub observation_count: DbCounter,
    pub frequency: String,
    #[serde(
        serialize_with = "serialize_finite_optional",
        deserialize_with = "deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub annualization_factor: Option<f64>,
    pub method_id: String,
    pub method_version: String,
    pub source_artifact_id: Id,
    pub higher_is_better: Option<bool>,
}

// serde_json normally encodes non-finite floats as null. That would erase the
// distinction between corrupt evidence and an honestly missing observation.
pub(crate) fn serialize_finite_optional<S: serde::Serializer>(
    value: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(number) if !number.is_finite() => Err(serde::ser::Error::custom("non-finite metric")),
        _ => value.serialize(serializer),
    }
}

pub(crate) fn deserialize_finite_optional<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    let value = Option::<f64>::deserialize(deserializer)?;
    if value.is_some_and(|number| !number.is_finite()) {
        return Err(serde::de::Error::custom("non-finite metric"));
    }
    Ok(value)
}

// Keep the public Vec<String> wire shape. Native builders express both layers
// without incorrectly attaching string bounds to the enclosing array.
fn method_allowlist_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Type};
    ArrayBuilder::new()
        .min_items(Some(1))
        .max_items(Some(64))
        .unique_items(true)
        .items(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(1))
                .max_length(Some(120)),
        )
        .into()
}
