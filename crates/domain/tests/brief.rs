use contracts::{brief::*, research::DataPartition, DbCounter};
use domain::{brief, DomainError};
fn example() -> BriefCreate {
    serde_json::from_str(include_str!("../../../tests/contracts/research-brief.json")).unwrap()
}
#[test]
fn authoring_distinguishes_fixed_and_variable_horizons_without_claiming_support() {
    let mut r = example();
    assert!(brief::content(&r.content, &r.bindings).is_ok());
    r.content.horizon_value = None;
    assert!(brief::content(&r.content, &r.bindings).is_err());
    r.content.horizon_kind = HorizonKind::VariableInterval;
    assert!(brief::content(&r.content, &r.bindings).is_ok());
    r.content.horizon_value = Some(DbCounter::ZERO);
    assert!(brief::content(&r.content, &r.bindings).is_err());
    r.content.horizon_kind = HorizonKind::FixedDuration;
    assert!(brief::content(&r.content, &r.bindings).is_err());
}
#[test]
fn sealed_permission_and_duplicate_binding_do_not_become_research_access() {
    let mut r = example();
    r.bindings[1].access_policy = DataAccess::ResearchRead;
    assert!(brief::content(&r.content, &r.bindings).is_err());
    r.bindings[1].access_policy = DataAccess::MetadataOnly;
    assert!(brief::content(&r.content, &r.bindings).is_ok());
    r.bindings[1].dataset_revision_id = r.bindings[0].dataset_revision_id;
    assert!(brief::content(&r.content, &r.bindings).is_err());
    r = example();
    r.bindings[0].role = DataPartition::Validation;
    r.bindings[0].access_policy = DataAccess::EvaluatorOnly;
    assert!(brief::content(&r.content, &r.bindings).is_err());
}
#[test]
fn invalid_authoring_returns_nonsecret_field_issues() {
    let mut r = example();
    r.content.hypothesis = " ".into();
    r.content.base_currency = "ZZZ".into();
    r.content.budget.max_experiments = 0;
    let Err(DomainError::Fields(issues)) = brief::content(&r.content, &r.bindings) else {
        panic!("field issues required")
    };
    for field in [
        "content.hypothesis",
        "content.base_currency",
        "content.budget",
    ] {
        assert!(issues.iter().any(|i| i.field == field));
    }
    let r = example();
    for n in [0, 65] {
        let bindings = vec![r.bindings[0].clone(); n];
        assert!(brief::content(&r.content, &bindings).is_err());
    }
}
