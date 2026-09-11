use chrono::{TimeZone, Utc};
use contracts::{evidence::*, DbCounter, Id, SchemaV1};
use domain::evidence::{evaluate_metrics, MetricCapability};

fn requirement(
    comparator: Comparator,
    low: Option<&str>,
    high: Option<&str>,
) -> MetricRequirementV1 {
    MetricRequirementV1 {
        schema_version: SchemaV1,
        metric_code: "risk".into(),
        scope: "total".into(),
        comparator,
        threshold_low: low.map(|x| x.parse().unwrap()),
        threshold_high: high.map(|x| x.parse().unwrap()),
        required: true,
        minimum_observations: DbCounter::new(1).unwrap(),
        method_allowlist: vec!["native-risk".into()],
    }
}

#[test]
fn exact_bounds_are_used_for_validation_and_every_gate_comparator() {
    let id = Id::new();
    let metric = MetricValueV1 {
        schema_version: SchemaV1,
        evaluation_id: id,
        metric_code: "risk".into(),
        scope: "total".into(),
        value: Some(0.1),
        status: MetricStatus::Ok,
        reason_code: None,
        unit: "fraction".into(),
        period_start: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        period_end: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
        observation_count: DbCounter::new(30).unwrap(),
        frequency: "daily".into(),
        annualization_factor: None,
        method_id: "native-risk".into(),
        method_version: "1".into(),
        source_artifact_id: Id::new(),
        higher_is_better: None,
    };
    let capability = MetricCapability {
        metric_code: "risk".into(),
        method_id: "native-risk".into(),
        method_version: "1".into(),
        unit: "fraction".into(),
        frequency: "daily".into(),
    };
    for (rule, expected) in [
        (
            requirement(Comparator::Ge, Some("0.10000000000000001"), None),
            Decision::Reject,
        ),
        (
            requirement(Comparator::Gt, Some("0.1"), None),
            Decision::Reject,
        ),
        (
            requirement(Comparator::Ge, Some("0.1"), None),
            Decision::Pass,
        ),
        (
            requirement(Comparator::Lt, None, Some("0.10000000000000001")),
            Decision::Pass,
        ),
        (
            requirement(Comparator::Le, None, Some("0.09999999999999999")),
            Decision::Reject,
        ),
        (
            requirement(
                Comparator::Between,
                Some("0.09999999999999999"),
                Some("0.10000000000000001"),
            ),
            Decision::Pass,
        ),
    ] {
        assert_eq!(
            evaluate_metrics(
                id,
                &[rule],
                std::slice::from_ref(&metric),
                std::slice::from_ref(&capability)
            )
            .unwrap()
            .decision,
            expected
        );
    }
    let reversed = requirement(
        Comparator::Between,
        Some("0.10000000000000001"),
        Some("0.1"),
    );
    assert!(evaluate_metrics(id, &[reversed], &[metric], &[capability]).is_err());
}
