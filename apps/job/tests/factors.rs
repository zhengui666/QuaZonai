//! Real native Parquet + Wasmi + ndarray-stats factor-only scientific regressions.
#[path = "support/factors_catalog.rs"]
mod factors_catalog;
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;

use contracts::{factors::*, science::ForecastMissingReason};
use factors_catalog::mixed_catalog;
use job::{
    catalog::load_catalog,
    factors::{evaluate_factor, pearson_ic},
};
use market::{count, module, INTERVAL_NS};
use nautilus_model::instruments::InstrumentAny;
use ndarray::Array2;
use ndarray_stats::CorrelationExt;
use std::path::Path;

const FX: &str = "EUR/USD.SIM";
const EQUITY: &str = "AAA.SIM";
const EMA_SPREAD: &str = "local.get 3 local.get 2 f64.sub f64.const 1000000 f64.mul";
const VOLUME_SCORE: &str = "local.get 4 f64.const 50 f64.sub";

fn point<'a>(
    result: &'a NativeFactorEvaluationResultV1,
    asset: &str,
    minute: u64,
) -> &'a NativeFactorPointV1 {
    result
        .points
        .iter()
        .find(|point| {
            point.observation.instrument_id == asset
                && point.observation.event_ns == count(minute * INTERVAL_NS)
        })
        .unwrap()
}

fn assert_counts(
    metric: &NativeFactorMetricV1,
    observations: u64,
    included: u64,
    warmup: u64,
    incomplete: u64,
    crossing: u64,
    unavailable: u64,
) {
    let counts = &metric.counts;
    assert_eq!(counts.observations.get(), observations);
    assert_eq!(counts.included.get(), included);
    assert_eq!(counts.excluded.get(), observations - included);
    assert_eq!(counts.indicator_warmup.get(), warmup);
    assert_eq!(counts.incomplete_labels.get(), incomplete);
    assert_eq!(counts.cross_boundary_labels.get(), crossing);
    assert_eq!(counts.prediction_outside_partition.get(), unavailable);
    assert_eq!(
        observations,
        included + warmup + incomplete + crossing + unavailable
    );
}

fn native_ic(points: impl Iterator<Item = (f64, f64)>) -> f64 {
    let (scores, labels): (Vec<_>, Vec<_>) = points.unzip();
    Array2::from_shape_vec(
        (2, scores.len()),
        scores.iter().chain(&labels).copied().collect(),
    )
    .unwrap()
    .pearson_correlation()
    .unwrap()[[0, 1]]
}

