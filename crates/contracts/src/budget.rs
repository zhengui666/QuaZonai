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

// Runtime fields and serde remain unchanged. The native schema below also
// expresses the cost tuple; structural validity does not confer capability.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetV1 {
    pub schema_version: SchemaV1,
    pub max_experiments: u32,
    pub max_parallel_runs: u16,
    pub max_turns_per_mission: u16,
    pub max_repair_turns: u16,
    pub max_wall_seconds: u32,
    pub max_cpu_seconds: DbCounter,
    pub max_memory_mib: u32,
    pub max_output_bytes: DbCounter,
    pub max_cycles_per_day: u16,
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
            ("max_parallel_runs", 1, u16::MAX as u64, KnownFormat::Int32),
            (
                "max_turns_per_mission",
                1,
                u16::MAX as u64,
                KnownFormat::Int32,
            ),
            ("max_repair_turns", 0, u16::MAX as u64, KnownFormat::Int32),
            ("max_wall_seconds", 1, u32::MAX as u64, KnownFormat::Int64),
            ("max_memory_mib", 1, u32::MAX as u64, KnownFormat::Int64),
            ("max_cycles_per_day", 1, u16::MAX as u64, KnownFormat::Int32),
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
        for name in ["max_cpu_seconds", "max_output_bytes"] {
            fields = fields
                .property(name, crate::scalars::positive_db_counter_schema())
                .required(name);
        }
        fields = fields
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
