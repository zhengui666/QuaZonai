//! Real native Parquet/Wasm source panels; independent scalar regression/QP oracles.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use contracts::{ensemble_fit::*, factors::*, science::*, SchemaV1};
use market::{count, INTERVAL_NS};
use nautilus_model::types::Price;
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;

/// A declared synthetic quadratic price path, on the original five-decimal grid.
/// With the native SMA seed, a linear price path has a constant fast/slow EMA
/// spread, so it cannot identify the slope in the independent scalar OLS oracle.
fn nonlinear_market() -> (tempfile::TempDir, NativeSimulationRequestV1) {
    let (original, simulation) = market::market("0", 100);
    let data = job::catalog::load_catalog(original.path(), &simulation.selection).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    let price =
        |ticks: u64| Price::from(format!("{}.{:05}", ticks / 100_000, ticks % 100_000).as_str());
    for mut series in data.series {
        assert!(series.instrument_updates.is_empty());
        let base = match series.bar_type.instrument_id().to_string().as_str() {
            "EUR/USD.SIM" => 100_000,
            "GBP/USD.SIM" => 200_000,
            other => panic!("unexpected fixture instrument: {other}"),
        };
        catalog.write_instruments(vec![series.instrument]).unwrap();
        for bar in &mut series.bars {
            let n = bar.ts_event.as_u64() / INTERVAL_NS;
            let close = base + 100 * n + n * n;
            bar.open = price(close);
            bar.high = price(close + 20);
            bar.low = price(close - 20);
            bar.close = price(close);
        }
        catalog
            .write_to_parquet(&series.bars, None, None, None)
            .unwrap();
    }
    // Preserve the original separately declared fixture venue-fee source.
    let fees = contracts::catalogs::MAKER_TAKER_FEES_FILE;
    std::fs::copy(original.path().join(fees), directory.path().join(fees)).unwrap();
    (directory, simulation)
}

