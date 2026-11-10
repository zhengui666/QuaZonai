use contracts::{review_science::*, DbCounter, SchemaV1};
use job::review_science::analyze;

fn request(analysis: NativeReviewAnalysisV1) -> NativeReviewScienceRequestV1 {
    NativeReviewScienceRequestV1 {
        schema_version: SchemaV1,
        source_digest: format!("sha256:{}", "a".repeat(64)),
        analysis,
    }
}
fn times(n: usize) -> Vec<DbCounter> {
    (0..n).map(|i| DbCounter::new(i as u64).unwrap()).collect()
}
fn series(id: &str, returns: Vec<f64>) -> NativeReviewReturnSeriesV1 {
    NativeReviewReturnSeriesV1 {
        id: id.into(),
        returns,
    }
}

#[test]
fn incomplete_ledger_never_manufactures_selection_statistics() {
    let r = request(NativeReviewAnalysisV1::Selection {
        total_trials: 3,
        ledger_complete: false,
        selected_trial_id: "a".into(),
        trials: vec![series("a", vec![0.01; 64]), series("b", vec![0.0; 64])],
        timestamps_ns: times(64),
        frequency: "DAY".into(),
        block_length: 1,
    });
    let out = analyze(&r).unwrap();
    assert!(out
        .metrics
        .iter()
        .all(|m| m.value.is_none() && m.reason.as_deref() == Some("INCOMPLETE_TRIAL_LEDGER")));
}

#[test]
fn dependent_return_replication_does_not_inflate_dsr_confidence() {
    let returns: Vec<_> = (0..3000)
        .map(|i| if (i / 100) % 3 == 0 { 0.02 } else { -0.001 })
        .collect();
    let other: Vec<_> = returns.iter().map(|v| v - 0.002).collect();
    let r = request(NativeReviewAnalysisV1::Selection {
        total_trials: 2,
        ledger_complete: true,
        selected_trial_id: "a".into(),
        trials: vec![series("a", returns), series("b", other)],
        timestamps_ns: times(3000),
        frequency: "DAY".into(),
        block_length: 100,
    });
    let out = analyze(&r).unwrap();
    let dsr = out.metrics.iter().find(|m| m.code == "dsr").unwrap();
    assert_eq!(dsr.value, None);
    assert_eq!(dsr.status, NativeReviewMetricStatusV1::Unsupported);
    assert_eq!(
        dsr.reason.as_deref(),
        Some("DSR_SERIAL_DEPENDENCE_UNSUPPORTED")
    );
}

#[test]
fn explicitly_complete_single_trial_uses_the_zero_selection_benchmark() {
    let returns = (0..180)
        .map(|i| if i % 3 == 0 { -0.001 } else { 0.02 })
        .collect();
    let result = analyze(&request(NativeReviewAnalysisV1::Selection {
        total_trials: 1,
        ledger_complete: true,
        selected_trial_id: "only".into(),
        trials: vec![series("only", returns)],
        timestamps_ns: times(180),
        frequency: "DAY".into(),
        block_length: 1,
    }))
    .unwrap();
    assert!(
        result
            .metrics
            .iter()
            .find(|metric| metric.code == "dsr")
            .unwrap()
            .value
            .unwrap()
            > 0.99
    );
    assert!(result
        .limitations
        .iter()
        .any(|line| line.contains("single-trial")));
    assert!(result
        .metrics
        .iter()
        .find(|metric| metric.code == "pbo")
        .unwrap()
        .value
        .is_none());
}

#[test]
fn shared_account_absolute_evidence_uses_original_complete_daily_grid() {
    let timestamps_ns = (0..180)
        .map(|i| DbCounter::new(i * 86_400_000_000_000).unwrap())
        .collect();
    let result = analyze(&request(NativeReviewAnalysisV1::Portfolio {
        candidate: series(
            "joint",
            (0..180)
                .map(|i| if i % 3 == 0 { -0.001 } else { 0.02 })
                .collect(),
        ),
        baseline: series("cash", vec![0.0; 180]),
        timestamps_ns,
        frequency: "DAY".into(),
        block_length: 5,
    }))
    .unwrap();
    assert!(
        result
            .metrics
            .iter()
            .find(|m| m.code == "candidate_net_return_lower")
            .unwrap()
            .value
            .unwrap()
            > 0.0
    );
    assert_eq!(
        result
            .metrics
            .iter()
            .find(|m| m.code == "return_coverage")
            .unwrap()
            .value,
        Some(1.0)
    );
    assert_eq!(
        result
            .metrics
            .iter()
            .find(|m| m.code == "positive_slice_fraction")
            .unwrap()
            .value,
        Some(1.0)
    );
}

