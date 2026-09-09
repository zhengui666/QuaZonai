//! Frozen allocation contracts. A numerical result never confers delivery authority.
use crate::{DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const MAX_ALLOCATION_ASSETS: usize = 256;
pub const MAX_ALLOCATION_GROUPS: usize = 64;

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
    #[schema(minimum = 1, maximum = 100000)]
    pub max_iterations: u32,
    /// Numerical stopping tolerance, not permission to violate the mandate.
    pub solver_tolerance: DecimalValue,
    pub accept_inaccurate: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationAssetV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub currency: String,
    #[serde(serialize_with = "serialize_finite")]
    pub expected_return: f64,
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
    pub objective: AllocationObjective,
    pub risk: AllocationRisk,
    #[schema(schema_with = crate::budget::currency_schema)]
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub risk_aversion: DecimalValue,
    pub current_cash_weight: DecimalValue,
    pub exposure_tolerance: DecimalValue,
    pub constraints: PortfolioConstraintsV1,
    pub settings: AllocatorSettingsV1,
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

fn serialize_finite<S: serde::Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom("non-finite allocation input"));
    }
    value.serialize(serializer)
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
