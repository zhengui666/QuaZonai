use contracts::{experiments::ExperimentProposalV1, Id, SchemaV1};
use domain::experiments::proposal;
use serde_json::json;

fn request() -> ExperimentProposalV1 {
    ExperimentProposalV1 {
        schema_version: SchemaV1,
        cycle_id: Id::new(),
        family_id: Id::new(),
        parent_experiment_id: None,
        hypothesis: "A falsifiable question\nwith economic rationale".into(),
        expected_failure_modes: "Costs, selection bias and missing observations".into(),
        proposal_artifact_id: Id::new(),
        parameter_artifact_id: Id::new(),
        code_artifact_id: Some(Id::new()),
    }
}

#[test]
fn strict_proposal_never_accepts_runtime_or_qualification_authority() {
    for field in [
        "id",
        "project_id",
        "root_lineage_id",
        "ordinal",
        "run_id",
        "author_run_id",
        "author_attempt_id",
        "trial_source",
        "outcome",
        "decision",
        "origin",
        "budget",
    ] {
        let mut value = serde_json::to_value(request()).unwrap();
        value[field] = json!("PASS");
        assert!(
            serde_json::from_value::<ExperimentProposalV1>(value).is_err(),
            "{field}"
        );
    }
    for missing in [
        "schema_version",
        "cycle_id",
        "family_id",
        "hypothesis",
        "expected_failure_modes",
        "proposal_artifact_id",
        "parameter_artifact_id",
    ] {
        let mut value = serde_json::to_value(request()).unwrap();
        value.as_object_mut().unwrap().remove(missing);
        assert!(
            serde_json::from_value::<ExperimentProposalV1>(value).is_err(),
            "{missing}"
        );
    }
}

#[test]
fn text_is_bounded_in_unicode_characters_and_controls_are_rejected() {
    let mut value = request();
    assert!(proposal(&value).is_ok());
    value.hypothesis = "研".repeat(8000);
    assert!(proposal(&value).is_ok());
    value.hypothesis.push('究');
    assert!(proposal(&value).is_err());
    for invalid in ["", " \t\n", "invalid\0field", "invalid\u{7f}field"] {
        value = request();
        value.expected_failure_modes = invalid.into();
        assert!(proposal(&value).is_err());
    }
}

#[test]
fn artifact_roles_are_distinct_and_an_uncompiled_proposal_is_explicit() {
    let mut value = request();
    value.code_artifact_id = None;
    assert!(proposal(&value).is_ok());
    value.code_artifact_id = Some(value.parameter_artifact_id);
    assert!(proposal(&value).is_err());
    value.code_artifact_id = None;
    value.parameter_artifact_id = value.proposal_artifact_id;
    assert!(proposal(&value).is_err());
}
