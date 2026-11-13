//! Bind fixed native operations to the already admitted immutable job inputs.
use crate::{research::invalid, DomainError};
use contracts::{
    execution::NativeTaskParametersV1,
    research::ArtifactInputRole,
    runtime_jobs::{JobSpecV1, RuntimeInputV1},
    science::NativeBarSelectionV1,
    Id,
};
use std::collections::BTreeSet;

pub mod features;
mod output;
pub use output::{output_bindings, output_shape, quality as native_quality};

fn bad(field: &str) -> DomainError {
    invalid(field, "NATIVE_TASK_BINDING_INVALID")
}

/// A missing measurement is valid only when execution metering was disabled.
/// u128 accommodates sums of explicit DbCounter budgets without imposing a cap.
pub(crate) fn fuel_within_budget(
    consumed: Option<contracts::DbCounter>,
    budget: Option<u128>,
) -> bool {
    match (consumed, budget) {
        (Some(consumed), Some(budget)) => u128::from(consumed.get()) <= budget,
        (None, None) => true,
        _ => false,
    }
}

fn selection(value: &NativeBarSelectionV1) -> Result<(), DomainError> {
    if value.bar_types.is_empty()
        || value.event_start_ns >= value.event_end_ns
        || value.event_end_ns > value.decision_cutoff_ns
    {
        return Err(bad("selection"));
    }
    let mut names = BTreeSet::new();
    for name in &value.bar_types {
        crate::control::text(name, 1, 300, false).map_err(|_| bad("selection.bar_types"))?;
        if !names.insert(name) {
            return Err(bad("selection.bar_types"));
        }
    }
    Ok(())
}

pub fn forecast_request(
    request: &contracts::science::NativeForecastRequestV1,
) -> Result<(), DomainError> {
    selection(&request.selection)?;
    let parameters = &request.parameters;
    if parameters.fast_period == 0
        || parameters.slow_period <= parameters.fast_period
        || parameters.label_horizon_observations == 0
        || parameters.total_fuel == Some(contracts::DbCounter::ZERO)
    {
        return Err(bad("forecast_parameters"));
    }
    Ok(())
}

fn dataset(spec: &JobSpecV1, id: Id) -> bool {
    spec.inputs.iter().any(
        |input| matches!(input, RuntimeInputV1::Dataset { revision_id, .. } if *revision_id == id),
    )
}

fn forecast_inputs(
    spec: &JobSpecV1,
    dataset_revision_id: Id,
    model_artifact_id: Id,
    calibration_artifact_id: Option<Id>,
) -> Result<(), DomainError> {
    if !dataset(spec, dataset_revision_id)
        || !artifact(spec, model_artifact_id, ArtifactInputRole::Model)
        || calibration_artifact_id.is_some_and(|id| {
            id == model_artifact_id
                || id == spec.parameters_artifact_id
                || !artifact(spec, id, ArtifactInputRole::Model)
        })
        || spec.inputs.iter().any(|input| match input {
            RuntimeInputV1::Dataset { revision_id, .. } => *revision_id != dataset_revision_id,
            RuntimeInputV1::Artifact {
                artifact_id, role, ..
            } => {
                !(*artifact_id == model_artifact_id && *role == ArtifactInputRole::Model
                    || Some(*artifact_id) == calibration_artifact_id
                        && *role == ArtifactInputRole::Model
                    || *artifact_id == spec.parameters_artifact_id
                        && *role == ArtifactInputRole::Parameters)
            }
        })
    {
        return Err(bad("forecast_inputs"));
    }
    Ok(())
}

fn artifact(spec: &JobSpecV1, id: Id, expected: ArtifactInputRole) -> bool {
    spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Artifact { artifact_id, role, .. } if *artifact_id == id && *role == expected))
}

