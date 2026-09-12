//! Real Parquet, Wasmi, native CV and OLS. Synthetic market data never confers PASS.
#[path = "support/command.rs"]
mod command;
#[path = "support/market.rs"]
mod market;
use contracts::{
    brief::TargetKind,
    evidence::MetricStatus,
    research::{SplitKind, SplitPolicyV1},
    science::{
        NativeAlphaValidationRequestV1, NativeAlphaValidationResultV1, NativeSimulationRequestV1,
    },
    Id, SchemaV1,
};
use job::validation::{validate_alpha, ScoreCalibration};
use market::{count, forecast_request, market, module};

fn request(source: &NativeSimulationRequestV1) -> NativeAlphaValidationRequestV1 {
    NativeAlphaValidationRequestV1 {
        schema_version: SchemaV1,
        forecast: forecast_request(source),
        split_policy: SplitPolicyV1 {
            schema_version: SchemaV1,
            kind: SplitKind::WalkForward,
            train_size: count(8),
            test_size: count(3),
            step_size: Some(count(3)),
            group_count: None,
            test_group_count: None,
            purge_observations: count(2),
            embargo_observations: count(1),
            label_horizon_observations: Some(count(2)),
            interval_validation_required: true,
            sealed_revision_id: Id::new(),
        },
        target_kind: TargetKind::Score,
    }
}

#[test]
fn independent_native_folds_fit_only_original_training_labels() {
    let (directory, source) = market("0", 25);
    let request = request(&source);
    let actual = validate_alpha(directory.path(), &request, &module("local.get 0")).unwrap();
    assert_eq!(actual.folds.len(), 6);
    assert_eq!(actual.unique_test_observations.get(), 18);
    let first = &actual.folds[0];
    assert_eq!(first.training_ordinals, (2..10).collect::<Vec<_>>());
    assert_eq!(
        first
            .test_points
            .iter()
            .map(|p| p.observation.ordinal)
            .collect::<Vec<_>>(),
        vec![13, 14, 15]
    );
    let scores = (3..11)
        .map(|i| 1.0 + f64::from(i) * 0.001)
        .collect::<Vec<_>>();
    let labels = scores
        .iter()
        .map(|price| (price + 0.002) / price - 1.0)
        .collect::<Vec<_>>();
    let reference = ScoreCalibration::fit(&scores, &labels).unwrap();
    let calibration = first.calibration.as_ref().unwrap();
    assert_eq!(calibration.training_observations.get(), 8);
    assert_eq!(calibration.status, MetricStatus::Ok);
    assert!((calibration.intercept.unwrap() - reference.coefficients()[0]).abs() < 1e-12);
    assert!((calibration.slope.unwrap() - reference.coefficients()[1]).abs() < 1e-12);
    for point in &first.test_points {
        let forecast = point.observation.forecast.unwrap();
        let predicted = reference.predict(&[forecast]).unwrap()[0];
        assert!((point.expected_return.unwrap() - predicted).abs() < 1e-12);
        assert!(point.observation.label_available_ns.unwrap() > point.observation.available_ns);
        assert!(point.observation.label_reason.is_none());
    }
    assert!(first.metrics.iter().all(|m| m.status == MetricStatus::Ok));
    assert!(first.metrics[0].value.unwrap() < -0.999);
    // Independent elementary RMSE reference; the implementation invokes ndarray-stats.
    let reference_rmse = (first
        .test_points
        .iter()
        .map(|p| (p.expected_return.unwrap() - p.observation.label_return.unwrap()).powi(2))
        .sum::<f64>()
        / 3.0)
        .sqrt();
    assert!((reference_rmse - first.metrics[1].value.unwrap()).abs() < 1e-15);
    assert_eq!(actual.native_versions["solow-cv"], "0.7.3");
    assert_eq!(actual.native_versions["ndarray-stats"], "0.7.0");
    assert!(actual.consumed_fuel.get() > 0);
}

fn counter_model() -> Vec<u8> {
    wat::parse_str("(module (global $n (mut f64) (f64.const 0)) (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) global.get $n f64.const 1 f64.add global.set $n global.get $n))").unwrap()
}

