//! Optional calculation limits retained for native immutable requests.
use crate::{DbCounter, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobLimitsV1 {
    pub schema_version: SchemaV1,
    /// Zero for trusted non-trial stages or Mission control; scientific trials are positive.
    #[schema(minimum=0, maximum=4294967295u64, format=Int64)]
    pub experiments: u32,
    pub cpu_seconds: Option<DbCounter>,
    #[schema(minimum=1, maximum=4294967295u64, format=Int64)]
    pub wall_seconds: Option<u32>,
    #[schema(minimum=1, maximum=4294967295u64, format=Int64)]
    pub memory_mib: Option<u32>,
    pub output_bytes: Option<DbCounter>,
}
