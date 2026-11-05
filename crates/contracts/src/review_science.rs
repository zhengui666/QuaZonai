//! Bounded read-only diagnostics over original evaluation evidence. No gate or weights.
use crate::{factors::NativeFactorEvaluationResultV1, DbCounter, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewScienceRequestV1 {
    pub schema_version: SchemaV1,
    /// Digest of the immutable source envelope, verified by the trusted caller.
    pub source_digest: String,
    pub analysis: NativeReviewAnalysisV1,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeReviewPartitionV1 {
    Validation,
    Heldout,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeReviewFactorScopeV1 {
    PooledTimeSeries,
    CrossSectional,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum NativeReviewAnalysisV1 {
    Factor {
        evidence: NativeFactorEvaluationResultV1,
        partition: NativeReviewPartitionV1,
        scope: NativeReviewFactorScopeV1,
        /// Number of synchronized event groups per bootstrap block; never rows/assets.
        block_length: u32,
    },
    Selection {
        /// Includes unsuccessful attempts; never inferred as one from a missing ledger.
        total_trials: u32,
        ledger_complete: bool,
        selected_trial_id: String,
        /// Aligned, nonoverlapping same-frequency NET strategy returns, not labels.
        trials: Vec<NativeReviewReturnSeriesV1>,
        timestamps_ns: Vec<DbCounter>,
        frequency: String,
        block_length: u32,
    },
    Portfolio {
        candidate: NativeReviewReturnSeriesV1,
        baseline: NativeReviewReturnSeriesV1,
        timestamps_ns: Vec<DbCounter>,
        frequency: String,
        block_length: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewReturnSeriesV1 {
    pub id: String,
    pub returns: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeReviewMetricStatusV1 {
    Available,
    InsufficientEvidence,
    InvalidInput,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewMetricV1 {
    pub code: String,
    pub value: Option<f64>,
    pub status: NativeReviewMetricStatusV1,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewScienceResultV1 {
    pub schema_version: SchemaV1,
    pub source_digest: String,
    pub method_version: String,
    pub scope: String,
    pub observations: u32,
    pub metrics: Vec<NativeReviewMetricV1>,
    /// Explicit scope and statistical assumptions; never a claim of qualification.
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFactorDiagnosticsV1 {
    pub method_version: String,
    pub scope: NativeReviewFactorScopeV1,
    pub observations: u32,
    pub metrics: Vec<NativeReviewMetricV1>,
    pub limitations: Vec<String>,
}

/// Validates source-owner metadata already bound to actual manifest bytes by the host.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewSourceRequestV1 {
    pub schema_version: SchemaV1,
    /// Kept in original wire form so validation cannot rewrite timestamp precision.
    #[schema(value_type = crate::catalogs::RuntimeCatalogMetadataV1)]
    pub metadata: serde_json::Value,
    pub observed_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewSourceResultV1 {
    pub schema_version: SchemaV1,
    pub method_version: String,
    /// Contract validation only; actual source permission/PIT facts remain the original declarations.
    pub validated: bool,
    #[schema(value_type = crate::catalogs::RuntimeCatalogMetadataV1)]
    pub metadata: serde_json::Value,
}