#[test]
fn every_asset_fold_train_test_and_disjoint_block_has_fresh_model_state() {
    let (directory, source) = market("0", 32);
    let mut request = request(&source);
    request.target_kind = TargetKind::ExpectedReturn;
    request.split_policy.kind = SplitKind::CpcvFixedHorizon;
    request.split_policy.train_size = count(3);
    request.split_policy.test_size = count(2);
    request.split_policy.step_size = None;
    request.split_policy.group_count = Some(4);
    request.split_policy.test_group_count = Some(2);
    let actual = validate_alpha(directory.path(), &request, &counter_model()).unwrap();
    assert_eq!(actual.folds.len(), 12);
    assert_eq!(actual.unique_test_observations.get(), 56);
    let all_tests: usize = actual.folds.iter().map(|f| f.test_points.len()).sum();
    assert!(all_tests > actual.unique_test_observations.get() as usize);
    for fold in actual.folds {
        assert!(fold.calibration.is_none());
        let mut previous = None;
        let mut expected = 0.0;
        for point in &fold.test_points {
            if previous != point.observation.ordinal.checked_sub(1) {
                expected = 0.0;
            }
            expected += 1.0;
            assert_eq!(point.observation.forecast, Some(expected));
            assert_eq!(point.expected_return, Some(expected));
            assert!(!fold.training_ordinals.contains(&point.observation.ordinal));
            previous = Some(point.observation.ordinal);
        }
    }
}

#[test]
fn missing_calibration_or_correlation_is_not_a_zero_metric() {
    let (directory, source) = market("0", 25);
    let mut request = request(&source);
    let result = validate_alpha(directory.path(), &request, &module("f64.const 7")).unwrap();
    for fold in result.folds {
        let calibration = fold.calibration.unwrap();
        assert_eq!(calibration.status, MetricStatus::InsufficientData);
        assert_eq!(
            calibration.reason_code.as_deref(),
            Some("CALIBRATION_CONSTANT_SCORE")
        );
        assert!(calibration.intercept.is_none() && calibration.slope.is_none());
        assert!(fold.test_points.iter().all(|p| p.expected_return.is_none()));
        assert!(fold.metrics.iter().all(|m| m.value.is_none()
            && m.reason_code.is_some()
            && m.status == MetricStatus::InsufficientData));
    }
    request.target_kind = TargetKind::ExpectedReturn;
    let huge = validate_alpha(directory.path(), &request, &module("f64.const 1.7e308")).unwrap();
    for fold in huge.folds {
        assert_eq!(fold.metrics[1].status, MetricStatus::Failed);
        assert_eq!(fold.metrics[1].value, None);
        assert_eq!(
            fold.metrics[1].reason_code.as_deref(),
            Some("NATIVE_METRIC_NONFINITE")
        );
    }
}

#[test]
fn original_horizon_whole_task_fuel_and_all_folds_are_required() {
    let (directory, source) = market("0", 25);
    let mut full = request(&source);
    let wasm = counter_model();
    let mut one = full.clone();
    one.forecast.selection.bar_types.truncate(1);
    let one = validate_alpha(directory.path(), &one, &wasm).unwrap();
    full.forecast.parameters.total_fuel = one.consumed_fuel;
    assert!(validate_alpha(directory.path(), &full, &wasm).is_err());
    full = request(&source);
    full.split_policy.label_horizon_observations = Some(count(1));
    assert!(validate_alpha(directory.path(), &full, &wasm).is_err());
    full = request(&source);
    full.split_policy.train_size = count(100);
    assert!(validate_alpha(directory.path(), &full, &wasm).is_err());
    full = request(&source);
    assert!(validate_alpha(directory.path(), &full, &module("f64.const nan")).is_err());
}

#[test]
fn real_job_process_returns_native_fold_evidence_and_rejects_unknown_fields() {
    use std::{ffi::OsStr, io::Write};
    let (directory, source) = market("0", 25);
    let request = request(&source);
    let mut model = tempfile::NamedTempFile::new().unwrap();
    model.write_all(&module("local.get 0")).unwrap();
    let args = [
        OsStr::new("validate-alpha"),
        OsStr::new("--catalog"),
        directory.path().as_os_str(),
        OsStr::new("--model"),
        model.path().as_os_str(),
    ];
    let output = command::command(&args, &request);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let report: NativeAlphaValidationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report.folds.len(), 6);
    let mut invalid = serde_json::to_value(request).unwrap();
    invalid["test_labels"] = serde_json::json!([1, 2, 3]);
    let failed = command::command(&args, &invalid);
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    assert_eq!(failed.stderr, b"QZ_NATIVE_JOB_FAILED\n");
}
