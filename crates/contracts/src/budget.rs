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
    #[schema(format = Int64, minimum = 1, maximum = 4294967295u64)]
    pub max_experiments: u32,
    #[schema(minimum = 1, maximum = 65535)]
    pub max_parallel_runs: u16,
    #[schema(minimum = 1, maximum = 65535)]
    pub max_turns_per_mission: u16,
    #[schema(minimum = 0, maximum = 65535)]
    pub max_repair_turns: u16,
    #[schema(format = Int64, minimum = 1, maximum = 4294967295u64)]
    pub max_wall_seconds: u32,
    #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
    pub max_cpu_seconds: DbCounter,
    #[schema(format = Int64, minimum = 1, maximum = 4294967295u64)]
    pub max_memory_mib: u32,
    #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
    pub max_output_bytes: DbCounter,
    #[schema(minimum = 1, maximum = 65535)]
    pub max_cycles_per_day: u16,
    #[schema(format = Int64, minimum = 0, maximum = 4294967295u64)]
    pub min_cycle_interval_seconds: u32,
    #[schema(schema_with = crate::scalars::optional_positive_db_counter_schema)]
    pub max_tokens: Option<DbCounter>,
    pub max_cost_decimal: Option<DecimalValue>,
    #[schema(schema_with = optional_cost_currency_schema)]
    pub cost_currency: Option<String>,
    pub cost_enforcement: CostEnforcement,
}

fn optional_cost_currency_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Type};
    // Enumerate only the finite wire alphabet. The pinned upstream lookup, also
    // used by domain validation, owns membership; no second currency table.
    let mut codes = Vec::new();
    for a in b'A'..=b'Z' {
        for b in b'A'..=b'Z' {
            for c in b'A'..=b'Z' {
                let bytes = [a, b, c];
                let code = std::str::from_utf8(&bytes).expect("ASCII currency code");
                if iso_currency::Currency::from_code(code).is_some() {
                    codes.push(code.to_owned());
                }
            }
        }
    }
    OneOfBuilder::new()
        .item(ObjectBuilder::new().schema_type(Type::Null))
        .item(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(3))
                .max_length(Some(3))
                .enum_values(Some(codes)),
        )
        .into()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StopRuleV1 {
    pub schema_version: SchemaV1,
    #[schema(minimum = 1, maximum = 65535)]
    pub stop_on_qualified_count: u16,
    pub stop_on_budget: bool,
    #[schema(minimum = 1, maximum = 65535)]
    pub stop_on_no_improvement_trials: Option<u16>,
    pub stop_on_invalid_data: bool,
}
