//! Native scientific DTOs and the immutable file protocol consumed by job.
#![forbid(unsafe_code)]

pub mod account_observation;
pub mod brief;
pub mod capacity;
pub mod capital_exit;
pub mod catalogs;
pub mod data;
pub mod delivery;
pub mod equity_curve;
pub mod evidence;
pub mod execution;
pub mod execution_assumptions;
pub mod execution_preflight;
pub mod experiment_summary;
pub mod factors;
pub mod forward;
pub mod lifecycle;
pub mod portfolio;
pub mod portfolio_history;
pub mod recorded_fees;
pub mod research;
pub mod research_currency;
pub mod runs;
pub mod runtime;
pub mod runtime_jobs;
pub mod scalars;
pub mod science;
pub mod settings;
pub mod settlement;
pub mod spot_cash;
pub mod spot_cash_report;
pub mod spot_cash_source;
pub mod spot_fees;
pub mod strategy_portfolio;

pub use scalars::{DbCounter, DecimalValue, Id, Revision, SchemaV1, Timestamp};

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(
    evidence::MetricValueV1,
    evidence::MetricRequirementV1,
    execution::NativeTaskParametersV1,
    execution::NativeJobOutputIndexV1,
    execution::NativeDataQualityReportV1,
    runtime_jobs::JobSpecV1,
    catalogs::RuntimeCatalogMetadataV1,
    portfolio::AllocationInputV1,
    portfolio::AllocationResultV1,
    science::NativeForecastRequestV1,
    science::NativeForecastResultV1,
    factors::NativeFactorEvaluationRequestV1,
    factors::NativeFactorEvaluationResultV1,
    factors::NativeFactorEvaluationSummaryV1,
    science::ExperimentEvaluationParametersV1,
    science::NativeExperimentEvaluationRequestV1,
    science::NativeExperimentEvaluationResultV1,
    science::NativeAlphaValidationRequestV1,
    science::NativeAlphaValidationResultV1,
    science::NativeAlphaSealedRequestV1,
    science::NativeAlphaSealedResultV1,
    science::NativeFrozenCalibrationV1,
    capacity::NativeCapacityRequestV1,
    capacity::NativeCapacityReportV1,
    capacity::NativeAccountSizingRequestV1,
    capacity::NativeAccountSizingResultV1,
    science::NativeTargetBlendRequestV1,
    science::NativeTargetBlendResultV1,
    science::NativeSimulationRequestV1,
    science::NativeSimulationResultV1,
    science::NativePortfolioStudyRequestV1,
    science::NativePortfolioStudyResultV1,
    strategy_portfolio::NativeStrategyCompositionRequestV1,
    strategy_portfolio::NativeStrategyCompositionResultV1,
    account_observation::AccountObservationSubmitV1,
    account_observation::AccountObservationSubmitV2,
    equity_curve::EquityCurveV1,
    forward::NativeForwardRequestV1,
    forward::NativeForwardResultV1
)))]
struct NativeContracts;

/// Source-derived scientific schemas, not an HTTP service specification.
pub fn openapi_json() -> Result<String, serde_json::Error> {
    let mut document = NativeContracts::openapi();
    document.info.title = "QuaZonai native scientific contracts".into();
    let mut value = serde_json::to_value(document)?;
    value.sort_all_objects();
    Ok(serde_json::to_string(&value)? + "\n")
}
