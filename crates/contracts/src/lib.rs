//! Strict wire types shared by the control-plane entrypoints.
//!
//! This initial slice does not implement HTTP routes, persistence, authorization,
//! independent evaluation, or the complete Issue 62 acceptance contract.
#![forbid(unsafe_code)]

pub mod account_observation;
pub mod capital_exit;
pub mod agent_evaluation;
pub mod artifacts;
pub mod auth;
pub mod brief;
pub mod budget;
pub mod catalogs;
pub mod codex;
pub mod control;
pub mod cycles;
pub mod data;
pub mod delivery;
pub mod equity_curve;
pub mod evidence;
pub mod execution;
pub mod execution_assumptions;
pub mod execution_preflight;
pub mod experiment_summary;
pub mod experiments;
pub mod forward;
pub mod http;
pub mod imports;
pub mod lifecycle;
pub mod portfolio;
pub mod portfolio_history;
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
    execution_preflight::ExecutionPreflightScopeV1,
    execution_preflight::PolymarketPreflightSideV1,
    execution_preflight::PolymarketPreflightQuantityV1,
    execution_preflight::PolymarketPreflightTimeInForceV1,
    execution_preflight::PolymarketPreflightOrderV1,
    execution_preflight::PolymarketExecutionPreflightV1,
    execution_preflight::ExecutionPreflightFailureStageV1,
    execution_preflight::PolymarketExecutionPreflightOutcomeV1,
    execution_preflight::PolymarketExecutionPreflightReportV1,
    account_observation::NativeClientObservationSchemaV2,
    account_observation::AccountObservationSubmitV2,
    account_observation::AccountObservationReceiptV2,
    account_observation::AccountClientBindingV2,
    http::Problem,
    catalogs::RecordedFeatureFragmentV1,
    catalogs::RecordedFeatureInputsV1,
    data::RecordedFeatureRegisterV1,
    data::RecordedFeatureRegisterIntentV1,
    data::RecordedFeatureSourceBindingV1,
    data::RecordedFeatureViewV1,
    data::RecordedFeatureListQuery,
    data::RecordedFeatureListV1,
    strategy_portfolio::StrategyOutputKindV1,
    strategy_portfolio::StrategyAlphaAdoptV1,
    strategy_portfolio::StrategyAlphaAdoptIntentV1,
    strategy_portfolio::AcceptedExperimentSourceV1,
    strategy_portfolio::FeatureReplayInitializationV1,
    strategy_portfolio::FrozenTargetPolicyV1,
    strategy_portfolio::StrategyAlphaVersionV1,
    strategy_portfolio::AlphaVersionEnvelopeV2,
    strategy_portfolio::StrategyAllocationMethodV1,
    strategy_portfolio::StrategyPortfolioSourceV1,
    strategy_portfolio::StrategyReleaseSourceV1,
    strategy_portfolio::StrategyMandateContentV1,
    strategy_portfolio::StrategyMandateCreateV1,
    strategy_portfolio::StrategyMandateViewV1,
    strategy_portfolio::MandateCreateEnvelopeV2,
    strategy_portfolio::MandateViewEnvelopeV2,
    strategy_portfolio::StrategyMemberSelectionV1,
    strategy_portfolio::FreshPaperCashV1,
    strategy_portfolio::StrategyCurrentInputsV1,
    strategy_portfolio::StrategyPortfolioPurposeV1,
    strategy_portfolio::StrategyPortfolioBuildV1,
    strategy_portfolio::PortfolioBuildEnvelopeV2,
    strategy_portfolio::NativeStrategyMemberV1,
    strategy_portfolio::StrategyInputProvenanceV1,
    strategy_portfolio::NativeStrategyCompositionRequestV1,
    strategy_portfolio::StrategyCompositionOutcomeV1,
    strategy_portfolio::NativeStrategyCompositionResultV1,
    strategy_portfolio::StrategyPortfolioCandidateV1,
    strategy_portfolio::StrategyPortfolioSummaryV1,
    strategy_portfolio::StrategyPortfolioSummaryOutcomeV1,
    experiment_summary::NativeSimulationSummaryV1,
    strategy_portfolio::PortfolioCandidateEnvelopeV2,
    strategy_portfolio::PortfolioCandidateListEnvelopeV2,
    strategy_portfolio::StrategyReleaseCreateV1,
    strategy_portfolio::ReleaseCreateEnvelopeV2,
    strategy_portfolio::NativeTargetDecisionSourceV1,
    strategy_portfolio::TargetPackageVersionV2,
    strategy_portfolio::TargetPackageV2,
    strategy_portfolio::TargetPackageEnvelopeV2,
    strategy_portfolio::HandoffClaimViewV2,
    strategy_portfolio::StrategyReleaseViewV1,
    strategy_portfolio::ReleaseViewEnvelopeV2,
    capital_exit::CapitalExitPreviewRequestV1,
    capital_exit::CapitalExitPreviewV1,
    capital_exit::CapitalExitStartV1,
    capital_exit::CapitalExitActionV1,
    capital_exit::CapitalExitViewV1,
    capital_exit::CapitalExitClaimV1,
    capital_exit::CapitalExitOwnerEvidenceV1,
    capital_exit::CapitalExitEvidenceViewV1,
    capital_exit::CapitalExitOwnerAssessmentV1,
    capital_exit::CapitalExitAssessmentReceiptV1,
    account_observation::AccountObservationSubmitV1,
    account_observation::AccountObservationReceiptV1,
    account_observation::AccountCurrentV1,
    account_observation::AccountSourceV1,
    equity_curve::EquityCurveQuery,
    equity_curve::EquityCurveV1,
    delivery::ForecastReleaseSourceV2,
    delivery::ForecastEvaluationSourceV2,
    delivery::FrozenForwardDatasetV2,
    delivery::ForecastTargetPackageV2,
    science::PaperInitializationRefV1,
    delivery::DownstreamCapabilitiesV1,
    delivery::DownstreamProbeRequestV1,
    delivery::DownstreamProbeViewV1,
    delivery::DownstreamReadinessV1,
    delivery::ReleaseCreateV1,
    delivery::ReleaseApproveV1,
    delivery::ApprovalViewV1,
    delivery::HandoffOfferV1,
    delivery::HandoffClaimV1,
    delivery::PaperInitialExecutionConsumeV1,
    delivery::PaperInitialExecutionStateV1,
    delivery::PaperInitialExecutionViewV1,
    delivery::HandoffAckV1,
    delivery::HandoffAckOutcomeV1,
    delivery::ApprovalRevokeV1,
    delivery::AutomationModeV1,
    delivery::AutomationPolicyContentV1,
    delivery::AutomationAuthorizeV1,
    delivery::AutomationPolicyViewV1,
    delivery::PolicyRevokeV1,
    delivery::PolicyRevocationViewV1,
    delivery::ApprovalRevocationViewV1,
    delivery::HandoffClaimViewV1,
    delivery::HandoffViewV1,
    delivery::HandoffStateV1,
    delivery::ReleaseRejectV1,
    delivery::ReleaseReopenV1,
    delivery::ReleaseDecisionViewV1,
    delivery::ReleaseViewV1,
    portfolio::PortfolioStudyRequestV1,
    cycles::BriefFreezeV1,
    cycles::FrozenBriefV1,
    cycles::CycleStartIntent,
    cycles::ExternalCycleStartIntent,
    cycles::CycleFinishExternalIntent,
    experiments::ExperimentEvaluateIntent,
    experiment_summary::ExperimentSummaryV1,
    science::FeatureObservationsV1,
    science::ExperimentEvaluationParametersV1,
    science::NativeExperimentEvaluationRequestV1,
    science::NativeExperimentEvaluationResultV1,
    cycles::CycleViewV1,
    cycles::CycleStartedV1,
    cycles::CycleSelectionV1,
    cycles::CycleSelectionTrialV1,
    data::DataSourceCreate,
    data::DataSourceUpdate,
    data::DataSourceView,
    data::DataGrantCreate,
    data::DataGrantRevoke,
    data::DataGrantView,
    data::DataGrantRevocationView,
    data::DatasetRegister,
    data::DatasetView,
    data::DatasetEvidenceViewV1,
    data::DatasetQualitySummaryV1,
    data::UniverseView,
    data::DataListQuery,
    data::DataLicenseState,
    catalogs::RuntimeCatalogMetadataV1,
    catalogs::CatalogVersionQuery,
    runtime::RuntimeCapabilitiesV1,
    execution::NativeTaskParametersV1,
    science::PortfolioTargetsV1,
    execution::NativeJobOutputIndexV1,
    execution::NativeDatasetQualityV1,
    execution::NativeDataQualityReportV1,
    execution::NativeModelCompilationV1,
    runtime_jobs::JobSpecV1,
    runtime_jobs::RuntimeJobStatusV1,
    runtime_jobs::RuntimeCancelV1,
    runtime_jobs::ResultManifestV1,
    runtime_jobs::RuntimeObjectReceiptV1,
    runtime::RuntimeProbeRequestV1,
    runtime::RuntimeProbeViewV1,
    runtime::RuntimeReadinessV1,
    settings::IntegrationSecretCreate,
    settings::IntegrationSecretView,
    settings::RuntimeCreate,
    settings::RuntimeUpdate,
    settings::RuntimeView,
    settings::DownstreamCreate,
    settings::DownstreamUpdate,
    settings::DownstreamView,
    agent_evaluation::AgentEvaluationReportV1,
    artifacts::ArtifactCreate,
    artifacts::ArtifactView,
    evidence::AlphaView,
    evidence::AlphaVersionView,
    evidence::CalibrationView,
    evidence::QualificationView,
    execution_assumptions::ExecutionAssumptionsCreateV1,
    forward::DownstreamWeightsSubmitV1,
    forward::DownstreamWeightsViewV1,
    forward::ForwardMessageSubmitV1,
    forward::ForwardMessageViewV1,
    forward::ForwardReportV1,
    forward::ForwardWindowQueryV1,
    forward::ForwardWindowViewV1,
    execution_assumptions::ExecutionAssumptionsViewV1,
    evidence::EvaluationView,
    brief::BriefCreate,
    brief::BriefCreateIntent,
    brief::BriefUpdate,
    brief::BriefView,
    research::InputSetCreate,
    research::InputSetView,
    research::InputSetSummary,
    research::ResearchListQuery,
    research::EvaluationPolicyCreate,
    research::EvaluationPolicyView,
    research::FieldIssue,
    control::MachineSessionView,
    control::ProjectCreate,
    control::ProjectUpdate,
    control::ProjectView,
    control::ListQuery,
    control::PrincipalCreate,
    control::PrincipalUpdate,
    control::PrincipalView,
    control::CredentialIssue,
    control::CredentialRevoke,
    control::CredentialView,
    control::CredentialCreated,
    control::OperatorCommand,
    control::OperatorGrantRequest,
    control::OperatorGrantView,
    auth::BrowserSession,
    SchemaV1,
    Id,
    DbCounter,
    Revision,
    DecimalValue,
    budget::BudgetV1,
    budget::CostEnforcement,
    budget::StopRuleV1,
    codex::ConnectionMode,
    codex::ProfileOrigin,
    codex::SavedModelSettingsV1,
    codex::ModelCapabilityV1,
    codex::ReasoningEffortCapability,
    codex::CodexProfileViewV1,
    codex::CodexObservationV1,
    codex::CodexHomeBindingV1,
    codex::CodexAccountOperationV1,
    codex::CodexAccountStartV1,
    evidence::MetricStatus,
    evidence::EvidenceStatus,
    evidence::Decision,
    evidence::Comparator,
    evidence::MetricRequirementV1,
    evidence::MetricValueV1,
    experiments::ExperimentProposalV1,
    experiments::ExperimentView,
    runs::ProjectState,
    runs::RunState,
    runs::RunKind,
    runs::RunSnapshotV1,
    runs::RunRebalanceViewV1,
    runs::RunRebalanceV1,
    lifecycle::JobLimitsV1,
    lifecycle::RunCancelV1,
    lifecycle::RunListQuery,
    lifecycle::RunEventKind,
    lifecycle::RunStatePayload,
    lifecycle::RunEventV1,
    lifecycle::RunEventBatchV1,
    portfolio::AllocationInputV1,
    portfolio::AllocationResultV1,
    science::NativeForecastRequestV1,
    science::NativeForecastResultV1,
    science::NativeAlphaValidationRequestV1,
    science::NativeAlphaValidationResultV1,
    science::NativeAlphaSealedRequestV1,
    science::NativeAlphaSealedResultV1,
    science::NativeFrozenCalibrationV1,
    science::NativeSimulationRequestV1,
    science::NativeSimulationResultV1,
    spot_cash::ReportCurrencyValuationV1,
    spot_cash::ReportCurrencyDailyReturnsV1,
    spot_cash_report::NativeSpotCashReportV1,
    spot_cash_source::FrozenSpotCandleSourceV1,
    spot_cash_source::NativeSpotCashSourceEvidenceV1,
    spot_cash_report::NativeSpotCashSummaryV1,
    science::NativePortfolioStudyRequestV1,
    science::NativePortfolioStudyResultV1,
    forward::NativeForwardResultV1
)))]
struct DomainContracts;

/// Deterministic, native-generator output. No handwritten parallel JSON schema.
pub fn openapi_json() -> Result<String, serde_json::Error> {
    let mut document = DomainContracts::openapi();
    document.info.title = "QuaZonai typed domain contracts (initial slice)".into();
    document.info.description = Some(
        "Shared strict domain and authentication DTOs. \
         HTTP routes are generated by the server crate; no full-system acceptance is implied."
            .into(),
    );
    let mut value = serde_json::to_value(document)?;
    value.sort_all_objects();
    // Machine-readable artifacts retain every native value without indentation.
    Ok(serde_json::to_string(&value)? + "\n")
}
