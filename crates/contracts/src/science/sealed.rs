//! Restricted held-out results. No fitting inputs or qualification authority.
use super::{NativeForecastRequestV1, NativeForecastResultV1, NativeValidationMetricV1};
use crate::{brief::TargetKind, DbCounter, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAlphaSealedRequestV1 {
    pub schema_version: SchemaV1,
    pub forecast: NativeForecastRequestV1,
    pub target_kind: TargetKind,
    pub research_available_through_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSealedAssetMetricsV1 {
    pub instrument_id: String,
    pub bar_type: String,
    pub observation_count: DbCounter,
    #[schema(min_items = 2, max_items = 2)]
    pub metrics: Vec<NativeValidationMetricV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAlphaSealedResultV1 {
    pub schema_version: SchemaV1,
    pub forecast: NativeForecastResultV1,
    /// Exactly one entry per original point, including warmup and unlabelled tails.
    #[serde(
        serialize_with = "serialize_returns",
        deserialize_with = "deserialize_returns"
    )]
    #[schema(min_items = 1, max_items = 1000000)]
    pub expected_returns: Vec<Option<f64>>,
    pub calibration_source_report_artifact_id: Option<Id>,
    pub calibration_fit_end_available_ns: Option<DbCounter>,
    pub native_versions: BTreeMap<String, String>,
    #[schema(min_items = 1, max_items = 256)]
    pub assets: Vec<NativeSealedAssetMetricsV1>,
}

fn serialize_returns<S: serde::Serializer>(
    values: &[Option<f64>],
    s: S,
) -> Result<S::Ok, S::Error> {
    if values.iter().flatten().any(|v| !v.is_finite()) {
        return Err(serde::ser::Error::custom("nonfinite expected return"));
    }
    values.serialize(s)
}

fn deserialize_returns<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Option<f64>>, D::Error> {
    let values = Vec::<Option<f64>>::deserialize(d)?;
    if values.iter().flatten().any(|v| !v.is_finite()) {
        return Err(serde::de::Error::custom("nonfinite expected return"));
    }
    Ok(values)
}
