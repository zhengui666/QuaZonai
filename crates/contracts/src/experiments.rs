//! An experiment proposal is an immutable research intent, never a PASS decision.
use crate::{Id, Revision, SchemaV1, Timestamp};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentProposalV1 {
    #[schema(inline)]
    pub schema_version: SchemaV1,
    #[schema(inline)]
    pub cycle_id: Id,
    #[schema(inline)]
    pub family_id: Id,
    #[schema(inline)]
    pub parent_experiment_id: Option<Id>,
    #[schema(min_length = 1, max_length = 8000)]
    pub hypothesis: String,
    #[schema(min_length = 1, max_length = 8000)]
    pub expected_failure_modes: String,
    #[schema(inline)]
    pub proposal_artifact_id: Id,
    #[schema(inline)]
    pub parameter_artifact_id: Id,
    #[schema(inline)]
    pub code_artifact_id: Option<Id>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExperimentSource {
    Codex,
    Optuna,
    Operator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExperimentOutcome {
    Pending,
    Supported,
    Rejected,
    Invalid,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExperimentResultVisibility {
    Pending,
    Research,
    Restricted,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentView {
    pub id: Id,
    pub project_id: Id,
    pub cycle_id: Id,
    pub family_id: Id,
    pub parent_experiment_id: Option<Id>,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub ordinal: u32,
    pub hypothesis: String,
    pub expected_failure_modes: String,
    pub proposal_artifact_id: Id,
    pub parameter_artifact_id: Option<Id>,
    pub code_artifact_id: Option<Id>,
    pub trial_source: ExperimentSource,
    /// The science execution, distinct from the Mission which authored this proposal.
    pub run_id: Option<Id>,
    pub author_run_id: Option<Id>,
    pub author_attempt_id: Option<Id>,
    pub result_visibility: ExperimentResultVisibility,
    /// Restricted results are null, not fabricated PENDING or successful results.
    pub outcome: Option<ExperimentOutcome>,
    pub outcome_reason: Option<String>,
    pub conclusion_artifact_id: Option<Id>,
    pub revision: Revision,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: Timestamp,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: Timestamp,
}