fn fixture() -> (tempfile::TempDir, NativeEnsembleFitRequestV1, Vec<Vec<u8>>) {
    let (catalog, simulation) = nonlinear_market();
    let source = NativeFactorEvaluationRequestV1 {
        schema_version: SchemaV1,
        forecast: market::forecast_request(&simulation),
        train_end_ns: count(70 * INTERVAL_NS),
        validation_end_ns: count(90 * INTERVAL_NS),
    };
    let modules = vec![
        market::module("local.get 0"),
        market::module("local.get 2 local.get 3 f64.sub"),
    ];
    let members = modules
        .iter()
        .enumerate()
        .map(|(i, module)| NativeEnsembleFitMemberV1 {
            factor_version_id: format!("factor-{i}"),
            source_evidence_ref: format!("native-report-{i}"),
            report: job::factors::evaluate_factor(catalog.path(), &source, module).unwrap(),
        })
        .collect();
    let request = NativeEnsembleFitRequestV1 {
        schema_version: SchemaV1,
        fit_end_ns: source.train_end_ns,
        members,
        folds: vec![
            NativeEnsembleFoldV1 {
                train_end_ns: count(25 * INTERVAL_NS),
                validation_start_ns: count(25 * INTERVAL_NS),
                validation_end_ns: count(45 * INTERVAL_NS),
            },
            NativeEnsembleFoldV1 {
                train_end_ns: count(45 * INTERVAL_NS),
                validation_start_ns: count(45 * INTERVAL_NS),
                validation_end_ns: count(65 * INTERVAL_NS),
            },
        ],
    };
    (catalog, request, modules)
}
fn application(
    catalog: &std::path::Path,
    request: &NativeEnsembleFitRequestV1,
    modules: &[Vec<u8>],
    fit: NativeEnsembleFitResultV1,
) -> NativeEnsembleApplyRequestV1 {
    NativeEnsembleApplyRequestV1 {
        schema_version: SchemaV1,
        fit,
        members: request
            .members
            .iter()
            .zip(modules)
            .map(|(member, module)| {
                let forecast_request = member.report.request.forecast.clone();
                NativeEnsembleApplyMemberV1 {
                    factor_version_id: member.factor_version_id.clone(),
                    forecast: job::forecast::forecast_factor(catalog, &forecast_request, module)
                        .unwrap(),
                    forecast_request,
                }
            })
            .collect(),
    }
}
fn scalar_ols(x: &[f64], y: &[f64]) -> (f64, f64) {
    let mx = x.iter().sum::<f64>() / x.len() as f64;
    let my = y.iter().sum::<f64>() / y.len() as f64;
    let slope = x
        .iter()
        .zip(y)
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>()
        / x.iter().map(|a| (a - mx).powi(2)).sum::<f64>();
    (my - slope * mx, slope)
}
#[test]
fn original_panels_fit_fold_local_ols_and_match_independent_scalar_qp() {
    let (_catalog, request, _) = fixture();
    let fit = job::ensemble_fit::fit(&request).unwrap();
    let mut predicted = Vec::new();
    let mut labels = Vec::new();
    for evidence in &fit.folds {
        let fold = &evidence.fold;
        for (m, member) in request.members.iter().enumerate() {
            for c in &evidence.member_calibrations[m] {
                let points: Vec<_> = member
                    .report
                    .points
                    .iter()
                    .filter(|p| {
                        p.observation.instrument_id == c.instrument_id
                            && p.observation.forecast.is_some()
                            && p.observation.label_return.is_some()
                            && p.observation.event_ns < fold.train_end_ns
                            && p.observation.available_ns < fold.train_end_ns
                            && p.label_event_ns.unwrap() < fold.train_end_ns
                            && p.observation.label_available_ns.unwrap() < fold.train_end_ns
                    })
                    .collect();
                let scores: Vec<_> = points
                    .iter()
                    .map(|p| p.observation.forecast.unwrap())
                    .collect();
                let mean = scores.iter().sum::<f64>() / scores.len() as f64;
                let centered_ss = scores.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
                assert!(
                    centered_ss > 1e-9,
                    "controlled nonlinear fixture must identify both OLS coefficients: {centered_ss:.17e}"
                );
                let (a, b) = scalar_ols(
                    &scores,
                    &points
                        .iter()
                        .map(|p| p.observation.label_return.unwrap())
                        .collect::<Vec<_>>(),
                );
                assert!(
                    (a - c.intercept).abs() < 1e-10,
                    "fold={} member={m} instrument={} n={} oracle_intercept={a:.17e} native_intercept={:.17e} oracle_slope={b:.17e} native_slope={:.17e} score_min={:.17e} score_max={:.17e} centered_ss={centered_ss:.17e}",
                    fold.train_end_ns.get(),
                    c.instrument_id,
                    scores.len(),
                    c.intercept,
                    c.slope,
                    scores.iter().copied().fold(f64::INFINITY, f64::min),
                    scores.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                );
                assert!(
                    (b - c.slope).abs() < 1e-8,
                    "oracle_slope={b:.17e} native_slope={:.17e}",
                    c.slope
                );
                assert!(c.last_label_available_ns < fold.train_end_ns);
            }
        }
        for (i, p) in request.members[0].report.points.iter().enumerate() {
            let o = &p.observation;
            if o.event_ns < fold.validation_start_ns
                || o.event_ns >= fold.validation_end_ns
                || o.available_ns >= fold.validation_end_ns
                || o.label_available_ns
                    .is_none_or(|v| v >= fold.validation_end_ns)
                || p.label_event_ns.is_none_or(|v| v >= fold.validation_end_ns)
                || o.forecast.is_none()
            {
                continue;
            }
            predicted.push(
                (0..2)
                    .map(|m| {
                        let c = evidence.member_calibrations[m]
                            .iter()
                            .find(|c| c.instrument_id == o.instrument_id)
                            .unwrap();
                        c.intercept
                            + c.slope
                                * request.members[m].report.points[i]
                                    .observation
                                    .forecast
                                    .unwrap()
                    })
                    .collect::<Vec<_>>(),
            );
            labels.push(o.label_return.unwrap());
        }
    }
    let numerator = predicted
        .iter()
        .zip(&labels)
        .map(|(x, y)| (x[0] - x[1]) * (y - x[1]))
        .sum::<f64>();
    let denominator = predicted.iter().map(|x| (x[0] - x[1]).powi(2)).sum::<f64>();
    let oracle = (numerator / denominator).clamp(0.0, 1.0);
    let w: Vec<f64> = fit
        .members
        .iter()
        .map(|m| m.weight.as_decimal().to_string().parse().unwrap())
        .collect();
    // Compare prediction loss: coefficients near a flat optimum are not identifiable.
    let loss = |a: f64| {
        predicted
            .iter()
            .zip(&labels)
            .map(|(x, y)| (a * x[0] + (1.0 - a) * x[1] - y).powi(2))
            .sum::<f64>()
            / labels.len() as f64
    };
    assert!(loss(w[0]) - loss(oracle) < 1e-11);
    assert!((loss(w[0]) - fit.oof_mean_squared_error).abs() < 1e-15);
    assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    let total: bigdecimal::BigDecimal = fit.members.iter().map(|m| m.weight.as_decimal()).sum();
    assert_eq!(total, bigdecimal::BigDecimal::from(1));
    assert_eq!(fit.oof_observations.get(), labels.len() as u64);
}

