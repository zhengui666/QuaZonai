//! Quality-adjusted research allocation; never an asset target or independent approval.
use crate::{DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeReviewAllocationKindV1 {
    Factor,
    Portfolio,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewAllocationMemberV1 {
    pub id: String,
    pub source_report_id: String,
    pub score: f64,
    pub observations: Vec<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewMemberCapV1 {
    pub id: String,
    pub maximum_weight: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewClusterCapV1 {
    pub id: String,
    pub members: Vec<String>,
    pub maximum_weight: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewCurrentWeightV1 {
    pub id: String,
    pub weight: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewAllocationRequestV1 {
    pub schema_version: SchemaV1,
    pub kind: NativeReviewAllocationKindV1,
    pub members: Vec<NativeReviewAllocationMemberV1>,
    pub member_caps: Vec<NativeReviewMemberCapV1>,
    pub cluster_caps: Vec<NativeReviewClusterCapV1>,
    pub current_weights: Vec<NativeReviewCurrentWeightV1>,
    /// L1 turnover; empty initial weights are represented by explicit zero rows.
    pub maximum_turnover: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewAllocatedMemberV1 {
    pub id: String,
    pub source_report_id: String,
    pub quality_coefficient: f64,
    pub prior: f64,
    pub weight: DecimalValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeReviewAllocationResultV1 {
    pub schema_version: SchemaV1,
    pub method: String,
    pub kind: NativeReviewAllocationKindV1,
    pub solver_version: String,
    pub members: Vec<NativeReviewAllocatedMemberV1>,
    pub covariance: Vec<Vec<f64>>,
    pub lambda: f64,
    pub objective: f64,
    pub primal_residual: f64,
    pub dual_residual: f64,
    pub requires_new_candidate: bool,
    pub requires_shared_account_study: bool,
}
