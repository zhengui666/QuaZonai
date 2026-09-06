//! Finite native metrics and frozen comparison requirements; no numeric estimator.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{DbCounter, DecimalValue, Id, SchemaV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MetricStatus {
    Ok,
    InsufficientData,
    Unsupported,
    InvalidInput,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceStatus {
    Valid,
    Invalid,
    Incomplete,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Pass,
    Reject,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Comparator {
    Gt,
    Ge,
    Lt,
    Le,
    Between,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricRequirementV1 {
    pub schema_version: SchemaV1,
    #[schema(min_length = 1, max_length = 120)]
    pub metric_code: String,
    #[schema(min_length = 1, max_length = 120)]
    pub scope: String,
    pub comparator: Comparator,
    pub threshold_low: Option<DecimalValue>,
    pub threshold_high: Option<DecimalValue>,
    pub required: bool,
    pub minimum_observations: DbCounter,
    #[schema(schema_with = method_allowlist_schema)]
    pub method_allowlist: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricValueV1 {
    pub schema_version: SchemaV1,
    pub evaluation_id: Id,
    pub metric_code: String,
    pub scope: String,
    #[serde(
        serialize_with = "serialize_finite_optional",
        deserialize_with = "deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub value: Option<f64>,
    pub status: MetricStatus,
    pub reason_code: Option<String>,
    pub unit: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub observation_count: DbCounter,
    pub frequency: String,
    #[serde(
        serialize_with = "serialize_finite_optional",
        deserialize_with = "deserialize_finite_optional"
    )]
    #[schema(required = true)]
    pub annualization_factor: Option<f64>,
    pub method_id: String,
    pub method_version: String,
    pub source_artifact_id: Id,
    pub higher_is_better: Option<bool>,
}

// serde_json normally encodes non-finite floats as null. That would erase the
// distinction between corrupt evidence and an honestly missing observation.
fn serialize_finite_optional<S: serde::Serializer>(
    value: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(number) if !number.is_finite() => Err(serde::ser::Error::custom("non-finite metric")),
        _ => value.serialize(serializer),
    }
}

fn deserialize_finite_optional<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    let value = Option::<f64>::deserialize(deserializer)?;
    if value.is_some_and(|number| !number.is_finite()) {
        return Err(serde::de::Error::custom("non-finite metric"));
    }
    Ok(value)
}

// Keep the public Vec<String> wire shape. Native builders express both layers
// without incorrectly attaching string bounds to the enclosing array.
fn method_allowlist_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Type};
    ArrayBuilder::new()
        .min_items(Some(1))
        .max_items(Some(64))
        .unique_items(true)
        .items(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(1))
                .max_length(Some(120)),
        )
        .into()
}
