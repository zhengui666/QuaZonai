//! Real Parquet, Wasmi, native CV and OLS. Synthetic market data never confers PASS.
#[path = "support/command.rs"]
mod command;
#[path = "support/market.rs"]
mod market;
use contracts::{
    brief::TargetKind,
    evidence::MetricStatus,
    research::SplitKind,
    science::{NativeAlphaValidationRequestV1, NativeAlphaValidationResultV1},
    Id, SchemaV1,
};
use job::validation::{validate_alpha, ScoreCalibration};
use market::{alpha_validation_request as request, count, market, module};

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
    assert!(accepted(&request, &actual));
    let original = &actual.folds[0].test_points[0].observation;
    let mut changed = actual.clone();
    let repeated = changed.folds[1..]
        .iter_mut()
        .flat_map(|fold| &mut fold.test_points)
        .find(|point| {
            point.observation.instrument_id == original.instrument_id
                && point.observation.ordinal == original.ordinal
        })
        .unwrap();
    repeated.observation.label_return = Some(original.label_return.unwrap() + 1.0);
    assert!(!accepted(&request, &changed));
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
    assert!(accepted(&request, &result));
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
    assert!(accepted(&request, &huge));
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

fn accepted(
    request: &NativeAlphaValidationRequestV1,
    report: &NativeAlphaValidationResultV1,
) -> bool {
    use contracts::{
        execution::NativeTaskParametersV1,
        runtime::RuntimeArtifactSchemaV1,
        runtime_jobs::{RuntimeOutputKind, RuntimeOutputV1},
        Revision,
    };
    let bytes = serde_json::to_vec(report).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::Report,
        schema: RuntimeArtifactSchemaV1 {
            name: "qz.alpha_validation".into(),
            version: "1".into(),
        },
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: count(bytes.len() as u64),
        media_type: "application/json".into(),
    };
    domain::execution::output_bindings(
        &NativeTaskParametersV1::ValidateAlpha {
            schema_version: SchemaV1,
            dataset_revision_id: Id::new(),
            model_artifact_id: Id::new(),
            request: Box::new(request.clone()),
        },
        chrono::DateTime::from_timestamp(1, 0).unwrap(),
        chrono::DateTime::from_timestamp(2, 0).unwrap(),
        &[(output, bytes)],
    )
    .is_ok()
}

#[test]
fn native_output_adoption_requires_every_original_fold_source_and_metric() {
    let (directory, source) = market("0", 25);
    let mut request = request(&source);
    let actual = validate_alpha(directory.path(), &request, &module("local.get 0")).unwrap();
    assert!(accepted(&request, &actual));
    for field in 0..17 {
        let mut bad = actual.clone();
        match field {
            0 => {
                bad.folds.remove(0);
            }
            1 => {
                bad.folds.pop();
            }
            2 => bad.folds[0].source_row_count = count(24),
            3 => bad.folds[0].training_ordinals[0] += 1,
            4 => bad.folds[0].test_points[0].observation.ordinal += 1,
            5 => bad.folds[0].fold_index = 1,
            6 => bad.folds[0].bar_type = "TEST.SIM-1-MINUTE-LAST-EXTERNAL".into(),
            7 => bad.folds[0].calibration = None,
            8 => {
                bad.folds[0]
                    .calibration
                    .as_mut()
                    .unwrap()
                    .training_observations = count(9)
            }
            9 => bad.folds[0].test_points[0].expected_return = None,
            10 => bad.folds[0].test_points[0].observation.label_return = None,
            11 => bad.folds[0].test_points[0].observation.label_available_ns = Some(count(1)),
            12 => bad.folds[0].metrics[0].status = MetricStatus::InsufficientData,
            13 => bad.folds[0].metrics[1].value = Some(-1.0),
            14 => {
                bad.native_versions
                    .insert("solow-cv".into(), "unverified".into());
            }
            15 => bad.unique_test_observations = count(actual.unique_test_observations.get() + 1),
            _ => bad.consumed_fuel = count(request.forecast.parameters.total_fuel.get() + 1),
        }
        assert!(!accepted(&request, &bad), "changed output field {field}");
    }
    request.split_policy.purge_observations = count(3);
    assert!(!accepted(&request, &actual));
    request.split_policy.purge_observations = count(2);
    request.forecast.selection.decision_cutoff_ns = count(10 * market::INTERVAL_NS);
    assert!(!accepted(&request, &actual));
}

