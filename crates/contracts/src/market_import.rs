//! Local source preparation; neither acquisition nor scientific qualification.
use crate::SchemaV1;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeMarketImportModality {
    #[default]
    TradeTick,
    BookSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeMarketImportRequestV1 {
    pub schema_version: SchemaV1,
    /// Defaults to original trade import. Book snapshots retain all price/size levels as native CLEAR/ADD batches.
    #[serde(default)]
    pub modality: NativeMarketImportModality,
    pub manifest_path: String,
    pub instruments_path: String,
    /// A new directory under an existing local parent. Existing outputs are never replaced.
    pub output_path: String,
    /// Original importer selection units: UTC epoch seconds, decimal strings.
    pub start_seconds: Option<String>,
    pub end_seconds: Option<String>,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    #[serde(default)]
    pub received_time: bool,
    pub maximum_receive_delay_ns: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeMarketImportResultV1 {
    pub schema_version: SchemaV1,
    pub catalog_path: String,
    pub report_path: String,
    pub research_qualified: bool,
    /// Report projection: integers outside the JavaScript safe range are exact decimal strings.
    /// The original importer JSON remains unchanged at report_path.
    pub report: serde_json::Value,
}

/// One explicitly selected local fill-observation source; not a market trade feed.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeFillObservationImportRequestV1 {
    pub schema_version: SchemaV1,
    pub source_path: String,
    pub source_revision: String,
    pub output_path: String,
    #[serde(default)]
    pub assume_source_local_time_utc: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fill_observation_import_requires_source_and_explicit_clock_assumption() {
        let mut value = serde_json::json!({"schema_version":1,"source_path":"/source.parquet","source_revision":"original","output_path":"/new"});
        let request: NativeFillObservationImportRequestV1 =
            serde_json::from_value(value.clone()).unwrap();
        assert!(!request.assume_source_local_time_utc);
        value["received_time"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<NativeFillObservationImportRequestV1>(value.clone()).is_err()
        );
        value.as_object_mut().unwrap().remove("received_time");
        value.as_object_mut().unwrap().remove("source_revision");
        assert!(serde_json::from_value::<NativeFillObservationImportRequestV1>(value).is_err());
    }
    #[test]
    fn import_scope_is_explicit_and_timestamps_stay_strings() {
        let mut request = serde_json::json!({"schema_version":1,"manifest_path":"/input/source.json","instruments_path":"/input/instruments.json","output_path":"/output/new","start_seconds":"1778284800"});
        let decoded: NativeMarketImportRequestV1 = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(decoded.start_seconds.as_deref(), Some("1778284800"));
        assert!(!decoded.received_time);
        assert_eq!(decoded.modality, NativeMarketImportModality::TradeTick);
        request["bar_seconds"] = serde_json::json!(1);
        assert!(serde_json::from_value::<NativeMarketImportRequestV1>(request.clone()).is_err());
        request.as_object_mut().unwrap().remove("bar_seconds");
        request["start_seconds"] = serde_json::json!(1778284800_u64);
        assert!(serde_json::from_value::<NativeMarketImportRequestV1>(request).is_err());
    }
}
