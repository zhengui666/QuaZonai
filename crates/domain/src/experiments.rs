//! Proposal validation, not scientific evaluation or qualification.
use crate::{DomainError, control, research::invalid};
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

pub fn evaluate(request: &contracts::experiments::ExperimentEvaluateV1) -> Result<(), DomainError> {
    if request.compile_limits.experiments != 1 {
        return Err(invalid(
            "compile_limits.experiments",
            "EXPERIMENT_CHARGE_REQUIRED",
        ));
    }
    let mut compile = request.compile_limits.clone();
    compile.experiments = 0;
    crate::data::bounded_native_limits(&compile)?;
    crate::data::bounded_native_limits(&request.evaluation_limits)
}

pub fn evaluation_parameters(
    value: &contracts::science::ExperimentEvaluationParametersV1,
) -> Result<(), DomainError> {
    crate::execution::features::schema(&value.feature_schema)?;
    crate::execution::features::artifact_ids(&value.feature_artifact_ids)?;
    crate::control::text(&value.instrument_id, 1, 200, false)?;
    if value.label_horizon_observations == 0
        || value.total_fuel == Some(contracts::DbCounter::ZERO)
        || value.target_ttl_ns == contracts::DbCounter::ZERO
    {
        return Err(invalid(
            "experiment_parameters",
            "EXPERIMENT_PARAMETER_INVALID",
        ));
    }
    crate::portfolio::simulation_settings(&value.settings)
}

/// Adoption freezes an existing result; no trial or financial verdict is added.
pub fn adopt_alpha(
    request: &contracts::strategy_portfolio::StrategyAlphaAdoptV1,
) -> Result<(), DomainError> {
    control::text(&request.name, 1, 200, false)?;
    Ok(())
}

#[cfg(test)]
mod target_adoption_tests {
    use super::*;
    use contracts::{
        Id, Revision, SchemaV1,
        control::{OperatorCommand, OperatorOperation},
        strategy_portfolio::*,
    };

    #[test]
    fn target_adoption_preserves_command_identity_and_accepts_the_u16_fold_ordinal() {
        let mut intent = StrategyAlphaAdoptIntentV1 {
            schema_version: SchemaV1,
            experiment_id: Id::new(),
            request: StrategyAlphaAdoptV1 {
                schema_version: SchemaV1,
                expected_revision: Revision::INITIAL,
                name: "Original policy".into(),
                source_fold_index: 31,
            },
        };
        adopt_alpha(&intent.request).unwrap();
        let command = OperatorCommand::ExperimentAdoptAlpha(intent.clone());
        control::command(&command).unwrap();
        assert_eq!(command.operation(), OperatorOperation::ExperimentAdoptAlpha);
        assert_eq!(command.operation().code(), "EXPERIMENT_ADOPT_ALPHA");
        assert!(!command.operation().creates());
        assert_eq!(
            command.normalized_request().unwrap(),
            serde_json::to_value(&intent).unwrap()
        );
        intent.request.source_fold_index = u16::MAX;
        assert!(adopt_alpha(&intent.request).is_ok());
        intent.request.source_fold_index = 0;
        intent.request.name = " ".into();
        assert!(adopt_alpha(&intent.request).is_err());
        intent.request.name = "a".repeat(201);
        assert!(adopt_alpha(&intent.request).is_err());
    }
}