/// Compiler preprocessing can read files. Therefore compilation gets no dataset,
/// model or other research artifact mount, including a caller's extra unused input.
pub fn task(spec: &JobSpecV1, parameters: &NativeTaskParametersV1) -> Result<(), DomainError> {
    crate::runtime_jobs::spec_shape(spec)?;
    if parameters.job_kind() != spec.job_kind {
        return Err(bad("operation"));
    }
    let expected = parameters.output_schemas();
    if expected.len() != spec.requested_output_schemas.len()
        || expected.iter().any(|schema| {
            !spec
                .requested_output_schemas
                .iter()
                .any(|actual| actual.name == schema.name && actual.version == schema.version)
        })
    {
        return Err(bad("requested_output_schemas"));
    }
    match parameters {
        NativeTaskParametersV1::EvaluateForward { request, .. } => {
            crate::forward::evaluation::request(request)?;
            let ids = request
                .sources
                .iter()
                .map(|s| s.report_artifact_id)
                .collect::<BTreeSet<_>>();
            if ids.contains(&spec.parameters_artifact_id)
                || ids
                    .iter()
                    .any(|id| !artifact(spec, *id, ArtifactInputRole::Report))
                || spec.inputs.iter().any(|input| match input {
                    RuntimeInputV1::Dataset { .. } => true,
                    RuntimeInputV1::Artifact {
                        artifact_id, role, ..
                    } => {
                        !(*artifact_id == spec.parameters_artifact_id
                            && *role == ArtifactInputRole::Parameters
                            || ids.contains(artifact_id) && *role == ArtifactInputRole::Report)
                    }
                })
            {
                return Err(bad("forward_inputs"));
            }
        }
        NativeTaskParametersV1::CompileModel {
            code_artifact_id, ..
        }
        | NativeTaskParametersV1::CompileFeatureModel {
            code_artifact_id, ..
        } => {
            if !artifact(spec, *code_artifact_id, ArtifactInputRole::Code)
                || spec.inputs.iter().any(|input| match input {
                    RuntimeInputV1::Dataset { .. } => true,
                    RuntimeInputV1::Artifact {
                        artifact_id, role, ..
                    } => {
                        !(*artifact_id == *code_artifact_id && *role == ArtifactInputRole::Code
                            || *artifact_id == spec.parameters_artifact_id
                                && *role == ArtifactInputRole::Parameters)
                    }
                })
            {
                return Err(bad("compiler_inputs"));
            }
        }
        NativeTaskParametersV1::ValidateData { selections, .. } => {
            if selections.is_empty() {
                return Err(bad("selections"));
            }
            let mut seen = BTreeSet::new();
            for value in selections {
                selection(&value.selection)?;
                if !dataset(spec, value.dataset_revision_id)
                    || !seen.insert(value.dataset_revision_id)
                {
                    return Err(bad("dataset_revision_id"));
                }
            }
            if spec.inputs.iter().any(|input| match input {
                RuntimeInputV1::Dataset { revision_id, .. } => !seen.contains(revision_id),
                RuntimeInputV1::Artifact {
                    artifact_id, role, ..
                } => {
                    *artifact_id != spec.parameters_artifact_id
                        || *role != ArtifactInputRole::Parameters
                }
            }) {
                return Err(bad("validation_inputs"));
            }
        }
        NativeTaskParametersV1::EvaluateAlpha {
            dataset_revision_id,
            model_artifact_id,
            request,
            ..
        } => {
            forecast_request(request)?;
            forecast_inputs(spec, *dataset_revision_id, *model_artifact_id, None)?;
        }
        NativeTaskParametersV1::EvaluateExperiment { .. } => return Err(bad("retired_task")),
        NativeTaskParametersV1::StudyPortfolio { .. }
        | NativeTaskParametersV1::BuildPortfolio { .. }
        | NativeTaskParametersV1::ComposeStrategyTargets { .. }
        | NativeTaskParametersV1::SimulatePortfolio { .. }
        | NativeTaskParametersV1::SimulateCandidate { .. }
        | NativeTaskParametersV1::SimulatePortfolioSequence { .. } => {
            return Err(bad("retired_task"))
        }
    }
    Ok(())
}
