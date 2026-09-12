//! Actual Parquet/Wasm/frozen OLS evaluation; synthetic data grants no qualification.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use contracts::{brief::TargetKind, evidence::MetricStatus, science::*, Id, SchemaV1};
use job::validation::{evaluate_sealed_alpha, validate_alpha};
use market::{count, instant, market, module, INTERVAL_NS};

fn prepared() -> (
    tempfile::TempDir,
    NativeAlphaSealedRequestV1,
    NativeFrozenCalibrationV1,
    Vec<u8>,
) {
    let (directory, source) = market("0", 50);
    let wasm = module("local.get 0");
    let mut train = market::alpha_validation_request(&source);
    train.forecast.selection.event_end_ns = count(26 * INTERVAL_NS);
    train.forecast.selection.decision_cutoff_ns = train.forecast.selection.event_end_ns;
    let report = validate_alpha(directory.path(), &train, &wasm).unwrap();
    let frozen = domain::execution::freeze_alpha_calibration(&train, &report, Id::new())
        .unwrap()
        .unwrap();
    let frozen = serde_json::from_slice(&serde_json::to_vec(&frozen).unwrap()).unwrap();
    let mut forecast = market::forecast_request(&source);
    forecast.selection.event_start_ns = count(26 * INTERVAL_NS);
    (
        directory,
        NativeAlphaSealedRequestV1 {
            schema_version: SchemaV1,
            forecast,
            target_kind: TargetKind::Score,
            research_available_through_ns: instant(25),
        },
        frozen,
        wasm,
    )
}

#[test]
fn held_out_native_predictions_apply_persisted_fit_without_refitting_or_losing_raw_scores() {
    let (directory, request, frozen, wasm) = prepared();
    let before = serde_json::to_vec(&frozen).unwrap();
    let result = evaluate_sealed_alpha(directory.path(), &request, &wasm, Some(&frozen)).unwrap();
    assert_eq!(serde_json::to_vec(&frozen).unwrap(), before);
    assert_eq!(result.forecast.points.len(), 50);
    assert_eq!(result.expected_returns.len(), 50);
    assert_eq!(
        result.calibration_source_report_artifact_id,
        Some(frozen.source_report_artifact_id)
    );
    assert_eq!(result.assets.len(), 2);
    for (index, points) in result.forecast.points.chunks(25).enumerate() {
        let fit = &frozen.assets[index].calibration;
        let returns = &result.expected_returns[index * 25..(index + 1) * 25];
        let mut squared = 0.0;
        let mut count = 0;
        for (point, expected) in points.iter().zip(returns) {
            match point.forecast {
                None => assert_eq!(*expected, None),
                Some(score) => {
                    assert!(point.available_ns > request.research_available_through_ns);
                    let reference = score * fit.slope.unwrap() + fit.intercept.unwrap();
                    assert!((expected.unwrap() - reference).abs() < 1e-12);
                    assert!(
                        score > 1.0,
                        "raw SCORE must not be overwritten by expected return"
                    );
                    if let Some(label) = point.label_return {
                        squared += (expected.unwrap() - label).powi(2);
                        count += 1;
                    }
                }
            }
        }
        assert_eq!(result.assets[index].observation_count.get(), count);
        assert_eq!(count, 21);
        assert!(
            (result.assets[index].metrics[1].value.unwrap() - (squared / count as f64).sqrt())
                .abs()
                < 1e-12
        );
        assert!(result.assets[index].metrics[0].value.unwrap() < -0.99);
        assert!(points.last().unwrap().label_return.is_none());
        assert!(returns.last().unwrap().is_some());
    }
    let mut earlier = request.clone();
    earlier.forecast.selection.event_end_ns = count(40 * INTERVAL_NS);
    earlier.forecast.selection.decision_cutoff_ns = earlier.forecast.selection.event_end_ns;
    let earlier = evaluate_sealed_alpha(directory.path(), &earlier, &wasm, Some(&frozen)).unwrap();
    for (p, r) in earlier.forecast.points.iter().zip(earlier.expected_returns) {
        let n = result
            .forecast
            .points
            .iter()
            .position(|x| x.instrument_id == p.instrument_id && x.ordinal == p.ordinal)
            .unwrap();
        assert_eq!(result.forecast.points[n].forecast, p.forecast);
        assert_eq!(result.expected_returns[n], r);
    }
}