#[test]
fn paired_net_deterioration_is_negative_and_time_mismatch_is_rejected() {
    let baseline: Vec<_> = (0..128)
        .map(|i| if i % 2 == 0 { 0.02 } else { -0.01 })
        .collect();
    let candidate: Vec<_> = baseline.iter().map(|x| x - 0.003).collect();
    let mut r = request(NativeReviewAnalysisV1::Portfolio {
        candidate: series("candidate", candidate),
        baseline: series("baseline", baseline),
        timestamps_ns: times(128),
        frequency: "DAY".into(),
        block_length: 2,
    });
    let out = analyze(&r).unwrap();
    let lower = out
        .metrics
        .iter()
        .find(|m| m.code == "net_return_delta_lower")
        .unwrap();
    assert!(lower.value.unwrap() < -0.0029);
    if let NativeReviewAnalysisV1::Portfolio { timestamps_ns, .. } = &mut r.analysis {
        timestamps_ns[3] = timestamps_ns[2];
    }
    assert!(analyze(&r).is_err());
}

#[test]
fn cscv_detects_training_winner_that_reverses_on_complement() {
    let first: Vec<_> = (0..128)
        .map(|i| {
            // The full mean is zero; any four blocks have a nonzero mean because
            // four distinct integer powers of two cannot sum to 127.5.
            ((1_u32 << (i / 16)) as f64 - 31.875) / 10_000.0
                + if i % 2 == 0 { 0.00001 } else { -0.00001 }
        })
        .collect();
    let second = first.iter().map(|v| -v).collect();
    let rows = vec![series("t0", first), series("t1", second)];
    let out = analyze(&request(NativeReviewAnalysisV1::Selection {
        total_trials: 2,
        ledger_complete: true,
        selected_trial_id: "t0".into(),
        trials: rows,
        timestamps_ns: times(128),
        frequency: "DAY".into(),
        block_length: 1,
    }))
    .unwrap();
    let pbo = out.metrics.iter().find(|m| m.code == "pbo").unwrap();
    assert_eq!(pbo.value, Some(1.0));
}

#[test]
fn invalid_source_and_nonfinite_or_fake_return_rows_are_rejected() {
    let mut r = request(NativeReviewAnalysisV1::Portfolio {
        candidate: series("a", vec![0.01; 64]),
        baseline: series("b", vec![0.0; 64]),
        timestamps_ns: times(64),
        frequency: "DAY".into(),
        block_length: 1,
    });
    r.source_digest = "unbound".into();
    assert!(analyze(&r).is_err());
    r.source_digest = format!("sha256:{}", "b".repeat(64));
    if let NativeReviewAnalysisV1::Portfolio { candidate, .. } = &mut r.analysis {
        candidate.returns[0] = f64::NAN;
    }
    assert!(analyze(&r).is_err());
}

#[test]
fn insufficient_history_is_not_a_zero_confidence_interval() {
    let r = request(NativeReviewAnalysisV1::Portfolio {
        candidate: series("a", vec![0.01; 20]),
        baseline: series("b", vec![0.0; 20]),
        timestamps_ns: times(20),
        frequency: "DAY".into(),
        block_length: 1,
    });
    let out = analyze(&r).unwrap();
    assert!(out
        .metrics
        .iter()
        .filter(|m| m.code.ends_with("lower") || m.code.ends_with("upper"))
        .all(|m| m.value.is_none()));
}

