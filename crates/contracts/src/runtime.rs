//! Runtime observations are transport evidence, not scientific qualification.
use crate::{runs::RunKind, DbCounter, Id, Revision, SchemaV1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

/// Map keys are values too: publish the native count/text constraints for both.
pub(crate) fn engine_versions_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    let text = || {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .min_length(Some(1))
            .max_length(Some(120))
            // Rust's Unicode White_Space set, not JavaScript's extra BOM whitespace.
            .pattern(Some(concat!(
                r"^(?=[\s\S]*[^\u0009-\u000D\u0020\u0085\u00A0\u1680",
                r"\u2000-\u200A\u2028\u2029\u202F\u205F\u3000])",
                r"[^\u0000-\u001F\u007F-\u009F]+(?![\s\S])"
            )))
    };
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .min_properties(Some(1))
        .max_properties(Some(64))
        .property_names(Some(text()))
        .additional_properties(Some(text()))
        .into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeDataKind {
    Bar,
    Quote,
    Trade,
    OrderBook,
    Fundamental,
    Event,
    DerivedFeature,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IsolationProfile {
    OciResearchV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeImageV1 {
    pub job_kind: RunKind,
    /// Native immutable OCI reference. Never a model-selected image.
    #[schema(min_length = 1, max_length = 512)]
    pub image_ref: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeArtifactSchemaV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    #[schema(min_length = 1, max_length = 40)]
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeVenueV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub venue: String,
    #[schema(min_items = 1, max_items = 16)]
    pub instrument_classes: Vec<String>,
    #[schema(min_items = 1, max_items = 7)]
    pub data_kinds: Vec<RuntimeDataKind>,
    pub expiry_and_settlement: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabelIntervalSupportV1 {
    pub fixed_bars: bool,
    pub fixed_duration: bool,
    pub variable_interval: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCapabilitiesV1 {
    pub schema_version: SchemaV1,
    #[schema(min_items = 1, max_items = 1)]
    pub protocol_versions: Vec<SchemaV1>,
    #[schema(min_length = 1, max_length = 120)]
    pub runtime_version: String,
    #[schema(schema_with = engine_versions_schema)]
    pub engine_versions: BTreeMap<String, String>,
    #[schema(min_items = 1, max_items = 8)]
    pub image_refs: Vec<RuntimeImageV1>,
    #[schema(value_type = std::collections::BTreeSet<RunKind>, min_items = 1, max_items = 8)]
    pub job_kinds: Vec<RunKind>,
    #[schema(min_items = 1, max_items = 64)]
    pub artifact_schemas: Vec<RuntimeArtifactSchemaV1>,
    #[schema(min_items = 1, max_items = 7)]
    pub data_kinds: Vec<RuntimeDataKind>,
    #[schema(max_items = 256)]
    pub venues: Vec<RuntimeVenueV1>,
    pub label_interval_support: LabelIntervalSupportV1,
    #[schema(max_items = 64)]
    pub solver_capabilities: Vec<String>,
    #[schema(minimum = 1, maximum = 1024)]
    pub max_cpu: u16,
    #[schema(minimum = 1)]
    pub max_memory_mib: u32,
    #[schema(schema_with = crate::scalars::positive_db_counter_schema)]
    pub max_output_bytes: DbCounter,
    #[schema(minimum = 1)]
    pub max_wall_seconds: u32,
    pub isolation_profile: IsolationProfile,
    pub checked_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProbeRequestV1 {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeProbeFailure {
    NotConfigured,
    EndpointDenied,
    TlsConfiguration,
    Authentication,
    Unavailable,
    ResponseLimit,
    ContractUnsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "status",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum RuntimeProbeOutcomeV1 {
    Available {
        capabilities: Box<RuntimeCapabilitiesV1>,
    },
    Unavailable {
        reason: RuntimeProbeFailure,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProbeViewV1 {
    pub id: Id,
    pub runtime_id: Id,
    pub integration_revision: Revision,
    pub snapshot_artifact_id: Id,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub outcome: RuntimeProbeOutcomeV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeReadinessState {
    NotChecked,
    Disabled,
    Stale,
    Unavailable,
    Available,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReadinessV1 {
    pub schema_version: SchemaV1,
    pub runtime_id: Id,
    pub integration_revision: Revision,
    pub state: RuntimeReadinessState,
    pub latest_observation: Option<RuntimeProbeViewV1>,
    pub available_job_kinds: Vec<RunKind>,
}