#[test]
fn two_asset_classes_share_time_cuts_not_asset_ordinal_splits() {
    let (catalog, request) = mixed_catalog(0.0, false);
    let market = load_catalog(catalog.path(), &request.forecast.selection).unwrap();
    assert!(matches!(
        market.series[0].instrument,
        InstrumentAny::CurrencyPair(_)
    ));
    assert!(matches!(
        market.series[1].instrument,
        InstrumentAny::Equity(_)
    ));
    let result = evaluate_factor(catalog.path(), &request, &module(VOLUME_SCORE)).unwrap();
    assert_eq!(result.points.len(), 54);
    assert_eq!(
        serde_json::to_value(&result.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    use NativeFactorPartitionV1::*;
    assert_eq!(point(&result, FX, 6).observation.ordinal, 5);
    assert_eq!(point(&result, EQUITY, 11).observation.ordinal, 5);
    assert_eq!(point(&result, FX, 6).partition, Train);
    assert_eq!(point(&result, EQUITY, 11).partition, Validation);
    assert_eq!(point(&result, FX, 11).partition, Validation);
    assert_eq!(point(&result, FX, 10).partition, Validation);
    assert_eq!(point(&result, FX, 22).partition, Heldout);
    assert_eq!(
        result
            .partitions
            .iter()
            .map(|p| p.partition)
            .collect::<Vec<_>>(),
        [Train, Validation, Heldout]
    );
    for partition in &result.partitions {
        assert_eq!(
            (partition.start_ns, partition.end_ns),
            domain::factors::partition_bounds(&request, partition.partition)
        );
        assert_eq!(
            partition
                .assets
                .iter()
                .map(|a| a.instrument_id.as_str())
                .collect::<Vec<_>>(),
            [FX, EQUITY]
        );
    }
    assert_counts(&result.partitions[0].metric, 14, 6, 4, 0, 4, 0);
    assert_counts(&result.partitions[1].metric, 18, 14, 0, 0, 4, 0);
    assert_counts(&result.partitions[2].metric, 22, 18, 0, 4, 0, 0);
    assert_counts(&result.partitions[0].assets[0].metric, 9, 5, 2, 0, 2, 0);
    assert_counts(&result.partitions[0].assets[1].metric, 5, 1, 2, 0, 2, 0);
    assert_eq!(
        result.partitions[0].assets[1].metric.pearson_ic.status,
        NativeFactorIcStatusV1::InsufficientSamples
    );
    assert_eq!(result.partitions[0].assets[1].metric.pearson_ic.value, None);
}

#[test]
fn two_wasm_factor_structures_keep_arbitrary_signed_scores_and_original_forecasts() {
    let (catalog, request) = mixed_catalog(0.0, false);
    let market = load_catalog(catalog.path(), &request.forecast.selection).unwrap();
    for body in [EMA_SPREAD, VOLUME_SCORE] {
        let wasm = module(body);
        let original = job::forecast::forecast(catalog.path(), &request.forecast, &wasm).unwrap();
        let result = evaluate_factor(catalog.path(), &request, &wasm).unwrap();
        assert_eq!(result.consumed_fuel, original.consumed_fuel);
        assert!(result.consumed_fuel.unwrap().get() > 0);
        for (factor, native) in result.points.iter().zip(&original.points) {
            assert_eq!(
                serde_json::to_value(&factor.observation).unwrap(),
                serde_json::to_value(native).unwrap()
            );
        }
        assert!(result
            .points
            .iter()
            .any(|p| p.observation.forecast.is_some_and(|score| score < -1.0)));
        if body == VOLUME_SCORE {
            assert!(result
                .points
                .iter()
                .any(|p| p.observation.forecast.is_some_and(|score| score > 1.0)));
            for (series, points) in market.series.iter().zip(
                result
                    .points
                    .chunk_by(|a, b| a.observation.instrument_id == b.observation.instrument_id),
            ) {
                for (bar, point) in series.bars.iter().zip(points).skip(2) {
                    assert_eq!(point.observation.forecast, Some(bar.volume.as_f64() - 50.0));
                }
            }
        }
        assert_eq!(result.methods["ic"], "ndarray-stats.pearson_correlation");
        assert_eq!(result.native_versions["ndarray-stats"], "0.7.0");
        assert_eq!(result.native_versions["wasmi"], "2.0.0");
        assert_eq!(result.native_versions["nautilus-model"], "0.63.0");
        // Every report matches the official raw calculation on precisely uncensored pairs.
        for partition in &result.partitions {
            let pairs = || {
                result
                    .points
                    .iter()
                    .filter(|p| p.partition == partition.partition && p.exclusion_reason.is_none())
            };
            let expected = native_ic(pairs().map(|p| {
                (
                    p.observation.forecast.unwrap(),
                    p.observation.label_return.unwrap(),
                )
            }));
            assert_eq!(partition.metric.pearson_ic.value, Some(expected));
            for asset in &partition.assets {
                let selected = pairs()
                    .filter(|p| p.observation.instrument_id == asset.instrument_id)
                    .collect::<Vec<_>>();
                if selected.len() >= 2 {
                    assert_eq!(
                        asset.metric.pearson_ic.value,
                        Some(native_ic(selected.iter().map(|p| (
                            p.observation.forecast.unwrap(),
                            p.observation.label_return.unwrap()
                        ))))
                    );
                }
            }
        }
    }
}

#[test]
fn event_horizons_and_late_label_availability_are_both_censored() {
    let (catalog, request) = mixed_catalog(0.0, true);
    let result = evaluate_factor(catalog.path(), &request, &module(VOLUME_SCORE)).unwrap();
    use NativeFactorExclusionReasonV1::*;
    let delayed = point(&result, FX, 7);
    assert_eq!(delayed.label_event_ns, Some(count(9 * INTERVAL_NS)));
    assert_eq!(
        delayed.observation.label_available_ns,
        Some(count(11 * INTERVAL_NS + 1))
    );
    assert!(delayed.observation.label_return.is_some());
    assert_eq!(delayed.exclusion_reason, Some(LabelCrossesPartition));
    // A label exactly at the exclusive event boundary is not a training sample.
    assert_eq!(
        point(&result, FX, 8).label_event_ns,
        Some(request.train_end_ns)
    );
    assert_eq!(
        point(&result, FX, 8).exclusion_reason,
        Some(LabelCrossesPartition)
    );
    // Sparse Equity labels use actual future bar clocks, not horizon * one minute.
    assert_eq!(
        point(&result, EQUITY, 7).label_event_ns,
        Some(count(11 * INTERVAL_NS))
    );
    assert_eq!(
        point(&result, EQUITY, 7).exclusion_reason,
        Some(LabelCrossesPartition)
    );
    assert_eq!(
        point(&result, FX, 9).exclusion_reason,
        Some(PredictionOutsidePartition)
    );
    assert_eq!(
        point(&result, FX, 19).exclusion_reason,
        Some(LabelCrossesPartition)
    );
    assert_eq!(
        point(&result, FX, 21).exclusion_reason,
        Some(PredictionOutsidePartition)
    );
    assert_eq!(
        point(&result, FX, 35).exclusion_reason,
        Some(LabelNotComplete)
    );
    assert_eq!(point(&result, FX, 35).label_event_ns, None);
    assert_eq!(
        point(&result, FX, 1).exclusion_reason,
        Some(IndicatorWarmup)
    );
    assert_eq!(
        point(&result, FX, 1).observation.label_reason,
        Some(ForecastMissingReason::IndicatorWarmup)
    );
    assert_counts(&result.partitions[0].metric, 14, 5, 4, 0, 4, 1);
    assert_counts(&result.partitions[1].metric, 18, 13, 0, 0, 4, 1);
    for p in result
        .points
        .iter()
        .filter(|p| p.exclusion_reason.is_none())
    {
        let (start, end) = domain::factors::partition_bounds(&request, p.partition);
        for clock in [
            p.observation.event_ns,
            p.observation.available_ns,
            p.label_event_ns.unwrap(),
            p.observation.label_available_ns.unwrap(),
        ] {
            assert!(clock >= start && clock < end);
        }
    }
}

#[test]
fn availability_equal_to_exclusive_cut_is_not_an_eligible_sample() {
    let (catalog, mut request) = mixed_catalog(0.0, false);
    request.train_end_ns = count(9 * INTERVAL_NS + 1);
    let result = evaluate_factor(catalog.path(), &request, &module(VOLUME_SCORE)).unwrap();
    assert_eq!(
        point(&result, FX, 7).label_event_ns,
        Some(count(9 * INTERVAL_NS))
    );
    assert_eq!(
        point(&result, FX, 7).observation.label_available_ns,
        Some(request.train_end_ns)
    );
    assert_eq!(
        point(&result, FX, 7).exclusion_reason,
        Some(NativeFactorExclusionReasonV1::LabelCrossesPartition)
    );
    assert_eq!(
        point(&result, FX, 9).observation.available_ns,
        request.train_end_ns
    );
    assert_eq!(
        point(&result, FX, 9).exclusion_reason,
        Some(NativeFactorExclusionReasonV1::PredictionOutsidePartition)
    );
}

#[test]
fn changing_future_prices_never_changes_earlier_scores_but_does_change_labels() {
    let (original_root, request) = mixed_catalog(0.0, false);
    let (changed_root, changed_request) = mixed_catalog(4.0, false);
    for body in [EMA_SPREAD, VOLUME_SCORE] {
        let original = evaluate_factor(original_root.path(), &request, &module(body)).unwrap();
        let changed =
            evaluate_factor(changed_root.path(), &changed_request, &module(body)).unwrap();
        for (before, after) in original
            .points
            .iter()
            .zip(&changed.points)
            .filter(|(p, _)| p.observation.event_ns < count(26 * INTERVAL_NS))
        {
            assert_eq!(before.observation.forecast, after.observation.forecast);
            assert_eq!(before.observation.event_ns, after.observation.event_ns);
            assert_eq!(
                before.observation.available_ns,
                after.observation.available_ns
            );
            assert_eq!(before.partition, after.partition);
        }
        assert_ne!(
            point(&original, FX, 24).observation.label_return,
            point(&changed, FX, 24).observation.label_return
        );
        for index in [0, 1] {
            assert_eq!(
                serde_json::to_value(&original.partitions[index]).unwrap(),
                serde_json::to_value(&changed.partitions[index]).unwrap()
            );
        }
    }
}

#[test]
fn constant_outputs_and_empty_partitions_truthfully_report_null_ic() {
    let (catalog, request) = mixed_catalog(0.0, false);
    let constant = evaluate_factor(catalog.path(), &request, &module("f64.const -12345")).unwrap();
    for partition in &constant.partitions {
        assert_eq!(
            partition.metric.pearson_ic.status,
            NativeFactorIcStatusV1::ConstantScore
        );
        assert_eq!(partition.metric.pearson_ic.value, None);
    }
    let mut narrow = request;
    narrow.train_end_ns = count(2 * INTERVAL_NS);
    narrow.validation_end_ns = count(3 * INTERVAL_NS);
    let result = evaluate_factor(catalog.path(), &narrow, &module(VOLUME_SCORE)).unwrap();
    for partition in &result.partitions[..2] {
        assert_eq!(partition.metric.counts.included.get(), 0);
        assert_eq!(
            partition.metric.pearson_ic.status,
            NativeFactorIcStatusV1::InsufficientSamples
        );
        assert_eq!(partition.metric.pearson_ic.value, None);
    }
    // An asset with no rows in a partition is still represented, not silently dropped.
    assert_eq!(
        result.partitions[1].assets[1]
            .metric
            .counts
            .observations
            .get(),
        0
    );
}

#[test]
fn native_pearson_reports_negative_raw_ic_and_unavailable_numerical_cases() {
    use NativeFactorIcStatusV1::*;
    let result = pearson_ic(&[-20.0, 3.0, 26.0], &[0.8, 0.4, 0.0]).unwrap();
    assert_eq!(result.status, Available);
    assert!((result.value.unwrap() + 1.0).abs() < 1e-12);
    for (scores, labels, status) in [
        (vec![], vec![], InsufficientSamples),
        (vec![7.0], vec![0.2], InsufficientSamples),
        (vec![7.0, 7.0], vec![0.2, 0.3], ConstantScore),
        (vec![-4.0, 7.0], vec![0.2, 0.2], ConstantLabel),
        (vec![f64::NAN, 7.0], vec![0.2, 0.3], NonfiniteData),
        (vec![-4.0, 7.0], vec![0.2, f64::INFINITY], NonfiniteData),
        (vec![-f64::MAX, f64::MAX], vec![0.2, 0.3], NonfiniteResult),
    ] {
        let actual = pearson_ic(&scores, &labels).unwrap();
        assert_eq!(actual.status, status);
        assert_eq!(actual.value, None);
    }
    assert!(pearson_ic(&[1.0], &[]).is_err());
}

#[test]
fn bad_boundaries_and_existing_forecast_rules_fail_before_any_catalog_or_model_read() {
    let (_, original) = mixed_catalog(0.0, false);
    for (train, validation) in [(0, 22), (22, 22), (25, 22), (10, 38), (37, 38)] {
        let mut request = original.clone();
        request.train_end_ns = count(train * INTERVAL_NS);
        request.validation_end_ns = count(validation * INTERVAL_NS);
        let error =
            evaluate_factor(Path::new("does-not-exist"), &request, b"not-wasm").unwrap_err();
        let Some(domain::DomainError::Fields(issues)) = error.downcast_ref::<domain::DomainError>()
        else {
            panic!("expected domain boundary diagnostic")
        };
        assert_eq!(issues[0].code, "FACTOR_PARTITION_BOUNDARIES");
    }
    let mut request = original.clone();
    request.forecast.selection.event_start_ns = request.train_end_ns;
    assert!(domain::factors::evaluation_request(&request).is_err());
    for invalid in 0..3 {
        let mut request = original.clone();
        match invalid {
            0 => request.forecast.parameters.label_horizon_observations = 0,
            1 => request.forecast.parameters.fast_period = request.forecast.parameters.slow_period,
            _ => request.forecast.parameters.total_fuel = Some(count(0)),
        }
        assert!(domain::factors::evaluation_request(&request).is_err());
    }
}

#[test]
fn strict_wire_contract_rejects_paths_accounts_targets_and_numeric_nanoseconds() {
    let (_, request) = mixed_catalog(0.0, false);
    let wire = serde_json::to_value(&request).unwrap();
    for key in [
        "catalog",
        "model",
        "targets",
        "settings",
        "target_kind",
        "portfolio",
    ] {
        let mut changed = wire.clone();
        changed[key] = serde_json::json!("not a request field");
        assert!(serde_json::from_value::<NativeFactorEvaluationRequestV1>(changed).is_err());
    }
    for key in ["train_end_ns", "validation_end_ns"] {
        let mut changed = wire.clone();
        changed[key] = serde_json::json!(100);
        assert!(serde_json::from_value::<NativeFactorEvaluationRequestV1>(changed).is_err());
    }
    let mut changed = wire.clone();
    changed["forecast"]["settings"] = serde_json::json!({});
    assert!(serde_json::from_value::<NativeFactorEvaluationRequestV1>(changed).is_err());
    let mut changed = wire;
    changed["schema_version"] = serde_json::json!(2);
    assert!(serde_json::from_value::<NativeFactorEvaluationRequestV1>(changed).is_err());
    let schemas: serde_json::Value =
        serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    for name in [
        "NativeFactorEvaluationRequestV1",
        "NativeFactorEvaluationResultV1",
        "NativeFactorEvaluationSummaryV1",
    ] {
        assert_eq!(
            schemas["components"]["schemas"][name]["additionalProperties"],
            false
        );
    }
}

#[test]
fn unmetered_factor_only_execution_produces_no_targets_positions_or_account_settings() {
    let (catalog, mut request) = mixed_catalog(0.0, false);
    let wasm = module(VOLUME_SCORE);
    let mut metered = evaluate_factor(catalog.path(), &request, &wasm).unwrap();
    request.forecast.parameters.total_fuel = None;
    let unmetered = evaluate_factor(catalog.path(), &request, &wasm).unwrap();
    assert_eq!(unmetered.consumed_fuel, None);
    metered.consumed_fuel = None;
    metered.request = request;
    let actual = serde_json::to_value(&unmetered).unwrap();
    assert_eq!(serde_json::to_value(metered).unwrap(), actual);
    let roundtrip: NativeFactorEvaluationResultV1 = serde_json::from_value(actual.clone()).unwrap();
    assert_eq!(roundtrip.points.len(), 54);
    let text = serde_json::to_string(&actual).unwrap();
    for forbidden in [
        "\"targets\"",
        "\"target_points\"",
        "\"positions\"",
        "\"orders\"",
        "\"settings\"",
        "\"qualifications\"",
        "\"PASS\"",
        "\"rmse\"",
    ] {
        assert!(
            !text.contains(forbidden),
            "factor output contained {forbidden}"
        );
    }
    let mut omitted = actual;
    omitted.as_object_mut().unwrap().remove("consumed_fuel");
    assert!(serde_json::from_value::<NativeFactorEvaluationResultV1>(omitted).is_err());
}

#[test]
fn wasm_state_is_isolated_between_instruments_and_fuel_is_one_task_budget() {
    let (catalog, mut request) = mixed_catalog(0.0, false);
    let wasm = wat::parse_str("(module (global $n (mut f64) (f64.const 0)) (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) global.get $n f64.const 1 f64.add global.set $n global.get $n))").unwrap();
    let both = evaluate_factor(catalog.path(), &request, &wasm).unwrap();
    assert_eq!(point(&both, FX, 3).observation.forecast, Some(1.0));
    assert_eq!(point(&both, EQUITY, 5).observation.forecast, Some(1.0));
    let mut single = request.clone();
    single.forecast.selection.bar_types.truncate(1);
    let one = evaluate_factor(catalog.path(), &single, &wasm).unwrap();
    assert!(both.consumed_fuel > one.consumed_fuel);
    request.forecast.parameters.total_fuel = one.consumed_fuel;
    assert!(evaluate_factor(catalog.path(), &request, &wasm).is_err());
}

#[test]
fn real_cli_accepts_strict_stdin_and_emits_only_factor_json() {
    let (catalog, request) = mixed_catalog(0.0, false);
    let model = catalog.path().join("factor.wasm");
    let wasm = module(VOLUME_SCORE);
    std::fs::write(&model, &wasm).unwrap();
    let args = [
        "evaluate-factor".as_ref(),
        "--catalog".as_ref(),
        catalog.path().as_os_str(),
        "--model".as_ref(),
        model.as_os_str(),
    ];
    let output = native::command(&args, &request);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: NativeFactorEvaluationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    let expected = evaluate_factor(catalog.path(), &request, &wasm).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let mut summary_args = args.to_vec();
    summary_args.push("--summary-only".as_ref());
    let summary = native::command(&summary_args, &request);
    assert!(
        summary.status.success(),
        "{}",
        String::from_utf8_lossy(&summary.stderr)
    );
    let summary_json: serde_json::Value = serde_json::from_slice(&summary.stdout).unwrap();
    let typed: NativeFactorEvaluationSummaryV1 = serde_json::from_slice(&summary.stdout).unwrap();
    assert_eq!(serde_json::to_value(typed).unwrap(), summary_json);
    let original_json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(serde_json::from_value::<NativeFactorEvaluationSummaryV1>(original_json).is_err());
    let mut full: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        full.as_object_mut()
            .unwrap()
            .remove("points")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        54
    );
    assert_eq!(summary_json, full);
    assert!(summary.stdout.len() < output.stdout.len() / 2);
    let mut bad = serde_json::to_value(&request).unwrap();
    bad["model"] = serde_json::json!("not accepted in stdin");
    let output = native::command(&args, &bad);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field `model`"));
    let mut bad = request;
    bad.validation_end_ns = bad.train_end_ns;
    let output = native::command(&args, &bad);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("FACTOR_PARTITION_BOUNDARIES"));
}

