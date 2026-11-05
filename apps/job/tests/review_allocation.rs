use contracts::{review_allocation::*, DecimalValue, SchemaV1};
fn d(s: &str) -> DecimalValue {
    s.parse::<DecimalValue>().unwrap()
}
fn request() -> NativeReviewAllocationRequestV1 {
    NativeReviewAllocationRequestV1 {
        schema_version: SchemaV1,
        kind: NativeReviewAllocationKindV1::Portfolio,
        members: vec![
            NativeReviewAllocationMemberV1 {
                id: "a".into(),
                source_report_id: "ra".into(),
                score: 100.0,
                observations: (0..60)
                    .map(|i| if i % 2 == 0 { 0.01 } else { -0.01 })
                    .collect(),
            },
            NativeReviewAllocationMemberV1 {
                id: "b".into(),
                source_report_id: "rb".into(),
                score: 80.0,
                observations: (0..60)
                    .map(|i| if i % 3 == 0 { 0.01 } else { -0.005 })
                    .collect(),
            },
        ],
        member_caps: vec![
            NativeReviewMemberCapV1 {
                id: "a".into(),
                maximum_weight: d("0.8"),
            },
            NativeReviewMemberCapV1 {
                id: "b".into(),
                maximum_weight: d("0.8"),
            },
        ],
        cluster_caps: vec![],
        current_weights: vec![
            NativeReviewCurrentWeightV1 {
                id: "a".into(),
                weight: d("0.5"),
            },
            NativeReviewCurrentWeightV1 {
                id: "b".into(),
                weight: d("0.5"),
            },
        ],
        maximum_turnover: d("1"),
    }
}
#[test]
fn quality_risk_and_exact_simplex_are_reported_with_sources() {
    let output = job::review_allocate::allocate(&request()).unwrap();
    assert_eq!(output.members.len(), 2);
    let sum = output
        .members
        .iter()
        .map(|m| m.weight.as_decimal())
        .sum::<bigdecimal::BigDecimal>();
    assert_eq!(sum, bigdecimal::BigDecimal::from(1));
    assert_eq!(output.members[0].quality_coefficient, 1.0);
    assert_eq!(output.members[1].quality_coefficient, 0.5);
    assert_eq!(output.members[0].source_report_id, "ra");
    assert!(output.requires_new_candidate && output.requires_shared_account_study);
    assert!(output.objective.is_finite() && output.lambda > 0.0);
    assert!(output.primal_residual < 1e-8 && output.dual_residual < 1e-8);
}
#[test]
fn score_changes_change_the_quality_prior_without_becoming_return_forecasts() {
    let mut input = request();
    input.kind = NativeReviewAllocationKindV1::Factor;
    let first = job::review_allocate::allocate(&input).unwrap();
    input.members[0].score = 80.0;
    input.members[1].score = 100.0;
    let second = job::review_allocate::allocate(&input).unwrap();
    assert!(first.members[0].weight.as_decimal() > second.members[0].weight.as_decimal());
    assert_eq!(first.covariance, second.covariance);
}
#[test]
fn infeasible_caps_do_not_return_fallback_weights() {
    let mut input = request();
    for cap in &mut input.member_caps {
        cap.maximum_weight = d("0.4");
    }
    assert!(job::review_allocate::allocate(&input)
        .unwrap_err()
        .to_string()
        .contains("NOT_OPTIMAL"));
}
#[test]
fn missing_zero_risk_or_unapproved_input_is_refused() {
    let mut input = request();
    input.members[0].observations = vec![0.0; 60];
    assert!(job::review_allocate::allocate(&input)
        .unwrap_err()
        .to_string()
        .contains("ZERO_RISK"));
    let mut input = request();
    input.members[0].score = 79.9;
    assert!(job::review_allocate::allocate(&input)
        .unwrap_err()
        .to_string()
        .contains("UNAPPROVED_SCORE"));
    let mut input = request();
    input.current_weights.pop();
    assert!(job::review_allocate::allocate(&input).is_err());
}
#[test]
fn cluster_and_turnover_constraints_remain_binding() {
    let mut input = request();
    input.maximum_turnover = d("0.1");
    input.cluster_caps = vec![NativeReviewClusterCapV1 {
        id: "one".into(),
        members: vec!["a".into()],
        maximum_weight: d("0.54"),
    }];
    let output = job::review_allocate::allocate(&input).unwrap();
    assert!(output.members[0].weight.as_decimal() <= d("0.54").as_decimal());
    let change = output
        .members
        .iter()
        .zip(&input.current_weights)
        .map(|(a, b)| (a.weight.as_decimal() - b.weight.as_decimal()).abs())
        .sum::<bigdecimal::BigDecimal>();
    assert!(change <= *input.maximum_turnover.as_decimal());
}

#[test]
fn allocation_accepts_the_complete_member_and_cluster_sets_past_previous_quotas() {
    let mut input = request();
    let template = input.members[0].clone();
    input.members = (0..65)
        .map(|index| NativeReviewAllocationMemberV1 {
            id: format!("m{index}"),
            source_report_id: format!("r{index}"),
            ..template.clone()
        })
        .collect();
    input.member_caps = input
        .members
        .iter()
        .map(|m| NativeReviewMemberCapV1 {
            id: m.id.clone(),
            maximum_weight: d("1"),
        })
        .collect();
    input.current_weights = input
        .members
        .iter()
        .map(|m| NativeReviewCurrentWeightV1 {
            id: m.id.clone(),
            weight: d("0"),
        })
        .collect();
    input.cluster_caps = (0..129)
        .map(|index| NativeReviewClusterCapV1 {
            id: format!("c{index}"),
            members: vec!["m0".into()],
            maximum_weight: d("1"),
        })
        .collect();
    let result = job::review_allocate::allocate(&input).unwrap();
    assert_eq!(result.members.len(), 65);
    assert!(result
        .members
        .iter()
        .all(|m| m.weight.as_decimal() > &bigdecimal::BigDecimal::from(0)));
}
