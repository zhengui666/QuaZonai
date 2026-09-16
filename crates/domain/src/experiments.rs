//! Proposal validation, not scientific evaluation or qualification.
use crate::{control, research::invalid, DomainError};
use contracts::experiments::ExperimentProposalV1;

pub fn proposal(request: &ExperimentProposalV1) -> Result<(), DomainError> {
    for (field, value) in [
        ("hypothesis", &request.hypothesis),
        ("expected_failure_modes", &request.expected_failure_modes),
    ] {
        control::text(value, 1, 8000, true).map_err(|_| invalid(field, "TEXT_RANGE"))?;
    }
    if request.proposal_artifact_id == request.parameter_artifact_id
        || request.code_artifact_id.is_some_and(|code| {
            code == request.proposal_artifact_id || code == request.parameter_artifact_id
        })
    {
        return Err(invalid(
            "proposal_artifact_id",
            "ARTIFACT_ROLES_MUST_DIFFER",
        ));
    }
    Ok(())
}
