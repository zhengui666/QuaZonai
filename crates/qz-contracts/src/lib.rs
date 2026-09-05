//! Strict wire types shared by the control-plane entrypoints.
//!
//! This initial slice does not implement HTTP routes, persistence, authorization,
//! independent evaluation, or the complete Issue 62 acceptance contract.
#![forbid(unsafe_code)]

pub mod budget;
pub mod codex;
pub mod evidence;
pub mod runs;
pub mod scalars;

pub use scalars::{DbCounter, DecimalValue, Id, Revision, SchemaV1, Timestamp};

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(
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
    evidence::MetricStatus,
    evidence::EvidenceStatus,
    evidence::Decision,
    evidence::Comparator,
    evidence::MetricRequirementV1,
    evidence::MetricValueV1,
    runs::ProjectState,
    runs::RunState,
    runs::RunKind,
    runs::RunSnapshotV1
)))]
struct DomainContracts;

/// Deterministic, native-generator output. No handwritten parallel JSON schema.
pub fn openapi_json() -> Result<String, serde_json::Error> {
    let mut document = DomainContracts::openapi();
    document.info.title = "QuaZonai typed domain contracts (initial slice)".into();
    document.info.description = Some(
        "Shared scalar, budget, Codex-settings, run and metric contracts. \
         No HTTP endpoint or full-system acceptance is implied."
            .into(),
    );
    let mut value = serde_json::to_value(document)?;
    value.sort_all_objects();
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}
