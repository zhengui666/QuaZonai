//! Native request schema dependencies. The domain remains the validation authority.
use super::*;
use utoipa::{
    openapi::{
        schema::{AdditionalProperties, AllOfBuilder, ObjectBuilder, OneOfBuilder, Type},
        RefOr,
    },
    PartialSchema,
};

type Schema = utoipa::openapi::schema::Schema;

fn object() -> ObjectBuilder {
    ObjectBuilder::new().schema_type(Type::Object)
}

fn strict_object() -> ObjectBuilder {
    object().additional_properties(Some(AdditionalProperties::FreeForm(false)))
}

fn literal(value: &str) -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .enum_values(Some([value]))
}

fn collect<T: ToSchema>(schemas: &mut Vec<(String, RefOr<Schema>)>) {
    schemas.push((T::name().into_owned(), T::schema()));
    T::schemas(schemas);
}

fn secret_variant(purpose: IntegrationSecretPurpose) -> AllOfBuilder {
    let (minimum, maximum, pattern) = match purpose {
        IntegrationSecretPurpose::Runtime => (
            RUNTIME_CREDENTIAL_MIN_LENGTH,
            8192,
            r"^[\u0021-\u007E]+(?![\s\S])",
        ),
        IntegrationSecretPurpose::Downstream | IntegrationSecretPurpose::CustomProvider => {
            (1, 8192, r"^[\u0021-\u007E]+(?![\s\S])")
        }
        IntegrationSecretPurpose::TlsCa => (1, 65536, r"^[\u0000-\u007F]+(?![\s\S])"),
    };
    AllOfBuilder::new()
        .item(
            strict_object()
                .property("intent", IntegrationSecretIntent::schema())
                .required("intent")
                .property(
                    "value",
                    ObjectBuilder::new()
                        .schema_type(Type::String)
                        .write_only(Some(true))
                        .min_length(Some(minimum))
                        .max_length(Some(maximum))
                        .pattern(Some(pattern)),
                )
                .required("value"),
        )
        .item(object().property(
            "intent",
            object().property("purpose", literal(purpose.code())),
        ))
}

impl PartialSchema for IntegrationSecretCreate {
    fn schema() -> RefOr<Schema> {
        OneOfBuilder::new()
            .item(secret_variant(IntegrationSecretPurpose::Runtime))
            .item(secret_variant(IntegrationSecretPurpose::Downstream))
            .item(secret_variant(IntegrationSecretPurpose::CustomProvider))
            .item(secret_variant(IntegrationSecretPurpose::TlsCa))
            .into()
    }
}

impl ToSchema for IntegrationSecretCreate {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        collect::<IntegrationSecretIntent>(schemas);
    }
}

/// A partial schema intersects the full strict configuration. In particular,
/// SYSTEM_CA does not constrain development_http: native origin validation still
/// requires explicit literal-loopback HTTP or ordinary verified HTTPS.
fn transport_dependency(pinned: bool, creating: bool) -> ObjectBuilder {
    let mut configuration = object().property(
        "tls_policy",
        literal(if pinned { "PINNED_CA" } else { "SYSTEM_CA" }),
    );
    if pinned {
        configuration = configuration.property(
            "development_http",
            ObjectBuilder::new()
                .schema_type(Type::Boolean)
                .enum_values(Some([false])),
        );
    }
    let mut dependency = object().property("configuration", configuration);
    if pinned && creating {
        dependency = dependency
            .property("ca_certificate_ref", Id::schema())
            .required("ca_certificate_ref");
    } else if !pinned {
        dependency = dependency.property(
            "ca_certificate_ref",
            ObjectBuilder::new().schema_type(Type::Null),
        );
    }
    dependency
}

fn runtime_request_schema(creating: bool) -> RefOr<Schema> {
    let mut base = strict_object()
        .property("schema_version", SchemaV1::schema())
        .required("schema_version")
        .property("configuration", RuntimeConfigurationV1::schema())
        .required("configuration")
        .property("ca_certificate_ref", Option::<Id>::schema());
    if creating {
        base = base
            .property("credential_ref", Id::schema())
            .required("credential_ref");
    } else {
        base = base
            .property("expected_revision", Revision::schema())
            .required("expected_revision")
            .property("credential_ref", Option::<Id>::schema());
    }
    AllOfBuilder::new()
        .item(base)
        .item(
            OneOfBuilder::new()
                .item(transport_dependency(false, creating))
                .item(transport_dependency(true, creating)),
        )
        .into()
}

impl PartialSchema for RuntimeCreate {
    fn schema() -> RefOr<Schema> {
        runtime_request_schema(true)
    }
}

impl ToSchema for RuntimeCreate {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        collect::<RuntimeConfigurationV1>(schemas);
    }
}

impl PartialSchema for RuntimeUpdate {
    fn schema() -> RefOr<Schema> {
        runtime_request_schema(false)
    }
}

impl ToSchema for RuntimeUpdate {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        collect::<RuntimeConfigurationV1>(schemas);
    }
}
