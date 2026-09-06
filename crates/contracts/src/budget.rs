use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{DbCounter, DecimalValue, SchemaV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CostEnforcement {
    Unavailable,
    Estimated,
    Exact,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BudgetV1 {
    pub schema_version: SchemaV1,
    #[schema(format = Int64, maximum = 4294967295u64)]
    pub max_experiments: u32,
    #[schema(maximum = 65535)]
    pub max_parallel_runs: u16,
    #[schema(maximum = 65535)]
    pub max_turns_per_mission: u16,
    #[schema(maximum = 65535)]
    pub max_repair_turns: u16,
    #[schema(format = Int64, maximum = 4294967295u64)]
    pub max_wall_seconds: u32,
    pub max_cpu_seconds: DbCounter,
    #[schema(format = Int64, maximum = 4294967295u64)]
    pub max_memory_mib: u32,
    pub max_output_bytes: DbCounter,
    #[schema(maximum = 65535)]
    pub max_cycles_per_day: u16,
    #[schema(format = Int64, maximum = 4294967295u64)]
    pub min_cycle_interval_seconds: u32,
    pub max_tokens: Option<DbCounter>,
    pub max_cost_decimal: Option<DecimalValue>,
    pub cost_currency: Option<String>,
    pub cost_enforcement: CostEnforcement,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StopRuleV1 {
    pub schema_version: SchemaV1,
    #[schema(maximum = 65535)]
    pub stop_on_qualified_count: u16,
    pub stop_on_budget: bool,
    #[schema(maximum = 65535)]
    pub stop_on_no_improvement_trials: Option<u16>,
    pub stop_on_invalid_data: bool,
}