fn factor_points(n: usize) -> Vec<contracts::factors::NativeFactorPointV1> {
    use contracts::{factors::*, science::NativeForecastPointV1};
    (0..n)
        .map(|i| NativeFactorPointV1 {
            observation: NativeForecastPointV1 {
                instrument_id: "asset".into(),
                ordinal: i as u32,
                event_ns: DbCounter::new(i as u64).unwrap(),
                available_ns: DbCounter::new(i as u64).unwrap(),
                forecast: Some((i % 19) as f64),
                forecast_reason: None,
                label_return: Some(-((i % 19) as f64) / 1000.0),
                label_available_ns: Some(DbCounter::new(i as u64 + 1).unwrap()),
                label_reason: None,
            },
            partition: NativeFactorPartitionV1::Validation,
            label_event_ns: Some(DbCounter::new(i as u64 + 1).unwrap()),
            exclusion_reason: None,
        })
        .collect()
}

#[test]
fn signed_rank_ic_and_interval_preserve_negative_direction() {
    let result = job::review_science::factor_diagnostics(
        &factor_points(180),
        contracts::factors::NativeFactorPartitionV1::Validation,
        1,
        NativeReviewFactorScopeV1::PooledTimeSeries,
    )
    .unwrap();
    for key in ["rank_ic", "rank_ic_lower", "rank_ic_upper"] {
        assert!(
            result
                .metrics
                .iter()
                .find(|m| m.code == key)
                .unwrap()
                .value
                .unwrap()
                < -0.999
        );
    }
    assert!(
        result
            .metrics
            .iter()
            .find(|m| m.code == "quantile_spread")
            .unwrap()
            .value
            .unwrap()
            < 0.0
    );
    assert_eq!(
        result
            .metrics
            .iter()
            .find(|m| m.code == "positive_slice_fraction")
            .unwrap()
            .value,
        Some(0.0)
    );
}

#[test]
fn overlapping_labels_use_time_groups_and_cross_sectional_scope_requires_assets() {
    let mut points = factor_points(180);
    for point in &mut points {
        point.label_event_ns = Some(DbCounter::new(point.observation.event_ns.get() + 60).unwrap());
    }
    let result = job::review_science::factor_diagnostics(
        &points,
        contracts::factors::NativeFactorPartitionV1::Validation,
        1,
        NativeReviewFactorScopeV1::PooledTimeSeries,
    )
    .unwrap();
    assert!(result
        .metrics
        .iter()
        .find(|m| m.code == "rank_ic_lower")
        .unwrap()
        .value
        .is_none());
    let cross = job::review_science::factor_diagnostics(
        &points,
        contracts::factors::NativeFactorPartitionV1::Validation,
        1,
        NativeReviewFactorScopeV1::CrossSectional,
    )
    .unwrap();
    assert!(cross
        .metrics
        .iter()
        .find(|m| m.code == "rank_ic")
        .unwrap()
        .value
        .is_none());
}

#[test]
fn cli_preserves_original_digest_and_emits_structured_evidence() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let r = request(NativeReviewAnalysisV1::Portfolio {
        candidate: series("a", vec![0.003; 64]),
        baseline: series("b", vec![0.0; 64]),
        timestamps_ns: times(64),
        frequency: "DAY".into(),
        block_length: 1,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("review-science")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&r).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativeReviewScienceResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result.source_digest, r.source_digest);
    assert_eq!(result.scope, "SHARED_ACCOUNT");
    assert!(
        (result
            .metrics
            .iter()
            .find(|m| m.code == "net_return_delta")
            .unwrap()
            .value
            .unwrap()
            - 0.003)
            .abs()
            < 1e-12
    );
}

#[test]
fn source_cli_rejects_missing_source_owner_metadata_without_success_output() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("review-source")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"schema_version":1,"metadata":{},"observed_at":"2026-11-02T00:00:00Z"}"#)
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("QZ_NATIVE_JOB_FAILED"));
}

