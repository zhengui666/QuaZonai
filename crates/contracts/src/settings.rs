//! Package version discriminator retained by the native target input adapter.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
/// Persisted release/configuration history may contain V1. Only V2 is deliverable.
pub enum PackageSchemaVersion {
    #[serde(rename = "1")]
    V1,
    #[serde(rename = "2")]
    V2,
}

impl PackageSchemaVersion {
    pub const fn is_deliverable(self) -> bool {
        matches!(self, Self::V2)
    }
}