#[test]
fn native_sma_seed_preserves_warmup_and_exposes_the_linear_spread_counterexample() {
    let (catalog, simulation) = market::market("0", 30);
    let request = market::forecast_request(&simulation);
    let fast =
        job::forecast::forecast_factor(catalog.path(), &request, &market::module("local.get 2"))
            .unwrap();
    let slow =
        job::forecast::forecast_factor(catalog.path(), &request, &market::module("local.get 3"))
            .unwrap();
    let spread = job::forecast::forecast_factor(
        catalog.path(),
        &request,
        &market::module("local.get 2 local.get 3 f64.sub"),
    )
    .unwrap();
    assert_eq!(fast.native_versions["nautilus-indicators"], "0.67.0");
    for (instrument, base) in [("EUR/USD.SIM", 1.0), ("GBP/USD.SIM", 2.0)] {
        for report in [&fast, &slow, &spread] {
            let points: Vec<_> = report
                .points
                .iter()
                .filter(|p| p.instrument_id == instrument)
                .collect();
            assert_eq!(points.len(), 30);
            for point in &points[..2] {
                assert_eq!(point.forecast, None);
                assert_eq!(
                    point.forecast_reason,
                    Some(ForecastMissingReason::IndicatorWarmup)
                );
                assert_eq!(point.label_return, None);
            }
            assert!(points[2..].iter().all(|p| p.forecast.is_some()));
            assert_eq!(points[2].ordinal, 2);
            assert_eq!(points[2].event_ns, count(3 * INTERVAL_NS));
            assert_eq!(points[2].available_ns, count(3 * INTERVAL_NS + 1));
            assert_eq!(
                points[2].label_available_ns,
                Some(count(5 * INTERVAL_NS + 1))
            );
        }
        let values = |report: &NativeForecastResultV1| -> Vec<f64> {
            report
                .points
                .iter()
                .filter(|p| p.instrument_id == instrument)
                .filter_map(|p| p.forecast)
                .collect()
        };
        let fast_values = values(&fast);
        let slow_values = values(&slow);
        let spread_values = values(&spread);
        // Exact decimal path p_n = base + n/1000. SMA_2 at n=2 is
        // base+.0015; its next EMA is base+.0025. SMA_3 at n=3 is
        // base+.002. Thereafter their lags are exactly .0005 and .001.
        for (index, ((fast, slow), spread)) in fast_values
            .iter()
            .zip(&slow_values)
            .zip(&spread_values)
            .enumerate()
        {
            let n = (index + 3) as f64;
            assert!((fast - (base + n / 1000.0 - 0.0005)).abs() < 1e-14);
            assert!((slow - (base + n / 1000.0 - 0.001)).abs() < 1e-14);
            assert!((spread - 0.0005).abs() < 1e-14);
        }
        // Contrast the independently derived old first-value seed at n=3.
        // These differences are semantic, far above binary rounding noise.
        let old_fast = base + 23.0 / 9000.0;
        let old_slow = base + 9.0 / 4000.0;
        assert!((fast_values[0] - old_fast).abs() > 5e-5);
        assert!((slow_values[0] - old_slow).abs() > 2e-4);
        let mean = spread_values.iter().sum::<f64>() / spread_values.len() as f64;
        let centered_ss = spread_values
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>();
        assert!(centered_ss < 1e-26);
    }
    // Exact constants keep the existing explicit rejection; near constants
    // are numerical evidence, not an independently identifiable slope oracle.
    let error =
        job::validation::ScoreCalibration::fit(&[0.0005; 4], &[0.0020, 0.0019, 0.0018, 0.0017])
            .err()
            .expect("exact constant predictor must not fit");
    assert!(error.to_string().contains("CALIBRATION_CONSTANT_SCORE"));
}
#[test]
fn heldout_and_selection_labels_cannot_change_fit_and_future_application_is_frozen() {
    let (catalog, request, modules) = fixture();
    let fit = job::ensemble_fit::fit(&request).unwrap();
    let mut changed = request.clone();
    for member in &mut changed.members {
        for p in &mut member.report.points {
            if p.observation.event_ns >= request.fit_end_ns {
                p.observation.label_return = Some(900.0);
                p.observation.forecast = Some(-800.0);
            }
        }
    }
    assert_eq!(
        serde_json::to_value(&fit).unwrap(),
        serde_json::to_value(job::ensemble_fit::fit(&changed).unwrap()).unwrap()
    );
    let mut apply = application(catalog.path(), &request, &modules, fit);
    let original = job::ensemble_fit::apply(&apply).unwrap();
    assert!(original
        .forecast
        .points
        .iter()
        .all(|p| p.event_ns >= request.fit_end_ns));
    for m in &mut apply.members {
        for p in &mut m.forecast.points {
            if p.label_return.is_some() {
                p.label_return = Some(900.0);
            }
        }
    }
    let changed = job::ensemble_fit::apply(&apply).unwrap();
    assert_eq!(
        original
            .forecast
            .points
            .iter()
            .map(|p| p.forecast)
            .collect::<Vec<_>>(),
        changed
            .forecast
            .points
            .iter()
            .map(|p| p.forecast)
            .collect::<Vec<_>>()
    );
}
#[test]
fn duplicate_predictor_does_not_expand_convex_hull_or_improve_loss() {
    let (catalog, mut request, mut modules) = fixture();
    let fit = job::ensemble_fit::fit(&request).unwrap();
    let original = job::ensemble_fit::apply(&application(
        catalog.path(),
        &request,
        &modules,
        fit.clone(),
    ))
    .unwrap();
    let mut duplicate = request.members[0].clone();
    duplicate.factor_version_id = "duplicate-same-predictor".into();
    duplicate.source_evidence_ref = "duplicate-original-report".into();
    request.members.push(duplicate);
    modules.push(modules[0].clone());
    let duplicate_fit = job::ensemble_fit::fit(&request).unwrap();
    assert!((fit.oof_mean_squared_error - duplicate_fit.oof_mean_squared_error).abs() < 1e-11);
    let applied = job::ensemble_fit::apply(&application(
        catalog.path(),
        &request,
        &modules,
        duplicate_fit,
    ))
    .unwrap();
    assert!(original
        .forecast
        .points
        .iter()
        .zip(&applied.forecast.points)
        .all(|(a, b)| match (a.forecast, b.forecast) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-6,
            (None, None) => true,
            _ => false,
        }));
}
#[test]
fn exact_zero_coefficients_are_valid_and_ignored_without_epsilon() {
    let (catalog, request, modules) = fixture();
    let mut fit = job::ensemble_fit::fit(&request).unwrap();
    fit.members[0].weight = "1".parse().unwrap();
    fit.members[1].weight = "0".parse().unwrap();
    let mut apply = application(catalog.path(), &request, &modules, fit);
    for p in &mut apply.members[1].forecast.points {
        p.forecast = None;
        p.forecast_reason = Some(ForecastMissingReason::IndicatorWarmup);
    }
    let output = job::ensemble_fit::apply(&apply).unwrap();
    for p in output.forecast.points {
        let original = apply.members[0]
            .forecast
            .points
            .iter()
            .find(|q| q.instrument_id == p.instrument_id && q.event_ns == p.event_ns)
            .unwrap();
        let c = apply.fit.members[0]
            .calibrations
            .iter()
            .find(|c| c.instrument_id == p.instrument_id)
            .unwrap();
        assert_eq!(
            p.forecast,
            original.forecast.map(|x| c.intercept + c.slope * x)
        );
    }
}
#[test]
fn malformed_alignment_time_folds_and_future_labels_fail_closed() {
    let (_, request, _) = fixture();
    let mut mixed_horizon = request.clone();
    for member in &mut mixed_horizon.members {
        member.report.request.forecast.selection.bar_types[1] =
            member.report.request.forecast.selection.bar_types[1].replace("MINUTE", "HOUR");
    }
    assert!(job::ensemble_fit::fit(&mixed_horizon)
        .unwrap_err()
        .to_string()
        .contains("COMMON_LABEL_HORIZON"));
    let mut bad = request.clone();
    bad.fit_end_ns = count(71 * INTERVAL_NS);
    assert!(job::ensemble_fit::fit(&bad)
        .unwrap_err()
        .to_string()
        .contains("OUTSIDE_TRAIN"));
    let mut bad = request.clone();
    bad.folds[1].validation_start_ns = bad.folds[0].validation_start_ns;
    assert!(job::ensemble_fit::fit(&bad).is_err());
    let mut bad = request.clone();
    bad.members[1].factor_version_id = bad.members[0].factor_version_id.clone();
    assert!(job::ensemble_fit::fit(&bad).is_err());
    let mut bad = request.clone();
    bad.members[1].report.points.swap(10, 11);
    assert!(job::ensemble_fit::fit(&bad).is_err());
    let mut bad = request.clone();
    for member in &mut bad.members {
        for p in &mut member.report.points {
            if p.observation.label_return.is_some() {
                p.observation.label_available_ns = Some(request.fit_end_ns);
            }
        }
    }
    assert!(job::ensemble_fit::fit(&bad).is_err());
}
#[test]
fn actual_cli_fit_and_apply_emit_typed_native_results() {
    let (catalog, request, modules) = fixture();
    let result = native::command(&["fit-ensemble".as_ref()], &request);
    assert!(result.status.success());
    let fit: NativeEnsembleFitResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    let apply = application(catalog.path(), &request, &modules, fit);
    let result = native::command(&["apply-ensemble".as_ref()], &apply);
    assert!(result.status.success());
    let output: NativeEnsembleApplyResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert!(!output.forecast.points.is_empty());
}