#[test]
fn complete_selection_reviews_all_trials_past_the_previous_cohort_quota() {
    let trials = (0..65)
        .map(|trial| {
            series(
                &format!("t{trial}"),
                (0..64)
                    .map(|i| if i % 2 == 0 { 0.01 } else { -0.009 } + trial as f64 * 0.00001)
                    .collect(),
            )
        })
        .collect();
    let result = analyze(&request(NativeReviewAnalysisV1::Selection {
        total_trials: 65,
        ledger_complete: true,
        selected_trial_id: "t64".into(),
        trials,
        timestamps_ns: times(64),
        frequency: "DAY".into(),
        block_length: 1,
    }))
    .unwrap();
    assert_eq!(result.scope, "SELECTION_COHORT");
    for code in ["dsr", "pbo"] {
        assert!(
            result
                .metrics
                .iter()
                .find(|m| m.code == code)
                .unwrap()
                .value
                .is_some(),
            "{code}"
        );
    }
}

#[test]
fn full_factor_diagnostics_detect_duplicates_beyond_the_previous_point_quota() {
    let mut points = factor_points(20_001);
    points[20_000] = points[0].clone();
    let result = job::review_science::factor_diagnostics(
        &points,
        contracts::factors::NativeFactorPartitionV1::Validation,
        1,
        NativeReviewFactorScopeV1::PooledTimeSeries,
    )
    .unwrap();
    let rank = result
        .metrics
        .iter()
        .find(|metric| metric.code == "rank_ic")
        .unwrap();
    assert_eq!(rank.status, NativeReviewMetricStatusV1::InvalidInput);
    assert_eq!(
        rank.reason.as_deref(),
        Some("REVIEW_DUPLICATE_FACTOR_POINT")
    );
}

#[test]
fn long_original_blocks_return_insufficient_evidence_without_a_resource_rejection() {
    let result = analyze(&request(NativeReviewAnalysisV1::Selection {
        total_trials: 1,
        ledger_complete: true,
        selected_trial_id: "only".into(),
        trials: vec![series("only", vec![0.01; 64])],
        timestamps_ns: times(64),
        frequency: "DAY".into(),
        block_length: 20_001,
    }))
    .unwrap();
    assert!(result.metrics.iter().all(|metric| metric.value.is_none()));
}

#[test]
fn cross_sectional_slices_keep_missing_groups_at_original_times() {
    // Two constant groups have no IC. Removing their positions before slicing
    // would change which original time quarters have positive IC.
    let directions = [1.0, 0.0, -1.0, -1.0, 1.0, 0.0, -1.0, 1.0];
    let mut points = Vec::new();
    for (event, direction) in directions.into_iter().enumerate() {
        for asset in 0..3 {
            let mut point = factor_points(1).pop().unwrap();
            point.observation.instrument_id = format!("asset-{asset}");
            point.observation.event_ns = DbCounter::new(event as u64).unwrap();
            point.observation.available_ns = point.observation.event_ns;
            point.observation.forecast = Some(asset as f64);
            point.observation.label_return = Some(direction * asset as f64);
            point.observation.label_available_ns = Some(DbCounter::new(event as u64 + 1).unwrap());
            point.label_event_ns = point.observation.label_available_ns;
            points.push(point);
        }
    }
    let result = job::review_science::factor_diagnostics(
        &points,
        contracts::factors::NativeFactorPartitionV1::Validation,
        1,
        NativeReviewFactorScopeV1::CrossSectional,
    )
    .unwrap();
    let value = |code: &str| {
        result
            .metrics
            .iter()
            .find(|m| m.code == code)
            .unwrap()
            .value
    };
    assert_eq!(value("coverage"), Some(1.0));
    assert_eq!(value("rank_ic"), Some(0.0));
    assert_eq!(value("positive_slice_fraction"), Some(0.5));
    assert_eq!(value("rank_ic_lower"), None);
}

