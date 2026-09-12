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

mod output;
pub mod validation;
pub use output::{alpha_validation_metrics, output_bindings, output_shape};

fn bad(field: &str) -> DomainError {
    invalid(field, "NATIVE_TASK_BINDING_INVALID")
}

fn selection(value: &NativeBarSelectionV1) -> Result<(), DomainError> {
    if !(1..=256).contains(&value.bar_types.len())
        || !(1..=1_000_000).contains(&value.maximum_rows)
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
        || parameters.slow_period > 10_000
        || !(1..=100_000).contains(&parameters.label_horizon_observations)
        || !(1..=1_000_000_000).contains(&parameters.total_fuel.get())
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

pub fn alpha_validation_request(
    request: &contracts::science::NativeAlphaValidationRequestV1,
) -> Result<(), DomainError> {
    forecast_request(&request.forecast)?;
    crate::research::split(&request.split_policy)?;
    let policy = &request.split_policy;
    let horizon = u64::from(request.forecast.parameters.label_horizon_observations);
    if policy.label_horizon_observations.map(|n| n.get()) != Some(horizon)
        || policy.purge_observations.get() < horizon
        || policy.train_size.get() < 3
        || [
            policy.train_size,
            policy.test_size,
            policy.purge_observations,
            policy.embargo_observations,
        ]
        .into_iter()
        .chain(policy.step_size)
        .any(|n| n.get() > validation::MAX_VALIDATION_ROWS as u64)
        || policy.group_count.is_some_and(|n| n > 16)
    {
        return Err(bad("validation_parameters"));
    }
    Ok(())
}

fn forecast_inputs(
    spec: &JobSpecV1,
    dataset_revision_id: Id,
    model_artifact_id: Id,
) -> Result<(), DomainError> {
    if !dataset(spec, dataset_revision_id)
        || !artifact(spec, model_artifact_id, ArtifactInputRole::Model)
        || spec.inputs.iter().any(|input| match input {
            RuntimeInputV1::Dataset { revision_id, .. } => *revision_id != dataset_revision_id,
            RuntimeInputV1::Artifact {
                artifact_id, role, ..
            } => {
                !(*artifact_id == model_artifact_id && *role == ArtifactInputRole::Model
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
        NativeTaskParametersV1::CompileModel {
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
            if !(1..=256).contains(&selections.len()) {
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
            forecast_inputs(spec, *dataset_revision_id, *model_artifact_id)?;
        }
        NativeTaskParametersV1::ValidateAlpha {
            dataset_revision_id,
            model_artifact_id,
            request,
            ..
        } => {
            alpha_validation_request(request)?;
            forecast_inputs(spec, *dataset_revision_id, *model_artifact_id)?;
            if !spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Dataset {revision_id, role: contracts::research::DataPartition::Validation, ..} if *revision_id == *dataset_revision_id)) {
                return Err(bad("validation_partition"));
            }
        }
        NativeTaskParametersV1::BuildPortfolio { request, .. } => {
            crate::portfolio::allocation_input(request)?;
            if spec
                .inputs
                .iter()
                .any(|input| matches!(input, RuntimeInputV1::Dataset { .. }))
            {
                return Err(bad("allocation_inputs"));
            }
        }
        NativeTaskParametersV1::SimulatePortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            selection(&request.selection)?;
            if !dataset(spec, *dataset_revision_id)
                || spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Dataset { revision_id, .. } if *revision_id != *dataset_revision_id))
                || !(1..=10_000).contains(&request.target_points.len())
            { return Err(bad("simulation_inputs")); }
        }
    }
    Ok(())
}