#[test]
fn held_out_contract_rejects_missing_fit_wrong_time_asset_horizon_and_forged_results() {
    let (directory, request, frozen, wasm) = prepared();
    assert!(evaluate_sealed_alpha(directory.path(), &request, &wasm, None).is_err());
    let result = evaluate_sealed_alpha(directory.path(), &request, &wasm, Some(&frozen)).unwrap();
    let mut invalid = request.clone();
    invalid.research_available_through_ns = instant(28);
    assert!(evaluate_sealed_alpha(directory.path(), &invalid, &wasm, Some(&frozen)).is_err());
    invalid = request.clone();
    invalid.forecast.parameters.label_horizon_observations += 1;
    assert!(evaluate_sealed_alpha(directory.path(), &invalid, &wasm, Some(&frozen)).is_err());
    invalid = request.clone();
    invalid.forecast.selection.bar_types.reverse();
    assert!(evaluate_sealed_alpha(directory.path(), &invalid, &wasm, Some(&frozen)).is_err());
    invalid = request.clone();
    invalid.target_kind = TargetKind::ExpectedReturn;
    assert!(evaluate_sealed_alpha(directory.path(), &invalid, &wasm, Some(&frozen)).is_err());
    for alter in [0, 1, 2, 3, 4] {
        let mut changed = result.clone();
        match alter {
            0 => {
                changed.expected_returns.pop();
            }
            1 => changed.expected_returns[2] = None,
            2 => changed.calibration_source_report_artifact_id = Some(Id::new()),
            3 => changed.assets[0].observation_count = count(1),
            _ => changed.assets[0].metrics.swap(0, 1),
        }
        assert!(domain::execution::check_alpha_sealed(&request, Some(&frozen), &changed).is_err());
    }
    let mut invalid = result;
    invalid.expected_returns[2] = Some(f64::NAN);
    assert!(serde_json::to_vec(&invalid).is_err());
}

#[test]
fn expected_returns_need_no_fit_and_missing_labels_are_not_zero_metrics() {
    let (directory, mut request, _, _) = prepared();
    request.target_kind = TargetKind::ExpectedReturn;
    request.forecast.selection.event_end_ns = count(29 * INTERVAL_NS);
    let result =
        evaluate_sealed_alpha(directory.path(), &request, &module("f64.const 0.01"), None).unwrap();
    assert_eq!(result.calibration_source_report_artifact_id, None);
    for asset in &result.assets {
        assert_eq!(asset.observation_count.get(), 0);
        for metric in &asset.metrics {
            assert_eq!(metric.status, MetricStatus::InsufficientData);
            assert_eq!(metric.reason_code.as_deref(), Some("NO_COMPLETE_LABELS"));
            assert_eq!(metric.value, None);
        }
    }
    for (point, expected) in result.forecast.points.iter().zip(result.expected_returns) {
        assert_eq!(expected, point.forecast);
    }
}

#[test]
fn real_sealed_cli_consumes_frozen_model_and_rejects_an_unbound_score() {
    let (directory, request, frozen, wasm) = prepared();
    let model = directory.path().join("model.wasm");
    let calibration = directory.path().join("calibration.json");
    std::fs::write(&model, wasm).unwrap();
    std::fs::write(&calibration, serde_json::to_vec(&frozen).unwrap()).unwrap();
    let args = [
        "evaluate-sealed-alpha".as_ref(),
        "--catalog".as_ref(),
        directory.path().as_os_str(),
        "--model".as_ref(),
        model.as_os_str(),
        "--calibration".as_ref(),
        calibration.as_os_str(),
    ];
    let result = native::command(&args, &request);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: NativeAlphaSealedResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    domain::execution::check_alpha_sealed(&request, Some(&frozen), &result).unwrap();
    let result = native::command(&args[..5], &request);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, b"QZ_NATIVE_JOB_FAILED\n");
}
