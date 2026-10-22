//! Frozen allocation contracts. A numerical result never confers delivery authority.
use crate::{DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const CLARABEL_CLASS: &str = "clarabel::solver::DefaultSolver";
pub const CLARABEL_VERSION: &str = "0.11.1";
pub const FIXED_ENSEMBLE_CLASS: &str = "ndarray::ArrayBase::dot";
pub const FIXED_ENSEMBLE_VERSION: &str = "0.17.1";
pub const SAMPLE_COVARIANCE_CLASS: &str = "ndarray_stats::CorrelationExt::cov";
pub const SAMPLE_COVARIANCE_VERSION: &str = "0.7.0";
pub const NAUTILUS_EXECUTION_VERSION: &str = "0.63.0";
pub const NAUTILUS_FILL_CLASS: &str = "nautilus_execution::models::fill::DefaultFillModel";
pub const NAUTILUS_POLYMARKET_FEE_CLASS: &str = "nautilus_polymarket::models::PolymarketFeeModel";
pub const NAUTILUS_FEE_CLASS: &str = "nautilus_execution::models::fee::MakerTakerFeeModel";
pub const NAUTILUS_LATENCY_CLASS: &str = "nautilus_execution::models::latency::StaticLatencyModel";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NautilusFillParametersV1 {
    pub prob_fill_on_limit: DecimalValue,
    pub prob_slippage: DecimalValue,
    pub random_seed: crate::DbCounter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NautilusFeeParametersV1 {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NautilusLatencyParametersV1 {
    pub base_latency_ns: crate::DbCounter,
    pub insert_latency_ns: crate::DbCounter,
    pub update_latency_ns: crate::DbCounter,
    pub cancel_latency_ns: crate::DbCounter,
}

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
    #[schema(min_items = 1)]
    pub bar_types: Vec<String>,
    #[schema(min_items = 1)]
    pub instrument_ids: Vec<String>,
    #[serde(serialize_with = "serialize_finite_values")]
    #[schema(min_items = 1)]
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
    #[schema(min_items = 1)]
    pub bar_types: Vec<String>,
    #[schema(min_items = 1)]
    pub instrument_ids: Vec<String>,
    #[schema(min_items = 2)]
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
    /// Opt-in CASH method. Cash remains the pre-fee residual divided by original
    /// NAV; this policy additionally preserves min_cash_weight after linear fees.
    /// Absence retains the original allocation semantics, including MARGIN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash_funding: Option<NativeCashFundingPolicyV1>,
    pub min_asset_weight: DecimalValue,
    pub max_asset_weight: DecimalValue,
    pub max_gross_exposure: DecimalValue,
    pub min_net_exposure: DecimalValue,
    pub max_net_exposure: DecimalValue,
    /// Gross traded asset notional divided by capital; cash is not charged twice.
    pub max_turnover_per_rebalance: DecimalValue,
    pub max_participation: Option<DecimalValue>,
    /// Positive per-horizon variance (VARIANCE) or expected loss return (CVAR) bound.
    /// Not volatility or annualized risk; allowance is bound * exposure_tolerance.
    pub max_ex_ante_risk: Option<DecimalValue>,
    pub group_bounds: Vec<GroupBoundV1>,
    pub asset_overrides: Vec<AssetBoundV1>,
    pub transaction_costs_ref: crate::Id,
    pub liquidity_ref: Option<crate::Id>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCashFundingPolicyV1 {
    /// sum(rate_i * abs(target_i - current_i)) + min_cash_weight <= cash_weight.
    /// Linear turnover estimates do not establish executable market capacity.
    PreserveMinCashAfterLinearTurnoverV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RiskBudgetAssetV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    pub share: DecimalValue,
    pub sign: RiskBudgetSign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskBudgetSign {
    Long,
    Short,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RiskBudgetSettingsV1 {
    pub schema_version: SchemaV1,
    pub risky_gross_exposure: DecimalValue,
    #[schema(min_items = 1)]
    pub assets: Vec<RiskBudgetAssetV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocatorSettingsV1 {
    pub schema_version: SchemaV1,
    pub risk_aversion: DecimalValue,
    /// Required for CVAR, absent/null for VARIANCE. Strictly between zero and one.
    pub cvar_confidence: Option<DecimalValue>,
    /// Required only for RISK_BUDGETING; explicit asset identity, share, sign and scale.
    pub risk_budgeting: Option<RiskBudgetSettingsV1>,
    #[schema(minimum = 1)]
    pub max_iterations: u32,
    /// Numerical stopping tolerance, not permission to violate the mandate.
    pub solver_tolerance: DecimalValue,
    pub accept_inaccurate: bool,
    /// Explicit numerical output protocol for the observed CASH funding method.
    /// Absence preserves strict rejection of the original converted solver point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash_numerical_output: Option<NativeCashNumericalOutputProtocolV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCashNumericalOutputProtocolV1 {
    ValidatedFeasibleSegmentV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCashNumericalOutcomeV1 {
    Unchanged,
    Corrected,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeCashAnchorKindV1 {
    /// Numerical point from the input asset weights and exact capital identity;
    /// not a new account observation or an assumed feasible HOLD decision.
    CurrentWeights,
    AllCash,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCashFeasibleAnchorV1 {
    pub kind: NativeCashAnchorKindV1,
    pub targets: Vec<AllocationTargetV1>,
    pub cash_weight: DecimalValue,
}

/// Original native solution versus the final published numerical point.
/// This protocol changes neither the optimization problem nor its stopping tolerances.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeCashNumericalOutputV1 {
    pub protocol: NativeCashNumericalOutputProtocolV1,
    pub outcome: NativeCashNumericalOutcomeV1,
    /// Clarabel coordinates in original order: assets, cash, gross epigraph, turnover epigraph.
    pub raw_x: Vec<f64>,
    pub raw_native_status: String,
    /// Original Clarabel reply, not independently reproduced bit-for-bit by
    /// point validation; native artifact provenance establishes its origin.
    pub raw_objective_value: f64,
    /// Original objective recomputed from the converted raw weights and actual
    /// absolute turnover, distinct from Clarabel's epigraph objective above.
    pub raw_point_objective_value: f64,
    pub raw_primal_residual: f64,
    pub raw_dual_residual: f64,
    pub raw_targets: Vec<AllocationTargetV1>,
    pub raw_cash_weight: DecimalValue,
    pub raw_constraint_error: Option<String>,
    pub anchor: Option<NativeCashFeasibleAnchorV1>,
    /// Fraction towards the validated anchor, on the exact 10^-18 protocol grid.
    pub segment_fraction: Option<DecimalValue>,
    /// Approximate display of objective_delta_exact; never the budget proof.
    pub objective_delta: Option<f64>,
    /// Exact final-minus-raw objective difference, using original P/2 and
    /// forecast IEEE-754 coefficients and exact decimal turnover fees.
    /// Canonical bounded plain decimal in objective units, not money/return.
    #[schema(max_length = 2048, pattern = r"^-?[0-9]+(?:\.[0-9]+)?$")]
    pub objective_delta_exact: Option<String>,
    /// Original tol_gap_abs (= solver_tolerance), in original objective units.
    pub objective_change_limit: DecimalValue,
    pub search_steps: u32,
    pub reason_code: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CandidateWeightsSourceV1 {
    PaperInitialCapital,
    ForwardSnapshot,
    LastTarget,
    None,
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
    /// QZ-owned recorded v2 quantity-bounded replay through official FillModel.
    PolymarketV2RecordedLiquidity {
        schema_version: SchemaV1,
        native_version: String,
        parameters: NautilusFillParametersV1,
    },
    /// QZ-owned recorded v2 transaction-cost replay through official FeeModel.
    PolymarketV2RecordedFees {
        schema_version: SchemaV1,
        native_version: String,
        parameters: crate::recorded_fees::RecordedTradeFeeParametersV1,
    },
    /// QZ-owned thin adapter through the official native FeeModel trait.
    FrozenSpotFeeScenario {
        schema_version: SchemaV1,
        native_version: String,
        parameters: crate::spot_fees::FrozenSpotFeeScenarioParametersV1,
    },
    NautilusDefaultFill {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: NautilusFillParametersV1,
    },
    NautilusMakerTaker {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: NautilusFeeParametersV1,
    },
    NautilusPolymarket {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: NautilusFeeParametersV1,
    },
    NautilusStaticLatency {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: NautilusLatencyParametersV1,
    },
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
                "NAUTILUS_DEFAULT_FILL",
                NAUTILUS_FILL_CLASS,
                NAUTILUS_EXECUTION_VERSION,
                NautilusFillParametersV1::schema(),
            ),
            (
                "NAUTILUS_MAKER_TAKER",
                NAUTILUS_FEE_CLASS,
                NAUTILUS_EXECUTION_VERSION,
                NautilusFeeParametersV1::schema(),
            ),
            (
                "NAUTILUS_POLYMARKET",
                NAUTILUS_POLYMARKET_FEE_CLASS,
                NAUTILUS_EXECUTION_VERSION,
                NautilusFeeParametersV1::schema(),
            ),
            (
                "NAUTILUS_STATIC_LATENCY",
                NAUTILUS_LATENCY_CLASS,
                NAUTILUS_EXECUTION_VERSION,
                NautilusLatencyParametersV1::schema(),
            ),
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
        for (kind, parameters) in [
            (
                "FROZEN_SPOT_FEE_SCENARIO",
                crate::spot_fees::FrozenSpotFeeScenarioParametersV1::schema(),
            ),
            (
                "POLYMARKET_V2_RECORDED_FEES",
                crate::recorded_fees::RecordedTradeFeeParametersV1::schema(),
            ),
            (
                "POLYMARKET_V2_RECORDED_LIQUIDITY",
                NautilusFillParametersV1::schema(),
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
                    .property("native_version", literal(NAUTILUS_EXECUTION_VERSION))
                    .required("native_version")
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
        crate::spot_fees::FrozenSpotFeeScenarioParametersV1::schemas(schemas);
        crate::recorded_fees::RecordedTradeFeeParametersV1::schemas(schemas);
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationAssetV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    #[schema(schema_with = crate::research_currency::schema)]
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
pub struct PortfolioReturnHistoryV1 {
    pub schema_version: SchemaV1,
    pub base_currency: String,
    pub horizon_kind: crate::brief::HorizonKind,
    pub horizon_value: crate::DbCounter,
    #[schema(min_items = 1)]
    pub instrument_ids: Vec<String>,
    #[schema(min_items = 1)]
    pub bar_types: Vec<String>,
    /// Common completed return windows, in strictly increasing end-time order.
    #[schema(min_items = 2)]
    pub end_ns: Vec<crate::DbCounter>,
    /// Latest availability of all assets in each window, not just event time.
    #[schema(min_items = 2)]
    pub available_ns: Vec<crate::DbCounter>,
    /// Asset-major simple returns over the same horizon as the forecasts.
    #[serde(serialize_with = "serialize_finite_matrix")]
    #[schema(schema_with = return_history_schema)]
    pub asset_returns: Vec<Vec<f64>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationInputV1 {
    pub schema_version: SchemaV1,
    pub forecasts: PortfolioForecastInputV1,
    pub objective: AllocationObjective,
    pub risk: AllocationRisk,
    #[schema(schema_with = crate::research_currency::schema)]
    pub base_currency: String,
    pub capital_assumption: DecimalValue,
    pub current_cash_weight: DecimalValue,
    pub exposure_tolerance: DecimalValue,
    pub constraints: PortfolioConstraintsV1,
    pub optimizer: NativeModelRefV1,
    pub alpha_ensemble: NativeModelRefV1,
    pub covariance_estimator: NativeModelRefV1,
    pub return_history: PortfolioReturnHistoryV1,
    #[schema(min_items = 1)]
    pub assets: Vec<AllocationAssetV1>,
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
    #[schema(schema_with = crate::research_currency::schema)]
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
    pub cvar_risk_budget_witness: Option<CvarRiskBudgetWitnessV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numerical_output: Option<NativeCashNumericalOutputV1>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CvarRiskBudgetWitnessV1 {
    pub schema_version: SchemaV1,
    /// Native dual weights in the original return-history scenario order.
    #[serde(serialize_with = "serialize_finite_values")]
    #[schema(min_items = 2)]
    pub scenario_weights: Vec<f64>,
}

fn serialize_finite_matrix<S: serde::Serializer>(
    value: &[Vec<f64>],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if value.iter().flatten().any(|number| !number.is_finite()) {
        return Err(serde::ser::Error::custom("non-finite return history"));
    }
    value.serialize(serializer)
}
fn return_history_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ArrayBuilder, KnownFormat, ObjectBuilder, SchemaFormat, Type};
    ArrayBuilder::new()
        .min_items(Some(1))
        .items(
            ArrayBuilder::new().min_items(Some(2)).items(
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
        .unique_items(true)
        .items(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(1))
                .max_length(Some(120)),
        )
        .into()
}