#[test]
fn nonfinite_or_trapping_models_fail_honestly_without_publishing_a_result() {
    let (catalog, request) = mixed_catalog(0.0, false);
    let model = catalog.path().join("bad-factor.wasm");
    for body in ["f64.const nan", "f64.const inf", "unreachable"] {
        let wasm = module(body);
        assert!(evaluate_factor(catalog.path(), &request, &wasm).is_err());
        std::fs::write(&model, &wasm).unwrap();
        let output = native::command(
            &[
                "evaluate-factor".as_ref(),
                "--catalog".as_ref(),
                catalog.path().as_os_str(),
                "--model".as_ref(),
                model.as_os_str(),
            ],
            &request,
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("SIGNAL_EXECUTION_FAILED"));
    }
}

#[test]
fn iterative_evaluation_never_reads_the_final_heldout_suffix() {
    let (catalog, mut request) = mixed_catalog(0.0, false);
    request.forecast.selection.event_end_ns = request.validation_end_ns;
    let result = evaluate_factor(catalog.path(), &request, &module(VOLUME_SCORE)).unwrap();
    assert!(result
        .points
        .iter()
        .all(|p| p.observation.event_ns < request.validation_end_ns));
    assert_eq!(result.partitions[2].metric.counts.observations.get(), 0);
    assert_eq!(
        result.partitions[2].metric.pearson_ic.status,
        NativeFactorIcStatusV1::InsufficientSamples
    );
    let (perturbed, _) = mixed_catalog(500.0, false);
    let other = evaluate_factor(perturbed.path(), &request, &module(VOLUME_SCORE)).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::to_value(other).unwrap()
    );
}

#[test]
fn durable_host_runs_real_factor_plugins_and_unattended_feedback() {
    let (catalog, request) = mixed_catalog(0.0, false);
    let input = catalog.path().join("research-input.json");
    std::fs::write(
        &input,
        serde_json::to_vec(&serde_json::json!({
            "catalog": catalog.path(), "request": request
        }))
        .unwrap(),
    )
    .unwrap();
    let output = std::process::Command::new("node")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../research"))
        .args([
            "--experimental-strip-types",
            "--test",
            "test/native-science.ts",
        ])
        .env("QZ_JOB_BINARY", env!("CARGO_BIN_EXE_job"))
        .env("QZ_NATIVE_FIXTURE", input)
        .output()
        .expect("Node and apps/research npm ci are required for durable integration");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
