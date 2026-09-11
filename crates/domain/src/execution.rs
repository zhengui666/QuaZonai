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

/// Shared shape check for the actual fixed native outputs. This is not a PIT,
/// statistical, investment-performance or qualification decision.
pub fn output_shape(
    output: &contracts::runtime_jobs::RuntimeOutputV1,
    bytes: &[u8],
) -> Result<(), DomainError> {
    use contracts::execution::{NativeDataQualityReportV1, NativeModelCompilationV1};
    let Some(contract) = contracts::runtime_jobs::native_output_contract(
        &output.schema.name,
        &output.schema.version,
    ) else {
        return Err(invalid(
            "native_output.schema",
            "NATIVE_OUTPUT_CONTRACT_INVALID",
        ));
    };
    if output.kind != contract.kind
        || output.media_type != contract.media_type
        || output.storage_version.get() != 1
        || bytes.is_empty()
        || bytes.len() as u64 != output.byte_count.get()
        || bytes.len() as u64 > contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES
    {
        return Err(invalid("native_output", "NATIVE_OUTPUT_CONTRACT_INVALID"));
    }
    macro_rules! native {
        ($kind:ty) => {
            serde_json::from_slice::<$kind>(bytes)
                .map(|_| ())
                .map_err(|_| invalid("native_output.body", "NATIVE_OUTPUT_CONTRACT_INVALID"))
        };
    }
    match contract.name {
        "qz.wasm_model"
            if bytes.starts_with(b"\0asm\x01\0\0\0") && bytes.len() <= 2 * 1024 * 1024 =>
        {
            Ok(())
        }
        "qz.model_compilation" => native!(NativeModelCompilationV1),
        "qz.data_quality" => native!(NativeDataQualityReportV1),
        "qz.native_forecast" => native!(contracts::science::NativeForecastResultV1),
        "qz.native_allocation" => native!(contracts::portfolio::AllocationResultV1),
        "qz.native_simulation" => native!(contracts::science::NativeSimulationResultV1),
        _ => Err(invalid(
            "native_output.schema",
            "NATIVE_OUTPUT_CONTRACT_INVALID",
        )),
    }
}

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

fn dataset(spec: &JobSpecV1, id: Id) -> bool {
    spec.inputs.iter().any(
        |input| matches!(input, RuntimeInputV1::Dataset { revision_id, .. } if *revision_id == id),
    )
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
            selection(&request.selection)?;
            if !dataset(spec, *dataset_revision_id)
                || !artifact(spec, *model_artifact_id, ArtifactInputRole::Model)
            {
                return Err(bad("forecast_inputs"));
            }
            let parameters = &request.parameters;
            if parameters.fast_period == 0
                || parameters.slow_period <= parameters.fast_period
                || parameters.slow_period > 10_000
                || !(1..=100_000).contains(&parameters.label_horizon_observations)
                || !(1..=1_000_000_000).contains(&parameters.total_fuel.get())
            {
                return Err(bad("forecast_parameters"));
            }
            if spec.inputs.iter().any(|input| match input {
                RuntimeInputV1::Dataset { revision_id, .. } => *revision_id != *dataset_revision_id,
                RuntimeInputV1::Artifact {
                    artifact_id, role, ..
                } => {
                    !(*artifact_id == *model_artifact_id && *role == ArtifactInputRole::Model
                        || *artifact_id == spec.parameters_artifact_id
                            && *role == ArtifactInputRole::Parameters)
                }
            }) {
                return Err(bad("forecast_inputs"));
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
