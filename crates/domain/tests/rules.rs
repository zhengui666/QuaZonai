//! Scientific evidence and exact numerical comparison rules; service admission is TypeScript.
use chrono::{Duration, TimeZone, Utc};
use contracts::{evidence::*, DbCounter, Id, SchemaV1};
use domain::evidence::*;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 5, 0, 0, 0).unwrap()
}
fn requirement() -> MetricRequirementV1 {
    MetricRequirementV1 {
        schema_version: SchemaV1,
        metric_code: "risk".into(),
        scope: "total".into(),
        comparator: Comparator::Le,
        threshold_low: None,
        threshold_high: Some("0.2".parse().unwrap()),
        required: true,
        minimum_observations: count(30),
        method_allowlist: vec!["upstream-risk".into()],
    }
}
fn metric(evaluation_id: Id) -> MetricValueV1 {
    MetricValueV1 {
        schema_version: SchemaV1,
        evaluation_id,
        metric_code: "risk".into(),
        scope: "total".into(),
        value: Some(0.1),
        status: MetricStatus::Ok,
        reason_code: None,
        unit: "fraction".into(),
        period_start: time() - Duration::days(31),
        period_end: time(),
        observation_count: count(30),
        frequency: "daily".into(),
        annualization_factor: Some(252.0),
        method_id: "upstream-risk".into(),
        method_version: "1.0".into(),
        source_artifact_id: Id::new(),
        higher_is_better: Some(false),
    }
}
fn capability() -> MetricCapability {
    MetricCapability {
        metric_code: "risk".into(),
        method_id: "upstream-risk".into(),
        method_version: "1.0".into(),
        unit: "fraction".into(),
        frequency: "daily".into(),
    }
}

#[test]
fn actual_supported_metrics_pass_or_reject_without_computing_an_estimator() {
    let id = Id::new();
    let m = metric(id);
    assert_eq!(
        evaluate_metrics(
            id,
            &[requirement()],
            std::slice::from_ref(&m),
            &[capability()]
        )
        .unwrap()
        .decision,
        Decision::Pass
    );
    let mut m = m;
    m.value = Some(0.3);
    assert_eq!(
        evaluate_metrics(id, &[requirement()], &[m], &[capability()])
            .unwrap()
            .decision,
        Decision::Reject
    );
}

#[test]
fn missing_required_evidence_dominates_both_pass_and_reject() {
    let id = Id::new();
    let mut other = requirement();
    other.metric_code = "another-required-metric".into();
    for value in [0.1, 0.3] {
        let mut m = metric(id);
        m.value = Some(value);
        let gate =
            evaluate_metrics(id, &[requirement(), other.clone()], &[m], &[capability()]).unwrap();
        assert_eq!(gate.decision, Decision::Inconclusive);
        assert_eq!(gate.evidence_status, EvidenceStatus::Incomplete);
    }
    assert_eq!(
        evaluate_metrics(id, &[requirement()], &[], &[])
            .unwrap()
            .decision,
        Decision::Inconclusive
    );
    assert!(evaluate_metrics(id, &[], &[], &[]).is_err());
}

#[test]
fn unsupported_method_version_units_frequency_or_count_never_qualify() {
    let id = Id::new();
    for m in [
        MetricValueV1 {
            method_version: "unaudited-version".into(),
            ..metric(id)
        },
        MetricValueV1 {
            unit: "percent".into(),
            ..metric(id)
        },
        MetricValueV1 {
            frequency: "hourly".into(),
            ..metric(id)
        },
        MetricValueV1 {
            observation_count: count(29),
            ..metric(id)
        },
        MetricValueV1 {
            status: MetricStatus::Unsupported,
            value: None,
            reason_code: Some("UPSTREAM_UNSUPPORTED".into()),
            ..metric(id)
        },
    ] {
        assert_eq!(
            evaluate_metrics(id, &[requirement()], &[m], &[capability()])
                .unwrap()
                .decision,
            Decision::Inconclusive
        );
    }
    assert_eq!(
        evaluate_metrics(id, &[requirement()], &[metric(id)], &[])
            .unwrap()
            .evidence_status,
        EvidenceStatus::Unsupported
    );
}

#[test]
fn corrupt_or_mixed_evidence_is_rejected_not_coerced_to_missing_or_pass() {
    let id = Id::new();
    for m in [
        MetricValueV1 {
            value: Some(f64::NAN),
            ..metric(id)
        },
        MetricValueV1 {
            value: None,
            ..metric(id)
        },
        MetricValueV1 {
            annualization_factor: Some(f64::INFINITY),
            ..metric(id)
        },
        MetricValueV1 {
            period_end: time() - Duration::days(60),
            ..metric(id)
        },
        MetricValueV1 {
            status: MetricStatus::Failed,
            value: None,
            reason_code: None,
            ..metric(id)
        },
        metric(Id::new()),
    ] {
        assert!(evaluate_metrics(id, &[requirement()], &[m], &[capability()]).is_err());
    }
    assert!(evaluate_metrics(
        id,
        &[requirement()],
        &[metric(id), metric(id)],
        &[capability()]
    )
    .is_err());
    let mut invalid = requirement();
    invalid.threshold_low = Some("1".parse().unwrap());
    assert!(evaluate_metrics(id, &[invalid], &[metric(id)], &[capability()]).is_err());
}