#[test]
fn frozen_native_calibrations_retain_original_training_rows_and_reject_changed_fit() {
    let (catalog, request, modules) = fixture();
    let fit = job::ensemble_fit::fit(&request).unwrap();
    let freeze = NativeEnsembleFreezeRequestV1 {
        schema_version: SchemaV1,
        fit: fit.clone(),
        members: request
            .members
            .iter()
            .map(|m| NativeEnsembleFreezeMemberV1 {
                factor_version_id: m.factor_version_id.clone(),
                report_artifact_id: contracts::Id::new(),
                report: m.report.clone(),
            })
            .collect(),
    };
    let result = native::command(&["freeze-ensemble-calibrations".as_ref()], &freeze);
    assert!(result.status.success());
    let frozen: NativeEnsembleFreezeResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    for (member, source) in frozen.members.iter().zip(&freeze.members) {
        domain::execution::check_alpha_calibration(&member.calibration).unwrap();
        assert_eq!(
            member.calibration.source_report_artifact_id,
            source.report_artifact_id
        );
        assert_eq!(
            member.calibration.selection_rule,
            "TEMPORAL_OOF_FULL_TRAIN_REFIT_V1"
        );
        assert_eq!(member.calibration.fit_end_available_ns, fit.fit_end_ns);
        assert!(member
            .calibration
            .assets
            .iter()
            .all(|a| a.training_end_available_ns < fit.fit_end_ns));
    }
    let mut bad = freeze.clone();
    bad.fit.members[0].calibrations[0].slope += 1.0;
    assert!(job::ensemble_fit::freeze(&bad)
        .unwrap_err()
        .to_string()
        .contains("ORIGINAL_FIT_MISMATCH"));
    let mut bad = freeze.clone();
    bad.members[0].factor_version_id = "another-model".into();
    assert!(job::ensemble_fit::freeze(&bad).is_err());
    // Feed the actual converted models through the established joint-account study.
    let (_, mut study, _) = market::study();
    study.source_selection = request.members[0].report.request.forecast.selection.clone();
    study.evaluation_start_ns = count(71 * INTERVAL_NS);
    study.research_available_through_ns = fit.fit_end_ns;
    study.mandate.rebalance_schedule.interval_seconds = Some(600);
    study.mandate.rebalance_schedule.target_ttl_seconds = 600;
    // Explicit research-method parameter, not inferred owner risk preference.
    // The separate MIN_RISK test retains its own original frozen method.
    study.mandate.objective = contracts::portfolio::AllocationObjective::MaxUtility;
    let mut objects = std::collections::BTreeMap::new();
    objects.insert(
        study.mandate.constraints.transaction_costs_ref,
        serde_json::to_vec(&study.execution_settings).unwrap(),
    );
    for (index, member) in study.members.iter_mut().enumerate() {
        let id = contracts::Id::new();
        member.calibration_artifact_id = Some(id);
        member.target_kind = contracts::brief::TargetKind::Score;
        member.parameters = fit.forecast_contract.parameters.clone();
        member.ensemble_weight = fit.members[index].weight.clone();
        objects.insert(
            id,
            serde_json::to_vec(&frozen.members[index].calibration).unwrap(),
        );
        objects.insert(member.model_artifact_id, modules[index].clone());
    }
    let result =
        job::study::evaluate(catalog.path(), &study, |id| Ok(objects[&id].clone())).unwrap();
    assert_eq!(
        result.frames.len(),
        3,
        "{:?}",
        result
            .frames
            .iter()
            .map(|f| &f.allocation)
            .collect::<Vec<_>>()
    );
    assert!(result.simulation.is_some());
    assert!(result.frames.iter().all(|frame| frame
        .input
        .forecasts
        .members
        .iter()
        .zip(&fit.members)
        .all(|(a, b)| a.ensemble_weight == b.weight)));
    assert!(result.frames[1]
        .input
        .assets
        .iter()
        .any(|a| a.current_weight.is_positive()));
}