#[test]
fn factor_validation_ignores_other_partitions_but_rejects_included_nonfinite() {
    use contracts::factors::NativeFactorPartitionV1;
    let mut points = factor_points(8);
    let baseline = job::review_science::factor_diagnostics(
        &points,
        NativeFactorPartitionV1::Validation,
        1,
        NativeReviewFactorScopeV1::PooledTimeSeries,
    )
    .unwrap();
    let mut other_partition = points[0].clone();
    other_partition.partition = NativeFactorPartitionV1::Heldout;
    other_partition.observation.forecast = Some(f64::NAN);
    points.push(other_partition);
    let actual = job::review_science::factor_diagnostics(
        &points,
        NativeFactorPartitionV1::Validation,
        1,
        NativeReviewFactorScopeV1::PooledTimeSeries,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(baseline).unwrap()
    );
    points[0].observation.forecast = Some(f64::INFINITY);
    assert_eq!(
        job::review_science::factor_diagnostics(
            &points,
            NativeFactorPartitionV1::Validation,
            1,
            NativeReviewFactorScopeV1::PooledTimeSeries,
        )
        .unwrap_err()
        .to_string(),
        "REVIEW_FACTOR_NONFINITE"
    );
}

#[path = "support/review_diagnostics_fixture.rs"]
mod review_diagnostics_fixture;

fn assert_equivalent_diagnostics(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
    case: &str,
) {
    use serde_json::Value;
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) if !a.is_i64() || !b.is_i64() => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-12 * b.abs().max(1.0),
                "{case}: {a} != {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{case}");
            for (a, b) in a.iter().zip(b) {
                assert_equivalent_diagnostics(a, b, case);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{case}"
            );
            for (key, value) in a {
                assert_equivalent_diagnostics(value, &b[key], case);
            }
        }
        _ => assert_eq!(actual, expected, "{case}"),
    }
}

