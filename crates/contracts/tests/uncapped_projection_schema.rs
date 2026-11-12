//! Complete read projections must not reject valid native evidence by old count caps.
use contracts::experiment_summary::{ExperimentEquityPreviewV1, ExperimentSummaryV1};
use serde_json::to_value;
use utoipa::PartialSchema;

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

#[test]
fn retired_research_fit_and_score_operations_are_not_exposed() {
    let schema = serde_json::from_str::<serde_json::Value>(&contracts::openapi_json().unwrap()).unwrap();
    let components = &schema["components"]["schemas"];
    for name in ["NativeEnsembleFitRequestV1", "NativeEnsembleApplyRequestV1",
        "NativeEnsembleFreezeRequestV1", "NativeReviewAllocationRequestV1",
        "NativeReviewScienceRequestV1", "NativeAlphaValidationRequestV1",
        "NativeAlphaSealedRequestV1", "NativeFactorEvaluationResultV1",
        "NativeFactorEvaluationSummaryV1", "NativeCatalogFactorScanSummaryV1",
        "NativeTradeTickFactorEvaluationResultV1"] {
        assert!(components.get(name).is_none(), "retired schema {name}");
    }
    for name in ["qz.alpha_validation", "qz.alpha_sealed"] {
        assert!(contracts::runtime_jobs::native_output_contract(name, "1").is_none());
    }
    for kind in ["VALIDATE_ALPHA", "EVALUATE_SEALED_ALPHA"] {
        let error = serde_json::from_value::<contracts::execution::NativeTaskParametersV1>(
            serde_json::json!({"operation": kind, "schema_version": 1})
        ).unwrap_err();
        assert!(error.to_string().contains("unknown variant"));
    }
}