#[test]
fn rolling_native_account_accepts_a_single_active_member_without_positive_padding() {
    let (catalog, mut study, module) = market::study();
    study.members[0].ensemble_weight = "1".parse().unwrap();
    study.members[1].ensemble_weight = "0".parse().unwrap();
    let result = job::study::evaluate(catalog.path(), &study, |id| {
        if id == study.mandate.constraints.transaction_costs_ref {
            Ok(serde_json::to_vec(&study.execution_settings).unwrap())
        } else {
            Ok(module.clone())
        }
    })
    .unwrap();
    assert!(result.simulation.is_some());
    assert!(result
        .frames
        .iter()
        .all(|frame| !frame.input.forecasts.members[1]
            .ensemble_weight
            .is_positive()));
    assert_eq!(
        result.frames[0].input.forecasts.members[0].ensemble_weight,
        "1".parse().unwrap()
    );
}

/// Expensive positive integration evidence is exported only by the scientific CI
/// target, then consumed unchanged by the real TypeScript/native integration.
#[test]
fn export_multi_day_native_pm_fixture_when_requested() {
    let Some(destination) = std::env::var_os("QZ_PM_EXPORT_FIXTURE_DIR") else {
        return;
    };
    let root = std::path::PathBuf::from(destination);
    let (catalog, simulation) = market::micro_lot_market("0", 14_000);
    let heldout_request = NativeFactorEvaluationRequestV1 {
        schema_version: SchemaV1,
        forecast: market::forecast_request(&simulation),
        train_end_ns: count(4320 * INTERVAL_NS),
        validation_end_ns: count(8640 * INTERVAL_NS),
    };
    // Exactly match research iterationRequest(data, true): sealed rows remain
    // in the catalog but neither factor panels nor fitting may read them.
    let mut factor_request = heldout_request.clone();
    factor_request.forecast.selection.event_end_ns = factor_request.validation_end_ns;
    factor_request.forecast.selection.decision_cutoff_ns = factor_request.validation_end_ns;
    // On the explicitly synthetic linear price fixture inverse-price is a
    // useful forecast. No production model or metric is manufactured here.
    let bodies = ["local.get 0", "f64.const 1 local.get 0 f64.div"];
    let modules: Vec<_> = bodies.iter().map(|body| market::module(body)).collect();
    let catalog_prefix =
        job::catalog_prefix::digest(catalog.path(), &factor_request.forecast.selection).unwrap();
    let fit_request = NativeEnsembleFitRequestV1 {
        schema_version: SchemaV1,
        fit_end_ns: factor_request.train_end_ns,
        folds: [(1080, 2160), (2160, 3240), (3240, 4320)]
            .into_iter()
            .map(|(start, end)| NativeEnsembleFoldV1 {
                train_end_ns: count(start * INTERVAL_NS),
                validation_start_ns: count(start * INTERVAL_NS),
                validation_end_ns: count(end * INTERVAL_NS),
            })
            .collect(),
        members: modules
            .iter()
            .enumerate()
            .map(|(index, module)| NativeEnsembleFitMemberV1 {
                factor_version_id: format!("multi-day-factor-{index}"),
                source_evidence_ref: format!("native-multi-day-report-{index}"),
                report: job::factors::evaluate_factor(catalog.path(), &factor_request, module)
                    .unwrap(),
            })
            .collect(),
    };
    let fit = job::ensemble_fit::fit(&fit_request).unwrap();
    assert_eq!(
        serde_json::to_value(&catalog_prefix).unwrap(),
        serde_json::to_value(
            job::catalog_prefix::digest(catalog.path(), &factor_request.forecast.selection)
                .unwrap()
        )
        .unwrap()
    );
    let freeze_request = NativeEnsembleFreezeRequestV1 {
        schema_version: SchemaV1,
        fit: fit.clone(),
        members: fit_request
            .members
            .iter()
            .map(|m| NativeEnsembleFreezeMemberV1 {
                factor_version_id: m.factor_version_id.clone(),
                report_artifact_id: contracts::Id::new(),
                report: m.report.clone(),
            })
            .collect(),
    };
    let frozen = job::ensemble_fit::freeze(&freeze_request).unwrap();
    let (catalog, template, _) = market::portfolio_from_market((catalog, simulation));
    let ttl_seconds = 86_400_u32;
    let mut study = NativePortfolioStudyRequestV1 {
        schema_version: SchemaV1,
        settlements: vec![],
        source_selection: template.selection,
        evaluation_start_ns: count(4320 * INTERVAL_NS + 1),
        manual_cutoffs_ns: None,
        calendar: None,
        research_available_through_ns: fit.fit_end_ns,
        mandate: template.mandate,
        execution_settings: template.execution_settings,
        rolling_liquidity: None,
        assets: template.assets,
        members: template.members,
    };
    study.source_selection.event_end_ns = factor_request.validation_end_ns;
    study.source_selection.decision_cutoff_ns = factor_request.validation_end_ns;
    study.mandate.capital_assumption = "10000000".parse().unwrap();
    study.execution_settings.starting_capital = study.mandate.capital_assumption.clone();
    study.execution_settings.snapshot_interval_ms = 60_000;
    study.mandate.objective = contracts::portfolio::AllocationObjective::MaxUtility;
    study.mandate.rebalance_schedule.kind = contracts::portfolio::RebalanceKind::FixedInterval;
    study.mandate.rebalance_schedule.interval_seconds = Some(ttl_seconds);
    study.mandate.rebalance_schedule.target_ttl_seconds = ttl_seconds;
    study.mandate.rebalance_schedule.max_input_age_seconds = 120;
    study.mandate.constraints.min_cash_weight = "0.01".parse().unwrap();
    study.mandate.constraints.max_cash_weight = "0.01".parse().unwrap();
    study.mandate.constraints.min_net_exposure = "0.99".parse().unwrap();
    study.mandate.constraints.max_net_exposure = "0.99".parse().unwrap();
    for asset in &mut study.assets {
        asset.current_weight = "0".parse().unwrap();
    }
    let mut objects = std::collections::BTreeMap::new();
    objects.insert(
        study.mandate.constraints.transaction_costs_ref,
        serde_json::to_vec(&study.execution_settings).unwrap(),
    );
    for (index, member) in study.members.iter_mut().enumerate() {
        let id = contracts::Id::new();
        member.calibration_artifact_id = Some(id);
        member.target_kind = contracts::brief::TargetKind::Score;
        member.parameters = fit.forecast_contract.parameters.clone();
        member.ensemble_weight = fit.members[index].weight.clone();
        objects.insert(
            id,
            serde_json::to_vec(&frozen.members[index].calibration).unwrap(),
        );
        objects.insert(member.model_artifact_id, modules[index].clone());
    }
    let result =
        job::study::evaluate(catalog.path(), &study, |id| Ok(objects[&id].clone())).unwrap();
    assert_eq!(
        result.frames.len(),
        3,
        "{:?}",
        result
            .frames
            .iter()
            .map(|f| &f.allocation)
            .collect::<Vec<_>>()
    );
    let mut replay = result.simulation_request.unwrap();
    // Native dispatch can occur after the provisional cutoff. Preserve the
    // registered protocol's full TTL from each ACTUAL original decision clock.
    for point in &mut replay.target_points {
        point.valid_until_ns = count(point.asof_ns.get() + u64::from(ttl_seconds) * 1_000_000_000);
    }
    let replay_result = job::simulation::simulate(catalog.path(), &replay).unwrap();
    let value = |key: &str| {
        replay_result
            .statistics
            .iter()
            .find(|s| s.native_key == key)
            .and_then(|s| s.value)
    };
    assert_eq!(value("Calendar Expected Days"), Some(2.0));
    assert_eq!(value("Calendar Coverage Ratio"), Some(1.0));
    assert!(value("Calendar Geometric Daily Return").is_some());
    assert!(replay.selection.event_end_ns <= factor_request.validation_end_ns);
    assert!(replay.target_points.iter().all(
        |p| p.valid_until_ns.get() - p.asof_ns.get() == u64::from(ttl_seconds) * 1_000_000_000
    ));
    fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    // A fresh unique directory prevents a previous short catalog contaminating
    // this export. The source names the exact independently persisted catalog.
    let catalog_path = root.join("multi-day-micro-lot-sealed-catalog");
    copy_tree(catalog.path(), &catalog_path);
    let members:Vec<_>=bodies.iter().enumerate().map(|(index,body)|serde_json::json!({
        "factorVersionId":fit_request.members[index].factor_version_id,"sourceTrialId":index+1,
        "model":format!("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) {body}))")
    })).collect();
    // This is an explicit registered TEST baseline, not a learned allocation or
    // a production owner preference. The native study above separately solves
    // asset weights subject to the same 1% cash reserve.
    let policy = serde_json::json!({
        "name":"SYNTHETIC_TEST_ONLY_MICRO_LOT_99_PERCENT_BASELINE",
        "kind":"cross-market-threshold",
        "instrumentIds":["EUR/USD.SIM","GBP/USD.SIM"],
        "allocations":[{"instrumentId":"EUR/USD.SIM","weight":"0.5"},{"instrumentId":"GBP/USD.SIM","weight":"0.5"}],
        "threshold":0,"investedWeight":"0.99",
        "targetTtlNs":(u64::from(ttl_seconds)*1_000_000_000).to_string(),
        "simulationSettings":study.execution_settings,"forecastMode":"price-volume"
    });
    let source = serde_json::json!({"fit":fit,"fitRequest":fit_request,"members":members,"request":factor_request,"catalogPrefix":catalog_prefix,
        "policy":policy,"simulationRequest":replay,"simulation":replay_result,
        "data":{"catalog":catalog_path,"request":factor_request},
        "heldoutData":{"catalog":catalog_path,"request":heldout_request},
        "catalog":catalog_path,"fixtureProvenance":"SYNTHETIC_TEST_ONLY_MICRO_LOT_ORIGINAL_NATIVE_PARQUET_WASM_CLARABEL_NAUTILUS"});
    std::fs::write(
        root.join("source.pending.json"),
        serde_json::to_vec(&source).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("study-request.json"),
        serde_json::to_vec_pretty(&study).unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("objects")).unwrap();
    for (id, bytes) in &objects {
        std::fs::write(root.join("objects").join(id.to_string()), bytes).unwrap();
    }
    // Exercise the actual policy consumer, not a Rust reimplementation of its
    // threshold/union-clock logic. Keep its minute-clock baseline distinct from
    // the explicitly registered daily native-study oracle above.
    let output = std::process::Command::new("node")
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../research"))
        .args(["--experimental-strip-types", "--input-type=module", "-e", r#"
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { canonical } from './src/documents.ts';
import { iterationRequest } from './plugins/native-wasm.ts';
import { replayFittedEnsemble } from './plugins/native-fitted-portfolio.ts';
const root = process.env.QZ_PM_EXPORT_FIXTURE_DIR;
const source = JSON.parse(await readFile(join(root, 'source.pending.json'), 'utf8'));
assert.equal(canonical(iterationRequest(source.heldoutData, true)), canonical(source.request));
assert.equal(canonical(iterationRequest(source.data, true)), canonical(source.request));
const replay = await replayFittedEnsemble({ data: source.data, members: source.members, fit: source.fit, catalogPrefix: source.catalogPrefix,
  policy: source.policy, partition: 'VALIDATION', binary: process.env.QZ_JOB_BINARY }, BACKGROUND_CONTEXT);
const stats = replay.evidence.simulation.statistics;
assert.equal(stats.find(value => value.native_key === 'Calendar Coverage Ratio').value, 1);
assert.ok(stats.find(value => value.native_key === 'Calendar Expected Days').value >= 2);
assert.ok(replay.evidence.simulationRequest.target_points.length > 4000);
await writeFile(join(root, 'registered-policy-replay.json'), JSON.stringify(replay));
console.log('Registered TEST threshold policy: actual forecast/apply/native shared-account replay verified.');
"#])
        .env("QZ_JOB_BINARY", env!("CARGO_BIN_EXE_job"))
        .output()
        .expect("Node and apps/research npm ci are required for exported PM integration evidence");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::rename(root.join("source.pending.json"), root.join("source.json")).unwrap();
}