#[test]
fn factor_diagnostics_match_complete_frozen_dev_outputs() {
    // Captured from the unmodified 8e4059b0 job review-science CLI, not from a
    // second implementation of ranking or resampling. See the task validation.
    let expected: Vec<serde_json::Value> =
        serde_json::from_str(r###"[{"case":"review-pooled_time_series-regular","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":0.007504703961890878,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00022582151779453014,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.75,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.04148248766962754,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.052656439359362386,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.75,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-irregular","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":718,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":0.01249362445325643,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00047657420704295706,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.75,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.04465994833879525,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.06201017830142362,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-ties_signed_zero","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.012545989506726928,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"quantile_monotonicity","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.0971291660732981,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.07779069449686983,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.25,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-constant","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"CONSTANT_OR_INSUFFICIENT_FACTOR_PAIRS"},{"code":"quantile_spread","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"quantile_monotonicity","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"INCOMPLETE_TIME_SLICES"}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-excluded","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":988,"metrics":[{"code":"coverage","value":0.8576388888888888,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.0019534970806312788,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":-8.057050733298874e-05,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.5,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.04993967682661205,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.04835541611612075,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-partitioned","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":768,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.008049922634134649,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":-5.38821909751925e-05,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.25,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":32.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.07632230393527753,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.05053052270121415,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-overlap","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":0.007504703961890878,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00022582151779453014,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.75,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":24.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":0.75,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=4 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-duplicate","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":1153,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":null,"status":"INVALID_INPUT","reason":"REVIEW_DUPLICATE_FACTOR_POINT"}],"limitations":["Factor labels are forward price returns, not executable strategy PnL."]}},{"case":"review-pooled_time_series-missing","error":"REVIEW_INCLUDED_FACTOR_VALUE_MISSING"},{"case":"review-pooled_time_series-groups-0","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":0,"metrics":[{"code":"coverage","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"EMPTY_PARTITION"},{"code":"rank_ic","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"CONSTANT_OR_INSUFFICIENT_FACTOR_PAIRS"},{"code":"quantile_spread","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"quantile_monotonicity","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"bootstrap_blocks","value":0.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"INCOMPLETE_TIME_SLICES"}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=1 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-groups-8","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":96,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.013206343007245738,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.0014333858543417366,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.75,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":4.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-groups-59","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":708,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.016076026127603074,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":-0.0004185267857142857,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.5,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":29.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-pooled_time_series-groups-60","result":{"method_version":"qz.review-science.v1","scope":"POOLED_TIME_SERIES","observations":720,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.01427197214539056,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":-0.00035981976946009963,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.0,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":30.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.06529014898821811,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.04079163370581451,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.25,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-regular","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.005244755244755257,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00041368272569444444,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.4453125,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.05594405594405594,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.042395104895104896,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-irregular","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":718,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.03936029248529249,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.0006510416666666667,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.4482758620689655,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.13220789627039628,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.028885003885003868,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.0,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-ties_signed_zero","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.010739130120674303,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"quantile_monotonicity","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.09697230816669312,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.08026478703045944,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.25,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-constant","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"CONSTANT_OR_INSUFFICIENT_FACTOR_PAIRS"},{"code":"quantile_spread","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"quantile_monotonicity","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"INCOMPLETE_TIME_SLICES"}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-excluded","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":988,"metrics":[{"code":"coverage","value":0.8576388888888888,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.02272727272727273,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00030517578125,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.5104166666666666,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":48.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.0710227272727273,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.029671717171717186,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.25,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-partitioned","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":768,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.021197552447552444,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00020090738932291652,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.43359375,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":32.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.09069055944055944,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.03780594405594408,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-overlap","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":1152,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.005244755244755257,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00041368272569444444,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.4453125,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":24.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=4 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-duplicate","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":1153,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":null,"status":"INVALID_INPUT","reason":"REVIEW_DUPLICATE_FACTOR_POINT"}],"limitations":["Factor labels are forward price returns, not executable strategy PnL."]}},{"case":"review-cross_sectional-missing","error":"REVIEW_INCLUDED_FACTOR_VALUE_MISSING"},{"case":"review-cross_sectional-groups-0","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":0,"metrics":[{"code":"coverage","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"EMPTY_PARTITION"},{"code":"rank_ic","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"CONSTANT_OR_INSUFFICIENT_FACTOR_PAIRS"},{"code":"quantile_spread","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"quantile_monotonicity","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"QUANTILE_GROUPS_INSUFFICIENT"},{"code":"bootstrap_blocks","value":0.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"INCOMPLETE_TIME_SLICES"}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=1 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-groups-8","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":96,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.029720279720279713,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":0.00010172526041666674,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.5,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":4.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":0.5,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-groups-59","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":708,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.03140926869740431,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":-0.00015172581214689272,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.4279661016949153,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":29.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"rank_ic_upper","value":null,"status":"INSUFFICIENT_EVIDENCE","reason":"BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS"},{"code":"positive_slice_fraction","value":0.25,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}},{"case":"review-cross_sectional-groups-60","result":{"method_version":"qz.review-science.v1","scope":"CROSS_SECTIONAL","observations":720,"metrics":[{"code":"coverage","value":1.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic","value":-0.02797202797202799,"status":"AVAILABLE","reason":null},{"code":"quantile_spread","value":-1.3563368055555651e-05,"status":"AVAILABLE","reason":null},{"code":"quantile_monotonicity","value":0.42916666666666664,"status":"AVAILABLE","reason":null},{"code":"bootstrap_blocks","value":30.0,"status":"AVAILABLE","reason":null},{"code":"rank_ic_lower","value":-0.08601398601398604,"status":"AVAILABLE","reason":null},{"code":"rank_ic_upper","value":0.025174825174825163,"status":"AVAILABLE","reason":null},{"code":"positive_slice_fraction","value":0.0,"status":"AVAILABLE","reason":null}],"limitations":["Factor labels are forward price returns, not executable strategy PnL.","Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block=2 synchronized event groups, at least actual label overlap; not a future-profit probability."]}}]"###).unwrap();
    let cases = review_diagnostics_fixture::cases();
    assert_eq!(cases.len(), expected.len());
    for (case, expected) in cases.into_iter().zip(expected) {
        assert_eq!(expected["case"].as_str(), Some(case.name.as_str()));
        let actual = job::review_science::factor_diagnostics(
            &case.points,
            contracts::factors::NativeFactorPartitionV1::Validation,
            1,
            case.scope,
        );
        if let Some(error) = expected.get("error") {
            assert_eq!(
                actual.unwrap_err().to_string(),
                error.as_str().unwrap(),
                "{}",
                case.name
            );
        } else {
            // Keep all evidence/status/count fields exact. The shared native
            // correlation estimator may differ at floating-point rounding scale.
            let actual = serde_json::to_value(actual.unwrap()).unwrap();
            assert_equivalent_diagnostics(&actual, &expected["result"], &case.name);
        }
    }
}
