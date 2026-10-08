use serde::{Deserialize, Serialize};
use utoipa::{PartialSchema, ToSchema};

use crate::{DbCounter, DecimalValue, SchemaV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CostEnforcement {
    Unavailable,
    Estimated,
    Exact,
}

// The native schema also expresses the cost tuple and explicit absent execution
// limits. Structural validity does not confer runtime capability.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetV1 {
    pub schema_version: SchemaV1,
    pub max_experiments: u32,
    /// None removes the application slot ceiling; native capability checks remain.
    pub max_parallel_runs: Option<u32>,
    /// None means no application Mission-turn cap.
    pub max_turns_per_mission: Option<u32>,
    /// None means no repair-turn cap; Some(0) explicitly disables repairs.
    pub max_repair_turns: Option<u32>,
    /// None means no application wall-time budget; native service capability still applies.
    pub max_wall_seconds: Option<u32>,
    /// None means no application cumulative CPU budget; CPU rate is separate.
    pub max_cpu_seconds: Option<DbCounter>,
    pub max_memory_mib: Option<u32>,
    /// None removes the task output budget, not transport/parser/storage safety.
    pub max_output_bytes: Option<DbCounter>,
    /// None means no application daily Cycle quota.
    pub max_cycles_per_day: Option<u32>,
    pub min_cycle_interval_seconds: u32,
    pub max_tokens: Option<DbCounter>,
    pub max_cost_decimal: Option<DecimalValue>,
    pub cost_currency: Option<String>,
    pub cost_enforcement: CostEnforcement,
}

pub(crate) fn currency_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    static CODES: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        // Only the pinned native lookup owns membership, not another ISO table.
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
        codes
    });
    ObjectBuilder::new()
        .schema_type(Type::String)
        .min_length(Some(3))
        .max_length(Some(3))
        .enum_values(Some(CODES.iter().cloned()))
        .into()
}

fn optional_cost_currency_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Type};
    OneOfBuilder::new()
        .item(ObjectBuilder::new().schema_type(Type::Null))
        .item(currency_schema())
        .into()
}

impl PartialSchema for BudgetV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{
            AdditionalProperties, AllOfBuilder, KnownFormat, ObjectBuilder, OneOfBuilder,
            SchemaFormat, Type,
        };
        let mut fields = ObjectBuilder::new()
            .schema_type(Type::Object)
            .additional_properties(Some(AdditionalProperties::FreeForm(false)))
            .property("schema_version", SchemaV1::schema())
            .required("schema_version");
        for (name, minimum, maximum, format) in [
            ("max_experiments", 1u64, u32::MAX as u64, KnownFormat::Int64),
            (
                "min_cycle_interval_seconds",
                0,
                u32::MAX as u64,
                KnownFormat::Int64,
            ),
        ] {
            fields = fields
                .property(
                    name,
                    ObjectBuilder::new()
                        .schema_type(Type::Integer)
                        .format(Some(SchemaFormat::KnownFormat(format)))
                        .minimum(Some(minimum))
                        .maximum(Some(maximum)),
                )
                .required(name);
        }
        for (name, minimum) in [
            ("max_parallel_runs", 1_u64),
            ("max_turns_per_mission", 1),
            ("max_repair_turns", 0),
            ("max_cycles_per_day", 1),
            ("max_memory_mib", 1),
        ] {
            fields = fields.property(
                name,
                OneOfBuilder::new()
                    .item(ObjectBuilder::new().schema_type(Type::Null))
                    .item(
                        ObjectBuilder::new()
                            .schema_type(Type::Integer)
                            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int64)))
                            .minimum(Some(minimum))
                            .maximum(Some(u32::MAX as u64)),
                    ),
            );
        }
        for name in ["max_cpu_seconds", "max_output_bytes"] {
            fields = fields.property(name, crate::scalars::optional_positive_db_counter_schema());
        }
        fields = fields
            .property(
                "max_wall_seconds",
                OneOfBuilder::new()
                    .item(ObjectBuilder::new().schema_type(Type::Null))
                    .item(
                        ObjectBuilder::new()
                            .schema_type(Type::Integer)
                            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int64)))
                            .minimum(Some(1_u64))
                            .maximum(Some(u32::MAX as u64)),
                    ),
            )
            .property(
                "max_tokens",
                crate::scalars::optional_positive_db_counter_schema(),
            )
            .property(
                "max_cost_decimal",
                OneOfBuilder::new()
                    .item(ObjectBuilder::new().schema_type(Type::Null))
                    .item(DecimalValue::schema()),
            )
            .property("cost_currency", optional_cost_currency_schema())
            .property(
                "cost_enforcement",
                utoipa::openapi::Ref::from_schema_name("CostEnforcement"),
            )
            .required("cost_enforcement");
        let unavailable = ObjectBuilder::new()
            .property(
                "cost_enforcement",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .enum_values(Some(["UNAVAILABLE"])),
            )
            .required("cost_enforcement")
            .property(
                "max_cost_decimal",
                ObjectBuilder::new().schema_type(Type::Null),
            )
            .property(
                "cost_currency",
                ObjectBuilder::new().schema_type(Type::Null),
            );
        let cost = |mode: &'static str| {
            ObjectBuilder::new()
                .property(
                    "cost_enforcement",
                    ObjectBuilder::new()
                        .schema_type(Type::String)
                        .enum_values(Some([mode])),
                )
                .required("cost_enforcement")
                .property(
                    "max_cost_decimal",
                    AllOfBuilder::new().item(DecimalValue::schema()).item(
                        ObjectBuilder::new()
                            .schema_type(Type::String)
                            .pattern(Some(r"^(?!-)(?=[0-9+.]*[1-9])")),
                    ),
                )
                .required("max_cost_decimal")
                .property("cost_currency", currency_schema())
                .required("cost_currency")
        };
        AllOfBuilder::new()
            .item(fields)
            .item(
                OneOfBuilder::new()
                    .item(unavailable)
                    .item(cost("ESTIMATED"))
                    .item(cost("EXACT").description(Some(
                        "Requires a native exact-cost capability; currently rejected by admission.",
                    ))),
            )
            .into()
    }
}

impl ToSchema for BudgetV1 {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        schemas.push((SchemaV1::name().into_owned(), SchemaV1::schema()));
        SchemaV1::schemas(schemas);
        schemas.push((DecimalValue::name().into_owned(), DecimalValue::schema()));
        DecimalValue::schemas(schemas);
        schemas.push((
            CostEnforcement::name().into_owned(),
            CostEnforcement::schema(),
        ));
        CostEnforcement::schemas(schemas);
    }
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
