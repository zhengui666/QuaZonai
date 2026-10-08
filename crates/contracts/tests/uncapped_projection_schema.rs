//! Complete read projections must not reject valid native evidence by old count caps.
use contracts::{
    data::RecordedFeatureListV1,
    experiment_summary::{ExperimentEquityPreviewV1, ExperimentSummaryV1},
};
use serde_json::to_value;
use utoipa::PartialSchema;

#[test]
fn recorded_feature_listing_schema_keeps_every_registered_part() {
    let schema = to_value(RecordedFeatureListV1::schema()).unwrap();
    let items = &schema["properties"]["items"];
    assert_eq!(items["type"], "array");
    assert!(items.get("maxItems").is_none());
}

#[test]
fn experiment_summary_schema_preserves_complete_folds_and_feature_references() {
    let schema = to_value(ExperimentSummaryV1::schema()).unwrap();
    for field in ["feature_artifact_ids", "folds"] {
        let values = &schema["properties"][field];
        assert_eq!(values["type"], "array");
        assert_eq!(values["minItems"], 1);
        assert!(values.get("maxItems").is_none(), "{field}");
    }
}

#[test]
fn equity_preview_schema_allows_all_mandatory_gap_endpoints() {
    let schema = to_value(ExperimentEquityPreviewV1::schema()).unwrap();
    let points = &schema["properties"]["points"];
    assert_eq!(points["type"], "array");
    assert!(points.get("maxItems").is_none());
}
