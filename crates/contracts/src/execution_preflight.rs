//! External executor order-format requests, never target delivery or execution authority.
use crate::{DbCounter, DecimalValue, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::{PartialSchema, ToSchema};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionPreflightScopeV1 {
    OrderFormatOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolymarketPreflightSideV1 {
    Buy,
    Sell,
}

/// Units are caller choices. Preflight never prices shares or converts collateral.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PolymarketPreflightQuantityV1 {
    Pusd { amount: DecimalValue },
    OutcomeShares { amount: DecimalValue },
}

/// GTD carries its explicit native expiry; no clock or default horizon is supplied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PolymarketPreflightTimeInForceV1 {
    Gtc {},
    Gtd { expire_time_ns: DbCounter },
    Ioc {},
    Fok {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PolymarketPreflightOrderV1 {
    Market {
        time_in_force: PolymarketPreflightTimeInForceV1,
        reduce_only: bool,
    },
    Limit {
        time_in_force: PolymarketPreflightTimeInForceV1,
        price: DecimalValue,
        post_only: bool,
        reduce_only: bool,
    },
}

/// All native identity, sizing, timing and instruction choices are explicit.
/// There is deliberately no release, claim, account credential or approval field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PolymarketExecutionPreflightV1 {
    pub schema_version: SchemaV1,
    pub scope: ExecutionPreflightScopeV1,
    pub trader_id: String,
    pub strategy_id: String,
    pub client_order_id: String,
    pub instrument_id: String,
    /// Caller-generated RFC 4122 UUID version 4, checked by the official model.
    pub initialization_id: String,
    pub initialized_at_ns: DbCounter,
    pub side: PolymarketPreflightSideV1,
    pub quantity: PolymarketPreflightQuantityV1,
    pub order: PolymarketPreflightOrderV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionPreflightFailureStageV1 {
    RequestParsing,
    NativeIdentity,
    NativeRepresentation,
    NativeConstruction,
    OfficialAdapter,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum PolymarketExecutionPreflightOutcomeV1 {
    /// The exact declared format was accepted; no execution readiness is claimed.
    FormatValidated {
        request: PolymarketExecutionPreflightV1,
    },
    Rejected {
        stage: ExecutionPreflightFailureStageV1,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PolymarketExecutionPreflightReportV1 {
    pub schema_version: SchemaV1,
    pub scope: ExecutionPreflightScopeV1,
    pub native_version: String,
    pub outcome: PolymarketExecutionPreflightOutcomeV1,
}

// Utoipa's internally tagged enum derive does not close its variant objects.
// Keep the strict Serde boundary reflected in the source-defined native schema.
fn closed_variant(tag: &str, value: &str) -> utoipa::openapi::schema::ObjectBuilder {
    use utoipa::openapi::schema::{AdditionalProperties, ObjectBuilder, Type};
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(AdditionalProperties::FreeForm(false)))
        .property(
            tag,
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some([value])),
        )
        .required(tag)
}

fn schema_ref<T: ToSchema>() -> utoipa::openapi::Ref {
    utoipa::openapi::Ref::from_schema_name(T::name())
}

fn collect<T: ToSchema>(
    schemas: &mut Vec<(
        String,
        utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
    )>,
) {
    schemas.push((T::name().into_owned(), T::schema()));
    T::schemas(schemas);
}

impl PartialSchema for PolymarketPreflightQuantityV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::OneOfBuilder;
        let variant = |unit| {
            closed_variant("unit", unit)
                .property("amount", DecimalValue::schema())
                .required("amount")
        };
        OneOfBuilder::new()
            .item(variant("PUSD"))
            .item(variant("OUTCOME_SHARES"))
            .into()
    }
}
impl ToSchema for PolymarketPreflightQuantityV1 {}

impl PartialSchema for PolymarketPreflightTimeInForceV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::OneOfBuilder;
        OneOfBuilder::new()
            .item(closed_variant("kind", "GTC"))
            .item(
                closed_variant("kind", "GTD")
                    .property("expire_time_ns", DbCounter::schema())
                    .required("expire_time_ns"),
            )
            .item(closed_variant("kind", "IOC"))
            .item(closed_variant("kind", "FOK"))
            .into()
    }
}
impl ToSchema for PolymarketPreflightTimeInForceV1 {}

impl PartialSchema for PolymarketPreflightOrderV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Type};
        let variant = |kind| {
            closed_variant("kind", kind)
                .property(
                    "time_in_force",
                    schema_ref::<PolymarketPreflightTimeInForceV1>(),
                )
                .required("time_in_force")
                .property(
                    "reduce_only",
                    ObjectBuilder::new().schema_type(Type::Boolean),
                )
                .required("reduce_only")
        };
        OneOfBuilder::new()
            .item(variant("MARKET"))
            .item(
                variant("LIMIT")
                    .property("price", DecimalValue::schema())
                    .required("price")
                    .property("post_only", ObjectBuilder::new().schema_type(Type::Boolean))
                    .required("post_only"),
            )
            .into()
    }
}
impl ToSchema for PolymarketPreflightOrderV1 {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        collect::<PolymarketPreflightTimeInForceV1>(schemas);
    }
}

impl PartialSchema for PolymarketExecutionPreflightOutcomeV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Type};
        OneOfBuilder::new()
            .item(
                closed_variant("status", "FORMAT_VALIDATED")
                    .property("request", schema_ref::<PolymarketExecutionPreflightV1>())
                    .required("request"),
            )
            .item(
                closed_variant("status", "REJECTED")
                    .property("stage", schema_ref::<ExecutionPreflightFailureStageV1>())
                    .required("stage")
                    .property("reason", ObjectBuilder::new().schema_type(Type::String))
                    .required("reason"),
            )
            .into()
    }
}
impl ToSchema for PolymarketExecutionPreflightOutcomeV1 {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        collect::<PolymarketExecutionPreflightV1>(schemas);
        collect::<ExecutionPreflightFailureStageV1>(schemas);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(components(schemas(PolymarketExecutionPreflightV1)))]
    struct RequestOnly;

    #[derive(OpenApi)]
    #[openapi(components(schemas(PolymarketExecutionPreflightReportV1)))]
    struct ReportOnly;

    fn assert_references_resolve(document: &Value, value: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                    assert!(reference.starts_with("#/components/schemas/"));
                    assert!(
                        document.pointer(&reference[1..]).is_some(),
                        "single-root export has unresolved reference {reference}"
                    );
                }
                for child in object.values() {
                    assert_references_resolve(document, child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    assert_references_resolve(document, child);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn request_and_report_single_root_exports_collect_all_native_references() {
        for api in [RequestOnly::openapi(), ReportOnly::openapi()] {
            let document = serde_json::to_value(api).unwrap();
            assert_references_resolve(&document, &document);
            for name in [
                "PolymarketPreflightQuantityV1",
                "PolymarketPreflightOrderV1",
                "PolymarketPreflightTimeInForceV1",
            ] {
                assert!(
                    document["components"]["schemas"].get(name).is_some(),
                    "missing {name}"
                );
            }
        }
    }
}
