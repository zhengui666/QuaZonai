//! Field-dependent native Runtime schemas, generated from the actual output registry.
use super::{RuntimeOutputV1, NATIVE_OUTPUT_CONTRACTS};
use utoipa::{
    openapi::{
        schema::{AdditionalProperties, ObjectBuilder, OneOfBuilder, Schema, Type},
        RefOr,
    },
    PartialSchema, ToSchema,
};

fn strict_object() -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(AdditionalProperties::FreeForm(false)))
}

fn literal(value: &str) -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .enum_values(Some([value]))
}

impl PartialSchema for RuntimeOutputV1 {
    fn schema() -> RefOr<Schema> {
        let mut alternatives = OneOfBuilder::new();
        for contract in NATIVE_OUTPUT_CONTRACTS {
            alternatives = alternatives.item(
                strict_object()
                    .property("kind", literal(contract.kind.code()))
                    .required("kind")
                    .property(
                        "schema",
                        strict_object()
                            .property("name", literal(contract.name))
                            .required("name")
                            .property("version", literal("1"))
                            .required("version"),
                    )
                    .required("schema")
                    .property("storage_ref", crate::Id::schema())
                    .required("storage_ref")
                    .property("storage_version", literal("1"))
                    .required("storage_version")
                    .property(
                        "byte_count",
                        crate::scalars::positive_db_counter_schema(),
                    )
                    .required("byte_count")
                    .property("media_type", literal(contract.media_type))
                    .required("media_type"),
            );
        }
        alternatives.into()
    }
}

impl ToSchema for RuntimeOutputV1 {}
