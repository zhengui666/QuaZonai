//! Local source preparation; neither acquisition nor scientific qualification.
use crate::SchemaV1;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeMarketImportRequestV1 {
    pub schema_version: SchemaV1,
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
    /// Unmodified report from the existing market-history importer.
    pub report: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_scope_is_explicit_and_timestamps_stay_strings() {
        let mut request = serde_json::json!({"schema_version":1,"manifest_path":"/input/source.json","instruments_path":"/input/instruments.json","output_path":"/output/new","start_seconds":"1778284800"});
        let decoded: NativeMarketImportRequestV1 = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(decoded.start_seconds.as_deref(), Some("1778284800"));
        assert!(!decoded.received_time);
        request["bar_seconds"] = serde_json::json!(1);
        assert!(serde_json::from_value::<NativeMarketImportRequestV1>(request.clone()).is_err());
        request.as_object_mut().unwrap().remove("bar_seconds");
        request["start_seconds"] = serde_json::json!(1778284800_u64);
        assert!(serde_json::from_value::<NativeMarketImportRequestV1>(request).is_err());
    }
}
