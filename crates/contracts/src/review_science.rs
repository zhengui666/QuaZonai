//! Supplied source metadata validation only; no research scoring.
use crate::SchemaV1;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
