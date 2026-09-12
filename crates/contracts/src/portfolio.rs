//! Frozen allocation contracts. A numerical result never confers delivery authority.
use crate::{DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const MAX_ALLOCATION_ASSETS: usize = 256;
pub const MAX_ALLOCATION_GROUPS: usize = 64;
pub const CLARABEL_CLASS: &str = "clarabel::solver::DefaultSolver";
pub const CLARABEL_VERSION: &str = "0.11.1";
pub const FIXED_ENSEMBLE_CLASS: &str = "ndarray::ArrayBase::dot";
pub const FIXED_ENSEMBLE_VERSION: &str = "0.17.1";
pub const SAMPLE_COVARIANCE_CLASS: &str = "ndarray_stats::CorrelationExt::cov";
pub const SAMPLE_COVARIANCE_VERSION: &str = "0.7.0";

/// Original forecast metadata; these identifiers alone never prove qualification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AlphaForecastV1 {
    pub alpha_id: crate::Id,
    pub alpha_version_id: crate::Id,
    pub forecast_unit: crate::evidence::ForecastUnit,
    pub horizon_kind: crate::brief::HorizonKind,
    pub horizon_value: crate::DbCounter,
    pub base_currency: String,
    pub asof_ns: crate::DbCounter,
    pub available_ns: crate::DbCounter,
    pub ensemble_weight: DecimalValue,
    #[schema(min_items = 1, max_items = 256)]
    pub bar_types: Vec<String>,
    #[schema(min_items = 1, max_items = 256)]
    pub instrument_ids: Vec<String>,
    #[serde(serialize_with = "serialize_finite_values")]
    #[schema(min_items = 1, max_items = 256)]
    pub forecasts: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PortfolioForecastInputV1 {
    pub schema_version: SchemaV1,
    pub decision_asof_ns: crate::DbCounter,
    pub forecast_asof_ns: crate::DbCounter,
    pub horizon_kind: crate::brief::HorizonKind,
    pub horizon_value: crate::DbCounter,
    pub base_currency: String,
    #[schema(minimum = 1)]
    pub max_input_age_seconds: u32,
    #[schema(min_items = 1, max_items = 256)]
    pub bar_types: Vec<String>,
    #[schema(min_items = 1, max_items = 256)]
    pub instrument_ids: Vec<String>,
    #[schema(min_items = 2, max_items = 256)]
    pub members: Vec<AlphaForecastV1>,
}

fn serialize_finite_values<S: serde::Serializer>(
    values: &[f64],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(serde::ser::Error::custom("non-finite forecast"));
    }
    values.serialize(serializer)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RebalanceKind {
    Manual,
    FixedInterval,
    CalendarSession,
}

/// Frozen scheduling intent, not a timer or permission to create a Release.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RebalanceScheduleV1 {
    pub schema_version: SchemaV1,
    pub kind: RebalanceKind,
    #[schema(required = true, minimum = 1)]
    pub interval_seconds: Option<u32>,
    #[schema(required = true, min_length = 1, max_length = 200)]
    pub calendar_ref: Option<String>,
    #[schema(min_length = 1, max_length = 200)]
    pub timezone: String,
    #[schema(required = true)]
    pub session_offset_seconds: Option<i32>,
    #[schema(minimum = 1)]
    pub max_input_age_seconds: u32,
    #[schema(minimum = 1)]
    pub target_ttl_seconds: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AllocationObjective {
    MinRisk,
    MaxUtility,
    RiskBudgeting,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AllocationRisk {
    Variance,
    Cvar,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupBoundV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub group_id: String,
    pub min: DecimalValue,
    pub max: DecimalValue,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetBoundV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    pub min: DecimalValue,
    pub max: DecimalValue,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PortfolioConstraintsV1 {
    pub schema_version: SchemaV1,
    pub long_only: bool,
    pub min_cash_weight: DecimalValue,
    pub max_cash_weight: DecimalValue,
    pub min_asset_weight: DecimalValue,
    pub max_asset_weight: DecimalValue,
    pub max_gross_exposure: DecimalValue,
    pub min_net_exposure: DecimalValue,
    pub max_net_exposure: DecimalValue,
    /// Gross traded asset notional divided by capital; cash is not charged twice.
    pub max_turnover_per_rebalance: DecimalValue,
    pub max_participation: Option<DecimalValue>,
    pub max_ex_ante_risk: Option<DecimalValue>,
    #[schema(max_items = 64)]
    pub group_bounds: Vec<GroupBoundV1>,
    #[schema(max_items = 256)]
    pub asset_overrides: Vec<AssetBoundV1>,
    pub transaction_costs_ref: crate::Id,
    pub liquidity_ref: Option<crate::Id>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocatorSettingsV1 {
    pub schema_version: SchemaV1,
    pub risk_aversion: DecimalValue,
    #[schema(minimum = 1, maximum = 100000)]
    pub max_iterations: u32,
    /// Numerical stopping tolerance, not permission to violate the mandate.
    pub solver_tolerance: DecimalValue,
    pub accept_inaccurate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FixedEnsembleParametersV1 {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SampleCovarianceParametersV1 {
    #[schema(minimum = 1, maximum = 1)]
    pub ddof: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MandateContentV1 {
    pub objective: AllocationObjective,
    pub risk_measure: AllocationRisk,
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub universe_version_id: crate::Id,
    pub covariance_estimator: NativeModelRefV1,
    pub alpha_ensemble: NativeModelRefV1,
    pub optimizer: NativeModelRefV1,
    pub constraints: PortfolioConstraintsV1,
    pub rebalance_schedule: RebalanceScheduleV1,
    pub required_evaluation_policy_id: crate::Id,
    pub execution_assumptions_id: crate::Id,
    pub exposure_tolerance: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MandateCreateV1 {
    pub schema_version: SchemaV1,
    pub project_id: crate::Id,
    pub runtime_id: crate::Id,
    pub expected_runtime_revision: crate::Revision,
    pub content: MandateContentV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MandateViewV1 {
    pub id: crate::Id,
    pub project_id: crate::Id,
    pub version: u32,
    pub content: MandateContentV1,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Only implemented native adapters. Role and linked upstream identity are
/// checked before execution; this reference does not authorize a model import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "adapter_kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum NativeModelRefV1 {
    SampleCovariance {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: SampleCovarianceParametersV1,
    },
    ClarabelQp {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: AllocatorSettingsV1,
    },
    FixedWeightedForecast {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: FixedEnsembleParametersV1,
    },
}

impl utoipa::PartialSchema for NativeModelRefV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{AdditionalProperties, ObjectBuilder, OneOfBuilder, Type};
        let literal = |value| {
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some([value]))
        };
        let mut schema = OneOfBuilder::new();
        for (kind, class, version, parameters) in [
            (
                "CLARABEL_QP",
                CLARABEL_CLASS,
                CLARABEL_VERSION,
                AllocatorSettingsV1::schema(),
            ),
            (
                "FIXED_WEIGHTED_FORECAST",
                FIXED_ENSEMBLE_CLASS,
                FIXED_ENSEMBLE_VERSION,
                FixedEnsembleParametersV1::schema(),
            ),
            (
                "SAMPLE_COVARIANCE",
                SAMPLE_COVARIANCE_CLASS,
                SAMPLE_COVARIANCE_VERSION,
                SampleCovarianceParametersV1::schema(),
            ),
        ] {
            schema = schema.item(
                ObjectBuilder::new()
                    .schema_type(Type::Object)
                    .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                    .property("schema_version", SchemaV1::schema())
                    .required("schema_version")
                    .property("adapter_kind", literal(kind))
                    .required("adapter_kind")
                    .property("upstream_class", literal(class))
                    .required("upstream_class")
                    .property("upstream_version", literal(version))
                    .required("upstream_version")
                    .property("parameters", parameters)
                    .required("parameters"),
            );
        }
        schema.into()
    }
}
impl ToSchema for NativeModelRefV1 {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        AllocatorSettingsV1::schemas(schemas);
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationAssetV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub currency: String,
    pub current_weight: DecimalValue,
    /// Frozen all-in cost per unit of traded notional, never an inferred zero.
    pub transaction_cost_rate: DecimalValue,
    /// Observed eligible notional at the decision cut-off, in the base currency.
    pub available_notional: Option<DecimalValue>,
    #[schema(schema_with = group_membership_schema)]
    pub groups: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationInputV1 {
    pub schema_version: SchemaV1,
    pub forecasts: PortfolioForecastInputV1,
    pub objective: AllocationObjective,
    pub risk: AllocationRisk,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub current_cash_weight: DecimalValue,
    pub exposure_tolerance: DecimalValue,
    pub constraints: PortfolioConstraintsV1,
    pub optimizer: NativeModelRefV1,
    pub alpha_ensemble: NativeModelRefV1,
    #[schema(min_items = 1, max_items = 256)]
    pub assets: Vec<AllocationAssetV1>,
    /// Same asset ordering; per-decision-period covariance from the native estimator.
    #[serde(serialize_with = "serialize_finite_matrix")]
    #[schema(schema_with = covariance_schema)]
    pub covariance: Vec<Vec<f64>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SolverStatus {
    Optimal,
    AcceptableInaccurate,
    Infeasible,
    Unbounded,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationTargetV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub currency: String,
    pub weight: DecimalValue,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationResultV1 {
    pub schema_version: SchemaV1,
    pub solver_status: SolverStatus,
    pub reason_code: Option<String>,
    /// A failed solve has no targets, not an empty or fallback allocation.
    pub targets: Option<Vec<AllocationTargetV1>>,
    pub cash_weight: Option<DecimalValue>,
    #[schema(minimum = 0, maximum = 4294967295u64)]
    pub iterations: u32,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub objective_value: Option<f64>,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub primal_residual: Option<f64>,
    #[serde(
        serialize_with = "crate::evidence::serialize_finite_optional",
        deserialize_with = "crate::evidence::deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub dual_residual: Option<f64>,
}

fn serialize_finite_matrix<S: serde::Serializer>(
    value: &[Vec<f64>],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if value.iter().flatten().any(|number| !number.is_finite()) {
        return Err(serde::ser::Error::custom("non-finite covariance"));
    }
    value.serialize(serializer)
}
fn covariance_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ArrayBuilder, KnownFormat, ObjectBuilder, SchemaFormat, Type};
    ArrayBuilder::new()
        .min_items(Some(1))
        .max_items(Some(MAX_ALLOCATION_ASSETS))
        .items(
            ArrayBuilder::new()
                .min_items(Some(1))
                .max_items(Some(MAX_ALLOCATION_ASSETS))
                .items(
                    ObjectBuilder::new()
                        .schema_type(Type::Number)
                        .format(Some(SchemaFormat::KnownFormat(KnownFormat::Double))),
                ),
        )
        .into()
}
fn group_membership_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Type};
    ArrayBuilder::new()
        .max_items(Some(MAX_ALLOCATION_GROUPS))
        .unique_items(true)
        .items(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(1))
                .max_length(Some(120)),
        )
        .into()
}