#[test]
fn projected_native_metrics_preserve_every_fold_and_feed_the_existing_threshold_gate() {
    use contracts::evidence::{Comparator, Decision, EvidenceStatus, MetricRequirementV1};
    use domain::{evidence::evaluate_metrics, execution::alpha_validation_metrics};
    let (directory, source) = market("0", 25);
    let request = request(&source);
    let actual = validate_alpha(directory.path(), &request, &module("local.get 0")).unwrap();
    let evaluation = Id::new();
    let artifact = Id::new();
    let (metrics, capabilities) =
        alpha_validation_metrics(evaluation, artifact, &request, &actual).unwrap();
    assert_eq!(metrics.len(), actual.folds.len() * 2);
    assert_eq!(capabilities.len(), 2);
    assert_eq!(metrics[0].scope, "asset:0/fold:0");
    assert_eq!(metrics[11].scope, "asset:1/fold:2");
    for (fold, records) in actual.folds.iter().zip(metrics.as_chunks::<2>().0) {
        for (original, record) in fold.metrics.iter().zip(records) {
            assert_eq!(record.evaluation_id, evaluation);
            assert_eq!(record.source_artifact_id, artifact);
            assert_eq!(record.value, original.value);
            assert_eq!(record.status, original.status);
            assert_eq!(record.reason_code, original.reason_code);
            assert_eq!(record.method_version, "0.7.0");
            assert_eq!(record.frequency, "1-MINUTE-LAST-EXTERNAL;horizon=2");
            assert_eq!(
                record.observation_count.get(),
                fold.test_points.len() as u64
            );
            assert!(record.annualization_factor.is_none());
            let first = fold.test_points[0].observation.event_ns.get();
            let last = fold
                .test_points
                .last()
                .unwrap()
                .observation
                .label_available_ns
                .unwrap()
                .get();
            let lower = record.period_start.timestamp_nanos_opt().unwrap() as u64;
            let upper = record.period_end.timestamp_nanos_opt().unwrap() as u64;
            assert!(lower <= first && first - lower < 1000);
            assert!(upper >= last && upper - last < 1000);
        }
        assert_eq!(records[0].unit, "CORRELATION");
        assert_eq!(records[0].method_id, "ndarray-stats.pearson_correlation");
        assert_eq!(records[0].higher_is_better, Some(true));
        assert_eq!(records[1].unit, "RETURN_PER_HORIZON");
        assert_eq!(records[1].method_id, "ndarray-stats.root_mean_sq_err");
        assert_eq!(records[1].higher_is_better, Some(false));
    }
    let requirement = MetricRequirementV1 {
        schema_version: SchemaV1,
        metric_code: "RETURN_RMSE".into(),
        scope: "asset:0/fold:0".into(),
        comparator: Comparator::Le,
        threshold_low: None,
        threshold_high: Some("1".parse().unwrap()),
        required: true,
        minimum_observations: count(3),
        method_allowlist: vec!["ndarray-stats.root_mean_sq_err".into()],
    };
    // Only a numeric threshold check. Fixture provenance still forbids qualification.
    let gate = evaluate_metrics(
        evaluation,
        std::slice::from_ref(&requirement),
        &metrics,
        &capabilities,
    )
    .unwrap();
    assert_eq!(gate.decision, Decision::Pass);
    assert_eq!(gate.evidence_status, EvidenceStatus::Valid);
    let mut wrong_unit = metrics.clone();
    wrong_unit[1].unit = "UNITLESS_SCORE".into();
    let gate = evaluate_metrics(
        evaluation,
        std::slice::from_ref(&requirement),
        &wrong_unit,
        &capabilities,
    )
    .unwrap();
    assert_eq!(gate.decision, Decision::Inconclusive);
    assert_eq!(gate.evidence_status, EvidenceStatus::Unsupported);
    let constant = validate_alpha(directory.path(), &request, &module("f64.const 7")).unwrap();
    let (missing, capabilities) =
        alpha_validation_metrics(evaluation, artifact, &request, &constant).unwrap();
    assert_eq!(missing.len(), constant.folds.len() * 2);
    for records in missing.as_chunks::<2>().0 {
        assert_eq!(records[0].observation_count.get(), 3);
        assert_eq!(records[1].observation_count.get(), 0);
        assert_eq!(records[1].value, None);
        assert_eq!(records[1].status, MetricStatus::InsufficientData);
        assert_eq!(
            records[1].reason_code.as_deref(),
            Some("CALIBRATION_UNAVAILABLE")
        );
    }
    assert_eq!(
        evaluate_metrics(evaluation, &[requirement], &missing, &capabilities)
            .unwrap()
            .decision,
        Decision::Inconclusive
    );
    let mut partial = actual;
    partial.folds.pop();
    assert!(alpha_validation_metrics(evaluation, artifact, &request, &partial).is_err());
}