#[test]
fn invalid_metric_is_not_missing_evidence() {
    let id = Id::new();
    let mut m = metric(id);
    m.status = MetricStatus::InvalidInput;
    m.value = None;
    m.reason_code = Some("INVALID_DATA".into());
    let gate = evaluate_metrics(id, &[requirement()], &[m], &[capability()]).unwrap();
    assert_eq!(gate.evidence_status, EvidenceStatus::Invalid);
    assert_eq!(gate.decision, Decision::Inconclusive);
    assert!(gate.reasons.iter().any(|r| r.ends_with("INVALID_INPUT")));
}

#[test]
fn invalid_input_requires_registered_native_provenance_and_policy_allowlist() {
    let id = Id::new();
    let original = MetricValueV1 {
        status: MetricStatus::InvalidInput,
        value: None,
        reason_code: Some("BAD_INPUT".into()),
        ..metric(id)
    };
    for m in [
        MetricValueV1 {
            method_id: "unregistered".into(),
            ..original.clone()
        },
        MetricValueV1 {
            method_version: "unreviewed".into(),
            ..original.clone()
        },
        MetricValueV1 {
            unit: "wrong-unit".into(),
            ..original.clone()
        },
        MetricValueV1 {
            frequency: "wrong-frequency".into(),
            ..original.clone()
        },
    ] {
        let gate = evaluate_metrics(id, &[requirement()], &[m], &[capability()]).unwrap();
        assert_eq!(gate.evidence_status, EvidenceStatus::Unsupported);
        assert_eq!(gate.decision, Decision::Inconclusive);
        assert!(gate
            .reasons
            .iter()
            .all(|reason| !reason.ends_with(":INVALID_INPUT")));
    }
    let mut denied = requirement();
    denied.method_allowlist = vec!["another-method".into()];
    let gate = evaluate_metrics(
        id,
        &[denied],
        std::slice::from_ref(&original),
        &[capability()],
    )
    .unwrap();
    assert_eq!(gate.evidence_status, EvidenceStatus::Unsupported);
    let gate = evaluate_metrics(id, &[requirement()], &[original], &[capability()]).unwrap();
    assert_eq!(gate.evidence_status, EvidenceStatus::Invalid);
}

#[test]
fn frozen_decimal_threshold_is_not_rounded_into_the_reported_float() {
    let id = Id::new();
    let mut rule = requirement();
    rule.comparator = Comparator::Ge;
    rule.threshold_low = Some("0.10000000000000001".parse().unwrap());
    rule.threshold_high = None;
    assert_eq!(
        evaluate_metrics(id, &[rule], &[metric(id)], &[capability()])
            .unwrap()
            .decision,
        Decision::Reject
    );
}

#[test]
fn reversed_bounds_remain_invalid_even_when_both_round_to_the_same_float() {
    let id = Id::new();
    let mut rule = requirement();
    rule.comparator = Comparator::Between;
    rule.threshold_low = Some("0.10000000000000001".parse().unwrap());
    rule.threshold_high = Some("0.1".parse().unwrap());
    assert!(evaluate_metrics(id, &[rule], &[metric(id)], &[capability()]).is_err());
}

#[test]
fn decimal_metric_comparisons_obey_open_and_closed_boundaries_in_both_directions() {
    let id = Id::new();
    for (value, bound) in [
        (0.1, "0.1"),
        (-0.1, "-0.1"),
        (0.0, "0"),
        (1e-18, "0.000000000000000001"),
    ] {
        for (comparator, passes) in [
            (Comparator::Ge, true),
            (Comparator::Le, true),
            (Comparator::Gt, false),
            (Comparator::Lt, false),
            (Comparator::Between, true),
        ] {
            let mut rule = requirement();
            rule.comparator = comparator;
            rule.threshold_low = match comparator {
                Comparator::Lt | Comparator::Le => None,
                _ => Some(bound.parse().unwrap()),
            };
            rule.threshold_high = match comparator {
                Comparator::Gt | Comparator::Ge => None,
                _ => Some(bound.parse().unwrap()),
            };
            let mut report = metric(id);
            report.value = Some(value);
            let gate = evaluate_metrics(id, &[rule], &[report], &[capability()]).unwrap();
            assert_eq!(
                gate.decision,
                if passes {
                    Decision::Pass
                } else {
                    Decision::Reject
                },
                "{value}: {comparator:?}"
            );
        }
    }
}

#[test]
fn metric_extremes_are_compared_without_money_range_truncation() {
    let id = Id::new();
    for (value, decision) in [
        (f64::MAX, Decision::Reject),
        (-f64::MAX, Decision::Pass),
        (f64::from_bits(1), Decision::Pass),
    ] {
        let mut report = metric(id);
        report.value = Some(value);
        assert_eq!(
            evaluate_metrics(id, &[requirement()], &[report], &[capability()])
                .unwrap()
                .decision,
            decision
        );
    }
}
