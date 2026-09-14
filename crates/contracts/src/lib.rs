//! Strict wire types shared by the control-plane entrypoints.
//!
//! This initial slice does not implement HTTP routes, persistence, authorization,
//! independent evaluation, or the complete Issue 62 acceptance contract.
#![forbid(unsafe_code)]

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
pub mod evidence;
pub mod execution;
pub mod execution_assumptions;
pub mod experiments;
pub mod forward;
pub mod http;
pub mod lifecycle;
pub mod portfolio;
pub mod portfolio_history;
pub mod research;
pub mod runs;
pub mod runtime;
pub mod runtime_jobs;
pub mod scalars;
pub mod science;
pub mod settings;

pub use scalars::{DbCounter, DecimalValue, Id, Revision, SchemaV1, Timestamp};

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(
    http::Problem,
    delivery::TargetPackageV1,
    delivery::DownstreamCapabilitiesV1,
    delivery::DownstreamProbeRequestV1,
    delivery::DownstreamProbeViewV1,
    delivery::DownstreamReadinessV1,
    delivery::ReleaseCreateV1,
    delivery::ReleaseApproveV1,
    delivery::ApprovalViewV1,
    delivery::ReleaseRejectV1,
    delivery::ReleaseReopenV1,
    delivery::ReleaseDecisionViewV1,
    delivery::ReleaseViewV1,
    portfolio::PortfolioStudyRequestV1,
    cycles::BriefFreezeV1,
    cycles::FrozenBriefV1,
    cycles::CycleStartIntent,
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
    artifacts::ArtifactCreate,
    artifacts::ArtifactView,
    evidence::AlphaView,
    evidence::AlphaVersionView,
    evidence::CalibrationView,
    evidence::QualificationView,
    execution_assumptions::ExecutionAssumptionsCreateV1,
    forward::DownstreamWeightsSubmitV1,
    forward::DownstreamWeightsViewV1,
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
    auth::BootstrapStart,
    auth::BootstrapEnrollment,
    auth::BootstrapConfirm,
    auth::LoginRequest,
    auth::VerifyRequest,
    auth::BootstrapStatus,
    auth::BrowserSession,
    auth::TrustedDevice,
    auth::DeviceList,
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
    science::NativePortfolioStudyRequestV1,
    science::NativePortfolioStudyResultV1
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
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}
