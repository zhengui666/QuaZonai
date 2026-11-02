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
