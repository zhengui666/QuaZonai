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
mod portfolio;
pub mod validation;
pub use output::{
    alpha_sealed_metrics, alpha_sealed_policy, alpha_sealed_request, alpha_validation_metrics,
    alpha_validation_policy, check_alpha_calibration, check_alpha_sealed, check_portfolio_study,
    freeze_alpha_calibration, output_bindings, output_shape, portfolio_simulation_metrics,
};
pub use portfolio::{
    candidate_simulation, portfolio_build_liquidity, portfolio_build_request,
    portfolio_build_result, portfolio_costs, portfolio_execution_costs,
    portfolio_rolling_liquidity_assets, portfolio_sequence, portfolio_study_cutoffs,
    portfolio_study_liquidity_assets,
};

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
    let horizon = u64::from(request.forecast.parameters.label_horizon_observations);
    validation::policy_parameters(&request.split_policy, horizon)
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
            forecast_inputs(spec, *dataset_revision_id, *model_artifact_id, None)?;
        }
        NativeTaskParametersV1::ValidateAlpha {
            dataset_revision_id,
            model_artifact_id,
            request,
            ..
        } => {
            alpha_validation_request(request)?;
            forecast_inputs(spec, *dataset_revision_id, *model_artifact_id, None)?;
            if !spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Dataset {revision_id, role: contracts::research::DataPartition::Validation, ..} if *revision_id == *dataset_revision_id)) {
                return Err(bad("validation_partition"));
            }
        }
        NativeTaskParametersV1::EvaluateSealedAlpha {
            dataset_revision_id,
            model_artifact_id,
            calibration_artifact_id,
            request,
            ..
        } => {
            forecast_request(&request.forecast)?;
            forecast_inputs(
                spec,
                *dataset_revision_id,
                *model_artifact_id,
                *calibration_artifact_id,
            )?;
            if (request.target_kind == contracts::brief::TargetKind::Score) != calibration_artifact_id.is_some()
                || request.research_available_through_ns >= request.forecast.selection.decision_cutoff_ns
                || !spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Dataset { revision_id, role: contracts::research::DataPartition::Sealed, .. } if *revision_id == *dataset_revision_id)) {
                return Err(bad("sealed_inputs"));
            }
        }
        NativeTaskParametersV1::StudyPortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            portfolio_study_cutoffs(request)?;
            let objects = request
                .members
                .iter()
                .flat_map(|m| std::iter::once(m.model_artifact_id).chain(m.calibration_artifact_id))
                .collect::<BTreeSet<_>>();
            let costs = request.mandate.constraints.transaction_costs_ref;
            let liquidity = request.mandate.constraints.liquidity_ref;
            let calendar = request.calendar.as_ref().map(|c| c.artifact_id);
            if !spec.inputs.iter().any(|i| matches!(i, RuntimeInputV1::Dataset { revision_id, role: contracts::research::DataPartition::Discovery | contracts::research::DataPartition::Validation, .. } if revision_id == dataset_revision_id))
                || objects.iter().any(|id| !artifact(spec, *id, ArtifactInputRole::Model))
                || !artifact(spec, costs, ArtifactInputRole::Parameters)
                || liquidity.is_some_and(|id| !artifact(spec, id, ArtifactInputRole::Parameters))
                || calendar.is_some_and(|id| !artifact(spec, id, ArtifactInputRole::Parameters))
                || spec.inputs.iter().any(|i| match i {
                    RuntimeInputV1::Dataset { revision_id, role, .. } => revision_id != dataset_revision_id || !matches!(role, contracts::research::DataPartition::Discovery | contracts::research::DataPartition::Validation),
                    RuntimeInputV1::Artifact { artifact_id, role, .. } => !(*role == ArtifactInputRole::Model && objects.contains(artifact_id)
                        || *role == ArtifactInputRole::Parameters && (*artifact_id == costs || *artifact_id == spec.parameters_artifact_id || Some(*artifact_id) == liquidity || Some(*artifact_id) == calendar)),
                }) { return Err(bad("portfolio_study.inputs")); }
        }
        NativeTaskParametersV1::BuildPortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            portfolio_build_request(request)?;
            let liquidity = request
                .bar_liquidity
                .as_ref()
                .map(|b| b.assumption.report_artifact_id);
            let rolling = request
                .rolling_liquidity
                .as_ref()
                .and(request.mandate.constraints.liquidity_ref);
            let objects = request
                .members
                .iter()
                .flat_map(|m| std::iter::once(m.model_artifact_id).chain(m.calibration_artifact_id))
                .collect::<BTreeSet<_>>();
            if !spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Dataset { revision_id, role: contracts::research::DataPartition::Forward, .. } if revision_id == dataset_revision_id))
                || objects.iter().any(|id| !artifact(spec, *id, ArtifactInputRole::Model))
                || !artifact(spec, request.current_weights_artifact_id, ArtifactInputRole::Report)
                || !artifact(spec, request.mandate.constraints.transaction_costs_ref, ArtifactInputRole::Parameters)
                || liquidity.is_some_and(|id| !artifact(spec, id, ArtifactInputRole::DataQuality))
                || rolling.is_some_and(|id| !artifact(spec, id, ArtifactInputRole::Parameters))
                || spec.inputs.iter().any(|input| match input {
                    RuntimeInputV1::Dataset { revision_id, role, .. } => revision_id != dataset_revision_id || *role != contracts::research::DataPartition::Forward,
                    RuntimeInputV1::Artifact { artifact_id, role, .. } => !(*role == ArtifactInputRole::Model && objects.contains(artifact_id) || (*artifact_id == spec.parameters_artifact_id || *artifact_id == request.mandate.constraints.transaction_costs_ref || Some(*artifact_id) == rolling) && *role == ArtifactInputRole::Parameters || *artifact_id == request.current_weights_artifact_id && *role == ArtifactInputRole::Report || Some(*artifact_id) == liquidity && *role == ArtifactInputRole::DataQuality),
                }) {
                return Err(bad("allocation_inputs"));
            }
        }
        NativeTaskParametersV1::SimulatePortfolio {
            dataset_revision_id,
            request,
            ..
        }
        | NativeTaskParametersV1::SimulateCandidate {
            dataset_revision_id,
            request,
            ..
        }
        | NativeTaskParametersV1::SimulatePortfolioSequence {
            dataset_revision_id,
            request,
            ..
        } => {
            crate::portfolio::simulation_models(&request.settings)?;
            selection(&request.selection)?;
            if !dataset(spec, *dataset_revision_id)
                || spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Dataset { revision_id, .. } if *revision_id != *dataset_revision_id))
                || !(1..=10_000).contains(&request.target_points.len())
            { return Err(bad("simulation_inputs")); }
            let bound = match parameters {
                NativeTaskParametersV1::SimulateCandidate {
                    target_artifact_id,
                    settings_artifact_id,
                    source_selection,
                    ..
                } => {
                    if request.target_points.len() != 1 {
                        return Err(bad("candidate_simulation.inputs"));
                    }
                    Some((
                        vec![*target_artifact_id],
                        settings_artifact_id,
                        source_selection,
                    ))
                }
                NativeTaskParametersV1::SimulatePortfolioSequence {
                    sources,
                    settings_artifact_id,
                    source_selection,
                    ..
                } => {
                    let ids = sources
                        .iter()
                        .map(|s| s.target_artifact_id)
                        .collect::<BTreeSet<_>>();
                    let candidates = sources
                        .iter()
                        .map(|s| s.candidate_id)
                        .collect::<BTreeSet<_>>();
                    if !(2..=253).contains(&sources.len())
                        || sources.len() != request.target_points.len()
                        || ids.len() != sources.len()
                        || candidates.len() != sources.len()
                    {
                        return Err(bad("portfolio_sequence.inputs"));
                    }
                    Some((
                        ids.into_iter().collect(),
                        settings_artifact_id,
                        source_selection,
                    ))
                }
                _ => None,
            };
            if let Some((target_artifact_ids, settings_artifact_id, source_selection)) = bound {
                selection(source_selection)?;
                if source_selection.bar_types != request.selection.bar_types
                    || source_selection.maximum_rows != request.selection.maximum_rows
                    || source_selection.event_start_ns > request.selection.event_start_ns
                    || source_selection.event_end_ns < request.selection.event_end_ns
                    || source_selection.decision_cutoff_ns < request.selection.decision_cutoff_ns
                    || target_artifact_ids
                        .iter()
                        .any(|id| !artifact(spec, *id, ArtifactInputRole::Report))
                    || !artifact(spec, *settings_artifact_id, ArtifactInputRole::Parameters)
                    || spec.inputs.iter().any(|input| match input {
                        RuntimeInputV1::Dataset { role, .. } => {
                            *role != contracts::research::DataPartition::Forward
                        }
                        RuntimeInputV1::Artifact {
                            artifact_id, role, ..
                        } => {
                            !(*role == ArtifactInputRole::Report
                                && target_artifact_ids.contains(artifact_id)
                                || *role == ArtifactInputRole::Parameters
                                    && (*artifact_id == spec.parameters_artifact_id
                                        || artifact_id == settings_artifact_id))
                        }
                    })
                {
                    return Err(bad("candidate_simulation.inputs"));
                }
            }
        }
    }
    Ok(())
}
